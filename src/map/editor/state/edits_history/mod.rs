mod edit;
pub(in crate::map::editor::state) mod edit_type;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use glam::Vec2;
use hill_vacuum_shared::{continue_if_none, return_if_none};
use smallvec::smallvec;

use self::{edit::Edit, edit_type::EditType};
use super::{
    core::{draw_tool::cursor_polygon::FreeDrawStatus, tool::EditingTarget, Core},
    grid::Grid,
    manager::EntitiesManager,
    ui::Ui
};
use crate::{
    map::{
        brush::{
            convex_polygon::{ConvexPolygon, VertexesMove},
            Brush
        },
        drawer::{
            animation::{Animation, Timing},
            drawing_resources::DrawingResources,
            texture::{
                Texture,
                TextureReset,
                TextureRotation,
                TextureScale,
                TextureSettings,
                TextureSpriteSet
            }
        },
        editor::state::core::UndoRedoInterface,
        path::{MovementValueEdit, NodesMove, Path, StandbyValueEdit},
        properties::value::Value,
        thing::{catalog::ThingsCatalog, ThingId, ThingInstanceData}
    },
    utils::{
        collections::HvVec,
        hull::Flip,
        identifiers::{EntityId, Id},
        misc::TakeValue
    }
};

//=======================================================================//
// MACROS
//
//=======================================================================//

/// A macro to add a basic edit to the history.
macro_rules! push_edit {
    ($(($func:ident, ($($arg:ident: $t:ty),+), ($identifier:expr, $edit:expr))), +) => { paste::paste! { $(
        #[inline]
        pub(in crate::map::editor::state) fn [< $func >](&mut self, $($arg: $t, )+)
        {
            self.push_onto_current_edit($identifier, $edit);
        }
	)+}};
}

//=======================================================================//

/// A macro to add a cluster of a same edit to the history.
macro_rules! push_cluster {
    ($(($func:ident, $t:ty)),+) => { paste::paste! { $(
        #[inline]
        pub(in crate::map::editor::state) fn [< $func _cluster >](&mut self, iter: impl IntoIterator<Item = (Id, $t)>)
        {
            for item in iter.into_iter()
            {
                self.[< $func >](item.0, item.1);
            }
        }
	)+}};
}

//=======================================================================//

/// A macro to add a cluster of edit to the history if the cluster is not an empty set.
macro_rules! push_cluster_if_not_empty {
    ($(($func:ident, $ed_type:ident)),+) => { paste::paste! { $(
        #[inline]
        pub(in crate::map::editor::state) fn [< $func _cluster >]<'a>(&mut self, iter: impl IntoIterator<Item = &'a Id>)
        {
            self.push_if_not_empty(iter.into_iter(), EditType::$ed_type);
        }
	)+}};
}

//=======================================================================//

/// A macro to add an edit to the history that will panic if the set of elements it applies to is
/// empty.
macro_rules! push_with_amount_assertion {
    ($(($func:ident, ($($arg:ident: $t:ty),+), $edit:expr)),+) => { paste::paste! { $(
        #[inline]
        pub(in crate::map::editor::state) fn [< $func >](&mut self, iter: impl IntoIterator<Item = Id>, $($arg: $t, )+)
        {
            self.push_with_amount_assertion(iter.into_iter(), $edit);
        }
	)+}};
}

//=======================================================================//

/// A macro to add a snap edit to the history.
macro_rules! push_snap {
    ($(($func:ident, $edit:ident)),+) => { paste::paste! { $(
        #[inline]
        pub(in crate::map::editor::state) fn [< $func _snap >](&mut self, identifier: Id, value: Vec<(Vec<u8>, Vec2)>)
        {
            let empty = value.is_empty();
            let edit = EditType::[< $edit Snap >](value);

            assert!(!empty, "Edit {edit:?} has no associated entities.");

            self.push_onto_current_edit(smallvec![identifier], edit);
        }
	)+}};
}

//=======================================================================//

/// A macro to add a default animation edit.
macro_rules! default_animation {
    ($(($($func:ident)?, ($($arg:ident: $t:ty),+), $edit:expr)), +) => { paste::paste! { $(
        #[inline]
        pub(in crate::map::editor::state) fn [< default_animation $(_$func)? >](&mut self, $($arg: $t, )+)
        {
            self.push_onto_current_edit(HvVec::new(), $edit);
        }
	)+}};
}

//=======================================================================//

/// Generates the functions to purge all edits of a certain type.
macro_rules! purge {
    ($(($item:ident, $other:ident)),+) => { paste::paste! { $(
        #[inline]
        pub(in crate::map::editor::state) fn [< purge_ $item _edits >](&mut self)
        {
            let mut i = return_if_none!(self.[< earliest_ $item _edit >]);
            self.[< earliest_ $item _edit >] = None;

            while i < self.stack.len()
            {
                if !self.stack[i].[< purge_ $item _edits >]()
                {
                    i += 1;
                    continue;
                }

                self.stack.remove(i);

                if i < self.index
                {
                    self.index -= 1;
                }

                for j in [&mut self.earliest_tool_edit, &mut self.[< earliest_ $other _edit >]].into_iter().flatten()
                {
                    if i < *j
                    {
                        *j -= 1;
                    }
                }
            }
        }
    )+}};
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// Stack of chronologically ordered pre-edit brush states.
pub(in crate::map::editor) struct EditsHistory
{
    /// The chronology of the edits.
    stack: Vec<Edit>,
    /// The edit of the current frame, to be pushed onto the stack at the end of the current frame
    /// if it's not empty.
    current_edit: Edit,
    /// Whether an edit lasting more than a frame is happening.
    multiframe_edit: bool,
    /// The amount of states we can undo.
    index: usize,
    /// The index of the earliest tool edit, if any.
    earliest_tool_edit: Option<usize>,
    /// The index of the earliest texture edit, if any.
    earliest_texture_edit: Option<usize>,
    /// The index of the earliest [`ThingInstance`] edit.
    earliest_thing_edit: Option<usize>,
    /// Whether the push of the new [`Edit`] was halted to avoid the truncation of the history
    /// because it only contains selection edits
    selections_only_edit_halted: bool,
    /// The index of the edit where the file was saved the last time, if any.
    last_save_edit: Option<usize>
}

impl Default for EditsHistory
{
    #[inline]
    fn default() -> Self
    {
        Self {
            stack: Vec::with_capacity(100),
            current_edit: Edit::default(),
            multiframe_edit: false,
            index: 0,
            earliest_tool_edit: None,
            earliest_thing_edit: None,
            earliest_texture_edit: None,
            selections_only_edit_halted: false,
            last_save_edit: 0.into()
        }
    }
}

impl EditsHistory
{
    //=======================================================================//
    // Push edits

    #[rustfmt::skip]
    push_edit!(
        (brush_draw, (identifier: Id), (smallvec![identifier], EditType::DrawnBrush(None))),
		(brush_spawn, (identifier: Id, selected: bool), (smallvec![identifier], EditType::BrushSpawn(None, selected))),
        (drawn_brush_despawn, (brush: Brush), (smallvec![brush.id()], EditType::DrawnBrushDespawn(Some(brush.into_parts().0)))),
        (polygon_edit, (identifier: Id, polygon: ConvexPolygon), (smallvec![identifier], EditType::PolygonEdit(polygon))),
        (path_creation, (identifier: Id), (smallvec![identifier], EditType::PathCreation(None))),
        (free_draw_point_insertion, (p: Vec2, index: u8), (HvVec::new(), EditType::FreeDrawPointInsertion(p, index))),
        (free_draw_point_deletion, (p: Vec2, index: u8), (HvVec::new(), EditType::FreeDrawPointDeletion(p, index))),
        (entity_selection, (identifier: Id), (smallvec![identifier], EditType::EntitySelection)),
        (entity_deselection, (identifier: Id), (smallvec![identifier], EditType::EntityDeselection)),
		(vertex_insertion, (brush: &Brush, vx: (Vec2, u8)), (smallvec![brush.id()], EditType::VertexInsertion(vx))),
		(vertexes_deletion, (identifier: Id, vxs: Vec<(Vec2, u8)>), (smallvec![identifier], EditType::VertexesDeletion(vxs))),
        (sides_deletion, (identifier: Id, vxs: Vec<(Vec2, u8, bool)>), (smallvec![identifier], EditType::SidesDeletion(vxs))),
        (subtractee_selection, (identifier: Id), (smallvec![identifier], EditType::SubtracteeSelection)),
        (subtractee_deselection, (identifier: Id), (smallvec![identifier], EditType::SubtracteeDeselection)),
        (vertexes_selection, (identifier: Id, idxs: Vec<u8>), (smallvec![identifier], EditType::VertexesSelection(idxs))),
        (path_deletion, (identifier: Id, path: Path), (smallvec![identifier], EditType::PathDeletion(Some(path)))),
        (path_nodes_selection, (identifier: Id, idxs: Vec<u8>), (smallvec![identifier], EditType::PathNodesSelection(idxs))),
        (path_node_insertion, (identifier: Id, pos: Vec2, index: u8), (smallvec![identifier], EditType::PathNodeInsertion((pos, index)))),
        (path_nodes_deletion, (identifier: Id, nodes: Vec<(Vec2, u8)>), (smallvec![identifier], EditType::PathNodesDeletion(nodes))),
        (path_nodes_standby_time, (identifier: Id, edit: StandbyValueEdit), (smallvec![identifier], EditType::PathNodeStandby(edit))),
        (path_nodes_max_speed, (identifier: Id, edit: MovementValueEdit), (smallvec![identifier], EditType::PathNodeMaxSpeed(edit))),
        (path_nodes_min_speed, (identifier: Id, edit: MovementValueEdit), (smallvec![identifier], EditType::PathNodeMinSpeed(edit))),
        (path_nodes_accel_travel_percentage, (identifier: Id, edit: MovementValueEdit), (smallvec![identifier], EditType::PathNodeAcceleration(edit))),
        (path_nodes_decel_travel_percentage, (identifier: Id, edit: MovementValueEdit), (smallvec![identifier], EditType::PathNodeDeceleration(edit))),
        (attach, (identifier: Id, attachment: Id), (smallvec![identifier], EditType::BrushAttachment(attachment))),
        (detach, (identifier: Id, attachment: Id), (smallvec![identifier], EditType::BrushDetachment(attachment))),
        (thing_draw, (identifier: Id, thing: ThingInstanceData), (smallvec![identifier], EditType::DrawnThing(thing.into()))),
        (drawn_thing_despawn, (identifier: Id, thing: ThingInstanceData), (smallvec![identifier], EditType::DrawnThingDespawn(thing.into()))),
        (thing_spawn, (identifier: Id, thing: ThingInstanceData), (smallvec![identifier], EditType::ThingSpawn(thing.into()))),
        (thing_despawn, (identifier: Id, thing: ThingInstanceData), (smallvec![identifier], EditType::ThingDespawn(thing.into()))),
        (thing_change, (identifier: Id, thing: ThingId), (smallvec![identifier], EditType::ThingChange(thing))),
        (texture, (identifier: Id, texture: Option<String>), (smallvec![identifier], EditType::TextureChange(texture))),
        (texture_removal, (identifier: Id, texture: TextureSettings), (smallvec![identifier], EditType::TextureRemoval(Some(texture)))),
        (texture_offset_x, (identifier: Id, value: f32), (smallvec![identifier], EditType::TextureOffsetX(value))),
        (texture_offset_y, (identifier: Id, value: f32), (smallvec![identifier], EditType::TextureOffsetY(value))),
        (texture_scroll_x, (identifier: Id, value: f32), (smallvec![identifier], EditType::TextureScrollX(value))),
        (texture_scroll_y, (identifier: Id, value: f32), (smallvec![identifier], EditType::TextureScrollY(value))),
        (texture_scale, (identifier: Id, value: TextureScale), (smallvec![identifier], EditType::TextureScale(value))),
        (texture_scale_x, (identifier: Id, value: f32), (smallvec![identifier], EditType::TextureScaleX(value))),
        (texture_scale_y, (identifier: Id, value: f32), (smallvec![identifier], EditType::TextureScaleY(value))),
        (texture_parallax_x, (identifier: Id, value: f32), (smallvec![identifier], EditType::TextureParallaxX(value))),
        (texture_parallax_y, (identifier: Id, value: f32), (smallvec![identifier], EditType::TextureParallaxY(value))),
        (texture_angle, (identifier: Id, value: TextureRotation), (smallvec![identifier], EditType::TextureRotation(value))),
        (texture_rotation, (identifier: Id, value: TextureRotation), (smallvec![identifier], EditType::TextureRotation(value))),
        (texture_height, (identifier: Id, value: i8), (smallvec![identifier], EditType::TextureHeight(value))),
        (sprite, (identifier: Id, value: TextureSpriteSet), (smallvec![identifier], EditType::SpriteToggle(value))),
        (texture_reset, (identifier: Id, value: TextureReset), (smallvec![identifier], EditType::TextureReset(value))),
        (animation, (identifier: Id, animation: Animation), (smallvec![identifier], EditType::AnimationChange(animation))),
        (atlas_x, (identifier: Id, x: u32), (smallvec![identifier], EditType::AtlasAnimationColumns(x))),
        (atlas_y, (identifier: Id, y: u32), (smallvec![identifier], EditType::AtlasAnimationRows(y))),
        (atlas_len, (identifier: Id, len: usize), (smallvec![identifier], EditType::AtlasAnimationLen(len))),
        (atlas_timing, (identifier: Id, timing: Timing), (smallvec![identifier], EditType::AtlasAnimationTiming(timing.into()))),
        (atlas_uniform_time, (identifier: Id, time: f32), (smallvec![identifier], EditType::AtlasAnimationUniformTime(time))),
        (atlas_frame_time, (identifier: Id, value: (usize, f32)), (smallvec![identifier], EditType::AtlasAnimationFrameTime(value.0, value.1)))
	);

    #[rustfmt::skip]
    push_cluster!(
        (polygon_edit, ConvexPolygon),
        (vertexes_selection, Vec<u8>),
        (path_nodes_deletion, Vec<(Vec2, u8)>),
        (path_nodes_standby_time, StandbyValueEdit),
        (path_nodes_max_speed, MovementValueEdit),
        (path_nodes_min_speed, MovementValueEdit),
        (path_nodes_accel_travel_percentage, MovementValueEdit),
        (path_nodes_decel_travel_percentage, MovementValueEdit),
        (sides_deletion, Vec<(Vec2, u8, bool)>),
        (thing_change, ThingId),
        (texture, Option<String>),
        (texture_removal, TextureSettings),
        (texture_offset_x, f32),
        (texture_offset_y, f32),
        (texture_scale, TextureScale),
        (texture_scale_x, f32),
        (texture_scale_y, f32),
        (texture_scroll_x, f32),
        (texture_scroll_y, f32),
        (texture_parallax_x, f32),
        (texture_parallax_y, f32),
        (texture_angle, TextureRotation),
        (texture_height, i8),
        (texture_rotation, TextureRotation),
        (texture_reset, TextureReset),
        (animation, Animation),
        (atlas_x, u32),
        (atlas_y, u32),
        (atlas_len, usize),
        (atlas_timing, Timing),
        (atlas_uniform_time, f32),
        (atlas_frame_time, (usize, f32))
    );

    #[rustfmt::skip]
    push_cluster_if_not_empty!(
        (entity_selection, EntitySelection),
        (entity_deselection, EntityDeselection),
        (subtractee_selection, SubtracteeSelection),
        (subtractee_deselection, SubtracteeDeselection)
    );

    #[rustfmt::skip]
    push_with_amount_assertion!(
        (flip, (flip: Flip, flip_texture: bool), EditType::BrushFlip(flip, flip_texture)),
        (texture_flip, (y: bool), EditType::TextureFlip(y)),
        (animation_move_up, (index: usize, atlas: bool), EditType::ListAnimationFrameMoveUp(index, atlas)),
        (animation_move_down, (index: usize, atlas: bool), EditType::ListAnimationFrameMoveDown(index, atlas)),
        (list_animation_time, (index: usize, time: f32), EditType::ListAnimationTime(index, time)),
        (list_animation_new_frame, (texture: &str), EditType::ListAnimationNewFrame(texture.to_owned())),
        (list_animation_texture, (index: usize, texture: String), EditType::ListAnimationTexture(index, texture)),
        (list_animation_frame_removal, (index: usize, texture: String, time: f32), EditType::ListAnimationFrameRemoval(index, texture, time))
    );

    #[rustfmt::skip]
    push_snap!((vertexes, Vertexes), (path_nodes, PathNodes));

    #[rustfmt::skip]
    default_animation!(
        (, (texture: &Texture, animation: Animation), EditType::TAnimation(texture.name().to_owned(), animation)),
        (atlas_x, (texture: &Texture, x: u32), EditType::TAtlasAnimationX(texture.name().to_owned(), x)),
        (atlas_y, (texture: &Texture, y: u32), EditType::TAtlasAnimationY(texture.name().to_owned(), y)),
        (atlas_len, (texture: &Texture, len: usize), EditType::TAtlasAnimationLen(texture.name().to_owned(), len)),
        (atlas_timing, (texture: &Texture, timing: Timing), EditType::TAtlasAnimationTiming(texture.name().to_owned(), timing.into())),
        (atlas_uniform_time, (texture: &Texture, time: f32), EditType::TAtlasAnimationUniformTime(texture.name().to_owned(), time)),
        (atlas_frame_time, (texture: &Texture, index: usize, time: f32), EditType::TAtlasAnimationFrameTime(texture.name().to_owned(), index, time)),
        (move_up, (texture: &Texture, index: usize, atlas: bool), EditType::TAnimationMoveUp(texture.name().to_owned(), index, atlas)),
        (move_down, (texture: &Texture, index: usize, atlas: bool), EditType::TAnimationMoveDown(texture.name().to_owned(), index, atlas)),
        (list_new_frame, (texture: &Texture, new_texture: &str), EditType::TListAnimationNewFrame(texture.name().to_owned(), new_texture.to_owned())),
        (list_texture, (texture: &Texture, index: usize, prev_texture: &str), EditType::TListAnimationTexture(texture.name().to_owned(),index, prev_texture.to_owned())),
        (list_time, (texture: &Texture, index: usize, time: f32), EditType::TListAnimationTime(texture.name().to_owned(), index, time)),
        (list_frame_removal, (texture: &Texture, index: usize, prev_texture: &str, time: f32), EditType::TListAnimationFrameRemoval(texture.name().to_owned(),index, prev_texture.to_owned(), time))
    );

    purge!((texture, thing), (thing, texture));

    /// Push a new sub-edits on the current [`Edit`].
    #[inline]
    fn push_onto_current_edit(&mut self, identifiers: HvVec<Id>, edit: EditType)
    {
        if self.selections_only_edit_halted
        {
            self.force_push_frame_edit();
        }

        self.current_edit.push(identifiers, edit);
    }

    /// Push a sub-edit onto the current [`Edit`].
    #[inline]
    fn push_with_amount_assertion(
        &mut self,
        identifiers: impl IntoIterator<Item = Id>,
        edit_type: EditType
    )
    {
        let identifiers = identifiers.into_iter().collect::<HvVec<_>>();
        assert!(!identifiers.is_empty(), "Edit {edit_type:?} has no associated entities.");
        self.push_onto_current_edit(identifiers, edit_type);
    }

    #[allow(clippy::missing_docs_in_private_items)]
    #[inline]
    fn push_if_not_empty<'a>(
        &mut self,
        identifiers: impl IntoIterator<Item = &'a Id>,
        edit_type: EditType
    )
    {
        let identifiers = identifiers.into_iter().copied().collect::<HvVec<_>>();

        if identifiers.is_empty()
        {
            return;
        }

        self.push_onto_current_edit(identifiers, edit_type);
    }

    #[allow(clippy::missing_docs_in_private_items)]
    #[inline]
    pub(in crate::map::editor::state) fn brush_despawn(&mut self, brush: Brush, selected: bool)
    {
        let (data, id) = brush.into_parts();
        self.push_onto_current_edit(smallvec![id], EditType::BrushDespawn(Some(data), selected));
    }

    #[allow(clippy::missing_docs_in_private_items)]
    #[inline]
    pub(in crate::map::editor::state) fn vertexes_move(
        &mut self,
        vxs_move: Vec<(Id, Vec<VertexesMove>)>
    )
    {
        if vxs_move.is_empty()
        {
            return;
        }

        for (id, vx_move) in vxs_move
        {
            self.push_onto_current_edit(smallvec![id], EditType::VertexesMove(vx_move));
        }
    }

    #[allow(clippy::missing_docs_in_private_items)]
    #[inline]
    pub(in crate::map::editor::state) fn entity_move_cluster(
        &mut self,
        manager: &EntitiesManager,
        delta: Vec2,
        move_texture: bool
    )
    {
        if manager.any_selected_brushes()
        {
            let identifiers = manager.selected_brushes_ids().copied().collect::<HvVec<_>>();
            assert!(!identifiers.is_empty(), "EditType::BrushMove has no associated entities.");
            self.push_onto_current_edit(identifiers, EditType::BrushMove(delta, move_texture));
        }

        if !manager.any_selected_things()
        {
            return;
        }

        let identifiers = manager.selected_things().map(EntityId::id).collect::<HvVec<_>>();
        assert!(!identifiers.is_empty(), "EditType::ThingMove has no associated entities.");
        self.push_onto_current_edit(identifiers, EditType::ThingMove(delta));
    }

    #[allow(clippy::missing_docs_in_private_items)]
    #[inline]
    pub(in crate::map::editor::state) fn thing_move(&mut self, identifier: Id, delta: Vec2)
    {
        self.push_onto_current_edit(smallvec![identifier], EditType::ThingMove(delta));
    }

    #[allow(clippy::missing_docs_in_private_items)]
    #[inline]
    pub(in crate::map::editor::state) fn texture_move_cluster(
        &mut self,
        manager: &EntitiesManager,
        delta: Vec2
    )
    {
        let mut identifiers = HvVec::new();

        for brush in manager.selected_textured_brushes()
        {
            identifiers.push(brush.id());

            for id in continue_if_none!(brush.attachments_iter())
            {
                identifiers.push(*id);
            }
        }

        assert!(!identifiers.is_empty(), "EditType::TextureMove has no associated entities.");
        self.push_onto_current_edit(identifiers, EditType::TextureMove(delta));
    }

    #[allow(clippy::missing_docs_in_private_items)]
    #[inline]
    pub(in crate::map::editor::state) fn path_nodes_move(
        &mut self,
        nodes_move: Vec<(Id, Vec<NodesMove>)>
    )
    {
        if nodes_move.is_empty()
        {
            return;
        }

        for (id, node_move) in nodes_move
        {
            self.push_onto_current_edit(smallvec![id], EditType::PathNodesMove(node_move));
        }
    }

    #[allow(clippy::missing_docs_in_private_items)]
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn path_nodes_selection_cluster(
        &mut self,
        iter: impl IntoIterator<Item = (Id, Vec<u8>)>
    ) -> bool
    {
        iter.into_iter().fold(false, |_, item| {
            self.path_nodes_selection(item.0, item.1);
            true
        })
    }

    #[allow(clippy::missing_docs_in_private_items)]
    #[inline]
    pub(in crate::map::editor::state) fn property(
        &mut self,
        key: &str,
        iter: impl IntoIterator<Item = (Id, Value)>
    )
    {
        if self.selections_only_edit_halted
        {
            self.force_push_frame_edit();
        }

        self.current_edit.push_property(key, iter);
    }

    /// Pushes the current [`Edit`] on the history.
    /// The history is truncated if any edits were undone.
    /// # Panics
    /// Panics if the current edit is empty, or the edit is not finished, or edit push is halted by
    /// a selection only edit.
    #[inline]
    fn execute_frame_edit_push(&mut self)
    {
        /// Updates the `earliest_edit` to fit within `len`.
        #[inline]
        fn update_earliest_edit(earliest_edit: &mut Option<usize>, contains_edit: bool, len: usize)
        {
            match earliest_edit
            {
                Some(idx) =>
                {
                    if contains_edit
                    {
                        *idx = (*idx).min(len);
                    }
                    else if len < *idx
                    {
                        *earliest_edit = None;
                    }
                },
                None =>
                {
                    if contains_edit
                    {
                        *earliest_edit = len.into();
                    }
                }
            };
        }

        assert!(
            !self.current_edit.is_empty() &&
                self.concluded_edit() &&
                !self.selections_only_edit_halted,
            "Unsuitable state for edit push, empty {} concluded edit {} halted {}",
            self.current_edit.is_empty(),
            self.concluded_edit(),
            self.selections_only_edit_halted
        );

        if self.index != self.stack.len()
        {
            self.stack.truncate(self.index);

            if let Some(idx) = self.last_save_edit
            {
                if idx > self.index
                {
                    self.last_save_edit = None;
                }
            }
        }

        let len = self.stack.len();
        update_earliest_edit(
            &mut self.earliest_tool_edit,
            self.current_edit.contains_tool_edit(),
            len
        );
        update_earliest_edit(
            &mut self.earliest_texture_edit,
            self.current_edit.contains_texture_edit(),
            len
        );

        update_earliest_edit(
            &mut self.earliest_thing_edit,
            self.current_edit.contains_thing_edit(),
            len
        );

        if let Some(idx) = &mut self.last_save_edit
        {
            if self.current_edit.only_contains_selection_edits()
            {
                *idx += 1;
            }
        }

        self.stack.push(self.current_edit.take_value());
        self.index += 1;
    }

    /// Pushes the current [`Edit`] on the history unless it is empty, or it is not concluded, or if
    /// edit push is halted by a selection only edit.
    #[inline]
    pub(in crate::map::editor::state) fn push_frame_edit(&mut self)
    {
        if self.current_edit.is_empty() || !self.concluded_edit()
        {
            return;
        }

        if self.selections_only_edit_halted ||
            (self.index != self.stack.len() &&
                self.current_edit.only_contains_entity_selection_edits())
        {
            self.selections_only_edit_halted = true;
            return;
        }

        self.execute_frame_edit_push();
    }

    /// Forcefully push the current [`Edit`] even if edit push is halted by a selection only edit.
    #[inline]
    fn force_push_frame_edit(&mut self)
    {
        if self.current_edit.is_empty() || !self.concluded_edit()
        {
            assert!(!self.selections_only_edit_halted, "Edit push halted by selection only edit.");
            return;
        }

        self.selections_only_edit_halted = false;
        self.execute_frame_edit_push();
    }

    /// Removes all sub-edits only necessary to the previously active tool from the history.
    #[inline]
    pub(in crate::map::editor::state) fn purge_tools_edits(
        &mut self,
        prev_editing_target: EditingTarget,
        current_editing_target: EditingTarget
    )
    {
        assert!(
            current_editing_target.requires_tool_edits_purge(prev_editing_target),
            "Tool change does not required tool edits purge."
        );

        if matches!(
            (prev_editing_target, current_editing_target),
            (EditingTarget::BrushFreeDraw(_), EditingTarget::Draw) |
                (
                    EditingTarget::BrushFreeDraw(FreeDrawStatus::Polygon),
                    EditingTarget::BrushFreeDraw(FreeDrawStatus::Inactive)
                ) |
                (EditingTarget::PathFreeDraw, EditingTarget::Path)
        )
        {
            self.purge_free_draw_edits();
            return;
        }

        let mut i = return_if_none!(self.earliest_tool_edit);
        self.earliest_tool_edit = None;

        while i < self.stack.len()
        {
            let edit = &mut self.stack[i];

            if !edit.purge_tools_edits()
            {
                i += 1;
                continue;
            }

            self.stack.remove(i);

            if i < self.index
            {
                self.index -= 1;
            }
        }
    }

    /// Removes all free draw sub-edits from the history.
    #[inline]
    pub(in crate::map::editor::state) fn purge_free_draw_edits(&mut self)
    {
        let mut i = return_if_none!(self.earliest_tool_edit);

        if self.stack[i].contains_free_draw_edit()
        {
            self.earliest_tool_edit = None;
        }

        while i < self.stack.len()
        {
            if !self.stack[i].purge_free_draw_edits()
            {
                i += 1;
                continue;
            }

            self.stack.remove(i);

            if i < self.index
            {
                self.index -= 1;
            }
        }
    }

    /// Whether there is an ongoing multiframe edit.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) const fn multiframe_edit(&self) -> bool
    {
        self.multiframe_edit
    }

    /// Starts a multiframe edit.
    #[inline]
    pub(in crate::map::editor::state) fn start_multiframe_edit(&mut self)
    {
        assert!(!self.multiframe_edit, "Multiframe edit already enabled.");
        self.multiframe_edit = true;
    }

    /// Ends a multiframe edit.
    #[inline]
    pub(in crate::map::editor::state) fn end_multiframe_edit(&mut self)
    {
        assert!(self.multiframe_edit, "Multiframe edit not enabled.");
        self.multiframe_edit = false;
    }

    /// Whether there are no unsaved edits.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn no_unsaved_edits(&self) -> bool
    {
        let idx = match self.last_save_edit
        {
            Some(idx) =>
            {
                if idx == self.index
                {
                    return true;
                }

                if self.index < idx
                {
                    return false;
                }

                idx
            },
            None => 0
        };

        (idx..self.index).all(|i| self.stack[i].contains_free_draw_edit())
    }

    /// Sets the current edit to be the one of the last save.
    #[inline]
    pub(in crate::map::editor::state) fn reset_last_save_edit(&mut self)
    {
        self.last_save_edit = self.index.into();
    }

    #[inline]
    pub(in crate::map::editor::state) fn override_edit_tag(&mut self, tag: &str)
    {
        self.current_edit.override_tag(tag);
    }

    //=======================================================================//
    // Info

    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) const fn index(&self) -> usize { self.index }

    /// Whether there is no ongoing edit.
    #[inline]
    #[must_use]
    const fn concluded_edit(&self) -> bool { !self.multiframe_edit }

    //=======================================================================//
    // Undo/redo

    /// Undoes a change, ergo reverts identifiers which were modified with a single
    /// action to their previous state.
    #[inline]
    pub(in crate::map::editor::state) fn undo(
        &mut self,
        interface: &mut UndoRedoInterface,
        drawing_resources: &mut DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid,
        ui: &mut Ui
    )
    {
        // Nothing to be undone.
        if self.index == 0
        {
            return;
        }

        if self.current_edit.only_contains_entity_selection_edits()
        {
            self.force_push_frame_edit();
        }

        self.index -= 1;

        let edit = &mut self.stack[self.index];
        edit.undo(interface, drawing_resources, things_catalog, grid, ui);

        if edit.only_contains_selection_edits()
        {
            if let Some(idx) = &mut self.last_save_edit
            {
                *idx -= 1;
            }
        }
    }

    /// Redoes a change for a cluster of identifiers that were edited in group.
    #[inline]
    pub(in crate::map::editor::state) fn redo(
        &mut self,
        interface: &mut UndoRedoInterface,
        drawing_resources: &mut DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid,
        ui: &mut Ui
    )
    {
        if self.index == self.stack.len()
        {
            return;
        }

        if self.current_edit.only_contains_entity_selection_edits()
        {
            self.current_edit
                .undo(interface, drawing_resources, things_catalog, grid, ui);
            self.current_edit.clear();
            self.selections_only_edit_halted = false;
        }

        let edit = &mut self.stack[self.index];
        edit.redo(interface, drawing_resources, things_catalog, grid, ui);

        if edit.only_contains_selection_edits()
        {
            if let Some(idx) = &mut self.last_save_edit
            {
                *idx += 1;
            }
        }

        self.index += 1;
    }

    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn show(
        &self,
        ui: &mut egui::Ui,
        core: &Core
    ) -> Option<usize>
    {
        #[inline]
        fn set_post_index_visuals(ui: &mut egui::Ui)
        {
            let visuals = &mut ui.visuals_mut().widgets.inactive;
            visuals.weak_bg_fill = egui::Color32::from_gray(visuals.weak_bg_fill.r() - 20);
        }

        // Cannot undo/redo.
        if !core.undo_redo_available()
        {
            ui.add_enabled(false, egui::Button::new("Opened Map"));

            for ed in &self.stack
            {
                ui.add_enabled(false, egui::Button::new(ed.tag()));
            }

            return None;
        }

        // Clicked the first, no point in doing special handling.
        if ui.add(egui::Button::new("Opened map")).clicked()
        {
            set_post_index_visuals(ui);

            for edit in &self.stack
            {
                _ = ui.button(edit.tag());
            }

            return 0.into();
        }

        let mut clicked = None;
        let mut i = 0;

        while i < self.index
        {
            if ui.button(self.stack[i].tag()).clicked()
            {
                clicked = (i + 1).into();
                i += 1;
                break;
            }

            i += 1;
        }

        while i < self.index
        {
            _ = ui.button(self.stack[i].tag());
            i += 1;
        }

        set_post_index_visuals(ui);

        // One button was clicked, quickly draw the others.
        if clicked.is_some()
        {
            for ed in self.stack.iter().skip(self.index)
            {
                _ = ui.button(ed.tag());
            }
        }
        else
        {
            // No button clicked, check the remaining ones.
            let mut iter = self.stack.iter().enumerate().skip(self.index);

            for (i, ed) in &mut iter
            {
                if ui.button(ed.tag()).clicked()
                {
                    clicked = (i + 1).into();
                    break;
                }
            }

            for (_, ed) in iter
            {
                _ = ui.button(ed.tag());
            }
        }

        clicked
    }
}
