//! The tile prefabs: what shape they are, and putting them together into a level.
//!
//! A tile is measured once, from an instance of its model: its squares of floor give its cells,
//! and a side of a cell with neither a wall nor another cell of the tile next to it is an opening.
//! That is all the maze planner needs, and it keeps the planner honest about the models as they
//! are rather than as they were meant to be - an axis flipped on export shows up here, not as a
//! corridor into a wall.

use crate::generate::{Dir, Piece, TileKind, TileSet, TileShape, DIRS};
use fyrox::{
    asset::manager::ResourceManager,
    core::{
        algebra::{Matrix4, Point3, UnitQuaternion, Vector3},
        math::aabb::AxisAlignedBoundingBox,
        pool::Handle,
    },
    graph::SceneGraph,
    material::{Material, MaterialResource},
    resource::model::{Model, ModelResource, ModelResourceExtension},
    scene::{
        base::BaseBuilder,
        mesh::{
            buffer::{VertexAttributeUsage, VertexReadTrait},
            surface::{SurfaceBuilder, SurfaceData, SurfaceResource},
            Mesh, MeshBuilder,
        },
        node::Node,
        pivot::PivotBuilder,
        transform::TransformBuilder,
        Scene,
    },
};

/// The tiles random mazes are made of.
const PIPE_TILE: &str = "data/maze_pipe.glb";
const CORNER_TILE: &str = "data/maze_l.glb";
const TEE_TILE: &str = "data/maze_t.glb";
const CROSS_TILE: &str = "data/maze_plus.glb";
/// How thick the wall closing a dead end is, in meters.
const WALL_THICKNESS: f32 = 0.1;

/// The four tile models.
#[derive(Debug, Clone, PartialEq)]
pub struct Prefabs {
    pub pipe: ModelResource,
    pub corner: ModelResource,
    pub tee: ModelResource,
    pub cross: ModelResource,
}

impl Prefabs {
    /// Starts loading the tile models.
    pub fn request(resources: &ResourceManager) -> Self {
        Self {
            pipe: resources.request::<Model>(PIPE_TILE),
            corner: resources.request::<Model>(CORNER_TILE),
            tee: resources.request::<Model>(TEE_TILE),
            cross: resources.request::<Model>(CROSS_TILE),
        }
    }

    fn model(&self, kind: TileKind) -> &ModelResource {
        match kind {
            TileKind::Pipe => &self.pipe,
            TileKind::Corner => &self.corner,
            TileKind::Tee => &self.tee,
            TileKind::Cross => &self.cross,
        }
    }

    /// Each model, with the file it comes from.
    pub fn all(&self) -> [(&'static str, &ModelResource); 4] {
        [
            (PIPE_TILE, &self.pipe),
            (CORNER_TILE, &self.corner),
            (TEE_TILE, &self.tee),
            (CROSS_TILE, &self.cross),
        ]
    }
}

/// What the tiles measured: their shapes, and what a wall of theirs is made of.
#[derive(Debug, Clone, PartialEq)]
pub struct Measured {
    pub shapes: TileSet,
    /// The width of a cell, in meters.
    pub cell: f32,
    /// The height of a wall, in meters.
    pub height: f32,
    /// The material of the tiles' walls, so a wall closing a dead end looks like the others.
    pub wall_material: Option<MaterialResource>,
}

struct Outline {
    /// Floor cells, in cells from the tile's origin.
    cells: Vec<(i32, i32)>,
    /// Cell sides that are neither walled nor shared with another cell of the tile.
    openings: Vec<((i32, i32), Dir)>,
    cell: f32,
    height: f32,
    wall_material: Option<MaterialResource>,
}

/// World-space bounds of a mesh's geometry, from its vertices: a freshly instantiated node's
/// cached bounds are not up to date yet.
fn mesh_bounds(node: &Node) -> Option<AxisAlignedBoundingBox> {
    let mesh = node.cast::<Mesh>()?;
    let transform = node.global_transform();
    let mut bounds: Option<AxisAlignedBoundingBox> = None;
    for surface in mesh.surfaces() {
        let data = surface.data();
        let data = data.data_ref();
        for vertex in data.vertex_buffer.iter() {
            let Ok(position) = vertex.read_3_f32(VertexAttributeUsage::Position) else {
                continue;
            };
            let point = transform.transform_point(&Point3::from(position)).coords;
            match bounds.as_mut() {
                Some(b) => b.add_point(point),
                None => bounds = Some(AxisAlignedBoundingBox::from_point(point)),
            }
        }
    }
    bounds
}

/// Measures a tile from a temporary instance of it.
fn outline(model: &ModelResource, scene: &mut Scene) -> Result<Outline, String> {
    let root = model.instantiate(scene);
    scene.graph.update_hierarchical_data();

    let mut floors: Vec<AxisAlignedBoundingBox> = Vec::new();
    let mut walls: Vec<(AxisAlignedBoundingBox, Handle<Node>)> = Vec::new();
    let mut height = 0.0f32;
    for handle in scene.graph.traverse_handle_iter(root) {
        let node = &scene.graph[handle];
        let Some(bounds) = mesh_bounds(node) else {
            continue;
        };
        let size = bounds.max - bounds.min;
        if crate::platform::var("MAZE_DEBUG").is_some() {
            fyrox::core::log::Log::info(format!(
                "Tile mesh {:?}: {:.2?} to {:.2?}",
                scene.graph[handle].name(),
                bounds.min,
                bounds.max
            ));
        }
        height = height.max(bounds.max.y);
        if size.y < 0.05 && bounds.max.y.abs() < 0.2 && size.x > 1.0 && size.z > 1.0 {
            floors.push(bounds);
        } else if size.y > 1.0 && (size.x < 0.05 || size.z < 0.05) {
            walls.push((bounds, handle));
        }
    }
    let wall_material = walls.first().and_then(|&(_, handle)| {
        scene.graph[handle]
            .cast::<Mesh>()
            .and_then(|mesh| mesh.surfaces().first().map(|s| s.material().clone()))
    });
    scene.graph.remove_node(root);

    let Some(first) = floors.first() else {
        return Err("it has no floor".into());
    };
    let cell = first.max.x - first.min.x;
    let at = |b: &AxisAlignedBoundingBox| {
        let c = b.center();
        ((c.x / cell).round() as i32, (c.z / cell).round() as i32)
    };
    let cells: Vec<(i32, i32)> = floors.iter().map(at).collect();

    let mut openings = Vec::new();
    for &(x, z) in &cells {
        for d in DIRS {
            if cells.contains(&(x + d.0, z + d.1)) {
                continue;
            }
            // A wall along this side: its middle is half a cell out from the cell's middle.
            let side = Vector3::new(
                (x as f32 + d.0 as f32 * 0.5) * cell,
                0.0,
                (z as f32 + d.1 as f32 * 0.5) * cell,
            );
            let walled = walls.iter().any(|(b, _)| {
                let c = b.center();
                (c.x - side.x).abs() < 0.3 && (c.z - side.z).abs() < 0.3
            });
            if !walled {
                openings.push(((x, z), d));
            }
        }
    }
    Ok(Outline {
        cells,
        openings,
        cell,
        height,
        wall_material,
    })
}

fn pipe_shape(outline: &Outline) -> Result<TileShape, String> {
    match (outline.cells.as_slice(), outline.openings.as_slice()) {
        ([(0, 0)], [(_, a), (_, b)]) if *a == (-b.0, -b.1) => Ok(TileShape::Pipe { axis: *a }),
        _ => Err(format!(
            "a straight piece should be one cell open at two opposite sides, but it has cells {:?} \
             open at {:?}",
            outline.cells, outline.openings
        )),
    }
}

fn junction_shape(outline: &Outline) -> Result<TileShape, String> {
    if !outline.cells.contains(&(0, 0)) {
        return Err("it has no cell at its origin".into());
    }
    let arms: Vec<Dir> = outline
        .cells
        .iter()
        .copied()
        .filter(|&c| c != (0, 0))
        .collect();
    // Each arm is one cell, open only at its far end, and the middle is closed elsewhere.
    let expected: Vec<((i32, i32), Dir)> = arms.iter().map(|&a| (a, a)).collect();
    let fits = arms.iter().all(|a| DIRS.contains(a))
        && outline.openings.len() == expected.len()
        && expected.iter().all(|o| outline.openings.contains(o));
    if fits {
        Ok(TileShape::Junction { arms })
    } else {
        Err(format!(
            "a junction should be a middle cell with one-cell arms open at their ends, but it has \
             cells {:?} open at {:?}",
            outline.cells, outline.openings
        ))
    }
}

/// Measures all four tiles.
pub fn measure(prefabs: &Prefabs, scene: &mut Scene) -> Result<Measured, String> {
    let named = |name: &str, r: Result<Outline, String>| r.map_err(|e| format!("{name} tile: {e}"));
    let pipe = named("straight", outline(&prefabs.pipe, scene))?;
    let corner = named("corner", outline(&prefabs.corner, scene))?;
    let tee = named("T", outline(&prefabs.tee, scene))?;
    let cross = named("crossroads", outline(&prefabs.cross, scene))?;
    let shapes = TileSet {
        pipe: pipe_shape(&pipe).map_err(|e| format!("straight tile: {e}"))?,
        corner: junction_shape(&corner).map_err(|e| format!("corner tile: {e}"))?,
        tee: junction_shape(&tee).map_err(|e| format!("T tile: {e}"))?,
        cross: junction_shape(&cross).map_err(|e| format!("crossroads tile: {e}"))?,
    };
    for (name, outline) in [("corner", &corner), ("T", &tee), ("crossroads", &cross)] {
        if (outline.cell - pipe.cell).abs() > 0.01 {
            return Err(format!(
                "the {name} tile's cells are {:.2} m wide, the straight tile's {:.2} m",
                outline.cell, pipe.cell
            ));
        }
    }
    Ok(Measured {
        shapes,
        cell: pipe.cell,
        height: pipe.height,
        wall_material: pipe.wall_material,
    })
}

/// Builds the level from its pieces, under one node. Returns that node, and each piece's node in
/// the order of `pieces`.
pub fn assemble(
    prefabs: &Prefabs,
    measured: &Measured,
    pieces: &[Piece],
    scene: &mut Scene,
) -> (Handle<Node>, Vec<Handle<Node>>) {
    let root = PivotBuilder::new(BaseBuilder::new().with_name("Level"))
        .build(&mut scene.graph)
        .to_base();
    let at = |cell: (i32, i32)| {
        Vector3::new(
            cell.0 as f32 * measured.cell,
            0.0,
            cell.1 as f32 * measured.cell,
        )
    };
    let mut nodes = Vec::with_capacity(pieces.len());
    for piece in pieces {
        let node = match *piece {
            Piece::Tile { kind, cell, turns } => {
                let node = prefabs.model(kind).instantiate(scene);
                let transform = scene.graph[node].local_transform_mut();
                transform.set_position(at(cell));
                transform.set_rotation(UnitQuaternion::from_axis_angle(
                    &Vector3::y_axis(),
                    (turns as f32 * 90.0).to_radians(),
                ));
                node
            }
            Piece::Wall { cell, dir } => {
                let middle = at(cell)
                    + Vector3::new(dir.0 as f32, 0.0, dir.1 as f32) * (measured.cell * 0.5)
                    + Vector3::new(0.0, measured.height * 0.5, 0.0);
                // Across the corridor: thin along `dir`, a cell wide the other way.
                let size = if dir.0 != 0 {
                    Vector3::new(WALL_THICKNESS, measured.height, measured.cell)
                } else {
                    Vector3::new(measured.cell, measured.height, WALL_THICKNESS)
                };
                let material = measured
                    .wall_material
                    .clone()
                    .unwrap_or_else(|| MaterialResource::new_embedded(Material::standard()));
                MeshBuilder::new(
                    BaseBuilder::new().with_name("DeadEnd").with_local_transform(
                        TransformBuilder::new().with_local_position(middle).build(),
                    ),
                )
                .with_surfaces(vec![SurfaceBuilder::new(SurfaceResource::new_embedded(
                    SurfaceData::make_cube(Matrix4::new_nonuniform_scaling(&size)),
                ))
                .with_material(material)
                .build()])
                .build(&mut scene.graph)
                .to_base()
            }
        };
        scene.graph.link_nodes(node, root);
        nodes.push(node);
    }
    (root, nodes)
}
