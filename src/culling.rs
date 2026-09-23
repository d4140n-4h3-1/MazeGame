//! Keeps a random maze's drawing and lighting to what the player can see.
//!
//! Looking down a corridor, the view reaches far past the walls that end it, and the renderer
//! would draw every tile and light every lamp out there: hundreds of each in a big maze, all
//! hidden behind the nearest wall. The maze's floor plan says exactly what can be seen from where
//! (see [`CellMap::visible_from`]), so everything else is hidden. Lamps are kept a little further
//! out, wherever their light can still reach a visible cell, so that a corner lit from around the
//! bend stays lit.

use crate::{
    fixtures::LAMP_RADIUS,
    generate::{self, CellMap, Piece, DIRS},
    tiles::Measured,
};
use fyrox::{
    core::{algebra::Vector3, pool::Handle},
    scene::{graph::Graph, node::Node},
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, PartialEq)]
pub struct Culling {
    map: CellMap,
    /// The width of a cell, in meters.
    cell: f32,
    /// Each piece of the maze, with the cells it covers.
    pieces: Vec<(Handle<Node>, Vec<(i32, i32)>)>,
    /// Each lamp, with the cell it hangs in.
    lamps: Vec<(Handle<Node>, (i32, i32))>,
    /// The cell the player was last seen in.
    at: Option<(i32, i32)>,
    /// What can be seen from the cell the player is in, and the cells next to it.
    seen: HashSet<(i32, i32)>,
    /// What can be seen from each cell the player has been in, so going back is free.
    seen_from: HashMap<(i32, i32), HashSet<(i32, i32)>>,
    /// Whether the lamps are switched on at all. Switched off, none of them is shown, wherever
    /// the player is.
    lamps_on: bool,
}

impl Culling {
    /// Culling for a maze made of `pieces`, whose nodes in the scene are `nodes`, in the same
    /// order.
    pub fn new(measured: &Measured, pieces: &[Piece], nodes: Vec<Handle<Node>>) -> Self {
        Self {
            map: CellMap::new(&measured.shapes, pieces),
            cell: measured.cell,
            pieces: nodes
                .into_iter()
                .zip(pieces)
                .map(|(node, piece)| {
                    let cells = generate::piece_cells(&measured.shapes, piece);
                    (node, cells.into_iter().map(|(cell, _)| cell).collect())
                })
                .collect(),
            lamps: Vec::new(),
            at: None,
            seen: HashSet::new(),
            seen_from: Default::default(),
            lamps_on: true,
        }
    }

    /// Adds the maze's lamps, which are placed after its pieces.
    pub fn add_lamps(&mut self, graph: &Graph, lamps: &[Handle<Node>]) {
        for &lamp in lamps {
            let position = **graph[lamp].local_transform().position();
            self.lamps.push((lamp, self.cell_of(position)));
        }
    }

    /// Switches the lamps on or off. What is shown is worked out afresh on the next update.
    pub fn set_lamps_on(&mut self, on: bool) {
        self.lamps_on = on;
        self.at = None;
    }

    /// Whether anything at `position` could be seen from where the player was at the last
    /// update. Everything could, before the first.
    pub fn can_see(&self, position: Vector3<f32>) -> bool {
        self.at.is_none() || self.seen.contains(&self.cell_of(position))
    }

    fn cell_of(&self, position: Vector3<f32>) -> (i32, i32) {
        (
            (position.x / self.cell).round() as i32,
            (position.z / self.cell).round() as i32,
        )
    }

    /// Shows what can be seen from where the player is. Only does anything when the player has
    /// moved to another cell, since what is visible depends on the cell rather than the spot.
    pub fn update(&mut self, graph: &mut Graph, player: Vector3<f32>) {
        let here = self.cell_of(player);
        if self.at == Some(here) {
            return;
        }
        self.at = Some(here);
        // The position read here is from before this frame's physics step, so by the time the
        // frame is drawn the player may have stepped into a neighbouring cell. What can be seen
        // from those is shown as well, or a distant wall newly in sight from there would appear
        // a frame late.
        let map = &self.map;
        let mut seen = HashSet::new();
        let reachable = DIRS
            .iter()
            .filter(|&&d| map.is_open(here, d))
            .map(|d| (here.0 + d.0, here.1 + d.1));
        for cell in std::iter::once(here).chain(reachable) {
            let from_cell = self
                .seen_from
                .entry(cell)
                .or_insert_with(|| map.visible_from(cell, f32::INFINITY));
            seen.extend(from_cell.iter().copied());
        }
        let lit = self
            .map
            .around(&seen, (LAMP_RADIUS / self.cell).ceil() as u32);
        for (node, cells) in &self.pieces {
            graph[*node].set_visibility(cells.iter().any(|c| seen.contains(c)));
        }
        for (lamp, cell) in &self.lamps {
            graph[*lamp].set_visibility(self.lamps_on && lit.contains(cell));
        }
        self.seen = seen;
    }
}
