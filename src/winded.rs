//! A droid out of breath shows it the way characters in games always have: sweat drops above its
//! head, bobbing and throbbing, for as long as it has to walk to get its breath back (see
//! [`crate::inhabitants`]). They fade in as it runs out, and away again once it can run.
//!
//! The drops are a sprite - always facing the camera, and lit by nothing but itself - drawn here
//! into a texture when the game starts, so there is no picture to load.

use fyrox::{
    core::{algebra::Vector3, color::Color, pool::Handle},
    graph::SceneGraph,
    material::{Material, MaterialResource},
    resource::texture::{Texture, TextureKind, TexturePixelKind, TextureResource},
    scene::{
        base::BaseBuilder,
        graph::Graph,
        node::Node,
        sprite::{Sprite, SpriteBuilder},
        transform::TransformBuilder,
    },
};

/// How many pixels across the drops' picture is, and how many samples across each pixel is
/// worked out from, to smooth its edges.
const PICTURE: usize = 128;
const SAMPLES: usize = 4;
/// How far above the droid's face they float, in meters, and how big they are across.
const ABOVE: f32 = 0.55;
const SIZE: f32 = 0.45;
/// How far they bob up and down, in meters, and how fast, in bobs a second; how much bigger they
/// throb, as a part of their size, and how fast.
const BOB: f32 = 0.04;
const BOB_RATE: f32 = 1.6;
const THROB: f32 = 0.1;
const THROB_RATE: f32 = 3.2;
/// How long they take to fade in or out, in seconds.
const FADE: f32 = 0.25;

/// Each drop: where the middle of its round end is, as a part of the picture across and down;
/// how big that end is across, as a part of the picture; and which way its tip points, in
/// radians from straight up, clockwise.
const DROPS: [(f32, f32, f32, f32); 3] = [
    (0.5, 0.62, 0.17, 0.0),
    (0.2, 0.5, 0.1, -0.55),
    (0.8, 0.5, 0.1, 0.55),
];
/// How long a drop's tip is from the middle of its round end, as a part of that end's size.
const TIP: f32 = 2.1;
/// How thick its outline is, as a part of the picture.
const OUTLINE: f32 = 0.028;
/// Its colours: the drop, its outline, and the glint on it; in sRGB.
const FILL: [u8; 3] = [110, 200, 255];
const EDGE: [u8; 3] = [16, 38, 92];
const GLINT: [u8; 3] = [255, 255, 255];

/// How far outside the drop of size `r`, with its round end's middle at the origin and its tip
/// straight up - `y` down - a point `(x, y)` is, roughly; 0 or less inside.
fn outside_drop(x: f32, y: f32, r: f32) -> f32 {
    let tip = -r * TIP;
    if y >= 0.0 {
        // The round end.
        (x * x + y * y).sqrt() - r
    } else if y > tip {
        // Narrowing to the tip.
        x.abs() - r * ((y - tip) / -tip).powf(1.3)
    } else {
        (x * x + (y - tip).powi(2)).sqrt()
    }
}

/// What colour the drops' picture is at `(u, v)`, across and down it from 0 to 1, and how solid.
fn drops_at(u: f32, v: f32) -> ([u8; 3], f32) {
    // Nearer ones in front: the middle drop last.
    let mut colour = ([0, 0, 0], 0.0);
    for &(cx, cy, r, angle) in DROPS[1..].iter().chain(std::iter::once(&DROPS[0])) {
        let (dx, dy) = (u - cx, v - cy);
        let (sin, cos) = angle.sin_cos();
        // Into the drop's own terms, turned so that its tip is straight up.
        let (x, y) = (dx * cos + dy * sin, -dx * sin + dy * cos);
        let out = outside_drop(x, y, r);
        if out > OUTLINE {
            continue;
        }
        let glint = ((x + r * 0.4).powi(2) + (y + r * 0.1).powi(2)).sqrt() < r * 0.28;
        colour = match out {
            _ if out > 0.0 => (EDGE, 1.0),
            _ if glint => (GLINT, 1.0),
            _ => (FILL, 1.0),
        };
    }
    colour
}

/// The drops' picture, `size` pixels across and down, as RGBA bytes, row by row from the top.
fn picture(size: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(size * size * 4);
    for row in 0..size {
        for column in 0..size {
            let (mut sum, mut solid) = ([0.0; 3], 0.0);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let u = (column as f32 + (sx as f32 + 0.5) / SAMPLES as f32) / size as f32;
                    let v = (row as f32 + (sy as f32 + 0.5) / SAMPLES as f32) / size as f32;
                    let (colour, alpha) = drops_at(u, v);
                    for (s, c) in sum.iter_mut().zip(colour) {
                        *s += c as f32 * alpha;
                    }
                    solid += alpha;
                }
            }
            let samples = (SAMPLES * SAMPLES) as f32;
            for s in sum {
                bytes.push(if solid > 0.0 { (s / solid) as u8 } else { 0 });
            }
            bytes.push((solid / samples * 255.0).round() as u8);
        }
    }
    bytes
}

/// Drops shown over one droid.
#[derive(Debug, PartialEq)]
struct Shown {
    /// Which droid, as an index, and the sprite.
    droid: usize,
    sprite: Handle<Node>,
    /// Where its face was last, how far faded in they are, from 0 to 1, and whether it is still
    /// out of breath.
    face: Vector3<f32>,
    faded: f32,
    still: bool,
}

/// The drops above the heads of the droids out of breath.
#[derive(Debug, Default, PartialEq)]
pub struct Winded {
    material: Option<MaterialResource>,
    shown: Vec<Shown>,
    /// How long they have been bobbing, in seconds.
    time: f32,
}

impl Winded {
    /// Draws the drops' picture.
    pub fn make() -> Self {
        let texture = Texture::from_bytes(
            TextureKind::Rectangle {
                width: PICTURE as u32,
                height: PICTURE as u32,
            },
            TexturePixelKind::RGBA8,
            picture(PICTURE),
        )
        .map(TextureResource::new_embedded);
        let mut material = Material::standard_sprite();
        material.bind("diffuseTexture", texture);
        Self {
            material: Some(MaterialResource::new_embedded(material)),
            shown: Vec::new(),
            time: 0.0,
        }
    }

    /// Shows the drops over each of `winded` - a droid, as an index, out of breath, and where its
    /// face is - fading them in, and fades them away over those that have their breath back,
    /// after another `dt`.
    pub fn update(&mut self, graph: &mut Graph, winded: &[(usize, Vector3<f32>)], dt: f32) {
        self.time += dt;
        let Some(material) = &self.material else {
            return;
        };
        for shown in &mut self.shown {
            shown.still = false;
        }
        for &(droid, face) in winded {
            match self.shown.iter_mut().find(|shown| shown.droid == droid) {
                Some(shown) => {
                    shown.face = face;
                    shown.still = true;
                }
                None => {
                    let sprite = SpriteBuilder::new(BaseBuilder::new().with_local_transform(
                        TransformBuilder::new().with_local_position(face).build(),
                    ))
                    .with_material(material.clone())
                    .with_size(0.0)
                    .build(graph)
                    .to_base();
                    self.shown.push(Shown {
                        droid,
                        sprite,
                        face,
                        faded: 0.0,
                        still: true,
                    });
                }
            }
        }
        let bob = BOB * (std::f32::consts::TAU * BOB_RATE * self.time).sin();
        let throb = 1.0 + THROB * (std::f32::consts::TAU * THROB_RATE * self.time).sin().max(0.0);
        let step = dt / FADE;
        self.shown.retain_mut(|shown| {
            shown.faded = match shown.still {
                true => shown.faded + step,
                false => shown.faded - step,
            }
            .clamp(0.0, 1.0);
            if !shown.still && shown.faded == 0.0 {
                if graph.is_valid_handle(shown.sprite) {
                    graph.remove_node(shown.sprite);
                }
                return false;
            }
            if let Ok(sprite) = graph.try_get_mut_of_type::<Sprite>(shown.sprite) {
                // Popping up as they fade in, and sinking as they fade out.
                let faded = shown.faded;
                sprite.set_size(SIZE * throb * (0.5 + 0.5 * faded));
                sprite.set_color(Color::from_rgba(255, 255, 255, (faded * 255.0) as u8));
                let rise = (faded - 1.0) * 0.1;
                sprite
                    .local_transform_mut()
                    .set_position(shown.face + Vector3::new(0.0, ABOVE + bob + rise, 0.0));
            }
            true
        });
    }

    /// Takes every one of them away, as the droids are cleared away.
    pub fn clear(&mut self, graph: &mut Graph) {
        for shown in self.shown.drain(..) {
            if graph.is_valid_handle(shown.sprite) {
                graph.remove_node(shown.sprite);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drops_are_solid_in_the_middle_and_clear_round_the_edge() {
        let size = 32;
        let bytes = picture(size);
        let alpha = |x: usize, y: usize| bytes[(y * size + x) * 4 + 3];
        let (cx, cy) = ((DROPS[0].0 * size as f32) as usize, (DROPS[0].1 * size as f32) as usize);
        assert_eq!(alpha(cx, cy), 255, "the middle drop");
        for (x, y) in [(0, 0), (size - 1, 0), (0, size - 1), (size - 1, size - 1)] {
            assert_eq!(alpha(x, y), 0, "corner {x}, {y}");
        }
        // Blue, with a dark outline round it.
        let colour = |x: usize, y: usize| &bytes[(y * size + x) * 4..(y * size + x) * 4 + 3];
        assert!(colour(cx, cy + 2)[2] > 200);
    }
}
