//! The light fixtures: glass housings in the ceiling, and the lamps that light them.
//!
//! The maze models mark their glass with a pure magenta color. That glass becomes real,
//! refractive glass, and each housing made of it gets a lamp. Behind the glass the models put
//! something that glows by itself - the bulb - which goes dark with the lamps: see [`Glow`].

use fyrox::{
    core::{
        algebra::{Matrix4, Point3, Vector3},
        color::Color,
        log::Log,
        math::aabb::AxisAlignedBoundingBox,
        pool::Handle,
        sstorage::ImmutableString,
    },
    fxhash::FxHashMap,
    graph::SceneGraph,
    material::{Material, MaterialProperty, MaterialResource, MaterialResourceBinding},
    scene::{
        base::BaseBuilder,
        graph::Graph,
        light::{point::PointLightBuilder, BaseLightBuilder},
        mesh::{
            buffer::{VertexAttributeUsage, VertexReadTrait},
            surface::SurfaceResource,
            Mesh,
        },
        node::Node,
        transform::TransformBuilder,
    },
};
use fyrox_gfx::{replace_materials, GlassMaterial};

/// How far a lamp's light reaches, in meters.
pub const LAMP_RADIUS: f32 = 7.5;
/// How brightly the panes glow while their lamps are on.
const PANE_GLOW: f32 = 0.5;
/// The material property that says how brightly a surface glows by itself, in the engine's
/// standard shaders and in the glass alike.
pub(crate) const EMISSION_STRENGTH: &str = "emissionStrength";
/// The material property that says what colour a surface is, in the engine's standard shaders.
pub(crate) const DIFFUSE_COLOR: &str = "diffuseColor";

/// Everything in a level that glows by itself: the glass panes and the bulbs behind them. Each
/// is a material the level has to itself, with how brightly it glows while the lights are on.
///
/// A level's own copies, because the materials it was built with are the tile models': they are
/// shared with every other copy of a tile, in this level and the next, and dimming those would
/// dim the models themselves.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Glow(Vec<(MaterialResource, MaterialProperty)>);

impl Glow {
    /// Lets everything glow as it was made to, or puts it all out.
    pub fn set_lit(&self, lit: bool) {
        for (material, strength) in &self.0 {
            let value = if lit {
                strength.clone()
            } else {
                unlit(strength)
            };
            material.data_ref().set_property(EMISSION_STRENGTH, value);
        }
    }
}

/// Gives the level its own copy of every material under `root` that glows by itself, so that it
/// can be put out with the lights (see [`Glow`]). Surfaces that shared a material share its copy.
pub fn claim_glow(graph: &mut Graph, root: Handle<Node>) -> Glow {
    let handles: Vec<Handle<Node>> = graph.traverse_handle_iter(root).collect();
    // For each material seen, its copy, or nothing if it does not glow.
    let mut copies: FxHashMap<u64, Option<MaterialResource>> = Default::default();
    let mut glow = Glow::default();
    for handle in handles {
        let Some(mesh) = graph[handle].cast_mut::<Mesh>() else {
            continue;
        };
        for surface in mesh.surfaces_mut() {
            let original = surface.material().clone();
            let copy = copies.entry(original.key()).or_insert_with(|| {
                let state = original.state();
                let material = state.data_ref()?;
                let strength = glow_strength(material)?;
                let copy = MaterialResource::new_embedded(material.clone());
                glow.0.push((copy.clone(), strength));
                Some(copy)
            });
            if let Some(copy) = copy {
                surface.set_material(copy.clone());
            }
        }
    }
    glow
}

/// How brightly `material` glows by itself, if it does at all.
pub(crate) fn glow_strength(material: &Material) -> Option<MaterialProperty> {
    let strength = property(material, EMISSION_STRENGTH)?;
    let glows = match &strength {
        MaterialProperty::Float(strength) => *strength > 0.0,
        MaterialProperty::Vector3(strength) => strength.max() > 0.0,
        _ => false,
    };
    glows.then_some(strength)
}

/// The property of `material` called `name`, if it was given one.
pub(crate) fn property(material: &Material, name: &str) -> Option<MaterialProperty> {
    let key = ImmutableString::new("properties");
    let Some(MaterialResourceBinding::PropertyGroup(group)) = material.bindings().get(&key) else {
        return None;
    };
    group.property_ref(name).cloned()
}

/// A glow strength like `strength`, turned all the way down.
fn unlit(strength: &MaterialProperty) -> MaterialProperty {
    match strength {
        MaterialProperty::Vector3(_) => MaterialProperty::Vector3(Vector3::zeros()),
        _ => MaterialProperty::Float(0.0),
    }
}

/// Turns the magenta marker surfaces under `root` into glass, which glows with its lamp: it is
/// added to `glow`. Returns how many surfaces there were.
pub fn glaze(graph: &mut Graph, root: Handle<Node>, glow: &mut Glow) -> usize {
    let glass = GlassMaterial {
        tint: Color::opaque(170, 90, 255),
        tint_strength: 0.45,
        // The panes are lamp covers, so they glow with the lamp behind them.
        emission: Color::opaque(225, 200, 255),
        emission_strength: PANE_GLOW,
        // A faint ripple, as in cast glass; perfectly flat panes barely bend what is behind.
        waviness: 0.04,
        ..Default::default()
    }
    .build_resource();
    let panes = replace_materials(graph, root, is_glass_marker, &glass);
    glow.0.push((glass, MaterialProperty::Float(PANE_GLOW)));
    panes
}

/// Puts a lamp inside every glass housing of the model. Sunlight does not get into the tubes, so
/// these are what light the corridors.
pub fn place_lamps(
    graph: &mut Graph,
    fixtures: &[AxisAlignedBoundingBox],
    share_within: f32,
) -> Vec<Handle<Node>> {
    let positions = lamp_positions(fixtures, share_within);
    let mut lamps = Vec::new();
    for position in positions.iter().copied() {
        let lamp = PointLightBuilder::new(
            BaseLightBuilder::new(
                BaseBuilder::new().with_local_transform(
                    TransformBuilder::new()
                        .with_local_position(position)
                        .build(),
                ),
            )
            .with_color(Color::opaque(255, 235, 200))
            .with_intensity(1.0)
            // Volumetric scattering costs about a third of the frame here and shows almost
            // nothing: the corridors are short and the lamps are flush with the ceiling.
            .with_scatter_enabled(false),
        )
        // Enough to cover the stretch of corridor a lamp is shared across, without spilling far
        // through the walls into the corridors next door, which have lamps of their own.
        .with_radius(LAMP_RADIUS)
        .build(graph);
        lamps.push(lamp.to_base());
    }
    Log::info(format!(
        "Maze: {} lamps for {} fixtures",
        positions.len(),
        fixtures.len()
    ));
    lamps
}

/// Finds the light fixtures of a level built from tiles: every mesh with glass in it is one.
pub fn tile_fixtures(graph: &Graph, root: Handle<Node>) -> Vec<AxisAlignedBoundingBox> {
    let mut fixtures = Vec::new();
    for handle in graph.traverse_handle_iter(root) {
        let node = &graph[handle];
        let Some(mesh) = node.cast::<Mesh>() else {
            continue;
        };
        let transform = node.global_transform();
        let mut bounds: Option<AxisAlignedBoundingBox> = None;
        for surface in mesh.surfaces() {
            let is_glass = {
                let material = surface.material();
                let state = material.state();
                state.data_ref().is_some_and(is_glass_marker)
            };
            if is_glass {
                let b = surface_bounds(&surface.data(), &transform);
                match bounds.as_mut() {
                    Some(all) => {
                        all.add_point(b.min);
                        all.add_point(b.max);
                    }
                    None => bounds = Some(b),
                }
            }
        }
        fixtures.extend(bounds);
    }
    fixtures
}

/// Finds the model's light fixtures: the glass housings. Surfaces that touch each other belong to
/// the same housing.
pub fn glass_fixtures(graph: &Graph, root: Handle<Node>) -> Vec<AxisAlignedBoundingBox> {
    const SAME_FIXTURE: f32 = 1.5;

    let mut boxes: Vec<AxisAlignedBoundingBox> = Vec::new();
    for handle in graph.traverse_handle_iter(root) {
        let node = &graph[handle];
        let Some(mesh) = node.cast::<Mesh>() else {
            continue;
        };
        let transform = node.global_transform();
        for surface in mesh.surfaces() {
            let is_glass = {
                let material = surface.material();
                let state = material.state();
                state.data_ref().is_some_and(is_glass_marker)
            };
            if !is_glass {
                continue;
            }
            boxes.push(surface_bounds(&surface.data(), &transform));
        }
    }

    // Merge the parts of one housing: its panes sit within a meter or so of each other.
    let mut merged: Vec<AxisAlignedBoundingBox> = Vec::new();
    for bounds in boxes {
        let center = bounds.center();
        match merged
            .iter_mut()
            .find(|other| other.center().metric_distance(&center) < SAME_FIXTURE)
        {
            Some(other) => {
                other.add_point(bounds.min);
                other.add_point(bounds.max);
            }
            None => merged.push(bounds),
        }
    }

    merged
}

/// Where the lamps go: just below the panes, because inside a housing the light would be shut in
/// by it. Fixtures in the same stretch of corridor share one lamp - each pane still has light
/// right behind it, and a deferred renderer pays for every light on screen, so fewer of them for
/// the same look is worth it.
fn lamp_positions(fixtures: &[AxisAlignedBoundingBox], share_within: f32) -> Vec<Vector3<f32>> {
    let mut groups: Vec<Vec<Vector3<f32>>> = Vec::new();
    for fixture in fixtures {
        let center = fixture.center();
        let position = Vector3::new(center.x, fixture.min.y - 0.15, center.z);
        match groups.iter_mut().find(|group| {
            group
                .iter()
                .any(|other| other.metric_distance(&position) < share_within)
        }) {
            Some(group) => group.push(position),
            None => groups.push(vec![position]),
        }
    }
    groups
        .iter()
        .map(|group| group.iter().sum::<Vector3<f32>>() / group.len() as f32)
        .collect()
}

/// World-space bounds of a surface's geometry.
fn surface_bounds(data: &SurfaceResource, transform: &Matrix4<f32>) -> AxisAlignedBoundingBox {
    let data = data.data_ref();
    let mut bounds = AxisAlignedBoundingBox::default();
    for vertex in data.vertex_buffer.iter() {
        if let Ok(position) = vertex.read_3_f32(VertexAttributeUsage::Position) {
            bounds.add_point(transform.transform_point(&Point3::from(position)).coords);
        }
    }
    bounds
}

/// The glass of the maze model: surfaces colored pure magenta.
fn is_glass_marker(material: &Material) -> bool {
    matches!(
        property(material, DIFFUSE_COLOR),
        Some(MaterialProperty::Color(color)) if color.r == 255 && color.g == 0 && color.b == 255
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use fyrox::scene::mesh::{
        surface::{SurfaceBuilder, SurfaceData},
        MeshBuilder,
    };

    fn material(glow: Vector3<f32>) -> MaterialResource {
        let mut material = Material::standard();
        material.set_property(EMISSION_STRENGTH, glow);
        MaterialResource::new_embedded(material)
    }

    fn mesh(graph: &mut Graph, material: &MaterialResource) -> Handle<Node> {
        let cube = SurfaceResource::new_embedded(SurfaceData::make_cube(Matrix4::identity()));
        MeshBuilder::new(BaseBuilder::new())
            .with_surfaces(vec![SurfaceBuilder::new(cube)
                .with_material(material.clone())
                .build()])
            .build(graph)
            .to_base()
    }

    fn material_of(graph: &Graph, mesh: Handle<Node>) -> MaterialResource {
        graph[mesh].cast::<Mesh>().unwrap().surfaces()[0]
            .material()
            .clone()
    }

    fn strength(material: &MaterialResource) -> Option<MaterialProperty> {
        glow_strength(&material.data_ref())
    }

    #[test]
    fn putting_out_the_lights_puts_out_the_level_and_leaves_the_models_alone() {
        let mut graph = Graph::new();
        let bulb = material(Vector3::repeat(20.0));
        let wall = material(Vector3::zeros());
        let root = graph.get_root();
        let (a, b, c) = (
            mesh(&mut graph, &bulb),
            mesh(&mut graph, &bulb),
            mesh(&mut graph, &wall),
        );

        let glow = claim_glow(&mut graph, root);
        assert_eq!(
            glow.0.len(),
            1,
            "one copy of the bulb, and none of the wall"
        );
        let copy = material_of(&graph, a);
        assert_eq!(
            copy.key(),
            material_of(&graph, b).key(),
            "the copy is shared"
        );
        assert_ne!(copy.key(), bulb.key(), "and is a copy");
        assert_eq!(
            material_of(&graph, c).key(),
            wall.key(),
            "what does not glow is kept"
        );

        glow.set_lit(false);
        assert_eq!(strength(&copy), None, "out");
        assert!(strength(&bulb).is_some(), "the model's own still glows");
        glow.set_lit(true);
        assert_eq!(
            strength(&copy),
            Some(MaterialProperty::Vector3(Vector3::repeat(20.0)))
        );
    }
}
