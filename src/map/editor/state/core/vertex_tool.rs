//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use glam::Vec2;
use hill_vacuum_shared::{
    continue_if_none,
    match_or_panic,
    return_if_no_match,
    return_if_none,
    NextValue
};

use super::{
    cursor_delta::CursorDelta,
    deselect_vertexes,
    draw_non_selected_brushes,
    path_tool::path_creation::PathCreation,
    rect::{Rect, RectTrait},
    selected_vertexes,
    tool::{
        subtools_buttons,
        DisableSubtool,
        DragSelection,
        EnabledTool,
        OngoingMultiframeChange,
        SubTool
    },
    ActiveTool
};
use crate::{
    map::{
        brush::{
            convex_polygon::{VertexHighlightMode, VertexesDeletionResult, VertexesMove},
            Brush,
            SplitPayload,
            SplitResult,
            VertexesMoveResult
        },
        drawer::{color::Color, drawing_resources::DrawingResources},
        editor::{
            cursor::Cursor,
            state::{
                core::{rect, VertexesToggle},
                edits_history::EditsHistory,
                grid::Grid,
                manager::EntitiesManager,
                ui::{ToolsButtons, UiBundle}
            },
            DrawBundle,
            ToolUpdateBundle
        },
        path::Path,
        selectable_vector::VectorSelectionResult
    },
    utils::{
        collections::{hv_hash_map, hv_hash_set, hv_vec, Ids},
        hull::{EntityHull, Hull},
        identifiers::{EntityId, Id},
        iterators::FilterSet,
        math::HashVec2,
        misc::{AssertedInsertRemove, Camera, TakeValue}
    },
    HvHashMap,
    HvVec
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The state of the tool.
#[must_use]
#[derive(Debug)]
enum Status
{
    /// Inactive.
    Inactive(Rect),
    /// Preparing for dragging vertexes.
    PreDrag(Vec2),
    /// Dragging vertexes.
    Drag(CursorDelta, HvVec<(Id, HvVec<VertexesMove>)>),
    /// Inserting a new vertex.
    NewVertex
    {
        /// The [`Id`] of the brush where the vertex is being inserted.
        identifier: Id,
        /// The index where the vertex is being inserted.
        index:      usize,
        /// The position of the vertex.
        vx:         Vec2
    },
    /// Selecting the brush where to insert a new vertex after having enabled it from the UI
    /// button.
    NewVertexUi,
    /// Creating a [`Path`] by clicking of the vertexes of the selected brushes.
    PolygonToPath(PathCreation)
}

impl Default for Status
{
    #[inline]
    fn default() -> Self { Self::Inactive(Rect::default()) }
}

impl EnabledTool for Status
{
    type Item = SubTool;

    #[inline]
    fn is_tool_enabled(&self, tool: Self::Item) -> bool
    {
        tool == match self
        {
            Self::NewVertexUi => SubTool::VertexInsert,
            Self::PolygonToPath(_) => SubTool::VertexPolygonToPath,
            _ => return false
        }
    }
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

selected_vertexes!(selected_vertexes_amount);

//=======================================================================//

/// An extended record of the selected brushes' selected vertexes.
#[must_use]
#[derive(Debug)]
struct BrushesWithSelectedVertexes
{
    /// The [`Id`]s of the brushes with selected vertexes.
    ids:            Ids,
    /// The selected vertexes.
    selected_vxs:   SelectedVertexes,
    /// The [`Id`]s of the brushes that can be split.
    splittable_ids: HvHashMap<Id, SplitPayload>,
    /// The [`Id`] of the brush that does not allow the split to occur.
    error_id:       Option<Id>
}

impl BrushesWithSelectedVertexes
{
    /// Returns a new [`BrushesWithSelectedVertexes`].
    #[inline]
    fn new() -> Self
    {
        Self {
            ids:            hv_hash_set![],
            selected_vxs:   SelectedVertexes::default(),
            splittable_ids: hv_hash_map![],
            error_id:       None
        }
    }

    /// Whether the merge subtool is available.
    #[inline]
    #[must_use]
    const fn vx_merge_available(&self) -> bool { self.selected_vxs.vx_merge_available() }

    /// Whether the split is available.
    #[inline]
    #[must_use]
    fn split_available(&self) -> bool
    {
        if !self.ids.is_empty() && self.error_id.is_none()
        {
            assert!(self.ids.len() == self.splittable_ids.len(), "Invalid split circumstances.");
            return true;
        }

        false
    }

    /// Removes the stored error [`Id`] if it is equal to `identifier`.
    #[inline]
    fn check_error_removal(&mut self, identifier: Id)
    {
        if self.error_id == identifier.into()
        {
            self.error_id = None;
        }
    }

    /// Inserts the selected vertexes info of `brush`.
    #[inline]
    fn insert(&mut self, brush: &Brush)
    {
        assert!(brush.has_selected_vertexes(), "Brush has no selected vertexes.");

        let id = brush.id();
        self.ids.insert(id);

        self.selected_vxs.insert(brush);

        match brush.check_split()
        {
            SplitResult::None => (),
            SplitResult::Invalid =>
            {
                self.splittable_ids.remove(&id);
                self.error_id = id.into();
            },
            SplitResult::Valid(payload) =>
            {
                self.splittable_ids.insert(id, payload);
                self.check_error_removal(id);
            }
        };
    }

    /// Removes the selected vertexes info of `brush`.
    #[inline]
    fn remove(&mut self, brush: &Brush)
    {
        assert!(!brush.has_selected_vertexes(), "Brush has selected vertexes.");

        let id = brush.id_as_ref();

        if self.ids.remove(id)
        {
            self.selected_vxs.remove(brush);
            self.splittable_ids.remove(id);
            self.check_error_removal(*id);
        }
    }

    /// Removes the selected vertexes associated with the brush with [`Id`] `identifier`.
    #[inline]
    fn remove_id(&mut self, manager: &EntitiesManager, identifier: Id)
    {
        assert!(!manager.entity_exists(identifier), "Brush exists.");

        if self.ids.remove(&identifier)
        {
            self.selected_vxs.remove_id(manager, identifier);
            self.splittable_ids.remove(&identifier);
            self.check_error_removal(identifier);
        }
    }

    /// Toggles the info of the selected vertexes of `brush`.
    #[inline]
    fn toggle_brush(&mut self, brush: &Brush)
    {
        if brush.has_selected_vertexes()
        {
            self.insert(brush);
            return;
        }

        self.remove(brush);
    }

    /// Executes the split.
    #[inline]
    fn split_brushes(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: &Grid
    )
    {
        if !self.split_available()
        {
            _ = manager.test_operation_validity(|_| self.error_id);
            return;
        }

        for p in self.splittable_ids.values()
        {
            let (main, other) = {
                let mut brush = manager.brush_mut(drawing_resources, grid, p.id());
                (brush.polygon(), brush.split(p))
            };

            self.ids.asserted_insert(
                manager
                    .replace_brush_with_partition(
                        drawing_resources,
                        edits_history,
                        grid,
                        Some(other).into_iter(),
                        p.id(),
                        |_| main
                    )
                    .next_value()
            );
        }

        edits_history.override_edit_tag("Brushes Split");

        self.splittable_ids.clear();
        self.error_id = (*self.ids.iter().next_value()).into();
    }
}

//=======================================================================//

/// The vertex tool.
#[derive(Debug)]
pub(in crate::map::editor::state::core) struct VertexTool(Status, BrushesWithSelectedVertexes);

impl DisableSubtool for VertexTool
{
    #[inline]
    fn disable_subtool(&mut self)
    {
        if matches!(
            self.0,
            Status::NewVertex { .. } | Status::NewVertexUi | Status::PolygonToPath(..)
        )
        {
            self.0 = Status::default();
        }
    }
}

impl OngoingMultiframeChange for VertexTool
{
    #[inline]
    #[must_use]
    fn ongoing_multi_frame_change(&self) -> bool
    {
        !matches!(
            self.0,
            Status::Inactive(..) |
                Status::PreDrag(_) |
                Status::NewVertexUi |
                Status::PolygonToPath(..)
        )
    }
}

impl DragSelection for VertexTool
{
    #[inline]
    fn drag_selection(&self) -> Option<Rect>
    {
        (*return_if_no_match!(&self.0, Status::Inactive(drag_selection), drag_selection, None))
            .into()
    }
}

impl VertexTool
{
    /// Returns a new [`ActiveTool`] in its vertex tool variant.
    #[inline]
    pub fn tool(drag_selection: Rect) -> ActiveTool
    {
        ActiveTool::Vertex(VertexTool(
            Status::Inactive(drag_selection),
            BrushesWithSelectedVertexes::new()
        ))
    }

    //==============================================================
    // Info

    /// Whether free draw is active.
    #[inline]
    #[must_use]
    pub const fn is_free_draw_active(&self) -> bool { matches!(self.0, Status::PolygonToPath(_)) }

    /// The cursor position to be used.
    #[inline]
    #[must_use]
    const fn cursor_pos(cursor: &Cursor) -> Vec2 { cursor.world() }

    /// Whether the merge subtool is available.
    #[inline]
    pub const fn vx_merge_available(&self) -> bool { self.1.vx_merge_available() }

    /// Whether the split subtool is available.
    #[inline]
    pub fn split_available(&self) -> bool { self.1.split_available() }

    //==============================================================
    // Update

    /// Updates the tool.
    #[inline]
    #[must_use]
    pub fn update(&mut self, bundle: &mut ToolUpdateBundle) -> Option<Path>
    {
        let cursor_pos = Self::cursor_pos(bundle.cursor);

        match &mut self.0
        {
            Status::Inactive(ds) =>
            {
                rect::update!(
                    ds,
                    cursor_pos,
                    bundle.inputs.left_mouse.pressed(),
                    {
                        if !bundle.inputs.left_mouse.just_pressed()
                        {
                            false
                        }
                        else if bundle.inputs.alt_pressed()
                        {
                            if let Some(s) = Self::alt_vertex_left_mouse(
                                bundle.manager,
                                cursor_pos,
                                bundle.camera.scale()
                            )
                            {
                                self.0 = s;
                            }

                            return None;
                        }
                        else if bundle.inputs.shift_pressed()
                        {
                            match Self::toggle_vertexes(bundle, cursor_pos, bundle.camera.scale())
                            {
                                VertexesToggle::None => true,
                                VertexesToggle::Selected =>
                                {
                                    self.0 = Status::PreDrag(cursor_pos);
                                    return None;
                                },
                                VertexesToggle::NonSelected => return None
                            }
                        }
                        else
                        {
                            if Self::exclusively_select_vertexes(
                                bundle,
                                &self.1,
                                cursor_pos,
                                bundle.camera.scale()
                            )
                            {
                                self.0 = Status::PreDrag(cursor_pos);
                                return None;
                            }

                            true
                        }
                    },
                    {
                        deselect_vertexes(
                            bundle.drawing_resources,
                            bundle.manager,
                            bundle.edits_history,
                            bundle.grid
                        );
                    },
                    hull,
                    {
                        Self::select_vertexes_from_drag_selection(bundle, &hull);
                    }
                );

                if bundle.inputs.enter.just_pressed()
                {
                    if bundle.inputs.alt_pressed()
                    {
                        self.0 = Status::PolygonToPath(PathCreation::None);
                    }
                    else
                    {
                        self.1.split_brushes(
                            bundle.drawing_resources,
                            bundle.manager,
                            bundle.edits_history,
                            bundle.grid
                        );
                    }

                    return None;
                }

                if bundle.inputs.back.just_pressed()
                {
                    // Vertex deletion.
                    Self::delete_selected_vertexes(bundle);
                    return None;
                }

                if bundle.inputs.ctrl_pressed()
                {
                    return None;
                }

                // Moving vertex with directional keys.
                let dir = return_if_none!(
                    bundle.inputs.directional_keys_vector(bundle.grid.size()),
                    None
                );
                let mut vxs_move = hv_vec![];

                if self.1.selected_vxs.any_selected_vx() &&
                    Self::move_vertexes(bundle, dir, &mut vxs_move)
                {
                    bundle.edits_history.vertexes_move(vxs_move);
                }
            },
            Status::PreDrag(pos) =>
            {
                if !bundle.inputs.left_mouse.pressed()
                {
                    self.0 = Status::Inactive(Rect::default());
                    return None;
                }

                if !bundle.cursor.moved()
                {
                    return None;
                }

                self.0 = Status::Drag(
                    return_if_none!(CursorDelta::try_new(bundle.cursor, bundle.grid, *pos), None),
                    hv_vec![]
                );
                bundle.edits_history.start_multiframe_edit();
            },
            Status::Drag(drag, cumulative_drag) =>
            {
                if !bundle.inputs.left_mouse.pressed()
                {
                    if drag.delta() != Vec2::ZERO
                    {
                        bundle.edits_history.vertexes_move(cumulative_drag.take_value());
                    }

                    bundle.edits_history.end_multiframe_edit();
                    self.0 = Status::default();
                }
                else if bundle.cursor.moved()
                {
                    drag.conditional_update(bundle.cursor, bundle.grid, |delta| {
                        Self::move_vertexes(bundle, delta, cumulative_drag)
                    });
                }
            },
            Status::NewVertexUi =>
            {
                if bundle.inputs.left_mouse.just_pressed()
                {
                    self.0 = return_if_none!(
                        Self::alt_vertex_left_mouse(
                            bundle.manager,
                            cursor_pos,
                            bundle.camera.scale()
                        ),
                        None
                    );
                }
            },
            Status::NewVertex {
                identifier,
                index,
                vx
            } =>
            {
                let mut brush =
                    bundle
                        .manager
                        .brush_mut(bundle.drawing_resources, bundle.grid, *identifier);

                if !bundle.inputs.left_mouse.pressed()
                {
                    let idx = u8::try_from(*index).unwrap();

                    if brush.try_vertex_insertion_at_index(*vx, idx.into(), false)
                    {
                        bundle.edits_history.vertex_insertion(&brush, (*vx, idx));
                    }

                    self.0 = Status::default();
                    return None;
                }

                let pos = bundle.cursor.world_snapped();

                if bundle.cursor.moved() && brush.is_new_vertex_at_index_valid(pos, *index)
                {
                    *vx = pos;
                }
            },
            Status::PolygonToPath(path) =>
            {
                if bundle.inputs.enter.just_pressed()
                {
                    let mut path = return_if_none!(path.path(), None);
                    path.translate(-path.node_at_index_pos(0));
                    self.0 = Status::Inactive(Rect::default());
                    return path.into();
                }

                let camera_scale = bundle.camera.scale();

                if bundle.inputs.left_mouse.just_pressed()
                {
                    let pos = return_if_none!(
                        bundle
                            .manager
                            .selected_brushes_at_pos(cursor_pos, camera_scale)
                            .iter()
                            .find_map(|brush| brush.nearby_vertex(cursor_pos, camera_scale)),
                        None
                    );

                    path.push(bundle.edits_history, pos, Vec2::ZERO);
                }
                else if bundle.inputs.right_mouse.just_pressed()
                {
                    path.remove(bundle.edits_history, cursor_pos, Vec2::ZERO, camera_scale);
                }
            }
        };

        None
    }

    /// Initializes the insertion of a new vertex.
    #[inline]
    fn alt_vertex_left_mouse(
        manager: &EntitiesManager,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> Option<Status>
    {
        if let Some(pos) = manager
            .selected_brushes_at_pos(cursor_pos, camera_scale)
            .iter()
            .find_map(|brush| brush.nearby_vertex(cursor_pos, camera_scale))
        {
            return Status::PolygonToPath(PathCreation::Point(pos)).into();
        }

        let (id, index) = manager
            .selected_brushes_at_pos(cursor_pos, camera_scale)
            .iter()
            .find_map(|brush| {
                brush
                    .vx_projection_insertion_index(cursor_pos)
                    .map(|idx| (brush.id(), idx))
            })?;

        Status::NewVertex {
            identifier: id,
            index,
            vx: cursor_pos
        }
        .into()
    }

    /// Exclusively selects the vertexes whose highlight is beneath `cursor_pos`.
    #[inline]
    fn exclusively_select_vertexes(
        bundle: &mut ToolUpdateBundle,
        brushes_with_selected_vertexes: &BrushesWithSelectedVertexes,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> bool
    {
        let mut id_vx_id = None;

        for (id, result) in bundle
            .manager
            .selected_brushes_mut_at_pos(
                bundle.drawing_resources,
                bundle.grid,
                cursor_pos,
                camera_scale
            )
            .map(|mut brush| {
                (
                    brush.id(),
                    brush.check_vertex_proximity_and_exclusively_select(cursor_pos, camera_scale)
                )
            })
        {
            match result
            {
                VectorSelectionResult::Selected => return true,
                VectorSelectionResult::NotSelected(vx, idx) =>
                {
                    id_vx_id = (id, vx, idx).into();
                    break;
                },
                VectorSelectionResult::None => ()
            };
        }

        let (id, vx, idx) = return_if_none!(id_vx_id, false);

        bundle.edits_history.vertexes_selection_cluster(
            brushes_with_selected_vertexes
                .ids
                .iter()
                .filter_set_with_predicate(id, |id| **id)
                .filter_map(|id| {
                    let mut brush =
                        bundle.manager.brush_mut(bundle.drawing_resources, bundle.grid, *id);

                    (!brush.hull().contains_point(vx))
                        .then(|| brush.deselect_vertexes().map(|idxs| (brush.id(), idxs)).unwrap())
                })
        );

        bundle.edits_history.vertexes_selection_cluster(
            bundle
                .manager
                .selected_brushes_mut_at_pos(bundle.drawing_resources, bundle.grid, vx, None)
                .filter_set_with_predicate(id, EntityId::id)
                .filter_map(|mut brush| {
                    brush.try_exclusively_select_vertex(vx).map(|idxs| (brush.id(), idxs))
                })
                .chain(Some((id, idx)))
        );

        true
    }

    /// Toggles the vertexes whose highlight is beneath `cursor_pos`.
    #[inline]
    #[must_use]
    fn toggle_vertexes(
        bundle: &mut ToolUpdateBundle,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> VertexesToggle
    {
        let mut brushes = bundle.manager.selected_brushes_mut_at_pos(
            bundle.drawing_resources,
            bundle.grid,
            cursor_pos,
            camera_scale
        );

        let (vx_pos, selected) = return_if_none!(
            brushes.by_ref().find_map(|mut brush| {
                let (vx_pos, idx, selected) = return_if_none!(
                    brush.toggle_vertex_nearby_cursor_pos(cursor_pos, camera_scale),
                    None
                );

                bundle.edits_history.vertexes_selection(brush.id(), hv_vec![idx]);
                (vx_pos, selected).into()
            }),
            VertexesToggle::None
        );

        bundle
            .edits_history
            .vertexes_selection_cluster(brushes.filter_map(|mut brush| {
                brush
                    .toggle_vertex_at_pos(vx_pos)
                    .map(|idx| (brush.id(), hv_vec![idx]))
            }));

        selected.into()
    }

    /// Moves the selected vertexes by `delta`, if possible. Also selects any non selected vertexes
    /// that overlap the moved ones.
    #[inline]
    fn move_vertexes(
        bundle: &mut ToolUpdateBundle,
        delta: Vec2,
        cumulative_move: &mut HvVec<(Id, HvVec<VertexesMove>)>
    ) -> bool
    {
        // Evaluate if the move is valid for all vertexes/sides.
        let mut move_payloads = hv_vec![];

        let valid = bundle.manager.test_operation_validity(|manager| {
            manager
                .selected_brushes_mut(bundle.drawing_resources, bundle.grid)
                .find_map(|mut brush| {
                    match brush.check_selected_vertexes_move(delta)
                    {
                        VertexesMoveResult::None => (),
                        VertexesMoveResult::Invalid => return brush.id().into(),
                        VertexesMoveResult::Valid(pl) => move_payloads.push(pl)
                    };

                    None
                })
        });

        if !valid
        {
            return false;
        }

        assert!(!move_payloads.is_empty(), "Move payloads is empty.");

        // Since everything went well confirm the move, store the vertexes and ids for
        // the overlap check.
        let mut moved_vertexes = hv_hash_set![];

        for payload in move_payloads
        {
            let id = payload.id();

            {
                let brush = bundle.manager.brush(id);

                for idx in payload.moved_indexes()
                {
                    moved_vertexes.insert(HashVec2(brush.vertex_at_index(idx.into()) + delta));
                }
            }

            let vx_move = bundle
                .manager
                .brush_mut(bundle.drawing_resources, bundle.grid, id)
                .apply_vertexes_move_result(payload);

            let mov = cumulative_move
                .iter_mut()
                .rev()
                .find_map(|(i, mov)| (*i == id).then_some(mov));

            match mov
            {
                Some(mov) =>
                {
                    if !mov.last_mut().unwrap().merge(&vx_move)
                    {
                        mov.push(vx_move);
                    }
                },
                None => cumulative_move.push((id, hv_vec![vx_move]))
            };
        }

        let mut selections = hv_vec![];

        for pos in moved_vertexes
        {
            selections.extend(
                bundle
                    .manager
                    .selected_brushes_mut_at_pos(bundle.drawing_resources, bundle.grid, pos.0, None)
                    .filter_map(|mut brush| {
                        brush.try_select_vertex(pos.0).map(|idx| (brush.id(), hv_vec![idx]))
                    })
            );
        }

        bundle.edits_history.vertexes_selection_cluster(selections);

        true
    }

    /// Deletes the selected vertexes, if possible.
    #[inline]
    fn delete_selected_vertexes(bundle: &mut ToolUpdateBundle)
    {
        let valid = bundle.manager.test_operation_validity(|manager| {
            manager
                .selected_brushes_ids()
                .find(|id| {
                    manager.brush(**id).check_selected_vertexes_deletion() ==
                        VertexesDeletionResult::Invalid
                })
                .copied()
        });

        if !valid
        {
            return;
        }

        for mut brush in bundle
            .manager
            .selected_brushes_mut(bundle.drawing_resources, bundle.grid)
        {
            bundle
                .edits_history
                .vertexes_deletion(brush.id(), continue_if_none!(brush.delete_selected_vertexes()));
        }
    }

    /// Selects the vertexes within `range`.
    #[inline]
    fn select_vertexes_from_drag_selection(bundle: &mut ToolUpdateBundle, range: &Hull)
    {
        let func = if bundle.inputs.shift_pressed()
        {
            Brush::select_vertexes_in_range
        }
        else
        {
            Brush::exclusively_select_vertexes_in_range
        };

        bundle.edits_history.vertexes_selection_cluster(
            bundle
                .manager
                .selected_brushes_mut(bundle.drawing_resources, bundle.grid)
                .filter_map(|mut brush| func(&mut brush, range).map(|vxs| (brush.id(), vxs)))
        );
    }

    /// Deletes the free draw path [`Node`] at `index`.
    #[inline]
    pub fn delete_free_draw_path_node(&mut self, index: usize)
    {
        let path = match_or_panic!(&mut self.0, Status::PolygonToPath(path), path);
        path.remove_index(index, Vec2::ZERO);
    }

    /// Inserts a free draw path [`Node`] at `index`.
    #[inline]
    pub fn insert_free_draw_path_node(&mut self, p: Vec2, index: usize)
    {
        let path = match_or_panic!(&mut self.0, Status::PolygonToPath(path), path);
        path.insert_at_index(p, index, Vec2::ZERO);
    }

    /// Updates the stored selected vertexes info.
    #[inline]
    pub fn update_selected_vertexes(&mut self, manager: &EntitiesManager, identifier: Id)
    {
        if manager.entity_exists(identifier)
        {
            self.1.toggle_brush(manager.brush(identifier));
            return;
        }

        self.1.remove_id(manager, identifier);
    }

    //==============================================================
    // Draw

    /// Draws the tool.
    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle)
    {
        /// Draws the selected and non selected brushes.
        #[inline]
        fn draw_selected_and_non_selected_brushes(bundle: &mut DrawBundle)
        {
            draw_non_selected_brushes(bundle);

            let DrawBundle {
                window,
                drawer,
                camera,
                manager,
                ..
            } = bundle;

            for brush in manager.selected_brushes()
            {
                brush.draw_with_vertex_highlights(
                    window,
                    camera,
                    drawer,
                    &VertexHighlightMode::Vertex
                );
            }
        }

        match &self.0
        {
            Status::Inactive(ds) =>
            {
                draw_selected_and_non_selected_brushes(bundle);
                bundle.drawer.hull(&return_if_none!(ds.hull()), Color::Hull);
            },
            Status::Drag(..) | Status::PreDrag(_) | Status::NewVertexUi =>
            {
                draw_selected_and_non_selected_brushes(bundle);
            },
            Status::NewVertex {
                identifier,
                index,
                vx
            } =>
            {
                draw_non_selected_brushes(bundle);

                let DrawBundle {
                    window,
                    drawer,
                    camera,
                    manager,
                    ..
                } = bundle;

                // Draw the one with the vertex insertion.
                manager.brush(*identifier).draw_with_vertex_highlights(
                    window,
                    camera,
                    drawer,
                    &VertexHighlightMode::NewVertex(*vx, *index)
                );

                for id in manager.selected_brushes_ids().copied().filter_set(*identifier)
                {
                    manager.brush(id).draw_with_vertex_highlights(
                        window,
                        camera,
                        drawer,
                        &VertexHighlightMode::Vertex
                    );
                }
            },
            Status::PolygonToPath(path) =>
            {
                draw_non_selected_brushes(bundle);

                for brush in bundle.manager.selected_brushes()
                {
                    brush.draw_selected(bundle.camera, bundle.drawer);

                    for vx in brush.vertexes()
                    {
                        bundle.drawer.square_highlight(vx, Color::NonSelectedVertex);
                    }
                }

                path.draw(bundle.window, bundle.camera, bundle.drawer, Vec2::ZERO);
            }
        };
    }

    /// Draws the subtools.
    #[inline]
    pub fn draw_subtools(
        &mut self,
        ui: &mut egui::Ui,
        bundle: &mut UiBundle,
        buttons: &mut ToolsButtons
    )
    {
        let insert_clicked = buttons.draw(ui, bundle, SubTool::VertexInsert, &self.0);
        let merge_clicked = buttons.draw(ui, bundle, SubTool::VertexMerge, &self.0);
        let split_clicked = buttons.draw(ui, bundle, SubTool::VertexSplit, &self.0);
        let to_path_clicked = buttons.draw(ui, bundle, SubTool::VertexPolygonToPath, &self.0);

        if merge_clicked
        {
            ActiveTool::merge_vertexes(
                bundle.brushes_default_properties,
                bundle.drawing_resources,
                bundle.manager,
                bundle.edits_history,
                bundle.grid,
                false
            );
            return;
        }

        if split_clicked
        {
            self.1.split_brushes(
                bundle.drawing_resources,
                bundle.manager,
                bundle.edits_history,
                bundle.grid
            );
            return;
        }

        subtools_buttons!(
            self.0,
            (
                insert_clicked,
                Status::NewVertexUi,
                Status::NewVertexUi,
                Status::PolygonToPath(_)
            ),
            (
                to_path_clicked,
                Status::PolygonToPath(PathCreation::None),
                Status::PolygonToPath(_),
                Status::NewVertexUi
            )
        );
    }
}
