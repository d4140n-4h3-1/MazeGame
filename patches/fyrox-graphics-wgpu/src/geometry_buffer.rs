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

use crate::server::WgpuGraphicsServer;
use fyrox_graphics::{
    core::{array_as_u8_slice, math::TriangleDefinition},
    error::FrameworkError,
    geometry_buffer::{
        AttributeKind, ElementsDescriptor, GpuGeometryBufferDescriptor, GpuGeometryBufferTrait,
    },
    ElementKind,
};
use std::cell::{Cell, RefCell};
use std::rc::Weak;

/// Returns `true` if the attribute is stored as integers that the shader must see as floats.
///
/// OpenGL hands every vertex attribute to the shader as floating point, converting integer data
/// by value unless it is normalized, and the engine's shaders are written for that. wgpu has no
/// vertex format that does the same, so such attributes are converted to `f32` on upload.
fn needs_float_conversion(kind: AttributeKind, normalized: bool) -> bool {
    !normalized && !matches!(kind, AttributeKind::Float)
}

/// Where one attribute lives in the engine's vertex data and in the uploaded data.
#[derive(Clone, Copy)]
struct AttributeRemap {
    kind: AttributeKind,
    component_count: usize,
    convert: bool,
    src_offset: usize,
    dst_offset: usize,
}

/// How a vertex buffer's data is rewritten before upload, for buffers with attributes that need
/// [`needs_float_conversion`].
struct VertexConversion {
    src_stride: usize,
    dst_stride: usize,
    attributes: Vec<AttributeRemap>,
}

impl VertexConversion {
    /// Rewrites vertex data laid out with the source stride into the converted layout.
    fn convert(&self, data: &[u8]) -> Vec<u8> {
        let count = data.len() / self.src_stride.max(1);
        let mut out = vec![0u8; count * self.dst_stride];
        for (src, dst) in data
            .chunks_exact(self.src_stride)
            .zip(out.chunks_exact_mut(self.dst_stride))
        {
            for attr in &self.attributes {
                let size = attr.kind.size();
                for c in 0..attr.component_count {
                    let s = attr.src_offset + c * size;
                    if !attr.convert {
                        dst[attr.dst_offset + c * size..attr.dst_offset + (c + 1) * size]
                            .copy_from_slice(&src[s..s + size]);
                        continue;
                    }
                    let value = match attr.kind {
                        AttributeKind::UnsignedByte => src[s] as f32,
                        AttributeKind::UnsignedShort => {
                            u16::from_le_bytes([src[s], src[s + 1]]) as f32
                        }
                        AttributeKind::UnsignedInt => {
                            u32::from_le_bytes([src[s], src[s + 1], src[s + 2], src[s + 3]]) as f32
                        }
                        AttributeKind::Float => unreachable!("floats are never converted"),
                    };
                    let d = attr.dst_offset + c * 4;
                    dst[d..d + 4].copy_from_slice(&value.to_le_bytes());
                }
            }
        }
        out
    }
}

/// Maps a Fyrox [`AttributeKind`] + component count + normalization flag to a
/// wgpu [`VertexFormat`].
fn attribute_format(kind: AttributeKind, cc: usize, normalized: bool) -> wgpu::VertexFormat {
    let kind = if needs_float_conversion(kind, normalized) {
        AttributeKind::Float
    } else {
        kind
    };
    match (kind, cc, normalized) {
        (AttributeKind::Float, 1, _) => wgpu::VertexFormat::Float32,
        (AttributeKind::Float, 2, _) => wgpu::VertexFormat::Float32x2,
        (AttributeKind::Float, 3, _) => wgpu::VertexFormat::Float32x3,
        (AttributeKind::Float, 4, _) => wgpu::VertexFormat::Float32x4,
        (AttributeKind::UnsignedByte, 4, true) => wgpu::VertexFormat::Unorm8x4,
        (AttributeKind::UnsignedByte, 4, false) => wgpu::VertexFormat::Uint8x4,
        (AttributeKind::UnsignedShort, 2, true) => wgpu::VertexFormat::Unorm16x2,
        (AttributeKind::UnsignedShort, 2, false) => wgpu::VertexFormat::Uint16x2,
        (AttributeKind::UnsignedShort, 4, true) => wgpu::VertexFormat::Unorm16x4,
        (AttributeKind::UnsignedShort, 4, false) => wgpu::VertexFormat::Uint16x4,
        (AttributeKind::UnsignedInt, 1, _) => wgpu::VertexFormat::Uint32,
        (AttributeKind::UnsignedInt, 2, _) => wgpu::VertexFormat::Uint32x2,
        (AttributeKind::UnsignedInt, 3, _) => wgpu::VertexFormat::Uint32x3,
        (AttributeKind::UnsignedInt, 4, _) => wgpu::VertexFormat::Uint32x4,
        _ => wgpu::VertexFormat::Float32x4,
    }
}

/// Wgpu implementation of [`GpuGeometryBufferTrait`].
///
/// Holds vertex buffers, their layouts, and an index (element) buffer for indexed
/// drawing. Supports triangles, lines, and points as element types.
///
/// Vertex attribute layouts are leaked once at construction to satisfy wgpu's
/// `'static` lifetime requirement — see the comment in [`new`](Self::new) for details.
pub struct WgpuGeometryBuffer {
    _server: Weak<WgpuGraphicsServer>,
    vertex_buffers: RefCell<Vec<wgpu::Buffer>>,
    vertex_buffer_layouts: Vec<wgpu::VertexBufferLayout<'static>>,
    /// Per vertex buffer, how its data is rewritten before upload, if at all.
    conversions: Vec<Option<VertexConversion>>,
    element_buffer: RefCell<wgpu::Buffer>,
    element_count: Cell<usize>,
    element_kind: ElementKind,
}

impl WgpuGeometryBuffer {
    /// Creates a new geometry buffer from the given descriptor.
    ///
    /// Creates vertex buffers with their attribute layouts and an index buffer.
    /// Attribute layouts are leaked once to satisfy wgpu's `'static` lifetime
    /// requirement for `VertexBufferLayout::attributes`.
    pub fn new(
        server: &WgpuGraphicsServer,
        desc: GpuGeometryBufferDescriptor,
    ) -> Result<Self, FrameworkError> {
        let mut vertex_buffers = Vec::new();
        let mut vertex_buffer_layouts = Vec::new();
        let mut conversions = Vec::new();

        for (i, buf) in desc.buffers.iter().enumerate() {
            let mut remaps = Vec::new();
            let mut src_offset = 0usize;
            let mut dst_offset = 0usize;
            for attr in buf.attributes {
                let convert = needs_float_conversion(attr.kind, attr.normalized);
                let dst_size = if convert {
                    4 * attr.component_count
                } else {
                    attr.kind.size() * attr.component_count
                };
                if convert {
                    // Keep converted floats on a 4-byte boundary.
                    dst_offset = dst_offset.next_multiple_of(4);
                }
                remaps.push(AttributeRemap {
                    kind: attr.kind,
                    component_count: attr.component_count,
                    convert,
                    src_offset,
                    dst_offset,
                });
                src_offset += attr.kind.size() * attr.component_count;
                dst_offset += dst_size;
            }
            let conversion = remaps.iter().any(|r| r.convert).then(|| VertexConversion {
                src_stride: buf.data.element_size,
                dst_stride: dst_offset.max(buf.data.element_size).next_multiple_of(4),
                attributes: remaps.clone(),
            });

            let converted = conversion
                .as_ref()
                .zip(buf.data.bytes)
                .map(|(conversion, bytes)| conversion.convert(bytes));
            let bytes = converted.as_deref().or(buf.data.bytes);

            let data_size = bytes.map(|b| b.len()).unwrap_or(0);
            let label_str = format!("{}VB{i}", desc.name);
            let buffer = server.state.device.create_buffer(&wgpu::BufferDescriptor {
                label: if server.named_objects {
                    Some(&label_str)
                } else {
                    None
                },
                size: data_size.max(1) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            if let Some(data) = bytes {
                if !data.is_empty() {
                    server.state.queue.write_buffer(&buffer, 0, data);
                }
            }

            let mut attributes = Vec::new();
            for (attr, remap) in buf.attributes.iter().zip(&remaps) {
                attributes.push(wgpu::VertexAttribute {
                    format: attribute_format(attr.kind, attr.component_count, attr.normalized),
                    offset: remap.dst_offset as u64,
                    shader_location: attr.location,
                });
            }

            let step_mode = if buf.attributes.iter().any(|a| a.divisor > 0) {
                wgpu::VertexStepMode::Instance
            } else {
                wgpu::VertexStepMode::Vertex
            };
            // NOTE: Attributes are leaked to satisfy wgpu's `'static` lifetime requirement
            // for `VertexBufferLayout::attributes`. Each geometry buffer leaks one small
            // `Vec<VertexAttribute>` per vertex buffer (typically < 100 bytes). This is
            // acceptable because geometry buffers are long-lived (tied to mesh lifetime)
            // and relatively few in number.
            vertex_buffer_layouts.push(wgpu::VertexBufferLayout {
                array_stride: conversion
                    .as_ref()
                    .map_or(buf.data.element_size, |c| c.dst_stride)
                    as u64,
                step_mode,
                attributes: attributes.leak(),
            });
            vertex_buffers.push(buffer);
            conversions.push(conversion);
        }

        let (element_count, element_data) = match desc.elements {
            ElementsDescriptor::Triangles(t) => (t.len(), array_as_u8_slice(t)),
            ElementsDescriptor::Lines(l) => (l.len(), array_as_u8_slice(l)),
            ElementsDescriptor::Points(p) => (p.len(), array_as_u8_slice(p)),
        };

        let ib_label = format!("{}IB", desc.name);
        let element_buffer = server.state.device.create_buffer(&wgpu::BufferDescriptor {
            label: if server.named_objects {
                Some(&ib_label)
            } else {
                None
            },
            size: element_data.len().max(1) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !element_data.is_empty() {
            server
                .state
                .queue
                .write_buffer(&element_buffer, 0, element_data);
        }

        Ok(Self {
            _server: server.weak_ref(),
            vertex_buffers: RefCell::new(vertex_buffers),
            vertex_buffer_layouts,
            conversions,
            element_buffer: RefCell::new(element_buffer),
            element_count: Cell::new(element_count),
            element_kind: desc.elements.element_kind(),
        })
    }

    /// Returns the number of elements (triangles, lines, or points) in the index buffer.
    pub fn element_count(&self) -> usize {
        self.element_count.get()
    }
    /// Returns the type of elements stored in the index buffer.
    pub fn element_kind(&self) -> ElementKind {
        self.element_kind
    }
    /// Returns a borrow of the vertex buffer list.
    pub fn vertex_buffers(&self) -> std::cell::Ref<'_, Vec<wgpu::Buffer>> {
        self.vertex_buffers.borrow()
    }
    /// Returns the vertex buffer layout descriptors for all vertex buffers.
    pub fn vertex_buffer_layouts(&self) -> &[wgpu::VertexBufferLayout<'static>] {
        &self.vertex_buffer_layouts
    }
    /// Returns a borrow of the index (element) buffer.
    pub fn element_buffer(&self) -> std::cell::Ref<'_, wgpu::Buffer> {
        self.element_buffer.borrow()
    }

    /// Writes index data to the element buffer, recreating it if the new data is larger
    /// than the current buffer capacity.
    fn write_element_data(&self, count: usize, data: &[u8]) {
        if let Some(server) = self._server.upgrade() {
            // Commands recorded before this write must not see its data.
            server.submit_pending();
            self.element_count.set(count);
            let mut eb = self.element_buffer.borrow_mut();
            if (data.len() as u64) <= eb.size() {
                server.state.queue.write_buffer(&eb, 0, data);
            } else {
                let new_buf = server.state.device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: data.len() as u64,
                    usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                server.state.queue.write_buffer(&new_buf, 0, data);
                *eb = new_buf;
            }
        }
    }
}

impl GpuGeometryBufferTrait for WgpuGeometryBuffer {
    fn set_buffer_data(&self, buffer_idx: usize, data: &[u8]) {
        let converted = self
            .conversions
            .get(buffer_idx)
            .and_then(|c| c.as_ref())
            .map(|c| c.convert(data));
        let data = converted.as_deref().unwrap_or(data);
        if let Some(server) = self._server.upgrade() {
            // Commands recorded before this write must not see its data.
            server.submit_pending();
            let mut bufs = self.vertex_buffers.borrow_mut();
            if let Some(buf) = bufs.get(buffer_idx) {
                if (data.len() as u64) <= buf.size() {
                    server.state.queue.write_buffer(buf, 0, data);
                } else {
                    let new_buf = server.state.device.create_buffer(&wgpu::BufferDescriptor {
                        label: None,
                        size: data.len() as u64,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    server.state.queue.write_buffer(&new_buf, 0, data);
                    bufs[buffer_idx] = new_buf;
                }
            }
        }
    }
    fn element_count(&self) -> usize {
        self.element_count.get()
    }
    fn set_triangles(&self, triangles: &[TriangleDefinition]) {
        self.write_element_data(triangles.len(), array_as_u8_slice(triangles));
    }
    fn set_lines(&self, lines: &[[u32; 2]]) {
        self.write_element_data(lines.len(), array_as_u8_slice(lines));
    }
    fn set_points(&self, points: &[u32]) {
        self.write_element_data(points.len(), array_as_u8_slice(points));
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn remap(
        kind: AttributeKind,
        component_count: usize,
        convert: bool,
        src_offset: usize,
        dst_offset: usize,
    ) -> AttributeRemap {
        AttributeRemap {
            kind,
            component_count,
            convert,
            src_offset,
            dst_offset,
        }
    }

    #[test]
    fn integer_attributes_become_floats_by_value() {
        // A skinned vertex: position (3 f32), bone weights (4 f32), bone indices (4 u8).
        let conversion = VertexConversion {
            src_stride: 32,
            dst_stride: 44,
            attributes: vec![
                remap(AttributeKind::Float, 3, false, 0, 0),
                remap(AttributeKind::Float, 4, false, 12, 12),
                remap(AttributeKind::UnsignedByte, 4, true, 28, 28),
            ],
        };

        let mut vertex = Vec::new();
        for v in [1.0f32, 2.0, 3.0, 0.25, 0.25, 0.25, 0.25] {
            vertex.extend_from_slice(&v.to_le_bytes());
        }
        vertex.extend_from_slice(&[0, 7, 42, 255]);
        let data = [vertex.clone(), vertex].concat();

        let out = conversion.convert(&data);
        assert_eq!(out.len(), 2 * 44);
        for chunk in out.chunks_exact(44) {
            let f = |i: usize| f32::from_le_bytes(chunk[i..i + 4].try_into().unwrap());
            assert_eq!([f(0), f(4), f(8)], [1.0, 2.0, 3.0]);
            assert_eq!(f(12), 0.25);
            assert_eq!([f(28), f(32), f(36), f(40)], [0.0, 7.0, 42.0, 255.0]);
        }
    }

    #[test]
    fn only_non_normalized_integers_are_converted() {
        assert!(needs_float_conversion(AttributeKind::UnsignedByte, false));
        assert!(needs_float_conversion(AttributeKind::UnsignedShort, false));
        assert!(!needs_float_conversion(AttributeKind::UnsignedByte, true));
        assert!(!needs_float_conversion(AttributeKind::Float, false));
        assert_eq!(
            attribute_format(AttributeKind::UnsignedByte, 4, false),
            wgpu::VertexFormat::Float32x4
        );
        assert_eq!(
            attribute_format(AttributeKind::UnsignedByte, 4, true),
            wgpu::VertexFormat::Unorm8x4
        );
    }
}
