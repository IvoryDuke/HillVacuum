pub mod nodes;
#[cfg(feature = "ui")]
pub(in crate::map) mod overall_values;

//=======================================================================//
// UI
//
//=======================================================================//

#[cfg(feature = "ui")]
pub(in crate::map) mod ui_mod
{
    //=======================================================================//
    // IMPORTS
    //
    //=======================================================================//

    use std::{fmt::Write, iter::Enumerate};

    use bevy::{transform::components::Transform, window::Window};
    use bevy_egui::egui;
    use glam::Vec2;
    use hill_vacuum_shared::{continue_if_none, return_if_none, NextValue};

    use crate::{
        map::{
            drawer::{
                color::Color,
                drawers::{EditDrawer, MapPreviewDrawer}
            },
            editor::state::{
                grid::Grid,
                manager::{Animators, Brushes}
            },
            path::{
                nodes::{
                    Node,
                    NodeViewer,
                    NodeWorld,
                    NodesInsertionIter,
                    NodesWorld,
                    NodesWorldMut
                },
                overall_values::OverallMovement
            },
            selectable_vector::{deselect_vectors, select_vectors_in_range, SelectableVector},
            thing::catalog::ThingsCatalog,
            OutOfBounds,
            Viewer,
            TOOLTIP_OFFSET
        },
        utils::{
            collections::{hv_hash_map, hv_hash_set, hv_vec, HvHashMap, HvHashSet, HvVec},
            hull::Hull,
            identifiers::{EntityCenter, EntityId},
            iterators::{FilterSet, PairIterator, SkipIndexIterator, TripletIterator},
            math::{
                lines_and_segments::line_point_product,
                AroundEqual,
                FastNormalize,
                HashVec2,
                NecessaryPrecisionValue
            },
            misc::{
                next,
                prev,
                AssertedInsertRemove,
                NoneIfEmpty,
                PointInsideUiHighlight,
                ReplaceValue,
                TakeValue,
                Toggle,
                VX_HGL_SIDE
            },
            overall_value::OverallValueInterface
        },
        Id,
        INDEXES
    };

    //=======================================================================//
    // MACROS
    //
    //=======================================================================//

    /// Implements the functions to draw the [`Node`]s.
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

    /// Implements the functions relative the selected [`Path`] nodes movement parameters.
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

    /// Implements the functions common to all entities that implement [`EditPath`].
    macro_rules! common_edit_path {
        ($(($value:ident, $t:ty)),+) => { paste::paste! { $(
            #[inline]
            fn [< set_selected_path_nodes_ $value >](&mut self, value: f32) -> Option<$t>
            {
                self.path_mut().[< set_selected_nodes_ $value >](value)
            }

            #[inline]
            fn [< undo_path_nodes_ $value _edit >](&mut self, edit: &$t)
            {
                self.path_mut().[< undo_ $value _edit >](edit)
            }

            #[inline]
            fn [< redo_path_nodes_ $value _edit >](&mut self, edit: &$t)
            {
                self.path_mut().[< redo_ $value _edit >](edit)
            }
        )+}};

        () => {
            #[inline]
            fn toggle_path_node_at_index(&mut self, index: usize) -> bool
            {
                self.path_mut().toggle_node_at_index(index)
            }

            #[inline]
            fn exclusively_select_path_node_at_index(&mut self, index: usize) -> crate::map::path::NodeSelectionResult
            {
                let center = self.center();
                self.path_mut().exclusively_select_path_node_at_index(center, index)
            }

            #[inline]
            #[must_use]
            fn deselect_path_nodes(&mut self) -> Option<crate::utils::collections::HvVec<u8>>
            {
                let center = self.center();
                self.path_mut().deselect_nodes(center)
            }

            #[inline]
            fn deselect_path_nodes_no_indexes(&mut self)
            {
                self.path_mut().deselect_nodes_no_indexes();
            }

            #[inline]
            #[must_use]
            fn select_path_nodes_in_range(&mut self, range: &Hull) -> Option<crate::utils::collections::HvVec<u8>>
            {
                let center = self.center();
                self.path_mut().select_nodes_in_range(center, range)
            }

            #[inline]
            fn select_all_path_nodes(&mut self) -> Option<crate::utils::collections::HvVec<u8>>
            {
                self.path_mut().select_all_nodes()
            }

            #[inline]
            #[must_use]
            fn exclusively_select_path_nodes_in_range(&mut self, range: &Hull) -> Option<crate::utils::collections::HvVec<u8>>
            {
                let center = self.center();
                self.path_mut().exclusively_select_nodes_in_range(center, range)
            }

            #[inline]
            fn try_insert_path_node_at_index(&mut self, cursor_pos: Vec2, index: usize) -> bool
            {
                let center = self.center();
                self.path_mut().try_insert_node_at_index(cursor_pos, index, center)
            }

            #[inline]
            fn insert_path_node_at_index(&mut self, pos: Vec2, index: usize)
            {
                let center = self.center();
                self.path_mut().insert_node_at_index(pos, index, center);
            }

            #[inline]
            fn insert_path_nodes_at_indexes(&mut self, nodes: &crate::utils::collections::HvVec<(Vec2, u8)>)
            {
                self.path_mut().insert_nodes_at_indexes(nodes);
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
            fn move_path_nodes_at_indexes(&mut self, snap: &crate::utils::collections::HvVec<(crate::utils::collections::HvVec<u8>, Vec2)>)
            {
                self.path_mut().move_nodes_at_indexes(snap);
            }

            #[inline]
            fn remove_selected_path_nodes(&mut self, payload: crate::map::path::NodesDeletionPayload) -> crate::utils::collections::HvVec<(Vec2, u8)>
            {
                assert!(
                    self.id() == payload.id(),
                    "NodesDeletionPayload ID is not equal to the entity's ID."
                );
                let payload = payload.payload();

                self.path_mut().delete_selected_nodes(payload.iter().rev().map(|(_, idx)| *idx as usize));
                payload
            }

            #[inline]
            fn remove_path_node_at_index(&mut self, index: usize)
            {
                self.path_mut().remove_nodes_at_indexes(Some(index));
            }

            #[inline]
            fn redo_selected_path_nodes_deletion(&mut self)
            {
                self.path_mut().redo_selected_nodes_deletion();
            }

            #[inline]
            #[must_use]
            fn snap_selected_path_nodes(
                &mut self,
                grid: &crate::map::editor::state::grid::Grid
            ) -> Option<crate::utils::collections::HvVec<(crate::utils::collections::HvVec<u8>, Vec2)>>
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

    /// A trait to get information on an entity that can have an associated [`Path`] and may have
    /// one.
    pub(in crate::map) trait Moving: EntityId + EntityCenter
    {
        /// Returns a reference to the associated [`Path`], if any.
        #[must_use]
        fn path(&self) -> Option<&Path>;

        /// Whether the entity has a [`Path`].
        #[must_use]
        fn has_path(&self) -> bool;

        /// Whether the entity could have a [`Path`].
        #[must_use]
        fn possible_moving(&self) -> bool;

        /// The [`Hull`] encompassing the nodes of the [`Path`] and the center of the entity. if
        /// any.
        #[inline]
        fn path_hull(&self) -> Option<Hull>
        {
            if !self.has_path()
            {
                return None;
            }

            calc_path_hull(self.path().unwrap(), self.center()).into()
        }

        /// Whether the [`Hull`] encompassing the nodes of the [`Path`] are out of bounds if the
        /// entity has center at `center`.
        #[inline]
        fn path_hull_out_of_bounds(&self, center: Vec2) -> bool
        {
            if !self.has_path()
            {
                return false;
            }

            calc_path_hull(self.path().unwrap(), center).out_of_bounds()
        }

        /// Returns the `OverallMovement` describing the movement settings of the selected nodes.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        #[inline]
        fn overall_selected_path_nodes_movement(&self) -> OverallMovement
        {
            self.path().unwrap().overall_selected_nodes_movement()
        }

        /// Whether the selected nodes of the [`Path`] can be legally moved by `delta`.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        #[inline]
        fn check_selected_path_nodes_move(&self, delta: Vec2) -> IdNodesMoveResult
        {
            (self.path().unwrap().check_selected_nodes_move(delta), self.id()).into()
        }

        /// Returns the nodes near `cursor_pos`.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        #[inline]
        fn path_nodes_nearby_cursor_pos(&self, cursor_pos: Vec2, camera_scale: f32) -> NearbyNodes
        {
            self.path()
                .unwrap()
                .nearby_nodes(cursor_pos, self.center(), camera_scale)
        }

        /// Whether the selected nodes of the [`Path`] can be legally deleted.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        #[inline]
        fn check_selected_nodes_deletion(&self) -> IdNodesDeletionResult
        {
            (self.path().unwrap().check_selected_nodes_deletion(), self.id()).into()
        }

        /// Returns the [`MovementSimulator`] necessary to animate the entity.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        #[inline]
        fn movement_simulator(&self) -> MovementSimulator
        {
            self.path().unwrap().movement_simulator(self.id())
        }

        /// Draws the [`Path`].
        /// # Panics
        /// Panics if the entity has no [`Path`].
        #[inline]
        fn draw_path(&self, window: &Window, camera: &Transform, drawer: &mut EditDrawer)
        {
            self.path().unwrap().draw(window, camera, drawer, self.center());
        }

        /// Draws the [`Path`] with semitransparent materials.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        #[inline]
        fn draw_semitransparent_path(&self, drawer: &mut EditDrawer)
        {
            self.path().unwrap().draw_semitransparent(drawer, self.center());
        }

        /// Draws the entity highlighted with an highlighted [`Path`].
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn draw_highlighted_with_path_nodes(
            &self,
            window: &Window,
            camera: &Transform,
            brushes: Brushes,
            catalog: &ThingsCatalog,
            drawer: &mut EditDrawer
        );

        /// Draws the entity highlighted with an highlighted [`Path`] and marked `highlighted_node`.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn draw_with_highlighted_path_node(
            &self,
            window: &Window,
            camera: &Transform,
            brushes: Brushes,
            catalog: &ThingsCatalog,
            drawer: &mut EditDrawer,
            highlighted_node: usize
        );

        /// Draws the entity and its [`Path`] with an extra node.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn draw_with_path_node_addition(
            &self,
            window: &Window,
            camera: &Transform,
            brushes: Brushes,
            catalog: &ThingsCatalog,
            drawer: &mut EditDrawer,
            pos: Vec2,
            index: usize
        );

        /// Draws the movement simulation.
        /// # Panics
        /// Panics if the entity has no [`Path`] or if `simulator` is associated to another entity.
        fn draw_movement_simulation(
            &self,
            window: &Window,
            camera: &Transform,
            brushes: Brushes,
            catalog: &ThingsCatalog,
            drawer: &mut EditDrawer,
            simulator: &MovementSimulator
        );

        /// Draws the movement simulation in map preview mode.
        /// # Panics
        /// Panics if the entity has no [`Path`] or if `simulator` is associated to another entity.
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

    /// A trait to edit the [`Path`] associated with an entity.
    pub(in crate::map) trait EditPath: EntityId + Moving
    {
        /// Binds a [`Path`] to the entity.
        fn set_path(&mut self, path: Path);

        /// Toggles the selection of the [`Node`] at index `index`.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn toggle_path_node_at_index(&mut self, index: usize) -> bool;

        /// Only selects the [`Node`] at `index` and returns a [`NodeSelectionResult`] describing
        /// what occurred.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn exclusively_select_path_node_at_index(&mut self, index: usize) -> NodeSelectionResult;

        /// Deselects the [`Nodes`] of the [`Path`] and returns the indexes of the deselected nodes.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        #[must_use]
        fn deselect_path_nodes(&mut self) -> Option<HvVec<u8>>;

        /// Deselects the [`Nodes`] of the [`Path`].
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn deselect_path_nodes_no_indexes(&mut self);

        /// Selects the [`Nodes`] of the [`Path`] within range and returns the indexes of the
        /// selected nodes.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        #[must_use]
        fn select_path_nodes_in_range(&mut self, range: &Hull) -> Option<HvVec<u8>>;

        /// Selects all the [`Nodes`] of the [`Path`] and returns the indexes of the selected nodes.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        #[must_use]
        fn select_all_path_nodes(&mut self) -> Option<HvVec<u8>>;

        /// Exclusively the [`Nodes`] of the [`Path`] and returns the indexes of the deselected
        /// nodes. # Panics
        /// Panics if the entity has no [`Path`].
        #[must_use]
        fn exclusively_select_path_nodes_in_range(&mut self, range: &Hull) -> Option<HvVec<u8>>;

        /// Tries to insert a [`Node`] with position `cursor_pos` at `index`, returns whether the
        /// operation was successful.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        #[must_use]
        fn try_insert_path_node_at_index(&mut self, cursor_pos: Vec2, index: usize) -> bool;

        /// Insert a [`Node`] with position `pos` at `index` into the [`Path`].
        /// # Panics
        /// Panics if the entity has no [`Path`] or if the inserted node generated an invalid path.
        fn insert_path_node_at_index(&mut self, pos: Vec2, index: usize);

        /// Inserts some [`Node`]s with certain position at a certain index in the [`Path`].
        /// # Panics
        /// Panics if the entity has no [`Path`] or the resulting path was invalid.
        fn insert_path_nodes_at_indexes(&mut self, nodes: &HvVec<(Vec2, u8)>);

        /// Executes the [`Path`]'s [`Node`]s move described by `payload` and returns the wrapped
        /// [`NodesMove`].
        /// # Panics
        /// Panics if the entity has no [`Path`] or the resulting path was invalid.
        #[inline]
        fn apply_selected_path_nodes_move(&mut self, payload: NodesMovePayload) -> NodesMove
        {
            assert!(
                payload.0 == self.id(),
                "NodesMovePayload's ID is not equal to the Entity's ID."
            );
            self.redo_path_nodes_move(&payload.1);
            payload.1
        }

        /// Undoes the [`Path`]'s [`Node`]s move described by `nodes_move`.
        /// # Panics
        /// Panics if the entity has no [`Path`] or the resulting path was invalid.
        fn undo_path_nodes_move(&mut self, nodes_move: &NodesMove);

        /// Redoes the [`Path`]'s [`Node`]s move described by `nodes_move`.
        /// # Panics
        /// Panics if the entity has no [`Path`] or the resulting path was invalid.
        fn redo_path_nodes_move(&mut self, nodes_move: &NodesMove);

        /// Undoes a [`Path`] [`Node`]s snap.
        /// # Panics
        /// Panics if the entity has no [`Path`] or the resulting path was invalid.
        fn move_path_nodes_at_indexes(&mut self, snap: &HvVec<(HvVec<u8>, Vec2)>);

        /// Removes the selected [`Path`] [`Node`]s as described by `payload` and returns the list
        /// of removed indexes and positions.
        /// # Panics
        /// Panics if the entity has no [`Path`] or the resulting path was invalid.
        fn remove_selected_path_nodes(
            &mut self,
            payload: NodesDeletionPayload
        ) -> HvVec<(Vec2, u8)>;

        /// Redoes the [`Path`]'s [`Node`] at `index`.
        /// # Panics
        /// Panics if the entity has no [`Path`] or the resulting path was invalid.
        fn remove_path_node_at_index(&mut self, index: usize);

        /// Redoes the selected [`Path`]'s [`Node`]s deletion.
        /// # Panics
        /// Panics if the entity has no [`Path`] or the resulting path was invalid.
        fn redo_selected_path_nodes_deletion(&mut self);

        /// Snaps the selected [`Path`]'s [`Node`]s to the grid. Returns how the nodes were moved.
        /// # Panics
        /// Panics if the entity has no [`Path`] or the resulting path was invalid.
        #[must_use]
        fn snap_selected_path_nodes(&mut self, grid: &Grid) -> Option<HvVec<(HvVec<u8>, Vec2)>>;

        /// Sets the standby time of the selected [`Path`]'s [`Node`]s to `value`, returns a
        /// [`StandbyValueEdit`] describing the outcome.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn set_selected_path_nodes_standby_time(&mut self, value: f32) -> Option<StandbyValueEdit>;

        /// Undoes the [`Path`]'s [`Node`]s standby time edit.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn undo_path_nodes_standby_time_edit(&mut self, edit: &StandbyValueEdit);

        /// Redoes the [`Path`]'s [`Node`]s standby time edit.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn redo_path_nodes_standby_time_edit(&mut self, edit: &StandbyValueEdit);

        /// Sets the max speed of the selected [`Path`]'s [`Node`]s to `value` and returns a
        /// [`MovementValueEdit`] describing the outcome.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn set_selected_path_nodes_max_speed(&mut self, value: f32) -> Option<MovementValueEdit>;

        /// Undoes the [`Path`]'s [`Node`]s max speed edit.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn undo_path_nodes_max_speed_edit(&mut self, edit: &MovementValueEdit);

        /// Redoes the [`Path`]'s [`Node`]s max speed edit.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn redo_path_nodes_max_speed_edit(&mut self, edit: &MovementValueEdit);

        /// Sets the min speed of the selected [`Path`]'s [`Node`]s to `value` and returns a
        /// [`MovementValueEdit`] describing the outcome.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn set_selected_path_nodes_min_speed(&mut self, value: f32) -> Option<MovementValueEdit>;

        /// Undoes the [`Path`]'s [`Node`]s min speed edit.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn undo_path_nodes_min_speed_edit(&mut self, edit: &MovementValueEdit);

        /// Redoes the [`Path`]'s [`Node`]s min speed edit.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn redo_path_nodes_min_speed_edit(&mut self, edit: &MovementValueEdit);

        /// Sets the acceleration travel percentage of the selected [`Path`]'s [`Node`]s to `value`
        /// and returns a [`MovementValueEdit`] describing the outcome.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn set_selected_path_nodes_accel_travel_percentage(
            &mut self,
            value: f32
        ) -> Option<MovementValueEdit>;

        /// Undoes the [`Path`]'s [`Node`]s accel travel percentage edit.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn undo_path_nodes_accel_travel_percentage_edit(&mut self, edit: &MovementValueEdit);

        /// Redoes the [`Path`]'s [`Node`]s accel travel percentage edit.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn redo_path_nodes_accel_travel_percentage_edit(&mut self, edit: &MovementValueEdit);

        /// Sets the deceleration travel percentage of the selected [`Path`]'s [`Node`]s to `value`
        /// and returns a [`MovementValueEdit`] describing the outcome.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn set_selected_path_nodes_decel_travel_percentage(
            &mut self,
            value: f32
        ) -> Option<MovementValueEdit>;

        /// Undoes the [`Path`]'s [`Node`]s decel travel percentage edit.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn undo_path_nodes_decel_travel_percentage_edit(&mut self, edit: &MovementValueEdit);

        /// Redoes the [`Path`]'s [`Node`]s decel travel percentage edit.
        /// # Panics
        /// Panics if the entity has no [`Path`].
        fn redo_path_nodes_decel_travel_percentage_edit(&mut self, edit: &MovementValueEdit);

        /// Removes the [`Path`] from the entity and returns it.
        /// # Panics
        /// Panics if the entity has no [`Path`].
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

    /// The result of the move of the selected [`Node`]s of a [`Path`].
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

    /// The result of the move of the selected [`Node`]s of the [`Path`] with its associated id.
    #[must_use]
    pub(in crate::map) enum IdNodesMoveResult
    {
        /// No nodes were moved.
        None,
        /// Moving the Nodes generate an invalid [`Path`].
        Invalid,
        /// Nodes can be moved.
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

    /// The struct describing how the nodes of the [`Path`] of an entity should be moved.
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
        /// Whether any vertexes were merged.
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
    pub(in crate::map) enum NodeSelectionResult
    {
        /// The node beneath the cursor was already selected.
        Selected,
        /// The node beneath the cursor was not previously selected, it was exclusively selected,
        /// and the vertexes at indexes were deselected.
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

    /// The result of the deletion of the selected [`Node`]s of the [`Path`] with its associated id.
    #[must_use]
    pub(in crate::map) enum IdNodesDeletionResult
    {
        /// No nodes deleted.
        None,
        /// Deleting the nodes creates an invalid [`Path`].
        Invalid,
        /// The deletion is valid, contains the positions and indexes of the deleted nodes.
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
                NodesDeletionResult::Valid(nodes) =>
                {
                    Self::Valid(NodesDeletionPayload(value.1, nodes))
                },
            }
        }
    }

    /// The struct describing which nodes of the [`Path`] of an entity should be removed.
    #[must_use]
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
        /// Consumes `self` and returns the underlying data.
        #[inline]
        pub fn payload(self) -> HvVec<(Vec2, u8)> { self.1 }
    }

    //=======================================================================//

    /// The state of the acceleration/deceleration phase of the travel of an entity from one node to
    /// the next.
    #[must_use]
    enum XcelerationPhase
    {
        /// Ongoing.
        Ongoing(f32),
        /// Concluded and there is some leftover time.
        Reupdate(f32),
        /// Just finished.
        Passed
    }
    //=======================================================================//
    // STRUCTS
    //
    //=======================================================================//

    /// An hashable [`f32`].
    #[derive(PartialEq)]
    struct HashF32(f32);

    impl Eq for HashF32 {}

    impl std::hash::Hash for HashF32
    {
        #[inline]
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) { self.0.to_bits().hash(state); }
    }

    //=======================================================================//

    /// A struct containing the values of the standby time of the selected [`Node`]s of a [`Path`]
    /// before they were changed.
    #[must_use]
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
        /// Returns a new [`StandbyValueEdit`].
        #[inline]
        fn new() -> Self { Self(hv_hash_map![]) }

        /// Inserts the standby time of the [`Node`] of a [`Path`] at index [`index`].
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

    /// A struct containing the values of the edited movement value and its opposite of the selected
    /// [`Node`]s of a [`Path`] before they were changed.
    #[must_use]
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
        /// Returns a new [`MovementValueEdit`].
        #[inline]
        fn new() -> Self { Self(hv_hash_map![]) }

        /// Inserts the values of the [`Node`] at `index` before the edit.
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

    /// The information required for the acceleration phase of the travel.
    #[must_use]
    #[derive(Clone, Copy)]
    struct AccelerationInfo
    {
        /// The acceleration.
        acceleration: f32,
        /// The segment describing the finish line of the acceleration.
        end:          [Vec2; 2]
    }

    //=======================================================================//

    /// The information required for the deceleration phase of the travel.
    #[must_use]
    #[derive(Clone, Copy)]
    struct DecelerationInfo
    {
        /// The deceleration.
        deceleration: f32,
        /// The segment describing the start line of the deceleration.
        start:        [Vec2; 2],
        /// The segment describing the finish line of the deceleration.
        end:          [Vec2; 2]
    }

    //=======================================================================//

    /// An iterator returning the [`Path`] [`Node`]s near the cursor.
    #[must_use]
    pub(in crate::map) struct NearbyNodes<'a>
    {
        /// All the [`Node`]s.
        nodes:        Enumerate<std::slice::Iter<'a, Node>>,
        /// The position of the cursor.
        cursor_pos:   Vec2,
        /// The center of the entity owning the [`Path`].
        center:       Vec2,
        /// The current camera scale.
        camera_scale: f32
    }

    impl Iterator for NearbyNodes<'_>
    {
        type Item = (u8, bool);

        #[inline]
        #[must_use]
        fn next(&mut self) -> Option<Self::Item>
        {
            for (i, node) in &mut self.nodes
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

    /// A struct that allows the path tool to simulate the movement of an entity that owns a
    /// [`Path`].
    #[must_use]
    #[derive(Clone, Copy)]
    pub(in crate::map) struct MovementSimulator
    {
        /// The [`Id`] of the entity.
        id:              Id,
        /// The start position (first [`Node`]).
        start:           Vec2,
        /// The current position.
        pos:             Vec2,
        /// The direction the entity must move to reach the next move.
        dir:             Vec2,
        /// The index of the Node to reach.
        target_index:    usize,
        /// The Node the entity is currently traveling from.
        current_node:    Node,
        /// The Node the entity is currently traveling to.
        target_node:     Node,
        /// The distance that separates the Nodes the entity is traveling to-from.
        travel_distance: f32,
        /// The time that has to pass before the entity can start moving from the current start
        /// Node.
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

        /// Returns the values relative to the travel from one [`Node`] to the next.
        #[inline]
        #[must_use]
        fn distance_accel_decel(
            current_node: &Node,
            target_node: &Node
        ) -> (Vec2, f32, Option<AccelerationInfo>, Option<DecelerationInfo>)
        {
            /// Returns the value of the acceleration/deceleration based on the parameters.
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
                    let acceleration =
                        xceleration(max_squared, min_squared, length * accel_percentage);
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
                target_index: 1,
                current_node,
                target_node,
                travel_distance,
                standby: 0f32,
                current_speed: current_node.movement.start_speed(),
                acceleration,
                deceleration
            }
        }

        /// Returns the distance between the position of the first [`Node`] and the current
        /// position.
        pub(in crate::map) fn movement_vec(&self) -> Vec2 { self.pos - self.start }

        /// How much more time must pass before the current xceleration phase is over.
        #[inline]
        fn xceleration_leftover_time(&self, end: Vec2, xceleration: f32, delta_time: f32) -> f32
        {
            let distance = end - self.pos;
            let delta = (self.current_speed * self.current_speed +
                2f32 * xceleration * distance.length())
            .sqrt();

            delta_time - ((delta - self.current_speed) / xceleration)
        }

        /// Updates the acceleration phase and returns an [`XcelerationPhase`] describing the update
        /// status.
        #[inline]
        fn acceleration_phase(
            &mut self,
            info: &AccelerationInfo,
            delta_time: f32
        ) -> XcelerationPhase
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
            let delta_time =
                self.xceleration_leftover_time(info.end[0], info.acceleration, delta_time);
            self.pos = info.end[0];
            self.current_speed = max_speed;
            XcelerationPhase::Reupdate(delta_time)
        }

        /// Updates the deceleration phase and returns an [`XcelerationPhase`] describing the update
        /// status.
        #[inline]
        fn deceleration_phase(
            &mut self,
            info: &DecelerationInfo,
            delta_time: f32
        ) -> XcelerationPhase
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

            let delta_time =
                self.xceleration_leftover_time(info.end[0], info.deceleration, delta_time);
            self.pos = info.end[0];
            self.current_speed = min_speed;
            XcelerationPhase::Reupdate(delta_time)
        }

        /// How much time must pass before the target is reached.
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
            /// Executes the post acceleration update.
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

                delta_time = self.standby.replace_value(0f32).abs();
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
                    match self.residual_delta_time(
                        average_speed,
                        self.target_node.pos(),
                        delta_time
                    )
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

            // Set travel properties toward the next node.
            let nodes = moving.path().unwrap().nodes();
            self.target_index = next(self.target_index, nodes.len());
            self.current_node = self.target_node.replace_value(nodes[self.target_index]);
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

    impl TakeValue for Buckets
    {
        #[inline]
        fn take_value(&mut self) -> Self { Self(self.0.take_value()) }
    }

    impl Buckets
    {
        /// Returns the indexes of the [`Node`]s at `pos`, if any.
        #[inline]
        #[must_use]
        pub fn get(&self, pos: Vec2) -> Option<&HvVec<usize>> { self.0.get(&HashVec2(pos)) }

        /// Returns an iterator to the grouped [`Node`]s.
        #[inline]
        pub fn iter(&self) -> impl Iterator<Item = (&HashVec2, &HvVec<usize>)> { self.0.iter() }

        /// Returns the position of the bucket containing `index`.
        /// # Panics
        /// Panics if `index` is not contained.
        #[inline]
        pub fn bucket_with_index(&self, index: usize) -> HashVec2
        {
            self.0
                .iter()
                .find_map(|(k, v)| v.contains(&index).then_some(*k))
                .unwrap()
        }

        /// Removes the `index` of a [`Node`] at `pos`.
        /// # Panics
        /// Panics if the values are not contained.
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

        /// Removes and re-inserts the [`Node`] at `index` which was moved to a new position.
        #[inline]
        fn move_index(&mut self, index: usize, pos: Vec2, new_pos: Vec2)
        {
            self.remove(index, pos);
            self.insert(index, new_pos);
        }
    }

    //=======================================================================//

    /// A path describing how an entity moves in space over time.
    #[must_use]
    #[derive(Clone)]
    pub struct Path
    {
        /// The [`Node`]s describing the travel.
        nodes:   HvVec<Node>,
        /// The [`Hull`] describing the area encompassing the path and the center of the owning
        /// entity.
        hull:    Hull,
        /// The nodes sorted in buckets for more efficient arrows drawing.
        buckets: Buckets
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

    impl<'a, I: ExactSizeIterator<Item = &'a Node> + Clone> From<I> for Path
    {
        #[inline]
        fn from(value: I) -> Self
        {
            let hull = Self::nodes_hull(value.clone());
            let mut buckets = Buckets::new();

            for (i, node) in value.clone().enumerate()
            {
                buckets.insert(i, node.pos());
            }

            let path = Self {
                nodes: hv_vec![collect; value.into_iter().copied()],
                hull,
                buckets
            };

            assert!(path.valid(), "From<HvVec<Node>> generated an invalid Path.");

            path
        }
    }

    impl Viewer for Path
    {
        type Item = HvVec<NodeViewer>;

        #[inline]
        fn from_viewer(value: Self::Item) -> Self
        {
            let nodes = hv_vec![collect; value.into_iter().map(|node| {
                Node {
                    selectable_vector: SelectableVector::new(node.pos),
                    movement:          node.movement
                }
            })];
            let hull = Path::nodes_hull(nodes.iter());
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
        }

        #[inline]
        #[must_use]
        fn to_viewer(self) -> Self::Item
        {
            hv_vec![collect; self.nodes.into_iter().map(|node| {
                NodeViewer {
                    pos:      node.pos(),
                    movement: node.movement
                }
            })]
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

        /// Creates a new [`Path`] from two points and the position of the entity center.
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
            let hull = Hull::from_points([node_0.pos(), node_1.pos()]);
            let mut buckets = Buckets::new();
            buckets.insert(0, node_0.pos());
            buckets.insert(1, node_1.pos());

            Self {
                nodes: hv_vec![node_0, node_1],
                hull,
                buckets
            }
        }

        //==============================================================
        // Info

        /// Returns the [`Hull`] encompassing all the [`Node`]s.
        #[inline]
        fn nodes_hull<'a, I: ExactSizeIterator<Item = &'a Node>>(nodes: I) -> Hull
        {
            Hull::from_points(nodes.map(Node::pos))
        }

        #[inline]
        pub const fn hull(&self) -> Hull { self.hull }

        /// Returns a reference to the vector containing the [`Node`]s of the path.
        #[inline]
        pub const fn nodes(&self) -> &HvVec<Node> { &self.nodes }

        /// Returns an instance of [`NodesWorld`] representing the [`Node`]s in world coordinates.
        #[inline]
        const fn nodes_world(&self, center: Vec2) -> NodesWorld
        {
            NodesWorld::new(self.nodes(), center)
        }

        /// Returns an instance of [`NodesWorldMut`] representing the [`Node`]s in world
        /// coordinates.
        #[inline]
        fn nodes_world_mut(&mut self, center: Vec2) -> NodesWorldMut
        {
            NodesWorldMut::new(&mut self.nodes, center)
        }

        /// Whether the [`Node`]s of the [`Path`] are valid.
        #[inline]
        #[must_use]
        fn nodes_valid(&self) -> bool
        {
            self.nodes()
                .pair_iter()
                .unwrap()
                .all(|[a, b]| !a.pos().around_equal_narrow(&b.pos()))
        }

        /// Whether the path is valid.
        #[inline]
        #[must_use]
        fn valid(&self) -> bool
        {
            self.hull.around_equal(&Self::nodes_hull(self.nodes().iter())) &&
                self.nodes().pair_iter().unwrap().enumerate().all(|([_, i], [a, b])| {
                    !a.pos().around_equal_narrow(&b.pos()) &&
                        return_if_none!(self.buckets.get(b.pos()), false).contains(&i)
                })
        }

        /// Returns a [`MovementSimulator`] that allows to show how the entity moves along the path
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

        /// Returns the position of the [`Node`] at `index`.
        #[inline]
        #[must_use]
        pub(in crate::map) fn node_at_index_pos(&self, index: usize) -> Vec2
        {
            self.nodes[index].selectable_vector.vec
        }

        //==============================================================
        // Update

        /// Updates the value of the cached [`Hull`].
        #[inline]
        fn update_hull(&mut self) { self.hull = Self::nodes_hull(self.nodes().iter()); }

        /// Snaps the selected [`Node`]s to the Grid.
        /// Returns a vector of the indexes and positions of the nodes that were snapped, if it was
        /// possible to do so without creating an invalid [`Path`].
        #[inline]
        #[must_use]
        pub(in crate::map) fn snap_selected_nodes(
            &mut self,
            grid: &Grid,
            center: Vec2
        ) -> Option<HvVec<(HvVec<u8>, Vec2)>>
        {
            use hill_vacuum_shared::continue_if_none;

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

        /// Returns the [`Node`]s near `cursor_pos`.
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

        /// Inserts a new [`Node`] at `index`.
        #[inline]
        fn insert(&mut self, index: usize, node: Node)
        {
            self.nodes.insert(index, node);
            self.buckets.insert(index, node.pos());
        }

        /// Removes the [`Node`] at `index`.
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

        /// Whether inserting a new [`Node`] with position `pos` at `index` creates a valid
        /// [`Path`].
        #[inline]
        #[must_use]
        fn is_node_at_index_valid(&self, pos: Vec2, index: usize, center: Vec2) -> bool
        {
            let index = index % self.len();

            !pos.around_equal(&self.nodes[prev(index, self.len())].world_pos(center)) &&
                !pos.around_equal(&self.nodes[index].world_pos(center))
        }

        /// Tries to insert a [`Node`] with position `pos` at `index`.
        /// # Panic
        /// Panics if inserting the node creates an invalid path.
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

        /// Inserts a [`Node`] with position `pos` at index `index`.
        /// # Panic
        /// Panics if inserting the node creates an invalid path.
        #[inline]
        pub(in crate::map) fn insert_node_at_index(&mut self, pos: Vec2, index: usize, center: Vec2)
        {
            assert!(
                self.try_insert_node_at_index(pos, index, center),
                "insert_node_at_index generated an invalid Path."
            );
        }

        /// Inserts many [`Node`]s, with certain positions, at certain indexes, and possibly
        /// selected. # Panic
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
        pub(in crate::map) fn remove_nodes_at_indexes(
            &mut self,
            idxs: impl IntoIterator<Item = usize>
        )
        {
            for idx in idxs
            {
                self.remove(idx);
            }

            self.update_hull();
            assert!(self.valid(), "remove_nodes_at_indexes generated an invalid Path.");
        }

        /// Checks whether deleting the selected [`Node`]s would create a valid path.
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

        /// Redoes the deletion of the selected [`Node`]s.
        /// # Panic
        /// Panics if inserting the node creates an invalid path.
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
            deselect_vectors(self.nodes_world_mut(center).iter())
        }

        /// Deselects the selected [`Node`]s, but does not return the indexes of the nodes that were
        /// deselected.
        #[inline]
        pub(in crate::map) fn deselect_nodes_no_indexes(&mut self)
        {
            for node in &mut self.nodes
            {
                node.selectable_vector.selected = false;
            }
        }

        /// Toggles the selection status of the [`Node`] at index `index` and returns whether it was
        /// selected.
        #[inline]
        pub(in crate::map) fn toggle_node_at_index(&mut self, index: usize) -> bool
        {
            let svec = &mut self.nodes[index].selectable_vector;
            svec.toggle();
            svec.selected
        }

        /// Checks whether there is a [`Node`] nearby `cursor_pos` and selects it if not already
        /// selected. Returns a [`NodeSelectionResult`] describing the outcome.
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
                .iter()
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

        /// Selects all non selected [`Node`]s within range and returns the indexes of the nodes
        /// whose selection was changed.
        #[inline]
        #[must_use]
        pub(in crate::map) fn select_nodes_in_range(
            &mut self,
            center: Vec2,
            range: &Hull
        ) -> Option<HvVec<u8>>
        {
            select_vectors_in_range(self.nodes_world_mut(center).iter(), range)
        }

        /// Exclusively selects all [`Node`]s within range and returns the indexes of the nodes
        /// whose selection was changed.
        #[inline]
        #[must_use]
        pub(in crate::map) fn exclusively_select_nodes_in_range(
            &mut self,
            center: Vec2,
            range: &Hull
        ) -> Option<HvVec<u8>>
        {
            if !range.overlaps(&(self.hull + center))
            {
                return self.deselect_nodes(center);
            }

            hv_vec![collect; self.nodes_world_mut(center)
                .iter()
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

        /// Selects all the [`Node`]s and returns the indexes of the nodes that were selected.
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

        /// Translates the [`Path`] by the vector `delta`.
        /// # Panics
        /// Panics if the generated [`Path`] is invalid.
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

        /// Checks whether moving the selected [`Node`]s by `delta` generates a valid path.
        /// Returns a [`NodesMoveResult`] describing the outcome.
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

        /// Move the [`Node`] at `index` by `delta`.
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
        /// Panics if the resulting path is invalid.
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
        /// Panics if the resulting path is invalid.
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

        //==============================================================
        // Movement

        /// Returns [`OverallMovement`] describing the movement status of the
        /// [`Node`]s.
        #[inline]
        pub(in crate::map) fn overall_selected_nodes_movement(&self) -> OverallMovement
        {
            let mut overall = OverallMovement::new();
            _ = self
                .nodes()
                .iter()
                .filter(|n| n.selectable_vector.selected)
                .any(|node| overall.stack(&node.movement));

            overall
        }

        /// Sets the standby time of the selected [`Node`]s and returns a [`StandbyValueEdit`]
        /// describing the outcome.
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

        /// Undoes a standby time edit.
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

        /// Redoes a standby time edit.
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
        // Draw

        /// Returns the coordinates of a shifted arrow.
        #[inline]
        fn shifted_arrowed_line_points(drawer: &EditDrawer, start: Vec2, end: Vec2)
            -> (Vec2, Vec2)
        {
            /// How much the arrow is shifted sideways from the segment connecting `start` and
            /// `end`.
            const ARROW_SHIFT: f32 = VX_HGL_SIDE / 2f32 * 3f32;

            let dir = (end - start).fast_normalize() * ARROW_SHIFT * drawer.camera_scale();
            let perp = dir.perp();
            (start + dir + perp, end - dir + perp)
        }

        /// Draws a shifted arrowed line.
        #[inline]
        fn shifted_arrowed_line(drawer: &mut EditDrawer, start: Vec2, end: Vec2, color: Color)
        {
            let (start, end) = Self::shifted_arrowed_line_points(drawer, start, end);
            drawer.arrowed_line(start, end, color);
        }

        /// Draws a semitransparent shifted arrowed line.
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

        /// Draws the line going from the center of the entity to the first [`Node`].
        #[inline]
        fn draw_knot(&self, drawer: &mut EditDrawer, center: Vec2, color: Option<Color>)
        {
            let color = color.unwrap_or(Color::BrushAnchor);
            drawer.square_highlight(center, color);
            drawer.line(center, self.nodes_world(center).first().0, color);
        }

        /// Draws the path with the requested color.
        #[inline]
        fn draw_with_color(
            &self,
            window: &Window,
            camera: &Transform,
            drawer: &mut EditDrawer,
            center: Vec2,
            color: Color,
            highlighted_node: Option<usize>
        )
        {
            self.draw_nodes(drawer, center, color);

            if drawer.show_tooltips()
            {
                self.tooltips(window, camera, drawer, center, color, highlighted_node);
            }
        }

        /// Draws the [`Path`].
        #[inline]
        pub(in crate::map) fn draw(
            &self,
            window: &Window,
            camera: &Transform,
            drawer: &mut EditDrawer,
            center: Vec2
        )
        {
            self.draw_knot(drawer, center, Color::BrushAnchor.into());

            self.draw_with_color(window, camera, drawer, center, Color::PathNode, None);
        }

        #[inline]
        pub(in crate::map) fn draw_no_tooltips(&self, drawer: &mut EditDrawer, center: Vec2)
        {
            self.draw_knot(drawer, center, Color::BrushAnchor.into());
            self.draw_nodes(drawer, center, Color::PathNode);
        }

        /// Draws the [`Path`] for the [`Prop`] screenshot.
        #[inline]
        pub(in crate::map) fn draw_prop(&self, drawer: &mut EditDrawer, center: Vec2)
        {
            let nodes = self.nodes_world(center);
            drawer.semitransparent_line(center, nodes.first().0, Color::BrushAnchor);
            self.draw_semitransparent_nodes_no_highlights(drawer, center, Color::PathNode);
        }

        /// Draws the [`Path`] with an highlighted [`Node`].
        #[inline]
        pub(in crate::map) fn draw_with_highlighted_path_node(
            &self,
            window: &Window,
            camera: &Transform,
            drawer: &mut EditDrawer,
            center: Vec2,
            highlighted_node: usize
        )
        {
            self.draw_knot(drawer, center, Color::HighlightedPath.into());

            self.draw_with_color(
                window,
                camera,
                drawer,
                center,
                Color::HighlightedPath,
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

        /// Draws the path including the [`Node`] being inserted into it.
        #[inline]
        pub(in crate::map) fn draw_with_node_insertion(
            &self,
            window: &Window,
            camera: &Transform,
            drawer: &mut EditDrawer,
            pos: Vec2,
            index: usize,
            center: Vec2
        )
        {
            self.draw_knot(drawer, center, Color::HighlightedPath.into());

            if !self.is_node_at_index_valid(pos, index, center)
            {
                self.draw_with_color(window, camera, drawer, center, Color::HighlightedPath, None);

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

            if !drawer.show_tooltips()
            {
                return;
            }

            let mut tooltip_text = String::new();
            let key = pos - center;

            /// Draws the tooltip of a bucket with indexes shifted by one if they come after the
            /// node being inserted.
            macro_rules! bucket_with_plus_one {
                ($bucket:ident) => {
                    self.bucket_tooltip(
                        window,
                        camera,
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

            Self::tooltip(window, camera, drawer, pos, &mut tooltip_text, Color::HighlightedPath);
        }

        /// Draws the path being free drawn.
        #[inline]
        pub(in crate::map) fn draw_free_draw_with_knot(
            &self,
            window: &Window,
            camera: &Transform,
            drawer: &mut EditDrawer,
            center: Vec2
        )
        {
            self.draw_knot(drawer, center, Color::CursorPolygon.into());
            self.draw_free_draw(window, camera, drawer, center);
        }

        /// Draws the [`Path`] in free draw mode.
        #[inline]
        pub(in crate::map) fn draw_free_draw(
            &self,
            window: &Window,
            camera: &Transform,
            drawer: &mut EditDrawer,
            center: Vec2
        )
        {
            self.draw_nodes(drawer, center, Color::CursorPolygon);

            if !drawer.show_tooltips()
            {
                return;
            }

            let mut tooltip_text = String::new();

            for (_, bucket) in self.buckets.iter()
            {
                self.regular_bucket_tooltip(
                    window,
                    camera,
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
            drawer: &mut EditDrawer,
            center: Vec2,
            movement_vec: Vec2
        )
        {
            let start = center + movement_vec;
            let end = self.nodes_world(center).first().0 + movement_vec;
            drawer.square_highlight(start, Color::BrushAnchor);
            drawer.square_highlight(end, Color::BrushAnchor);
            drawer.line(start, end, Color::BrushAnchor);

            self.draw_with_color(window, camera, drawer, center, Color::PathNode, None);
        }

        /// Extends `tooltip_text` with a comma, whatever is inserted by `f` and the string
        /// representation of `index`.
        #[inline]
        fn extend_bucket_tooltip<F: FnOnce(&mut String)>(
            tooltip_text: &mut String,
            index: usize,
            f: F
        )
        {
            if !tooltip_text.is_empty()
            {
                tooltip_text.push_str(", ");
            }

            f(tooltip_text);

            // If this panics I recommend you rethink the choices that led you to this issue.
            tooltip_text.push_str(INDEXES[index]);
        }

        /// Pushes a new index onto `tooltip_text`.
        #[inline]
        fn push_new_index(tooltip_text: &mut String, node: &NodeWorld, index: usize)
        {
            Self::extend_bucket_tooltip(tooltip_text, index, |text| {
                if node.1
                {
                    text.push('*');
                }
            });
        }

        /// Draws a tooltip.
        #[inline]
        fn tooltip(
            window: &Window,
            camera: &Transform,
            drawer: &mut EditDrawer,
            pos: Vec2,
            text: &mut String,
            color: Color
        )
        {
            let label = return_if_none!(drawer.vx_tooltip_label(pos));
            write!(text, ": {}", pos.necessary_precision_value()).ok();
            node_tooltip(window, camera, drawer, pos, label, text, drawer.egui_color(color));
            text.clear();
        }

        /// Draws the tooltip of a bucket of overlapping [`Node`]s.
        #[inline]
        fn bucket_tooltip<'a, F, I>(
            &self,
            window: &Window,
            camera: &Transform,
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

            Self::tooltip(window, camera, drawer, pos, tooltip_text, color);
        }

        /// Draws a standard bucket tooltip.
        #[inline]
        fn regular_bucket_tooltip<'a, I>(
            &self,
            window: &Window,
            camera: &Transform,
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

        /// Draws all the [`Path`]'s tooltips.
        #[inline]
        fn tooltips(
            &self,
            window: &Window,
            camera: &Transform,
            drawer: &mut EditDrawer,
            center: Vec2,
            color: Color,
            highlighted_node: Option<usize>
        )
        {
            let mut tooltip_text = String::new();

            if let Some(key) = highlighted_node.map(|idx| self.buckets.bucket_with_index(idx))
            {
                for (_, bucket) in
                    self.buckets.iter().filter_set_with_predicate(key, |(key, _)| **key)
                {
                    self.regular_bucket_tooltip(
                        window,
                        camera,
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
                    drawer,
                    center,
                    color,
                    self.buckets.get(key.0).unwrap().iter(),
                    &mut tooltip_text,
                    |text, node, idx| {
                        Self::extend_bucket_tooltip(text, idx, |t| {
                            if idx == highlighted_node
                            {
                                if node.1
                                {
                                    t.push('#');
                                }
                                else
                                {
                                    t.push('~');
                                }
                            }
                            else if node.1
                            {
                                t.push('*');
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

    /// A struct sorting the [`Node`]s of a [`Path`] in buckets based on their position.
    #[must_use]
    #[derive(Clone)]
    struct Buckets(HvHashMap<HashVec2, HvVec<usize>>);

    impl Buckets
    {
        /// Returns an empty [`Buckets`].
        #[inline]
        pub fn new() -> Self { Self(hv_hash_map![]) }

        /// Inserts the `index` of a [`Node`] at `pos`.
        /// # Panics
        /// Panics if `index` is already contained.
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
        drawer: &EditDrawer,
        pos: Vec2,
        label: &'static str,
        text: &str,
        fill_color: egui::Color32
    )
    {
        drawer.draw_tooltip_x_centered_above_pos(
            window,
            camera,
            label,
            text,
            pos,
            TOOLTIP_OFFSET,
            drawer.tooltip_text_color(),
            fill_color
        );
    }

    //=======================================================================//

    /// Returns the slightly buffed [`Hull`] encompassing the [`Path`] and the center of the owning
    /// entity.
    #[inline]
    pub(in crate::map) fn calc_path_hull(path: &Path, center: Vec2) -> Hull
    {
        (path.hull() + center)
            .merged(&Hull::from_points(Some(center)))
            .bumped(4f32)
    }
}

#[cfg(feature = "ui")]
pub(in crate::map) use ui_mod::*;
