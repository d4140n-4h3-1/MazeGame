// Copyright (c) 2019-present Dmitry Stepanov and Fyrox Engine contributors.
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! Converts the depth written by vertex shaders from OpenGL's clip space to wgpu's.
//!
//! The engine builds its projection matrices the OpenGL way, with clip-space depth running from
//! `-w` to `w`. wgpu clips everything below zero instead, which throws away the near half of
//! every depth range, and it stores the rest unchanged, so depth read back from a depth buffer
//! no longer means what the shaders (written for OpenGL's `0..1` window depth) expect.
//!
//! Rather than touching every matrix in the engine, the vertex entry point is wrapped: the
//! user's `vs_main` is renamed and called from a generated `vs_main` that remaps the output
//! position with `z = (z + w) / 2`. After that, depth values are exactly OpenGL's window depth.

/// Name the user's vertex entry point is renamed to.
const INNER_NAME: &str = "fyrox_user_vs_main";

/// Returns `source` with its `vs_main` wrapped as described in the [module docs](self), or
/// `None` if the entry point could not be understood - the source is then used unchanged.
pub fn remap_vertex_depth(source: &str) -> Option<String> {
    let fn_pos = find_word(source, "fn vs_main")?;
    let open = fn_pos + source[fn_pos..].find('(')?;
    let close = matching_paren(source, open)?;
    let body_open = close + source[close..].find('{')?;
    let params = &source[open + 1..close];
    let ret = source[close + 1..body_open]
        .trim()
        .strip_prefix("->")?
        .trim();

    // The `@vertex` attribute right before `fn`, which has to move to the wrapper.
    let attr_pos = source[..fn_pos].rfind("@vertex")?;
    if !source[attr_pos + "@vertex".len()..fn_pos].trim().is_empty() {
        return None;
    }

    let params: Vec<&str> = split_top_level(params)
        .into_iter()
        .filter(|p| !p.trim().is_empty())
        .collect();
    let mut inner_params = Vec::new();
    let mut args = Vec::new();
    for param in &params {
        let bare = strip_attributes(param);
        let name = bare.split(':').next()?.trim();
        if name.is_empty() {
            return None;
        }
        args.push(name.to_string());
        inner_params.push(bare.to_string());
    }

    let (inner_ret, fix) = if ret.contains("@builtin(position)") {
        (
            strip_attributes(ret).to_string(),
            "    out.z = (out.z + out.w) * 0.5;\n".to_string(),
        )
    } else {
        let field = position_field(source, ret)?;
        (
            ret.to_string(),
            format!("    out.{field}.z = (out.{field}.z + out.{field}.w) * 0.5;\n"),
        )
    };

    let mut result = String::with_capacity(source.len() + 256);
    result += &source[..attr_pos];
    result += &format!(
        "fn {INNER_NAME}({}) -> {inner_ret}",
        inner_params.join(", ")
    );
    result += &source[body_open..];
    result += &format!(
        "\n@vertex\nfn vs_main({}) -> {ret} {{\n    var out = {INNER_NAME}({});\n{fix}    return out;\n}}\n",
        params.join(", "),
        args.join(", ")
    );
    Some(result)
}

/// Finds `needle` where it is not part of a longer identifier.
fn find_word(source: &str, needle: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = source[from..].find(needle) {
        let at = from + i;
        let end = at + needle.len();
        let before_ok = at == 0 || !is_ident(source.as_bytes()[at - 1]);
        let after_ok = end >= source.len() || !is_ident(source.as_bytes()[end]);
        if before_ok && after_ok {
            return Some(at);
        }
        from = end;
    }
    None
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn matching_paren(source: &str, open: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in source[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + i);
                }
            }
            _ => (),
        }
    }
    None
}

/// Splits on commas that are not inside `()` or `<>`.
fn split_top_level(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in text.char_indices() {
        match c {
            '(' | '<' => depth += 1,
            ')' | '>' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&text[start..i]);
                start = i + 1;
            }
            _ => (),
        }
    }
    parts.push(&text[start..]);
    parts
}

/// Removes leading `@attr` / `@attr(...)` annotations.
fn strip_attributes(text: &str) -> &str {
    let mut rest = text.trim();
    while let Some(after_at) = rest.strip_prefix('@') {
        let name_len = after_at
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(after_at.len());
        let mut after = after_at[name_len..].trim_start();
        if after.starts_with('(') {
            let offset = text.len() - after.len();
            match matching_paren(text, offset) {
                Some(close) => after = &text[close + 1..],
                None => return rest,
            }
        }
        rest = after.trim_start();
    }
    rest
}

/// Finds the member of `struct type_name` that carries `@builtin(position)`.
fn position_field(source: &str, type_name: &str) -> Option<String> {
    let decl = find_word(source, &format!("struct {type_name}"))?;
    let open = decl + source[decl..].find('{')?;
    let close = open + source[open..].find('}')?;
    for member in source[open + 1..close].split(',') {
        if member.contains("@builtin(position)") {
            let bare = strip_attributes(member);
            return Some(bare.split(':').next()?.trim().to_string());
        }
    }
    None
}

#[cfg(test)]
mod test {
    use super::*;

    fn validate(source: &str) {
        let module = wgpu::naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|e| panic!("{}\n{source}", e.emit_to_string(source)));
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    }

    #[test]
    fn wraps_a_struct_returning_entry_point() {
        let source = r#"
            struct VertexInput {
                @location(0) vertexPosition: vec3f,
                @location(1) vertexTexCoord: vec2f,
            };

            struct VertexOutput {
                @builtin(position) position: vec4f,
                @location(0) texCoord: vec2f,
            };

            @vertex
            fn vs_main(input: VertexInput) -> VertexOutput {
                var output: VertexOutput;
                output.texCoord = input.vertexTexCoord;
                output.position = vec4f(input.vertexPosition, 1.0);
                return output;
            }
        "#;
        let wrapped = remap_vertex_depth(source).unwrap();
        assert!(wrapped.contains("out.position.z = (out.position.z + out.position.w) * 0.5;"));
        validate(&wrapped);
    }

    #[test]
    fn wraps_a_builtin_returning_entry_point_with_attributed_params() {
        let source = r#"
            @vertex fn vs_main(@location(0) vertexPosition: vec3f, @builtin(instance_index) i: u32) -> @builtin(position) vec4f {
                return vec4f(vertexPosition, f32(i));
            }
        "#;
        let wrapped = remap_vertex_depth(source).unwrap();
        assert!(wrapped.contains("fn fyrox_user_vs_main(vertexPosition: vec3f, i: u32) -> vec4f"));
        assert!(wrapped.contains("fyrox_user_vs_main(vertexPosition, i)"));
        validate(&wrapped);
    }

    #[test]
    fn leaves_sources_without_an_entry_point_alone() {
        assert!(remap_vertex_depth("fn helper() -> f32 { return 1.0; }").is_none());
    }
}
