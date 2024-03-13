pub mod nodes;
pub(in crate::map) mod overall_values;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{fmt::Write, iter::Enumerate};

use bevy::prelude::{Transform, Vec2, Window};
use bevy_egui::egui;
use serde::{Deserialize, Deserializer, Serialize};
use shared::{continue_if_none, return_if_none, NextValue};

use self::{
    nodes::{Node, NodeWorld, NodesWorld, NodesWorldMut},
    overall_values::OverallMovement
};
use super::{
    drawer::MapPreviewDrawer,
    editor::state::manager::{Animators, Brushes},
    selectable_vector::select_vectors_in_range,
    thing::catalog::ThingsCatalog
};
use crate::{
    map::{
        containers::{hv_hash_map, hv_hash_set, hv_vec, HvHashMap, HvHashSet},
        drawer::{color::Color, EditDrawer},
        editor::state::grid::Grid,
        path::nodes::NodesInsertionIter,
        selectable_vector::deselect_vectors,
        AssertedInsertRemove,
        HvVec,
        OutOfBounds,
        TOOLTIP_OFFSET
    },
    utils::{
        hull::{EntityHull, Hull},
        identifiers::{EntityCenter, EntityId, Id},
        iterators::{FilterSet, PairIterator, SkipIndexIterator, TripletIterator},
        math::{
            lines_and_segments::line_point_product,
            AroundEqual,
            FastNormalize,
            HashVec2,
            NecessaryPrecisionValue
        },
        misc::{next, prev, NoneIfEmpty, PointInsideUiHighlight, TakeValue, Toggle, VX_HGL_SIDE},
        overall_value::OverallValueInterface,
        tooltips::{draw_tooltip_x_centered_above_pos, to_egui_coordinates}
    },
    INDEXES
};

//=======================================================================//
// MACROS
//
//=======================================================================//

macro_rules! index_func {
    ($tooltip_text:ident, $index:ident, $f:block) => {
        if !$tooltip_text.is_empty()
        {
            $tooltip_text.push_str(", ");
        }

        $f

        // If this panics I recommend you rethink the choices that led you to this issue.
        $tooltip_text.push_str(INDEXES[$index]);
    };
}

//=======================================================================//

macro_rules! draw_nodes {
    ($(($func:ident, $square:ident, $line:ident)),+) => { paste::paste! { $(
        #[inline]
        fn [< nodes_ $line >](
            &self,
            drawer: &mut EditDrawer,
            node_j: &Node,
            node_i: &Node,
            center: Vec2,
            color: Color
        )
        {
            let bucket_j = self.buckets.get(node_j.pos()).unwrap();
            let bucket_i = self.buckets.get(node_i.pos()).unwrap();
            let start = node_j.world_pos(center);
            let end = node_i.world_pos(center);

            if bucket_i.len() == 1 && bucket_j.len() == 1
            {
                drawer.$line(start, end, color);
                return;
            }

            if bucket_i.iter().any(|idx| bucket_j.contains(&next(*idx, self.len())))
            {
                Self::[< shifted_ $line >](drawer, start, end, color);
            }
            else
            {
                drawer.$line(start, end, color);
            }
        }

        #[inline]
        fn $func(&self, drawer: &mut EditDrawer, center: Vec2, color: Color)
        {
            for idx in 0..self.len()
            {
                let node = &self.nodes[idx];
                let pos = node.world_pos(center);

                if node.selectable_vector.selected
                {
                    drawer.$square(pos, Color::SelectedPathNode);
                }
                else
                {
                    drawer.$square(pos, color);
                }
            }

            self.[<$func _no_highlights>](drawer, center, color);
        }

        #[inline]
        fn [<$func _no_highlights>](&self, drawer: &mut EditDrawer, center: Vec2, color: Color)
        {
            if self.len() == 2
            {
                let start = self.nodes[0].world_pos(center);
                let end = self.nodes[1].world_pos(center);
                Self::[< shifted_ $line >](drawer, start, end, color);
                Self::[< shifted_ $line >](drawer, end, start, color);
                return;
            }

            for [node_j, node_i] in self.nodes.pair_iter().unwrap()
            {
                self.[< nodes_ $line >](drawer, node_j, node_i, center, color);
            }
        }
    )+}};
}

//=======================================================================//

macro_rules! movement_values {
    ($(($value:ident, $opposite:ident)),+) => { paste::paste! { $(
        #[inline]
        pub(in crate::map) fn [< set_selected_nodes_ $value >](
            &mut self,
            $value: f32
        ) -> Option<MovementValueEdit>
        {
            let mut edit = MovementValueEdit::new();

            for (i, node) in self
                .nodes
                .iter_mut()
                .enumerate()
                .filter(|(_, n)| n.selectable_vector.selected)
            {
                edit.insert(i, continue_if_none!(node.movement.[< set_ $value >]($value)));
            }

            edit.none_if_empty()
        }

        #[inline]
        pub(in crate::map) fn [< undo_ $value _edit>](&mut self, edit: &MovementValueEdit)
        {
            for (delta, indexes) in &edit.0
            {
                for i in indexes
                {
                    let node = &mut self.nodes[*i];
                    let cur_value = node.movement.$value();
                    let cur_opposite = node.movement.$opposite();

                    _ = node.movement.[< set_ $value >](cur_value - delta.0.x);
                    _ = node.movement.[< set_ $opposite >](cur_opposite - delta.0.y);
                }
            }
        }

        #[inline]
        pub(in crate::map) fn [< redo_ $value _edit>](&mut self, edit: &MovementValueEdit)
        {
            for (delta, indexes) in &edit.0
            {
                for i in indexes
                {
                    let node = &mut self.nodes[*i];
                    let cur_value = node.movement.$value();
                    let cur_opposite = node.movement.$opposite();

                    _ = node.movement.[< set_ $value >](cur_value + delta.0.x);
                    _ = node.movement.[< set_ $opposite >](cur_opposite + delta.0.y);
                }
            }
        }
    )+}};
}

//=======================================================================//

macro_rules! common_edit_path {
    ($(($value:ident, $t:ty)),+) => { paste::paste! { $(
        #[inline]
        fn [< set_selected_path_nodes_ $value >](&mut self, value: f32) -> Option<$t>
        {
            self.path_mut_set_dirty().[< set_selected_nodes_ $value >](value)
        }

        #[inline]
        fn [< undo_path_nodes_ $value _edit >](&mut self, edit: &$t)
        {
            self.path_mut_set_dirty().[< undo_ $value _edit >](edit)
        }

        #[inline]
        fn [< redo_path_nodes_ $value _edit >](&mut self, edit: &$t)
        {
            self.path_mut_set_dirty().[< redo_ $value _edit >](edit)
        }
    )+}};

    () => {
        #[inline]
        fn toggle_path_node_at_index(&mut self, idx: usize) -> bool
        {
            self.path_mut_set_dirty().toggle_node_at_index(idx)
        }

        #[inline]
        fn exclusively_select_path_node_at_index(&mut self, index: usize) -> crate::map::path::NodeSelectionResult
        {
            let center = self.center();
            self.path_mut_set_dirty()
                .exclusively_select_path_node_at_index(center, index)
        }

        #[inline]
        #[must_use]
        fn deselect_path_nodes(&mut self) -> Option<HvVec<u8>>
        {
            let center = self.center();
            self.path_mut_set_dirty().deselect_nodes(center)
        }

        #[inline]
        fn deselect_path_nodes_no_indexes(&mut self)
        {
            let center = self.center();
            self.path_mut_set_dirty().deselect_nodes_no_indexes(center);
        }

        #[inline]
        #[must_use]
        fn select_path_nodes_in_range(&mut self, range: &Hull) -> Option<HvVec<u8>>
        {
            let center = self.center();
            self.path_mut_set_dirty().select_nodes_in_range(center, range)
        }

        #[inline]
        fn select_all_path_nodes(&mut self) -> Option<HvVec<u8>>
        {
            self.path_mut_set_dirty().select_all_nodes()
        }

        #[inline]
        #[must_use]
        fn exclusively_select_path_nodes_in_range(&mut self, range: &Hull) -> Option<HvVec<u8>>
        {
            let center = self.center();
            self.path_mut_set_dirty()
                .exclusively_select_nodes_in_range(center, range)
        }

        #[inline]
        fn try_insert_path_node_at_index(&mut self, cursor_pos: Vec2, index: usize) -> bool
        {
            let center = self.center();
            self.path_mut().try_insert_node_at_index(cursor_pos, index, center)
        }

        #[inline]
        fn insert_path_node_at_index(&mut self, pos: Vec2, idx: usize)
        {
            let center = self.center();
            self.path_mut().insert_node_at_index(pos, idx, center);
        }

        #[inline]
        fn insert_path_nodes_at_indexes(&mut self, nodes: &HvVec<(Vec2, u8)>)
        {
            self.path_mut_set_dirty().insert_nodes_at_indexes(nodes);
        }

        #[inline]
        fn undo_path_nodes_move(&mut self, nodes_move: &crate::map::path::NodesMove)
        {
            self.path_mut().undo_nodes_move(nodes_move);
        }

        #[inline]
        fn redo_path_nodes_move(&mut self, nodes_move: &crate::map::path::NodesMove)
        {
            self.path_mut().apply_selected_nodes_move(nodes_move);
        }

        #[inline]
        fn move_path_nodes_at_indexes(&mut self, snap: &HvVec<(HvVec<u8>, Vec2)>)
        {
            self.path_mut().move_nodes_at_indexes(snap);
        }

        #[inline]
        fn remove_selected_path_nodes(&mut self, payload: NodesDeletionPayload) -> HvVec<(Vec2, u8)>
        {
            assert!(
                self.id() == payload.id(),
                "NodesDeletionPayload ID is not equal to the Brush's ID."
            );
            let payload = payload.payload();

            self.path_mut_set_dirty()
                .delete_selected_nodes(payload.iter().rev().map(|(_, idx)| *idx as usize));
            payload
        }

        #[inline]
        fn remove_path_node_at_index(&mut self, idx: usize)
        {
            self.path_mut_set_dirty().remove_nodes_at_indexes(Some(idx).into_iter());
        }

        #[inline]
        fn redo_selected_path_nodes_deletion(&mut self)
        {
            self.path_mut_set_dirty().redo_selected_nodes_deletion();
        }

        #[inline]
        #[must_use]
        fn snap_selected_path_nodes(
            &mut self,
            grid: crate::map::editor::state::grid::Grid
        ) -> Option<HvVec<(HvVec<u8>, Vec2)>>
        {
            let center = self.center();
            self.path_mut().snap_selected_nodes(grid, center)
        }

        common_edit_path!(
            (standby_time, crate::map::path::StandbyValueEdit),
            (max_speed, crate::map::path::MovementValueEdit),
            (min_speed, crate::map::path::MovementValueEdit),
            (accel_travel_percentage, crate::map::path::MovementValueEdit),
            (decel_travel_percentage, crate::map::path::MovementValueEdit)
        );
    };
}

pub(in crate::map) use common_edit_path;

//=======================================================================//
// TRAITS
//
//=======================================================================//

pub(in crate::map) trait Moving: EntityId + EntityCenter
{
    #[must_use]
    fn path(&self) -> Option<&Path>;

    #[must_use]
    fn has_path(&self) -> bool;

    #[must_use]
    fn possible_moving(&self) -> bool;

    #[inline]
    fn path_hull(&self) -> Option<Hull>
    {
        if !self.has_path()
        {
            return None;
        }

        calc_path_hull(self.path().unwrap(), self.center()).into()
    }

    #[inline]
    fn path_hull_out_of_bounds(&self, center: Vec2) -> bool
    {
        if !self.has_path()
        {
            return false;
        }

        calc_path_hull(self.path().unwrap(), center).out_of_bounds()
    }

    #[inline]
    fn overall_selected_path_nodes_movement(&self) -> OverallMovement
    {
        self.path().unwrap().overall_selected_nodes_movement()
    }

    #[inline]
    fn check_selected_path_nodes_move(&self, delta: Vec2) -> IdNodesMoveResult
    {
        (self.path().unwrap().check_selected_nodes_move(delta), self.id()).into()
    }

    #[inline]
    fn path_nodes_nearby_cursor_pos(&self, cursor_pos: Vec2, camera_scale: f32) -> NearbyNodes
    {
        self.path()
            .unwrap()
            .nearby_nodes(cursor_pos, self.center(), camera_scale)
    }

    #[inline]
    fn check_selected_nodes_deletion(&self) -> IdNodesDeletionResult
    {
        (self.path().unwrap().check_selected_nodes_deletion(), self.id()).into()
    }

    #[inline]
    fn movement_simulator(&self) -> MovementSimulator
    {
        self.path().unwrap().movement_simulator(self.id())
    }

    #[inline]
    fn draw_path(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        drawer: &mut EditDrawer,
        show_tooltips: bool
    )
    {
        self.path().unwrap().draw(
            window,
            camera,
            egui_context,
            drawer,
            self.center(),
            show_tooltips
        );
    }

    #[inline]
    fn draw_semitransparent_path(&self, drawer: &mut EditDrawer)
    {
        self.path().unwrap().draw_semitransparent(drawer, self.center());
    }

    fn draw_highlighted_with_path_nodes(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        brushes: Brushes,
        catalog: &ThingsCatalog,
        drawer: &mut EditDrawer,
        show_tooltips: bool
    );

    fn draw_with_highlighted_path_node(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        brushes: Brushes,
        catalog: &ThingsCatalog,
        drawer: &mut EditDrawer,
        highlighted_node: usize,
        show_tooltips: bool
    );

    fn draw_with_path_node_addition(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        brushes: Brushes,
        catalog: &ThingsCatalog,
        drawer: &mut EditDrawer,
        pos: Vec2,
        idx: usize,
        show_tooltips: bool
    );

    fn draw_movement_simulation(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        brushes: Brushes,
        catalog: &ThingsCatalog,
        drawer: &mut EditDrawer,
        show_tooltips: bool,
        simulator: &MovementSimulator
    );

    fn draw_map_preview_movement_simulation(
        &self,
        camera: &Transform,
        brushes: Brushes,
        catalog: &ThingsCatalog,
        drawer: &mut MapPreviewDrawer,
        animators: &Animators,
        simulator: &MovementSimulator
    );
}

//=======================================================================//

pub(in crate::map) trait EditPath: EntityId + Moving
{
    fn set_path(&mut self, path: Path);

    fn toggle_path_node_at_index(&mut self, idx: usize) -> bool;

    fn exclusively_select_path_node_at_index(&mut self, index: usize) -> NodeSelectionResult;

    #[must_use]
    fn deselect_path_nodes(&mut self) -> Option<HvVec<u8>>;

    fn deselect_path_nodes_no_indexes(&mut self);

    #[must_use]
    fn select_path_nodes_in_range(&mut self, range: &Hull) -> Option<HvVec<u8>>;

    #[must_use]
    fn select_all_path_nodes(&mut self) -> Option<HvVec<u8>>;

    #[must_use]
    fn exclusively_select_path_nodes_in_range(&mut self, range: &Hull) -> Option<HvVec<u8>>;

    #[must_use]
    fn try_insert_path_node_at_index(&mut self, cursor_pos: Vec2, index: usize) -> bool;

    fn insert_path_node_at_index(&mut self, pos: Vec2, idx: usize);

    fn insert_path_nodes_at_indexes(&mut self, nodes: &HvVec<(Vec2, u8)>);

    #[inline]
    fn apply_selected_path_nodes_move(&mut self, payload: NodesMovePayload) -> NodesMove
    {
        assert!(payload.0 == self.id(), "NodesMovePayload's ID is not equal to the Entity's ID.");
        self.redo_path_nodes_move(&payload.1);
        payload.1
    }

    fn undo_path_nodes_move(&mut self, nodes_move: &NodesMove);

    fn redo_path_nodes_move(&mut self, nodes_move: &NodesMove);

    fn move_path_nodes_at_indexes(&mut self, snap: &HvVec<(HvVec<u8>, Vec2)>);

    fn remove_selected_path_nodes(&mut self, payload: NodesDeletionPayload) -> HvVec<(Vec2, u8)>;

    fn remove_path_node_at_index(&mut self, idx: usize);

    fn redo_selected_path_nodes_deletion(&mut self);

    #[must_use]
    fn snap_selected_path_nodes(&mut self, grid: Grid) -> Option<HvVec<(HvVec<u8>, Vec2)>>;

    fn set_selected_path_nodes_standby_time(&mut self, value: f32) -> Option<StandbyValueEdit>;

    fn undo_path_nodes_standby_time_edit(&mut self, edit: &StandbyValueEdit);

    fn redo_path_nodes_standby_time_edit(&mut self, edit: &StandbyValueEdit);

    fn set_selected_path_nodes_max_speed(&mut self, value: f32) -> Option<MovementValueEdit>;

    fn undo_path_nodes_max_speed_edit(&mut self, edit: &MovementValueEdit);

    fn redo_path_nodes_max_speed_edit(&mut self, edit: &MovementValueEdit);

    fn set_selected_path_nodes_min_speed(&mut self, value: f32) -> Option<MovementValueEdit>;

    fn undo_path_nodes_min_speed_edit(&mut self, edit: &MovementValueEdit);

    fn redo_path_nodes_min_speed_edit(&mut self, edit: &MovementValueEdit);

    fn set_selected_path_nodes_accel_travel_percentage(
        &mut self,
        value: f32
    ) -> Option<MovementValueEdit>;

    fn undo_path_nodes_accel_travel_percentage_edit(&mut self, edit: &MovementValueEdit);

    fn redo_path_nodes_accel_travel_percentage_edit(&mut self, edit: &MovementValueEdit);

    fn set_selected_path_nodes_decel_travel_percentage(
        &mut self,
        value: f32
    ) -> Option<MovementValueEdit>;

    fn undo_path_nodes_decel_travel_percentage_edit(&mut self, edit: &MovementValueEdit);

    fn redo_path_nodes_decel_travel_percentage_edit(&mut self, edit: &MovementValueEdit);

    fn take_path(&mut self) -> Path;
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The result of the insertion of a new [`Node`] in a [`Path`] during a path draw.
#[must_use]
pub(in crate::map) enum FreeDrawNodeDeletionResult
{
    /// Could not be inserted.
    None,
    /// A fully formed [`Path`] was generated.
    Path(Vec2, u8),
    /// Just a point so far.
    Point(Vec2, Vec2, u8)
}

//=======================================================================//

/// The result of the move of a new [`Node`] in a [`Path`].
#[must_use]
pub(in crate::map) enum NodesMoveResult
{
    /// No nodes were moved.
    None,
    /// Moving the Nodes generate an invalid [`Path`].
    Invalid,
    /// Nodes can be moved.
    Valid(NodesMove)
}

//=======================================================================//

#[must_use]
pub(in crate::map) enum IdNodesMoveResult
{
    None,
    Invalid,
    Valid(NodesMovePayload)
}

impl From<(NodesMoveResult, Id)> for IdNodesMoveResult
{
    #[inline]
    fn from(value: (NodesMoveResult, Id)) -> Self
    {
        match value.0
        {
            NodesMoveResult::None => Self::None,
            NodesMoveResult::Invalid => Self::Invalid,
            NodesMoveResult::Valid(m) => Self::Valid(NodesMovePayload(value.1, m))
        }
    }
}

#[must_use]
pub(in crate::map) struct NodesMovePayload(Id, NodesMove);

impl EntityId for NodesMovePayload
{
    #[inline]
    fn id(&self) -> Id { self.0 }

    #[inline]
    fn id_as_ref(&self) -> &Id { &self.0 }
}

//=======================================================================//

/// The data concerning the move of the selected path [`Node`]s.
#[must_use]
#[derive(Debug)]
pub(in crate::map) struct NodesMove
{
    /// The indexes of the moved nodes.
    moved:  HvVec<u8>,
    /// The indexes of the nodes that were merged.
    merged: HvVec<(Vec2, u8)>,
    /// The distance the nodes were moved.
    delta:  Vec2
}

impl NodesMove
{
    /// Whever any vertexes were merged.
    #[inline]
    #[must_use]
    pub fn has_merged_vertexes(&self) -> bool { !self.merged.is_empty() }

    /// Combine two [`NodesMove`].
    /// Returns false if they cannot be combined.
    #[inline]
    #[must_use]
    pub fn merge(&mut self, other: &Self) -> bool
    {
        if other.has_merged_vertexes() || self.has_merged_vertexes()
        {
            return false;
        }

        self.delta += other.delta;
        true
    }
}

//=======================================================================//

/// The result of the [`Node`]s selection process.
#[must_use]
#[derive(Debug)]
pub(in crate::map) enum NodeSelectionResult
{
    /// The node beneath the cursor was already selected.
    Selected,
    /// The node beneath the cursor was not selected, it was exclusively selected, and the vertexes
    /// at indexes were deselected.
    NotSelected(HvVec<u8>)
}

//=======================================================================//

/// The result of the [`Node`]s deletion process.
#[must_use]
pub(in crate::map) enum NodesDeletionResult
{
    /// No nodes deleted.
    None,
    /// Deleting the nodes creates an invalid [`Path`].
    Invalid,
    /// The deletion is valid, contains the positions and indexes of the deleted nodes.
    Valid(HvVec<(Vec2, u8)>)
}

//=======================================================================//

#[must_use]
#[derive(Debug)]
pub(in crate::map) enum IdNodesDeletionResult
{
    None,
    Invalid,
    Valid(NodesDeletionPayload)
}

impl From<(NodesDeletionResult, Id)> for IdNodesDeletionResult
{
    #[inline]
    fn from(value: (NodesDeletionResult, Id)) -> Self
    {
        match value.0
        {
            NodesDeletionResult::None => Self::None,
            NodesDeletionResult::Invalid => Self::Invalid,
            NodesDeletionResult::Valid(nodes) => Self::Valid(NodesDeletionPayload(value.1, nodes))
        }
    }
}

#[must_use]
#[derive(Debug)]
pub(in crate::map) struct NodesDeletionPayload(Id, HvVec<(Vec2, u8)>);

impl EntityId for NodesDeletionPayload
{
    #[inline]
    fn id(&self) -> Id { self.0 }

    #[inline]
    fn id_as_ref(&self) -> &Id { &self.0 }
}

impl NodesDeletionPayload
{
    #[inline]
    pub fn payload(self) -> HvVec<(Vec2, u8)> { self.1 }
}

//=======================================================================//

#[must_use]
enum XcelerationPhase
{
    Ongoing(f32),
    Reupdate(f32),
    Passed
}

//=======================================================================//
// TYPES
//
//=======================================================================//

#[derive(PartialEq, Debug)]
struct HashF32(f32);

impl Eq for HashF32 {}

impl std::hash::Hash for HashF32
{
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) { self.0.to_bits().hash(state); }
}

//=======================================================================//

#[must_use]
#[derive(Debug)]
pub(in crate::map) struct StandbyValueEdit(HvHashMap<HashF32, HvHashSet<usize>>);

impl NoneIfEmpty for StandbyValueEdit
{
    #[inline]
    fn none_if_empty(self) -> Option<Self>
    where
        Self: Sized
    {
        (!self.0.is_empty()).then_some(self)
    }
}

impl StandbyValueEdit
{
    #[inline]
    fn new() -> Self { Self(hv_hash_map![]) }

    #[inline]
    fn insert(&mut self, index: usize, value: f32)
    {
        let value = HashF32(value);

        match self.0.get_mut(&value)
        {
            Some(vec) => vec.asserted_insert(index),
            None => self.0.asserted_insert((value, hv_hash_set![index]))
        };
    }
}

//=======================================================================//

#[must_use]
#[derive(Debug)]
pub(in crate::map) struct MovementValueEdit(HvHashMap<HashVec2, HvHashSet<usize>>);

impl NoneIfEmpty for MovementValueEdit
{
    #[inline]
    fn none_if_empty(self) -> Option<Self>
    where
        Self: Sized
    {
        (!self.0.is_empty()).then_some(self)
    }
}

impl MovementValueEdit
{
    #[inline]
    fn new() -> Self { Self(hv_hash_map![]) }

    #[inline]
    fn insert(&mut self, index: usize, value: Vec2)
    {
        let value = HashVec2(value);

        match self.0.get_mut(&value)
        {
            Some(vec) => vec.asserted_insert(index),
            None => self.0.asserted_insert((value, hv_hash_set![index]))
        };
    }
}

//=======================================================================//

#[must_use]
#[derive(Clone, Copy, Debug)]
struct AccelerationInfo
{
    acceleration: f32,
    end:          [Vec2; 2]
}

//=======================================================================//

#[must_use]
#[derive(Clone, Copy, Debug)]
struct DecelerationInfo
{
    deceleration: f32,
    start:        [Vec2; 2],
    end:          [Vec2; 2]
}

//=======================================================================//

#[must_use]
pub(in crate::map) struct NearbyNodes<'a>
{
    nodes:        Enumerate<std::slice::Iter<'a, Node>>,
    cursor_pos:   Vec2,
    center:       Vec2,
    camera_scale: f32
}

impl<'a> Iterator for NearbyNodes<'a>
{
    type Item = (u8, bool);

    #[inline]
    #[must_use]
    fn next(&mut self) -> Option<Self::Item>
    {
        for (i, node) in self.nodes.by_ref()
        {
            if !node
                .world_pos(self.center)
                .is_point_inside_ui_highlight(self.cursor_pos, self.camera_scale)
            {
                continue;
            }

            return (u8::try_from(i).unwrap(), node.selectable_vector.selected).into();
        }

        None
    }
}

//=======================================================================//

/// A struct that allows the path tool to simulate the movement of a [`Brush`] that owns a
/// [`Path`].
#[must_use]
#[derive(Clone, Copy, Debug)]
pub(in crate::map) struct MovementSimulator
{
    /// The [`Id`] of the Brush.
    id:              Id,
    /// The start position (first [`Node`]).
    start:           Vec2,
    /// The current position.
    pos:             Vec2,
    /// The direction the Brush must move to reach the next move.
    dir:             Vec2,
    /// The index of the Node to reach.
    target_idx:      usize,
    /// The Node the Brush is currently traveling from.
    current_node:    Node,
    /// The Node the Brush is currently traveling to.
    target_node:     Node,
    /// The distance that separates the Nodes the Brush is traveling to-from.
    travel_distance: f32,
    /// The time that has to pass before the Brush can start moving from the current start Node.
    standby:         f32,
    /// The current move speed.
    current_speed:   f32,
    /// The acceleration values.
    acceleration:    Option<AccelerationInfo>,
    /// The deceleration values.
    deceleration:    Option<DecelerationInfo>
}

impl EntityId for MovementSimulator
{
    #[inline]
    fn id(&self) -> Id { self.id }

    #[inline]
    fn id_as_ref(&self) -> &Id { &self.id }
}

impl MovementSimulator
{
    // Velocity
    // v = v0 + a * t

    // Distance
    // x = x0 + v0 * t + 0.5 * a * t^2

    // Acceleration from velocities and travel distance.
    // v^2 = v0^2 + 2 * a * (x - x0)
    // a = 0.5 * (v^2 - v0^2) / (x - x0)

    #[inline]
    #[must_use]
    fn distance_accel_decel(
        current_node: &Node,
        target_node: &Node
    ) -> (Vec2, f32, Option<AccelerationInfo>, Option<DecelerationInfo>)
    {
        #[inline]
        #[must_use]
        fn xceleration(end_speed: f32, start_speed: f32, distance: f32) -> f32
        {
            0.5 * ((end_speed - start_speed) / distance)
        }

        let start = current_node.pos();
        let end = target_node.pos();
        let distance = end - start;
        let dir = distance.normalize();
        let perp = dir.perp();
        let length = distance.length();

        let max_squared = current_node.movement.max_speed() * current_node.movement.max_speed();
        let min_squared = current_node.movement.min_speed() * current_node.movement.min_speed();
        let accel_percentage = current_node.movement.scaled_accel_travel_percentage();
        let decel_percentage = current_node.movement.scaled_decel_travel_percentage();

        (
            distance.normalize(),
            length,
            (accel_percentage != 0f32).then(|| {
                let acceleration = xceleration(max_squared, min_squared, length * accel_percentage);
                let end = start + distance * accel_percentage;

                AccelerationInfo {
                    acceleration,
                    end: [end, end + perp]
                }
            }),
            (decel_percentage != 0f32).then(|| {
                let end = target_node.pos();
                let percent = current_node.movement.scaled_decel_travel_percentage();
                let distance = distance * percent;
                let decel_start = end - distance;

                DecelerationInfo {
                    deceleration: xceleration(min_squared, max_squared, length * percent),
                    start:        [decel_start, decel_start + perp],
                    end:          [end, end + perp]
                }
            })
        )
    }

    /// Creates a new [`MovementSimulator`].
    #[inline]
    fn new(path: &Path, id: Id) -> Self
    {
        let nodes = path.nodes();
        let current_node = nodes[0];
        let target_node = nodes[1];

        let (dir, travel_distance, acceleration, deceleration) =
            Self::distance_accel_decel(&current_node, &target_node);

        Self {
            id,
            start: current_node.pos(),
            pos: current_node.pos(),
            dir,
            target_idx: 1,
            current_node,
            target_node,
            travel_distance,
            standby: 0f32,
            current_speed: current_node.movement.start_speed(),
            acceleration,
            deceleration
        }
    }

    /// Returns the distance between the position of the first [`Node`] and the current position.
    pub(in crate::map) fn movement_vec(&self) -> Vec2 { self.pos - self.start }

    #[inline]
    fn xceleration_leftover_time(&self, end: Vec2, xceleration: f32, delta_time: f32) -> f32
    {
        let distance = end - self.pos;
        let delta = (self.current_speed * self.current_speed +
            2f32 * xceleration * distance.length())
        .sqrt();

        delta_time - ((delta - self.current_speed) / xceleration)
    }

    #[inline]
    fn acceleration_phase(&mut self, info: &AccelerationInfo, delta_time: f32) -> XcelerationPhase
    {
        if line_point_product(&info.end, self.pos) <= 0f32
        {
            return XcelerationPhase::Passed;
        }

        let final_speed = self.current_speed + info.acceleration * delta_time;
        let max_speed = self.current_node.movement.max_speed();

        match final_speed.total_cmp(&max_speed)
        {
            std::cmp::Ordering::Less =>
            {
                let average = (final_speed + self.current_speed) / 2f32;
                self.current_speed = final_speed;
                return XcelerationPhase::Ongoing(average);
            },
            std::cmp::Ordering::Equal =>
            {
                self.pos = info.end[0];
                return XcelerationPhase::Passed;
            },
            std::cmp::Ordering::Greater => ()
        };

        // End acceleration.
        let delta_time = self.xceleration_leftover_time(info.end[0], info.acceleration, delta_time);
        self.pos = info.end[0];
        self.current_speed = max_speed;
        XcelerationPhase::Reupdate(delta_time)
    }

    #[inline]
    fn deceleration_phase(&mut self, info: &DecelerationInfo, delta_time: f32) -> XcelerationPhase
    {
        if line_point_product(&info.end, self.pos) <= 0f32
        {
            return XcelerationPhase::Passed;
        }

        let final_speed = self.current_speed + info.deceleration * delta_time;
        let min_speed = self.current_node.movement.min_speed();

        match final_speed.total_cmp(&min_speed)
        {
            std::cmp::Ordering::Less => (),
            std::cmp::Ordering::Equal =>
            {
                self.pos = info.end[0];
                self.current_speed = min_speed;
                return XcelerationPhase::Passed;
            },
            std::cmp::Ordering::Greater =>
            {
                let average = (final_speed + self.current_speed) / 2f32;
                self.current_speed = final_speed;
                return XcelerationPhase::Ongoing(average);
            }
        };

        let delta_time = self.xceleration_leftover_time(info.end[0], info.deceleration, delta_time);
        self.pos = info.end[0];
        self.current_speed = min_speed;
        XcelerationPhase::Reupdate(delta_time)
    }

    #[inline]
    #[must_use]
    fn residual_delta_time(&self, speed: f32, end: Vec2, delta_time: f32) -> Option<f32>
    {
        let velocity = speed * delta_time * self.dir;
        let leftover_distance = (end - self.pos).length_squared();

        (velocity.length_squared() >= leftover_distance)
            .then(|| delta_time - leftover_distance.sqrt() / speed)
    }

    /// Updates the movement simulation.
    #[inline]
    pub fn update<T: Moving + ?Sized>(&mut self, moving: &T, mut delta_time: f32)
    {
        macro_rules! post_acceleration {
            ($info:ident) => {
                if line_point_product(&$info.start, self.pos) > 0f32
                {
                    if let Some(delta_time) =
                        self.residual_delta_time(self.current_speed, $info.start[0], delta_time)
                    {
                        self.pos = $info.start[0];
                        self.update(moving, delta_time);
                        return;
                    }

                    self.current_speed.into()
                }
                else
                {
                    match self.deceleration_phase(&$info, delta_time)
                    {
                        XcelerationPhase::Ongoing(average) => average.into(),
                        XcelerationPhase::Passed => None,
                        XcelerationPhase::Reupdate(delta_time) =>
                        {
                            self.update(moving, delta_time);
                            return;
                        }
                    }
                }
            };
        }
        // Consume standby time and keep going if delta time exceeds what is left
        if self.standby > 0f32
        {
            self.standby -= delta_time;

            if self.standby >= 0f32
            {
                return;
            }

            delta_time = std::mem::replace(&mut self.standby, 0f32).abs();
        }

        // Univorm movement.
        let average_speed = match (self.acceleration, self.deceleration)
        {
            (None, None) => self.current_speed.into(),
            (None, Some(decel_info)) => post_acceleration!(decel_info),
            (Some(info), None) =>
            {
                match self.acceleration_phase(&info, delta_time)
                {
                    XcelerationPhase::Ongoing(average) => average.into(),
                    XcelerationPhase::Passed => self.current_speed.into(),
                    XcelerationPhase::Reupdate(delta_time) =>
                    {
                        self.update(moving, delta_time);
                        return;
                    }
                }
            },
            (Some(accel_info), Some(decel_info)) =>
            {
                match self.acceleration_phase(&accel_info, delta_time)
                {
                    XcelerationPhase::Ongoing(average) => average.into(),
                    XcelerationPhase::Passed => post_acceleration!(decel_info),
                    XcelerationPhase::Reupdate(delta_time) =>
                    {
                        self.update(moving, delta_time);
                        return;
                    }
                }
            }
        };

        let delta_time = match average_speed
        {
            Some(average_speed) =>
            {
                match self.residual_delta_time(average_speed, self.target_node.pos(), delta_time)
                {
                    Some(delta_time) => delta_time,
                    None =>
                    {
                        self.pos += self.dir * delta_time * average_speed;
                        return;
                    }
                }
            },
            None => 0f32
        };

        // Fill in the leftover distance.
        self.pos = self.target_node.pos();

        // Set the standby.
        self.standby = self.target_node.movement.standby_time();

        // Set travel parameters toward the next node.
        let nodes = moving.path().unwrap().nodes();
        self.target_idx = next(self.target_idx, nodes.len());
        self.current_node = std::mem::replace(&mut self.target_node, nodes[self.target_idx]);
        self.current_speed = self.current_node.movement.start_speed();

        (self.dir, self.travel_distance, self.acceleration, self.deceleration) =
            Self::distance_accel_decel(&self.current_node, &self.target_node);

        // If we have leftover delta_time call recursion.
        if !delta_time.around_equal_narrow(&0f32)
        {
            self.update(moving, delta_time);
        }
    }
}

//=======================================================================//

#[must_use]
#[derive(Debug, Clone)]
struct Buckets(HvHashMap<HashVec2, HvVec<usize>>);

impl TakeValue for Buckets
{
    #[inline]
    fn take_value(&mut self) -> Self { Self(self.0.take_value()) }
}

impl Buckets
{
    #[inline]
    pub fn new() -> Self { Self(hv_hash_map![]) }

    #[inline]
    #[must_use]
    pub fn get(&self, pos: Vec2) -> Option<&HvVec<usize>> { self.0.get(&HashVec2(pos)) }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (&HashVec2, &HvVec<usize>)> { self.0.iter() }

    #[inline]
    pub fn bucket_with_index(&self, index: usize) -> HashVec2
    {
        self.0
            .iter()
            .find_map(|(k, v)| v.contains(&index).then_some(*k))
            .unwrap()
    }

    #[inline]
    pub fn insert(&mut self, index: usize, pos: Vec2)
    {
        let key = HashVec2(pos);

        for bucket in self.0.values_mut()
        {
            for idx in bucket.iter_mut().filter(|idx| **idx >= index)
            {
                *idx += 1;
            }
        }

        match self.0.get_mut(&key)
        {
            Some(idxs) =>
            {
                assert!(!idxs.contains(&index), "Bucket already contains index {index}");
                idxs.push(index);
                idxs.sort_unstable();
            },
            None => _ = self.0.insert(key, hv_vec![index])
        };
    }

    #[inline]
    pub fn remove(&mut self, index: usize, pos: Vec2)
    {
        let key = &HashVec2(pos);
        let bucket = self.0.get_mut(key).unwrap();
        let position = bucket.iter().position(|idx| *idx == index).unwrap();

        if bucket.len() == 1
        {
            self.0.remove(key);
        }
        else
        {
            self.0.get_mut(key).unwrap().remove(position);
        }

        for bucket in self.0.values_mut()
        {
            for idx in bucket.iter_mut().filter(|idx| **idx > index)
            {
                *idx -= 1;
            }
        }
    }

    #[inline]
    fn move_index(&mut self, index: usize, pos: Vec2, new_pos: Vec2)
    {
        self.remove(index, pos);
        self.insert(index, new_pos);
    }
}

//=======================================================================//

/// A path describing how a [`Brush`] moves in space over time.
#[must_use]
#[derive(Debug, Clone)]
pub struct Path
{
    nodes:   HvVec<Node>,
    hull:    Hull,
    buckets: Buckets
}

impl Serialize for Path
{
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer
    {
        self.nodes.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Path
{
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Path, D::Error>
    where
        D: Deserializer<'de>
    {
        HvVec::<Node>::deserialize(deserializer).map(|nodes| {
            let hull = Path::nodes_hull(&nodes);
            let mut buckets = Buckets::new();

            for (i, node) in nodes.iter().enumerate()
            {
                buckets.insert(i, node.pos());
            }

            Self {
                nodes,
                hull,
                buckets
            }
        })
    }
}

impl EntityHull for Path
{
    #[inline]
    fn hull(&self) -> Hull { self.hull }
}

impl PartialEq for Path
{
    #[inline]
    #[must_use]
    fn eq(&self, other: &Self) -> bool
    {
        self.len() == other.len() &&
            self.nodes
                .iter()
                .zip(&other.nodes)
                .all(|(a, b)| a.selectable_vector == b.selectable_vector)
    }
}

impl From<HvVec<Node>> for Path
{
    #[inline]
    fn from(value: HvVec<Node>) -> Self
    {
        let hull = Self::nodes_hull(&value);
        let mut buckets = Buckets::new();

        for (i, node) in value.iter().enumerate()
        {
            buckets.insert(i, node.pos());
        }

        let path = Self {
            nodes: value,
            hull,
            buckets
        };

        assert!(path.valid(), "From<HvVec<Node>> generated an invalid Path.");

        path
    }
}

impl Path
{
    draw_nodes!(
        (draw_nodes, square_highlight, arrowed_line),
        (
            draw_semitransparent_nodes,
            semitransparent_square_highlight,
            semitransparent_arrowed_line
        )
    );

    movement_values!(
        (max_speed, min_speed),
        (min_speed, max_speed),
        (accel_travel_percentage, decel_travel_percentage),
        (decel_travel_percentage, accel_travel_percentage)
    );

    //==============================================================
    // New

    /// Creates a new [`Path`] from two points and the position of the [`Brush`] center.
    #[inline]
    pub(in crate::map) fn new(points: &[Vec2; 2], center: Vec2) -> Self
    {
        assert!(
            !points[0].around_equal_narrow(&points[1]),
            "Points used to generate new Path are equal {}",
            points[0]
        );

        let node_0 = Node::from_world_pos(points[0], false, center);
        let node_1 = Node::from_world_pos(points[1], false, center);
        let hull = Hull::from_points([node_0.pos(), node_1.pos()].into_iter());
        let mut buckets = Buckets::new();
        buckets.insert(0, node_0.pos());
        buckets.insert(1, node_1.pos());

        Self {
            nodes: hv_vec![node_0, node_1],
            hull: hull.unwrap(),
            buckets
        }
    }

    //==============================================================
    // Info

    /// Returns the vector containing the [`Node`]s of the path.
    #[inline]
    pub const fn nodes(&self) -> &HvVec<Node> { &self.nodes }

    /// Returns an instance of [`NodesWorld`] representing the [`Node`]s in world coordinates.
    #[inline]
    fn nodes_world(&self, center: Vec2) -> NodesWorld { NodesWorld::new(self.nodes(), center) }

    /// Returns an instance of [`NodesWorldMut`] representing the [`Node`]s in world coordinates.
    #[inline]
    fn nodes_world_mut(&mut self, center: Vec2) -> NodesWorldMut
    {
        NodesWorldMut::new(&mut self.nodes, center)
    }

    #[inline]
    #[must_use]
    fn nodes_valid(&self) -> bool
    {
        self.nodes()
            .pair_iter()
            .unwrap()
            .all(|[a, b]| !a.pos().around_equal_narrow(&b.pos()))
    }

    /// Whever the path is valid.
    #[inline]
    #[must_use]
    fn valid(&self) -> bool
    {
        self.hull.around_equal(&Self::nodes_hull(self.nodes())) &&
            self.nodes().pair_iter().unwrap().enumerate().all(|([_, i], [a, b])| {
                !a.pos().around_equal_narrow(&b.pos()) &&
                    return_if_none!(self.buckets.get(b.pos()), false).contains(&i)
            })
    }

    /// Returns a [`MovementSimulator`] that allows to show how the [`Brush`] moves along the path
    /// [`Node`]s.
    #[inline]
    pub(in crate::map) fn movement_simulator(&self, id: Id) -> MovementSimulator
    {
        MovementSimulator::new(self, id)
    }

    /// Returns the amount of nodes.
    #[inline]
    #[must_use]
    pub(in crate::map) fn len(&self) -> usize { self.nodes.len() }

    /// Returns the indexes of the selected [`Node`]s.
    #[inline]
    pub(in crate::map) fn selected_nodes(&self) -> Option<HvVec<u8>>
    {
        hv_vec![collect; self
            .nodes()
            .iter()
            .enumerate()
            .filter_map(|(i, node)| {
                node.selectable_vector.selected.then_some(u8::try_from(i).unwrap())
            })
        ]
        .none_if_empty()
    }

    #[inline]
    #[must_use]
    pub(in crate::map) fn node_at_index_pos(&self, index: usize) -> Vec2
    {
        self.nodes[index].selectable_vector.vec
    }

    //==============================================================
    // Update

    #[inline]
    fn nodes_hull(nodes: &HvVec<Node>) -> Hull
    {
        Hull::from_points(nodes.iter().map(Node::pos)).unwrap()
    }

    /// Updates the value of the cached [`Hull`].
    #[inline]
    fn update_hull(&mut self) { self.hull = Self::nodes_hull(self.nodes()); }

    /// Snaps the selected [`Node`]s to the Grid.
    /// Returns a vector of the indexes and positions of the nodes that were snapped, if it was
    /// possible to do so without creating an invalid [`Path`].
    #[inline]
    #[must_use]
    pub(in crate::map) fn snap_selected_nodes(
        &mut self,
        grid: Grid,
        center: Vec2
    ) -> Option<HvVec<(HvVec<u8>, Vec2)>>
    {
        let mut moved_nodes: HvVec<(HvVec<u8>, Vec2)> = hv_vec![];

        'outer: for (i, node) in self
            .nodes
            .iter_mut()
            .enumerate()
            .filter(|(_, node)| node.selectable_vector.selected)
        {
            let node_world = node.world_pos(center);
            let delta = continue_if_none!(grid.snap_point(node_world)) - node_world;
            *node += delta;

            let idx = u8::try_from(i).unwrap();

            for (idxs, d) in &mut moved_nodes
            {
                if d.around_equal_narrow(&delta)
                {
                    idxs.push(idx);
                    continue 'outer;
                }
            }

            moved_nodes.push((hv_vec![idx], delta));
        }

        if moved_nodes.is_empty()
        {
            return None;
        }

        if !self.nodes_valid()
        {
            for (idxs, delta) in moved_nodes
            {
                for idx in idxs
                {
                    self.nodes[idx as usize] -= delta;
                }
            }

            return None;
        }

        for (idxs, delta) in &moved_nodes
        {
            for idx in idxs
            {
                let idx = *idx as usize;
                let new_pos = self.nodes[idx].selectable_vector.vec;
                self.buckets.move_index(idx, new_pos - *delta, new_pos);
            }
        }

        self.update_hull();
        Some(moved_nodes)
    }

    #[inline]
    pub(in crate::map) fn nearby_nodes(
        &self,
        cursor_pos: Vec2,
        center: Vec2,
        camera_scale: f32
    ) -> NearbyNodes
    {
        NearbyNodes {
            nodes: self.nodes().iter().enumerate(),
            cursor_pos,
            center,
            camera_scale
        }
    }

    //==============================================================
    // Free draw

    #[inline]
    fn insert(&mut self, index: usize, node: Node)
    {
        self.nodes.insert(index, node);
        self.buckets.insert(index, node.pos());
    }

    #[inline]
    fn remove(&mut self, index: usize)
    {
        let pos = self.nodes.remove(index).pos();
        self.buckets.remove(index, pos);
    }

    /// Inserts a [`Node`] created in free draw mode.
    /// # Panic
    /// Panics if the inserted node creates an invalid path.
    #[inline]
    pub(in crate::map) fn insert_free_draw_node_at_index(
        &mut self,
        p: Vec2,
        index: usize,
        center: Vec2
    )
    {
        self.insert(index, Node::from_world_pos(p, false, center));
        self.update_hull();
        assert!(self.valid(), "insert_free_draw_node_at_index generated an invalid Path.");
    }

    /// Tries deleting a [`Node`] created in free draw mode.
    #[inline]
    pub(in crate::map) fn try_delete_free_draw_node(
        &mut self,
        pos: Vec2,
        center: Vec2,
        camera_scale: f32
    ) -> FreeDrawNodeDeletionResult
    {
        let pos = pos - center;
        let i = *return_if_none!(
            self.buckets.iter().find_map(|(p, idxs)| {
                p.0.is_point_inside_ui_highlight(pos, camera_scale).then_some(idxs)
            }),
            FreeDrawNodeDeletionResult::None
        )
        .last()
        .unwrap();

        if self.len() == 2
        {
            return FreeDrawNodeDeletionResult::Point(
                self.nodes[next(i, 2)].world_pos(center),
                self.nodes[i].world_pos(center),
                u8::try_from(i).unwrap()
            );
        }

        let pos = self.nodes[i].world_pos(center);
        self.remove(i);
        self.update_hull();
        FreeDrawNodeDeletionResult::Path(pos, u8::try_from(i).unwrap())
    }

    /// Deletes the [`Node`] at `index` created in free draw mode.
    /// # Panic
    /// Panics if `index` is not within bounds.
    #[inline]
    #[must_use]
    pub(in crate::map) fn delete_free_draw_node_at_index(
        &mut self,
        index: usize,
        center: Vec2
    ) -> Option<Vec2>
    {
        let len = self.len();

        assert!(index < len, "Free draw node deletion index is higher than the nodes' length.");

        if len == 2
        {
            return Some(self.nodes[next(index, len)].world_pos(center));
        }

        self.remove(index);
        self.update_hull();
        None
    }

    //==============================================================
    // Insert / Remove

    #[inline]
    #[must_use]
    fn is_node_at_index_valid(&self, pos: Vec2, index: usize, center: Vec2) -> bool
    {
        let index = index % self.len();

        !pos.around_equal(&self.nodes[prev(index, self.len())].world_pos(center)) &&
            !pos.around_equal(&self.nodes[index].world_pos(center))
    }

    /// Tries to insert a [`Node`] with position `pos` at `idx`.
    #[inline]
    pub(in crate::map) fn try_insert_node_at_index(
        &mut self,
        pos: Vec2,
        index: usize,
        center: Vec2
    ) -> bool
    {
        if !self.is_node_at_index_valid(pos, index, center)
        {
            return false;
        }

        self.insert(index, Node::from_world_pos(pos, false, center));
        self.update_hull();
        assert!(self.valid(), "try_insert_node_at_index generated an invalid Path.");
        true
    }

    /// Inserts a [`Node`] with position `pos` at index `idx`.
    /// # Panic
    /// Panics if inserting the node creates an invalid path.
    #[inline]
    pub(in crate::map) fn insert_node_at_index(&mut self, pos: Vec2, idx: usize, center: Vec2)
    {
        assert!(
            self.try_insert_node_at_index(pos, idx, center),
            "insert_node_at_index generated an invalid Path."
        );
    }

    /// Inserts many [`Node`]s, with certain positions, at certain indexes, and possibly selected.
    /// # Panic
    /// Panics if the resulting path is invalid.
    #[inline]
    pub(in crate::map) fn insert_nodes_at_indexes(&mut self, nodes: &HvVec<(Vec2, u8)>)
    {
        for (vec, idx) in nodes
        {
            self.insert(*idx as usize, Node::new(*vec, true));
        }

        self.update_hull();
        assert!(self.valid(), "insert_nodes_at_indexes generated an invalid Path.");
    }

    /// Deletes the [`Node`]s at indexes `idxs`.
    /// # Panic
    /// Panics if the resulting path is invalid.
    #[inline]
    pub(in crate::map) fn remove_nodes_at_indexes(&mut self, idxs: impl Iterator<Item = usize>)
    {
        for idx in idxs
        {
            self.remove(idx);
        }

        self.update_hull();
        assert!(self.valid(), "remove_nodes_at_indexes generated an invalid Path.");
    }

    /// Checks whever deleting the selected [`Node`]s would create a valid path.
    #[inline]
    pub(in crate::map) fn check_selected_nodes_deletion(&self) -> NodesDeletionResult
    {
        let nodes = self.nodes();

        if nodes.len() == 2
        {
            if nodes.iter().fold(0, |i, a| {
                if a.selectable_vector.selected
                {
                    return i + 1;
                }

                i
            }) != 0
            {
                return NodesDeletionResult::Invalid;
            }

            return NodesDeletionResult::None;
        }

        let mut to_be_deleted_nodes = hv_vec![];

        for ([_, j, _], [v_i, v_j, v_k]) in nodes
            .triplet_iter()
            .unwrap()
            .enumerate()
            .filter(|(_, [_, v, _])| v.selectable_vector.selected)
        {
            if !v_i.selectable_vector.selected &&
                !v_k.selectable_vector.selected &&
                v_i.pos().around_equal(&v_k.pos())
            {
                return NodesDeletionResult::Invalid;
            }

            to_be_deleted_nodes.push((v_j.pos(), u8::try_from(j).unwrap()));
        }

        if to_be_deleted_nodes.is_empty()
        {
            return NodesDeletionResult::None;
        }

        if nodes.len() - to_be_deleted_nodes.len() < 2
        {
            return NodesDeletionResult::Invalid;
        }

        to_be_deleted_nodes.sort_by(|a, b| a.1.cmp(&b.1));
        NodesDeletionResult::Valid(to_be_deleted_nodes)
    }

    /// Deletes the selected [`Node`]s.
    /// # Panic
    /// Panics if the resulting path is invalid.
    #[inline]
    pub(in crate::map) fn delete_selected_nodes(
        &mut self,
        deletion_indexes: impl Iterator<Item = usize>
    )
    {
        for idx in deletion_indexes
        {
            self.remove(idx);
        }

        self.update_hull();
        assert!(self.valid(), "delete_selected_nodes generated an invalid Path.");
    }

    #[inline]
    pub(in crate::map) fn redo_selected_nodes_deletion(&mut self)
    {
        let mut i = 0;

        while i < self.len()
        {
            if self.nodes[i].selectable_vector.selected
            {
                self.remove(i);
                continue;
            }

            i += 1;
        }

        self.update_hull();
        assert!(self.valid(), "delete_nodes generated an invalid Path.");
    }

    //==============================================================
    // Selection

    /// Deselects the selected [`Node`]s.
    /// Returns the indexes of the deselected nodes, if any.
    #[inline]
    #[must_use]
    pub(in crate::map) fn deselect_nodes(&mut self, center: Vec2) -> Option<HvVec<u8>>
    {
        deselect_vectors!(self.nodes_world_mut(center))
    }

    /// Deselects the selected [`Node`]s, but does not return the indexes of the nodes that were
    /// deselected.
    #[inline]
    pub(in crate::map) fn deselect_nodes_no_indexes(&mut self, center: Vec2)
    {
        for node in self.nodes_world_mut(center).iter_mut()
        {
            *node.1 = false;
        }
    }

    /// Toggles the selection status of the [`Node`] at index `idx`.
    /// # Panic
    /// Panics if `idx` is out of bounds.
    #[inline]
    pub(in crate::map) fn toggle_node_at_index(&mut self, idx: usize) -> bool
    {
        let svec = &mut self.nodes[idx].selectable_vector;
        let selected = svec.selected;
        svec.toggle();
        selected
    }

    /// Checks whever there is a [`Node`] nearby `cursor_pos` and selects it if not already
    /// selected.
    #[inline]
    pub(in crate::map) fn exclusively_select_path_node_at_index(
        &mut self,
        center: Vec2,
        index: usize
    ) -> NodeSelectionResult
    {
        if self.nodes_world(center).nth(index).1
        {
            return NodeSelectionResult::Selected;
        }

        self.nodes[index].selectable_vector.toggle();
        let mut idxs = hv_vec![u8::try_from(index).unwrap()];

        for (i, node) in self
            .nodes_world_mut(center)
            .iter_mut()
            .enumerate()
            .skip_index(index)
            .unwrap()
            .filter(|(_, node)| *node.1)
        {
            idxs.push(u8::try_from(i).unwrap());
            *node.1 = false;
        }

        NodeSelectionResult::NotSelected(idxs)
    }

    /// Selects all non selected [`Node`]s within range.
    #[inline]
    #[must_use]
    pub(in crate::map) fn select_nodes_in_range(
        &mut self,
        center: Vec2,
        range: &Hull
    ) -> Option<HvVec<u8>>
    {
        select_vectors_in_range!(self.nodes_world_mut(center), range)
    }

    /// Selects all [`Node`]s within range, and deselects those that aren't.
    #[inline]
    #[must_use]
    pub(in crate::map) fn exclusively_select_nodes_in_range(
        &mut self,
        center: Vec2,
        range: &Hull
    ) -> Option<HvVec<u8>>
    {
        hv_vec![collect; self.nodes_world_mut(center)
            .iter_mut()
            .enumerate()
            .filter_map(|(i, node)| {
                let selected = std::mem::replace(
                    node.1,
                    range.contains_point(node.0)
                );

                (*node.1 != selected).then(|| u8::try_from(i).unwrap())
            })
        ]
        .none_if_empty()
    }

    #[inline]
    #[must_use]
    pub(in crate::map) fn select_all_nodes(&mut self) -> Option<HvVec<u8>>
    {
        hv_vec![collect; self.nodes
            .iter_mut()
            .enumerate()
            .filter_map(|(i, node)| {
                if node.selectable_vector.selected
                {
                    return None;
                }

                node.selectable_vector.selected = true;
                Some(u8::try_from(i).unwrap())
            })
        ]
        .none_if_empty()
    }

    //==============================================================
    // Move

    #[inline]
    pub(in crate::map) fn translate(&mut self, delta: Vec2)
    {
        for i in 0..self.len()
        {
            self.move_node(i, delta);
        }

        self.update_hull();
        assert!(self.valid(), "translate generated an invalid Path.");
    }

    /// Moves the [`Node`]s at indexes `idxs` by `delta`.
    /// # Panic
    /// Panics if the resulting path is invalid, or if any of the indexes is out of bounds.
    #[inline]
    pub(in crate::map) fn move_nodes_at_indexes(&mut self, snap: &HvVec<(HvVec<u8>, Vec2)>)
    {
        for (idxs, delta) in snap
        {
            for idx in idxs
            {
                self.move_node(*idx as usize, *delta);
            }
        }

        self.update_hull();
        assert!(self.valid(), "move_nodes_at_indexes generated an invalid Path.");
    }

    /// Checks whever moving the selected [`Node`]s by `delta` generates a valid path.
    #[inline]
    pub(in crate::map) fn check_selected_nodes_move(&self, delta: Vec2) -> NodesMoveResult
    {
        let moved = return_if_none!(self.selected_nodes(), NodesMoveResult::None);

        if moved
            .iter()
            .any(|idx| (self.nodes[*idx as usize].pos() + delta).out_of_bounds())
        {
            return NodesMoveResult::Invalid;
        }

        let len = self.len();
        let mut merged = hv_vec![];

        for i in moved.iter().map(|idx| *idx as usize)
        {
            let mut sv_i = self.nodes[i].selectable_vector;
            sv_i += delta;

            for j in [next(i, len), prev(i, len)]
            {
                let sv_j = self.nodes[j].selectable_vector;

                if sv_j.selected || !sv_j.vec.around_equal(&sv_i.vec)
                {
                    continue;
                }

                merged.push((sv_j.vec, u8::try_from(j).unwrap()));
                break;
            }
        }

        if len - merged.len() < 2
        {
            return NodesMoveResult::Invalid;
        }

        NodesMoveResult::Valid(NodesMove {
            moved,
            merged,
            delta
        })
    }

    #[inline]
    fn move_node(&mut self, index: usize, delta: Vec2)
    {
        let node = &mut self.nodes[index];
        let prev = node.selectable_vector.vec;
        *node += delta;
        self.buckets.move_index(index, prev, node.selectable_vector.vec);
    }

    /// Moves the path [`Node`]s according to the data contained in `nodes_move`.
    /// # Panic
    /// Panics if the resulting path is invalid, of `nodes_move` is incompatible with the current
    /// state of the path.
    #[inline]
    pub(in crate::map) fn apply_selected_nodes_move(&mut self, nodes_move: &NodesMove)
    {
        for idx in nodes_move.moved.iter().map(|idx| usize::from(*idx))
        {
            self.move_node(idx, nodes_move.delta);
        }

        for idx in nodes_move.merged.iter().rev().map(|(_, idx)| usize::from(*idx))
        {
            self.remove(idx);
        }

        self.update_hull();
        assert!(self.valid(), "apply_selected_nodes_move generated an invalid Path.");
    }

    /// Reverts the [`Node`]s move described in `nodes_move`.
    /// # Panic
    /// Panics if the resulting path is invalid, of `nodes_move` is incompatible with the current
    /// state of the path.
    #[inline]
    pub(in crate::map) fn undo_nodes_move(&mut self, nodes_move: &NodesMove)
    {
        for (vec, idx) in &nodes_move.merged
        {
            self.insert(usize::from(*idx), Node::new(*vec, false));
        }

        for idx in nodes_move.moved.iter().map(|idx| usize::from(*idx))
        {
            self.move_node(idx, -nodes_move.delta);
        }

        self.update_hull();
        assert!(self.valid(), "undo_nodes_move generated an invalid Path.");
    }

    #[inline]
    pub(in crate::map) fn set_selected_nodes_standby_time(
        &mut self,
        value: f32
    ) -> Option<StandbyValueEdit>
    {
        let mut edit = StandbyValueEdit::new();

        for (i, node) in self
            .nodes
            .iter_mut()
            .enumerate()
            .filter(|(_, n)| n.selectable_vector.selected)
        {
            edit.insert(i, continue_if_none!(node.movement.set_standby_time(value)));
        }

        edit.none_if_empty()
    }

    #[inline]
    pub(in crate::map) fn undo_standby_time_edit(&mut self, edit: &StandbyValueEdit)
    {
        for (delta, indexes) in &edit.0
        {
            for i in indexes
            {
                let node = &mut self.nodes[*i];
                let cur = node.movement.standby_time();
                _ = node.movement.set_standby_time(cur - delta.0);
            }
        }
    }

    #[inline]
    pub(in crate::map) fn redo_standby_time_edit(&mut self, edit: &StandbyValueEdit)
    {
        for (delta, indexes) in &edit.0
        {
            for i in indexes
            {
                let node = &mut self.nodes[*i];
                let cur = node.movement.standby_time();
                _ = node.movement.set_standby_time(cur + delta.0);
            }
        }
    }

    //==============================================================
    // Movement

    /// Returns [`OverallMovement`] and [`OverallStandby`] describing the movement status of the
    /// [`Node`]s.
    #[inline]
    pub(in crate::map) fn overall_selected_nodes_movement(&self) -> OverallMovement
    {
        let nodes = self.nodes().iter().filter(|n| n.selectable_vector.selected);
        let mut overall = OverallMovement::new();

        for node in nodes
        {
            if overall.stack(&node.movement)
            {
                break;
            }
        }

        overall
    }

    //==============================================================
    // Draw

    #[inline]
    fn shifted_arrowed_line_points(drawer: &EditDrawer, start: Vec2, end: Vec2) -> (Vec2, Vec2)
    {
        const ARROW_SHIFT: f32 = VX_HGL_SIDE / 2f32 * 3f32;

        let dir = (end - start).fast_normalize() * ARROW_SHIFT * drawer.camera_scale();
        let perp = dir.perp();
        (start + dir + perp, end - dir + perp)
    }

    #[inline]
    fn shifted_arrowed_line(drawer: &mut EditDrawer, start: Vec2, end: Vec2, color: Color)
    {
        let (start, end) = Self::shifted_arrowed_line_points(drawer, start, end);
        drawer.arrowed_line(start, end, color);
    }

    #[inline]
    fn shifted_semitransparent_arrowed_line(
        drawer: &mut EditDrawer,
        start: Vec2,
        end: Vec2,
        color: Color
    )
    {
        let (start, end) = Self::shifted_arrowed_line_points(drawer, start, end);
        drawer.semitransparent_arrowed_line(start, end, color);
    }

    /// Draws the line going from the center of the [`Brush`] to the first node.
    #[inline]
    fn draw_knot(&self, drawer: &mut EditDrawer, center: Vec2, color: Option<Color>)
    {
        let color = color.unwrap_or(Color::BrushAnchor);
        drawer.square_highlight(center, color);
        drawer.line(center, self.nodes_world(center).first().0, color);
    }

    /// Draws the path.
    #[inline]
    fn draw_with_color(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        drawer: &mut EditDrawer,
        center: Vec2,
        color: Color,
        show_tooltips: bool,
        highlighted_node: Option<usize>
    )
    {
        self.draw_nodes(drawer, center, color);

        if show_tooltips
        {
            self.tooltips(window, camera, egui_context, drawer, center, color, highlighted_node);
        }
    }

    #[inline]
    pub(in crate::map) fn draw(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        drawer: &mut EditDrawer,
        center: Vec2,
        show_tooltips: bool
    )
    {
        self.draw_knot(drawer, center, Color::BrushAnchor.into());

        self.draw_with_color(
            window,
            camera,
            egui_context,
            drawer,
            center,
            Color::PathNode,
            show_tooltips,
            None
        );
    }

    #[inline]
    pub(in crate::map) fn draw_prop(&self, drawer: &mut EditDrawer, center: Vec2)
    {
        let nodes = self.nodes_world(center);
        drawer.semitransparent_line(center, nodes.first().0, Color::BrushAnchor);
        self.draw_semitransparent_nodes_no_highlights(drawer, center, Color::PathNode);
    }

    #[inline]
    pub(in crate::map) fn draw_with_highlighted_path_node(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        drawer: &mut EditDrawer,
        center: Vec2,
        highlighted_node: usize,
        show_tooltips: bool
    )
    {
        self.draw_knot(drawer, center, Color::HighlightedPath.into());

        self.draw_with_color(
            window,
            camera,
            egui_context,
            drawer,
            center,
            Color::HighlightedPath,
            show_tooltips,
            highlighted_node.into()
        );
    }

    /// Draws the path with semitransparent materials.
    #[inline]
    pub(in crate::map) fn draw_semitransparent(&self, drawer: &mut EditDrawer, center: Vec2)
    {
        let nodes = self.nodes_world(center);
        drawer.semitransparent_square_highlight(center, Color::BrushAnchor);
        drawer.semitransparent_line(center, nodes.first().0, Color::BrushAnchor);
        self.draw_semitransparent_nodes(drawer, center, Color::PathNode);
    }

    /// Draws the path including the nodes being inserted into it.
    #[inline]
    pub(in crate::map) fn draw_with_node_insertion(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        drawer: &mut EditDrawer,
        pos: Vec2,
        index: usize,
        center: Vec2,
        show_tooltips: bool
    )
    {
        self.draw_knot(drawer, center, Color::HighlightedPath.into());

        if !self.is_node_at_index_valid(pos, index, center)
        {
            self.draw_with_color(
                window,
                camera,
                egui_context,
                drawer,
                center,
                Color::HighlightedPath,
                show_tooltips,
                None
            );

            return;
        }

        let nodes = NodesInsertionIter::new(self.nodes(), pos, index, center);

        for (i, [start, end]) in nodes.clone().enumerate()
        {
            // Draw the square of the end of the line.
            drawer.square_highlight(
                end.0,
                if end.1 { Color::SelectedPathNode } else { Color::HighlightedPath }
            );

            // Look for any back-forth routes.
            let mut shift_iter = nodes.clone().skip_index(i).unwrap();

            if shift_iter
                .by_ref()
                .any(|[a, b]| a.0.around_equal(&end.0) && b.0.around_equal(&start.0))
            {
                Self::shifted_arrowed_line(drawer, start.0, end.0, Color::HighlightedPath);
            }
            else
            {
                drawer.arrowed_line(start.0, end.0, Color::HighlightedPath);
            }
        }

        if !show_tooltips
        {
            return;
        }

        let mut tooltip_text = String::new();
        let key = pos - center;

        macro_rules! bucket_with_plus_one {
            ($bucket:ident) => {
                self.bucket_tooltip(
                    window,
                    camera,
                    egui_context,
                    drawer,
                    center,
                    Color::HighlightedPath,
                    $bucket,
                    &mut tooltip_text,
                    |text, node, mut idx| {
                        if idx >= index
                        {
                            idx += 1;
                        }

                        Self::push_new_index(text, node, idx);
                    }
                );
            };
        }

        if let Some(bucket) = self.buckets.get(key)
        {
            if !bucket.contains(&(index - 1)) && !bucket.contains(&index)
            {
                for (_, bucket) in self
                    .buckets
                    .iter()
                    .filter_set_with_predicate(HashVec2(key), |(key, _)| **key)
                {
                    bucket_with_plus_one!(bucket);
                }

                tooltip_text.push_str(INDEXES[index]);
                bucket_with_plus_one!(bucket);
                return;
            }
        }

        for (_, bucket) in self.buckets.iter()
        {
            bucket_with_plus_one!(bucket);
        }

        tooltip_text.push_str(INDEXES[index]);

        Self::tooltip(
            window,
            camera,
            egui_context,
            drawer,
            pos,
            &mut tooltip_text,
            Color::HighlightedPath
        );
    }

    /// Draws the path being free drawn.
    #[inline]
    pub(in crate::map) fn draw_free_draw_with_knot(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        drawer: &mut EditDrawer,
        center: Vec2,
        show_tooltips: bool
    )
    {
        self.draw_knot(drawer, center, Color::CursorPolygon.into());
        self.draw_free_draw(window, camera, egui_context, drawer, center, show_tooltips);
    }

    #[inline]
    pub(in crate::map) fn draw_free_draw(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        drawer: &mut EditDrawer,
        center: Vec2,
        show_tooltips: bool
    )
    {
        self.draw_nodes(drawer, center, Color::CursorPolygon);

        if !show_tooltips
        {
            return;
        }

        let mut tooltip_text = String::new();

        for (_, bucket) in self.buckets.iter()
        {
            self.regular_bucket_tooltip(
                window,
                camera,
                egui_context,
                drawer,
                center,
                Color::CursorPolygon,
                bucket,
                &mut tooltip_text
            );
        }
    }

    /// Draws the path while there is an ongoing movement simulation.
    #[inline]
    pub(in crate::map) fn draw_movement_simulation(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        drawer: &mut EditDrawer,
        center: Vec2,
        movement_vec: Vec2,
        show_tooltips: bool
    )
    {
        let start = center + movement_vec;
        let end = self.nodes_world(center).first().0 + movement_vec;
        drawer.square_highlight(start, Color::BrushAnchor);
        drawer.square_highlight(end, Color::BrushAnchor);
        drawer.line(start, end, Color::BrushAnchor);

        self.draw_with_color(
            window,
            camera,
            egui_context,
            drawer,
            center,
            Color::PathNode,
            show_tooltips,
            None
        );
    }

    #[inline]
    fn push_new_index(tooltip_text: &mut String, node: &NodeWorld, index: usize)
    {
        index_func!(tooltip_text, index, {
            if node.1
            {
                tooltip_text.push('*');
            }
        });
    }

    #[inline]
    fn tooltip(
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        drawer: &mut EditDrawer,
        pos: Vec2,
        text: &mut String,
        color: Color
    )
    {
        let label = return_if_none!(drawer.vx_tooltip_label(pos));
        write!(text, ": {}", pos.necessary_precision_value()).ok();
        node_tooltip(window, camera, egui_context, pos, label, text, color);
        text.clear();
    }

    #[inline]
    fn bucket_tooltip<'a, F, I>(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        drawer: &mut EditDrawer,
        center: Vec2,
        color: Color,
        bucket: I,
        tooltip_text: &mut String,
        mut index_func: F
    ) where
        F: FnMut(&mut String, &NodeWorld, usize),
        I: IntoIterator<Item = &'a usize>
    {
        let nodes = self.nodes_world(center);
        let mut bucket = bucket.into_iter();

        let idx = *bucket.next_value();
        let node = nodes.nth(idx);
        index_func(tooltip_text, &node, idx);
        let pos = node.0;

        for idx in bucket
        {
            index_func(tooltip_text, &nodes.nth(*idx), *idx);
        }

        Self::tooltip(window, camera, egui_context, drawer, pos, tooltip_text, color);
    }

    #[inline]
    fn regular_bucket_tooltip<'a, I>(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        drawer: &mut EditDrawer,
        center: Vec2,
        color: Color,
        bucket: I,
        tooltip_text: &mut String
    ) where
        I: IntoIterator<Item = &'a usize>
    {
        self.bucket_tooltip(
            window,
            camera,
            egui_context,
            drawer,
            center,
            color,
            bucket,
            tooltip_text,
            |text, node, idx| {
                Self::push_new_index(text, node, idx);
            }
        );
    }

    #[inline]
    fn tooltips(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        drawer: &mut EditDrawer,
        center: Vec2,
        color: Color,
        highlighted_node: Option<usize>
    )
    {
        let mut tooltip_text = String::new();

        if let Some(key) = highlighted_node.map(|idx| self.buckets.bucket_with_index(idx))
        {
            for (_, bucket) in self.buckets.iter().filter_set_with_predicate(key, |(key, _)| **key)
            {
                self.regular_bucket_tooltip(
                    window,
                    camera,
                    egui_context,
                    drawer,
                    center,
                    color,
                    bucket,
                    &mut tooltip_text
                );
            }

            let highlighted_node = highlighted_node.unwrap();

            self.bucket_tooltip(
                window,
                camera,
                egui_context,
                drawer,
                center,
                color,
                self.buckets.get(key.0).unwrap().iter(),
                &mut tooltip_text,
                |text, node, idx| {
                    index_func!(text, idx, {
                        if idx == highlighted_node
                        {
                            if node.1
                            {
                                text.push('#');
                            }
                            else
                            {
                                text.push('~');
                            }
                        }
                        else if node.1
                        {
                            text.push('*');
                        }
                    });
                }
            );

            return;
        }

        for (_, bucket) in self.buckets.iter()
        {
            self.regular_bucket_tooltip(
                window,
                camera,
                egui_context,
                drawer,
                center,
                color,
                bucket,
                &mut tooltip_text
            );
        }
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Draws the [`Node`] tooltip.
#[inline]
pub(in crate::map) fn node_tooltip(
    window: &Window,
    camera: &Transform,
    egui_context: &egui::Context,
    pos: Vec2,
    label: &'static str,
    text: &str,
    color: Color
)
{
    draw_tooltip_x_centered_above_pos(
        egui_context,
        label,
        egui::Order::Background,
        text,
        egui::TextStyle::Monospace,
        to_egui_coordinates(pos, window, camera),
        TOOLTIP_OFFSET,
        egui::Color32::BLACK,
        color.egui_color(),
        3f32
    );
}

#[inline]
#[must_use]
pub(in crate::map) fn calc_path_hull(path: &Path, center: Vec2) -> Hull
{
    (path.hull() + center)
        .merged(&Some(center).into_iter().into())
        .bumped(2f32)
}
