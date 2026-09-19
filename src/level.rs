//! A level in the scene: the maze itself, the collider that follows its shape, its lamps, and
//! where in it the player can walk.
//!
//! A level is readied in two steps. It is put into the scene first; its colliders only exist from
//! the next physics step on, so surveying it for walkable ground and lighting it - which needs
//! that survey's rays - comes a few frames later, in [`Level::finish`].

use crate::{
    culling::Culling,
    fixtures::{self, Glow},
    generate::Maze,
    inward,
    layout::WalkGrid,
    survey,
    tiles::{self, Measured, Prefabs},
};
use fyrox::{
    core::{algebra::Vector3, log::Log, math::aabb::AxisAlignedBoundingBox, pool::Handle},
    fxhash::FxHashSet,
    graph::SceneGraph,
    resource::model::{ModelResource, ModelResourceExtension},
    scene::{
        base::BaseBuilder,
        collider::{Collider, ColliderBuilder, ColliderShape, GeometrySource},
        graph::Graph,
        mesh::Mesh,
        node::Node,
        rigidbody::{RigidBodyBuilder, RigidBodyType},
        Scene,
    },
};

/// FBX models are authored in centimeters.
const FBX_SCALE: f32 = 0.01;

#[derive(Debug, Default, PartialEq)]
pub struct Level {
    /// Everything the level added to the scene, to be removed for the next one.
    nodes: Vec<Handle<Node>>,
    /// The static collider that follows the level's shape.
    collider: Handle<Collider>,
    /// The light fixtures, waiting for their lamps until the level is finished.
    fixtures: Vec<AxisAlignedBoundingBox>,
    /// How close together fixtures can be and still share a lamp.
    lamp_sharing: f32,
    /// The lamps, once the level is finished.
    lamps: Vec<Handle<Node>>,
    /// What glows by itself - the fixtures' glass and the bulbs behind it - and goes dark with
    /// the lamps.
    glow: Glow,
    /// What the player is looking for, if the model puts something in the maze to find.
    pub goal: Option<Vector3<f32>>,
    /// Where the player can walk, and the world position of that grid's corner, once finished.
    pub grid: Option<(WalkGrid, Vector3<f32>)>,
    /// What of a random maze is drawn and lit: only what can be seen from where the player is.
    culling: Option<Culling>,
}

impl Level {
    /// Puts a random maze into the scene, built from tiles. `doubled` is the surface data already
    /// made double-sided, which tiles share between all their copies, in every level.
    pub fn from_tiles(
        prefabs: &Prefabs,
        measured: &Measured,
        maze: &Maze,
        scene: &mut Scene,
        doubled: &mut FxHashSet<u64>,
    ) -> Result<Self, String> {
        let pieces = measured.shapes.build(maze)?;
        let (root, nodes) = tiles::assemble(prefabs, measured, &pieces, scene);
        let culling = Culling::new(measured, &pieces, nodes);
        Log::info(format!(
            "Maze: {}x{} junctions, {} pieces",
            maze.width,
            maze.depth,
            pieces.len()
        ));
        let mut level = Self::prepare(root, scene, false, doubled);
        level.culling = Some(culling);
        Ok(level)
    }

    /// Puts a fixed maze model into the scene.
    pub fn from_model(
        model: &ModelResource,
        fbx: bool,
        scene: &mut Scene,
        doubled: &mut FxHashSet<u64>,
    ) -> Self {
        let root = model.instantiate(scene);
        if fbx {
            scene.graph[root]
                .local_transform_mut()
                .set_scale(Vector3::repeat(FBX_SCALE));
        }
        Self::prepare(root, scene, true, doubled)
    }

    /// Readies a level under `root` for play: its glass, its lamps' places, and a static
    /// collider that follows its shape. `fixed` is for a maze model rather than tiles.
    fn prepare(
        root: Handle<Node>,
        scene: &mut Scene,
        fixed: bool,
        doubled: &mut FxHashSet<u64>,
    ) -> Self {
        // World transforms must be up to date before anything is measured in world space.
        scene.graph.update_hierarchical_data();

        // Each glass housing in the ceiling holds a lamp, so the lights go where the glass is.
        let fixtures = if fixed {
            fixtures::glass_fixtures(&scene.graph, root)
        } else {
            fixtures::tile_fixtures(&scene.graph, root)
        };
        Log::info(format!("Maze: {} light fixtures", fixtures.len()));

        // A model can put something in its maze to find. Tiles have nothing of the kind, and
        // their small parts - the light fixtures - would only be mistaken for it.
        let goal = if fixed {
            landmark(&scene.graph, root)
        } else {
            None
        };
        match goal {
            Some(goal) => Log::info(format!(
                "Maze: the model puts something to find at {:.1} {:.1} {:.1}",
                goal.x, goal.y, goal.z
            )),
            None => Log::info("Maze: nothing to find in the level, using a marker of our own"),
        }

        // Before the glass is put in, which is the level's own already.
        let mut glow = fixtures::claim_glow(&mut scene.graph, root);
        let panes = fixtures::glaze(&mut scene.graph, root, &mut glow);
        Log::info(format!("Maze: {panes} glass surfaces"));
        inward::make_double_sided(&mut scene.graph, root, doubled);
        scene.graph.update_hierarchical_data();

        let meshes: Vec<GeometrySource> = scene
            .graph
            .traverse_handle_iter(root)
            .filter(|&h| scene.graph[h].is_mesh())
            .map(GeometrySource)
            .collect();
        Log::info(format!("Maze: {} meshes", meshes.len()));

        let collider = ColliderBuilder::new(BaseBuilder::new())
            .with_shape(ColliderShape::trimesh(meshes))
            .build(&mut scene.graph);
        let body = RigidBodyBuilder::new(BaseBuilder::new().with_child(collider))
            .with_body_type(RigidBodyType::Static)
            .build(&mut scene.graph);

        Self {
            nodes: vec![root, body.to_base()],
            collider,
            fixtures,
            // A maze model's fixtures can sit close together and share a lamp; each tile's
            // fixture gets its own, since tiles are spaced for one each.
            lamp_sharing: if fixed { 4.0 } else { 0.0 },
            lamps: Vec::new(),
            glow,
            goal,
            grid: None,
            culling: None,
        }
    }

    /// Finishes readying the level once its colliders exist: surveys it for walkable ground and
    /// hangs its lamps. `ignore` is a mesh in the scene that is not part of the maze.
    pub fn finish(&mut self, graph: &mut Graph, ignore: Handle<Node>) {
        self.grid = survey::survey(graph, self.collider, ignore);
        let fixtures = std::mem::take(&mut self.fixtures);
        let lamps = fixtures::place_lamps(graph, &fixtures, self.lamp_sharing);
        if let Some(culling) = self.culling.as_mut() {
            culling.add_lamps(graph, &lamps);
        }
        self.nodes.extend(lamps.iter().copied());
        self.lamps = lamps;
    }

    /// Switches the level's lamps on or off, and everything that glows around them with them.
    pub fn set_lights(&mut self, graph: &mut Graph, on: bool) {
        for &lamp in &self.lamps {
            graph[lamp].set_visibility(on);
        }
        // With culling, only the lamps near what the player can see are shown; it works out
        // which on its next update.
        if let Some(culling) = self.culling.as_mut() {
            culling.set_lamps_on(on);
        }
        self.glow.set_lit(on);
    }

    /// Draws and lights only what can be seen from where the player is, if the level is culled.
    pub fn cull(&mut self, graph: &mut Graph, player: Vector3<f32>) {
        if let Some(culling) = self.culling.as_mut() {
            culling.update(graph, player);
        }
    }

    /// Removes the level from the scene, so a new one can be built.
    pub fn clear(&mut self, scene: &mut Scene) {
        for handle in self.nodes.drain(..) {
            if scene.graph.is_valid_handle(handle) {
                scene.graph.remove_node(handle);
            }
        }
        *self = Self::default();
    }
}

/// What the model puts in the maze to find: anything that is not part of its structure. Corridor
/// pieces are several meters across, so smaller meshes standing on their own are props, and the
/// biggest group of them - the pill in the middle of this maze, say - is the landmark.
fn landmark(graph: &Graph, root: Handle<Node>) -> Option<Vector3<f32>> {
    /// A corridor piece is at least this wide; anything smaller is a prop.
    const STRUCTURE_SIZE: f32 = 3.5;
    /// Props closer together than this belong to the same landmark.
    const SAME_LANDMARK: f32 = 2.0;

    let mut groups: Vec<AxisAlignedBoundingBox> = Vec::new();
    for handle in graph.traverse_handle_iter(root) {
        let node = &graph[handle];
        if node.cast::<Mesh>().is_none() {
            continue;
        }
        let bounds = node.world_bounding_box();
        let size = bounds.max - bounds.min;
        if size.x >= STRUCTURE_SIZE || size.z >= STRUCTURE_SIZE {
            continue;
        }
        match groups
            .iter_mut()
            .find(|group| group.center().metric_distance(&bounds.center()) < SAME_LANDMARK)
        {
            Some(group) => {
                group.add_point(bounds.min);
                group.add_point(bounds.max);
            }
            None => groups.push(bounds),
        }
    }
    groups.into_iter().map(|group| group.center()).next()
}
