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
        ChangeConditions,
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
                editor_state::InputsPresses,
                edits_history::EditsHistory,
                grid::Grid,
                manager::EntitiesManager,
                ui::ToolsButtons
            },
            DrawBundle,
            StateUpdateBundle,
            ToolUpdateBundle
        },
        selectable_vector::VectorSelectionResult
    },
    utils::{
        containers::{hv_hash_map, hv_hash_set, hv_vec, Ids},
        hull::{EntityHull, Hull},
        identifiers::{EntityId, Id},
        iterators::FilterSet,
        math::HashVec2,
        misc::{AssertedInsertRemove, Camera, TakeValue}
    },
    HvHashMap,
    HvVec,
    Path
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The state of the tool.
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
    #[must_use]
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
// TYPES
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
        edits_history: &mut EditsHistory
    )
    {
        if !self.split_available()
        {
            _ = manager.test_operation_validity(|_| self.error_id);
            return;
        }

        for p in self.splittable_ids.values()
        {
            let polygon = {
                let mut brush = manager.brush_mut(p.id());
                edits_history.polygon_edit(brush.id(), brush.polygon());
                brush.split(drawing_resources, p)
            };

            let properties = manager.brush(p.id()).properties();
            self.ids
                .asserted_insert(manager.spawn_brush(polygon, edits_history, properties));
        }

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
    pub fn update(
        &mut self,
        bundle: &mut ToolUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        grid: Grid
    ) -> Option<Path>
    {
        let cursor_pos = Self::cursor_pos(bundle.cursor);

        match &mut self.0
        {
            Status::Inactive(ds) =>
            {
                rect::update!(
                    ds,
                    cursor_pos,
                    bundle.camera.scale(),
                    inputs.left_mouse.pressed(),
                    {
                        if !inputs.left_mouse.just_pressed()
                        {
                            false
                        }
                        else if inputs.alt_pressed()
                        {
                            if let Some(s) = Self::initialize_new_vertex_insertion(
                                manager,
                                cursor_pos,
                                bundle.camera.scale()
                            )
                            {
                                self.0 = s;
                            }

                            return None;
                        }
                        else if inputs.shift_pressed()
                        {
                            match Self::toggle_vertexes(
                                manager,
                                edits_history,
                                cursor_pos,
                                bundle.camera.scale()
                            )
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
                                manager,
                                edits_history,
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
                        deselect_vertexes(manager, edits_history);
                    },
                    hull,
                    {
                        Self::select_vertexes_from_drag_selection(
                            manager,
                            edits_history,
                            &hull,
                            inputs.shift_pressed()
                        );
                    }
                );

                if inputs.enter.just_pressed()
                {
                    if inputs.shift_pressed()
                    {
                        self.0 = Status::PolygonToPath(PathCreation::None);
                    }
                    else
                    {
                        self.1.split_brushes(bundle.drawing_resources, manager, edits_history);
                    }

                    return None;
                }

                if inputs.back.just_pressed()
                {
                    // Vertex deletion.
                    Self::delete_selected_vertexes(bundle, manager, edits_history);
                    return None;
                }

                if inputs.ctrl_pressed()
                {
                    return None;
                }

                // Moving vertex with directional keys.
                let dir = return_if_none!(inputs.directional_keys_vector(grid.size()), None);
                let mut vxs_move = hv_vec![];

                if self.1.selected_vxs.any_selected_vx() &&
                    Self::move_vertexes(bundle, manager, edits_history, dir, &mut vxs_move)
                {
                    edits_history.vertexes_move(vxs_move);
                }
            },
            Status::PreDrag(pos) =>
            {
                if !inputs.left_mouse.pressed()
                {
                    self.0 = Status::Inactive(Rect::default());
                    return None;
                }

                if !bundle.cursor.moved()
                {
                    return None;
                }

                self.0 = Status::Drag(
                    return_if_none!(CursorDelta::try_new(bundle.cursor, grid, *pos), None),
                    hv_vec![]
                );
                edits_history.start_multiframe_edit();
            },
            Status::Drag(drag, cumulative_drag) =>
            {
                if !inputs.left_mouse.pressed()
                {
                    if drag.delta() != Vec2::ZERO
                    {
                        edits_history.vertexes_move(cumulative_drag.take_value());
                    }

                    edits_history.end_multiframe_edit();
                    self.0 = Status::default();
                }
                else if bundle.cursor.moved()
                {
                    drag.conditional_update(bundle.cursor, grid, |delta| {
                        Self::move_vertexes(bundle, manager, edits_history, delta, cumulative_drag)
                    });
                }
            },
            Status::NewVertexUi =>
            {
                if inputs.left_mouse.just_pressed()
                {
                    self.0 = return_if_none!(
                        Self::initialize_new_vertex_insertion(
                            manager,
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
                let mut brush = manager.brush_mut(*identifier);

                if !inputs.left_mouse.pressed()
                {
                    let idx = u8::try_from(*index).unwrap();

                    if brush.try_vertex_insertion_at_index(
                        bundle.drawing_resources,
                        *vx,
                        idx.into(),
                        false
                    )
                    {
                        edits_history.vertex_insertion(&brush, (*vx, idx));
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
                if inputs.enter.just_pressed()
                {
                    let mut path = return_if_none!(path.path(), None);
                    path.translate(-path.node_at_index_pos(0));
                    self.0 = Status::Inactive(Rect::default());
                    return path.into();
                }

                let camera_scale = bundle.camera.scale();

                if inputs.left_mouse.just_pressed()
                {
                    let pos = return_if_none!(
                        manager
                            .selected_brushes_at_pos(cursor_pos, camera_scale)
                            .iter()
                            .find_map(|brush| brush.nearby_vertex(cursor_pos, camera_scale)),
                        None
                    );

                    path.push(edits_history, pos, Vec2::ZERO);
                }
                else if inputs.right_mouse.just_pressed()
                {
                    path.remove(edits_history, cursor_pos, Vec2::ZERO, camera_scale);
                }
            }
        };

        None
    }

    /// Initializes the insertion of a new vertex.
    #[inline]
    fn initialize_new_vertex_insertion(
        manager: &EntitiesManager,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> Option<Status>
    {
        let (id, index) = manager
            .selected_brushes_at_pos(cursor_pos, camera_scale)
            .iter()
            .find_map(|brush| {
                brush
                    .vx_projection_insertion_index(cursor_pos)
                    .map(|idx| (brush.id(), idx))
            })?;

        Some(Status::NewVertex {
            identifier: id,
            index,
            vx: cursor_pos
        })
    }

    /// Exclusively selects the vertexes whose highlight is beneath `cursor_pos`.
    #[inline]
    fn exclusively_select_vertexes(
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        brushes_with_selected_vertexes: &BrushesWithSelectedVertexes,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> bool
    {
        let mut id_vx_id = None;

        for (id, result) in
            manager
                .selected_brushes_mut_at_pos(cursor_pos, camera_scale)
                .map(|mut brush| {
                    (
                        brush.id(),
                        brush.check_vertex_proximity_and_exclusively_select(
                            cursor_pos,
                            camera_scale
                        )
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

        edits_history.vertexes_selection_cluster(
            brushes_with_selected_vertexes
                .ids
                .iter()
                .filter_set_with_predicate(id, |id| **id)
                .filter_map(|id| {
                    let mut brush = manager.brush_mut(*id);

                    (!brush.hull().contains_point(vx))
                        .then(|| brush.deselect_vertexes().map(|idxs| (brush.id(), idxs)).unwrap())
                })
        );

        edits_history.vertexes_selection_cluster(
            manager
                .selected_brushes_mut_at_pos(vx, None)
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
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> VertexesToggle
    {
        let mut brushes = manager.selected_brushes_mut_at_pos(cursor_pos, camera_scale);
        let (vx_pos, selected) = return_if_none!(
            brushes.by_ref().find_map(|mut brush| {
                let (vx_pos, idx, selected) = return_if_none!(
                    brush.toggle_vertex_nearby_cursor_pos(cursor_pos, camera_scale),
                    None
                );

                edits_history.vertexes_selection(brush.id(), hv_vec![idx]);
                (vx_pos, selected).into()
            }),
            VertexesToggle::None
        );

        edits_history.vertexes_selection_cluster(brushes.filter_map(|mut brush| {
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
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        delta: Vec2,
        cumulative_move: &mut HvVec<(Id, HvVec<VertexesMove>)>
    ) -> bool
    {
        // Evaluate if the move is valid for all vertexes/sides.
        let mut move_payloads = hv_vec![];

        let valid = manager.test_operation_validity(|manager| {
            manager.selected_brushes_mut().find_map(|mut brush| {
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
                let brush = manager.brush(id);

                for idx in payload.moved_indexes()
                {
                    moved_vertexes.insert(HashVec2(brush.vertex_at_index(idx.into()) + delta));
                }
            }

            let vx_move = manager
                .brush_mut(id)
                .apply_vertexes_move_result(bundle.drawing_resources, payload);

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
            selections.extend(manager.selected_brushes_mut_at_pos(pos.0, None).filter_map(
                |mut brush| brush.try_select_vertex(pos.0).map(|idx| (brush.id(), hv_vec![idx]))
            ));
        }

        edits_history.vertexes_selection_cluster(selections.into_iter());

        true
    }

    /// Deletes the selected vertexes, if possible.
    #[inline]
    fn delete_selected_vertexes(
        bundle: &mut ToolUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory
    )
    {
        let valid = manager.test_operation_validity(|manager| {
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

        for mut brush in manager.selected_brushes_mut()
        {
            edits_history.vertexes_deletion(
                brush.id(),
                continue_if_none!(brush.delete_selected_vertexes(bundle.drawing_resources))
            );
        }
    }

    /// Selects the vertexes within `range`.
    #[inline]
    fn select_vertexes_from_drag_selection(
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        range: &Hull,
        shift_pressed: bool
    )
    {
        let func = if shift_pressed
        {
            Brush::select_vertexes_in_range
        }
        else
        {
            Brush::exclusively_select_vertexes_in_range
        };

        edits_history.vertexes_selection_cluster(
            manager
                .selected_brushes_mut()
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
    pub fn draw(&self, bundle: &mut DrawBundle, manager: &EntitiesManager, show_tooltips: bool)
    {
        /// Draws the selected and non selected brushes.
        #[inline]
        fn draw_selected_and_non_selected_brushes(
            bundle: &mut DrawBundle,
            manager: &EntitiesManager,
            show_tooltips: bool
        )
        {
            draw_non_selected_brushes(bundle, manager);

            let DrawBundle {
                window,
                egui_context,
                drawer,
                camera,
                ..
            } = bundle;

            for brush in manager.selected_brushes()
            {
                brush.draw_with_vertex_highlights(
                    window,
                    camera,
                    drawer,
                    egui_context,
                    &VertexHighlightMode::Vertex,
                    show_tooltips
                );
            }
        }

        match &self.0
        {
            Status::Inactive(ds) =>
            {
                draw_selected_and_non_selected_brushes(bundle, manager, show_tooltips);
                bundle.drawer.hull(&return_if_none!(ds.hull()), Color::Hull);
            },
            Status::Drag(..) | Status::PreDrag(_) | Status::NewVertexUi =>
            {
                draw_selected_and_non_selected_brushes(bundle, manager, show_tooltips);
            },
            Status::NewVertex {
                identifier,
                index,
                vx
            } =>
            {
                draw_non_selected_brushes(bundle, manager);

                let DrawBundle {
                    window,
                    egui_context,
                    drawer,
                    camera,
                    ..
                } = bundle;

                // Draw the one with the vertex insertion.
                manager.brush(*identifier).draw_with_vertex_highlights(
                    window,
                    camera,
                    drawer,
                    egui_context,
                    &VertexHighlightMode::NewVertex(*vx, *index),
                    show_tooltips
                );

                for id in manager.selected_brushes_ids().copied().filter_set(*identifier)
                {
                    manager.brush(id).draw_with_vertex_highlights(
                        window,
                        camera,
                        drawer,
                        egui_context,
                        &VertexHighlightMode::Vertex,
                        show_tooltips
                    );
                }
            },
            Status::PolygonToPath(path) =>
            {
                draw_non_selected_brushes(bundle, manager);

                let DrawBundle {
                    drawer,
                    camera,
                    window,
                    egui_context,
                    ..
                } = bundle;

                for brush in manager.selected_brushes()
                {
                    brush.draw_selected(camera, drawer);

                    for vx in brush.vertexes()
                    {
                        drawer.square_highlight(vx, Color::NonSelectedVertex);
                    }
                }

                path.draw(window, camera, egui_context, drawer, show_tooltips, Vec2::ZERO);
            }
        };
    }

    /// Draws the subtools.
    #[inline]
    pub fn draw_subtools(
        &mut self,
        ui: &mut egui::Ui,
        bundle: &StateUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        buttons: &mut ToolsButtons,
        tool_change_conditions: &ChangeConditions
    )
    {
        let insert_clicked =
            buttons.draw(ui, bundle, SubTool::VertexInsert, tool_change_conditions, &self.0);
        let merge_clicked =
            buttons.draw(ui, bundle, SubTool::VertexMerge, tool_change_conditions, &self.0);
        let split_clicked =
            buttons.draw(ui, bundle, SubTool::VertexSplit, tool_change_conditions, &self.0);
        let to_path_clicked =
            buttons.draw(ui, bundle, SubTool::VertexPolygonToPath, tool_change_conditions, &self.0);

        if merge_clicked
        {
            ActiveTool::merge_vertexes(
                bundle.default_properties.brushes,
                manager,
                edits_history,
                false
            );
            return;
        }

        if split_clicked
        {
            self.1.split_brushes(bundle.drawing_resources, manager, edits_history);
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
