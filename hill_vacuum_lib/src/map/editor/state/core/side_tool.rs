//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::Vec2;
use bevy_egui::egui;
use shared::{match_or_panic, return_if_no_match, return_if_none};

use super::{
    deselect_vertexes,
    drag::Drag,
    drag_area::DragArea,
    draw_non_selected_brushes,
    tool::{subtools_buttons, ChangeConditions, EnabledTool, SubTool},
    ActiveTool,
    VertexesToggle
};
use crate::{
    map::{
        brush::{
            convex_polygon::{
                ConvexPolygon,
                ExtrusionResult,
                SideSelectionResult,
                VertexHighlightMode,
                VertexesMove,
                XtrusionInfo
            },
            Brush,
            SidesDeletionResult,
            VertexesMoveResult,
            XtrusionPayload,
            XtrusionResult
        },
        drawer::color::Color,
        editor::{
            cursor_pos::Cursor,
            state::{
                core::drag_area::{self, DragAreaTrait},
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
        hv_hash_set,
        hv_vec,
        HvVec,
        Ids
    },
    utils::{
        hull::{EntityHull, Hull},
        identifiers::{EntityId, Id},
        iterators::FilterSet,
        math::{lines_and_segments::closest_point_on_line, AroundEqual, HashVec2},
        misc::{Camera, TakeValue}
    }
};

//=======================================================================//
// TYPES
//
//=======================================================================//

#[derive(Debug)]
pub(in crate::map::editor::state::core) struct XTrusionDrag(Vec2);

impl XTrusionDrag
{
    #[inline]
    #[must_use]
    pub const fn new() -> Self { Self(Vec2::ZERO) }

    #[inline]
    pub fn conditional_update<F: FnMut(Vec2) -> bool>(
        &mut self,
        cursor: &Cursor,
        grid: Grid,
        line: &[Vec2; 2],
        mut dragger: F
    )
    {
        let cursor_pos = cursor.world();
        let origin = closest_point_on_line(line[0], line[1], cursor_pos);
        let mut delta = cursor_pos - origin;

        if cursor.snap()
        {
            let length = delta.length();
            let norm_delta = delta / length;

            let length_vec = Vec2::new(length, 0f32);
            let length = grid.square(length_vec).nearest_corner_to_point(length_vec).x;
            delta = norm_delta * length;
        }

        if !delta.around_equal(&self.0) && dragger(delta)
        {
            self.0 = delta;
        }
    }
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[derive(Debug)]
enum XtrusionMode
{
    Xtrusion(HvVec<XtrusionPayload>),
    Intrusion
    {
        payloads:       HvVec<XtrusionPayload>,
        left_polygons:  HvVec<ConvexPolygon>,
        right_polygons: HvVec<ConvexPolygon>
    },
    Extrusion(HvVec<(Id, XtrusionInfo, ConvexPolygon)>)
}

//=======================================================================//

#[derive(Debug)]
enum Status
{
    Inactive(DragArea),
    PreDrag(Vec2),
    Drag(Drag, HvVec<(Id, HvVec<VertexesMove>)>),
    Xtrusion
    {
        mode:   XtrusionMode,
        normal: Vec2,
        line:   [Vec2; 2],
        drag:   XTrusionDrag
    },
    XtrusionUi
}

impl Default for Status
{
    #[inline]
    #[must_use]
    fn default() -> Self { Self::Inactive(DragArea::default()) }
}

impl EnabledTool for Status
{
    type Item = SubTool;

    #[inline]
    fn is_tool_enabled(&self, tool: Self::Item) -> bool
    {
        tool == match self
        {
            Status::XtrusionUi => SubTool::SideXtrusion,
            _ => return false
        }
    }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

#[must_use]
#[derive(Debug)]
struct BrushesWithSelectedSides
{
    ids:               Ids,
    one_selected_side: Ids,
    error_id:          Option<Id>
}

impl BrushesWithSelectedSides
{
    #[inline]
    fn new() -> Self
    {
        Self {
            ids:               hv_hash_set![],
            one_selected_side: hv_hash_set![],
            error_id:          None
        }
    }

    #[inline]
    #[must_use]
    fn xtrusion_available(&self) -> bool
    {
        let len = self.ids.len();
        len != 0 && len == self.one_selected_side.len()
    }

    #[inline]
    fn insert(&mut self, brush: &Brush)
    {
        let id = brush.id();

        match brush.selected_vertexes_amount()
        {
            0 => panic!("Brush does not have selected vertexes."),
            1 => _ = self.one_selected_side.insert(id),
            _ =>
            {
                self.one_selected_side.remove(&id);
                self.error_id = id.into();
            }
        };

        self.ids.insert(id);
    }

    #[inline]
    fn remove(&mut self, brush: &Brush)
    {
        assert!(!brush.has_selected_vertexes(), "Brush still has selected vertexes.");

        let id = brush.id_as_ref();

        if self.ids.remove(id)
        {
            self.one_selected_side.remove(id);
        }
    }

    #[inline]
    fn remove_id(&mut self, manager: &EntitiesManager, identifier: Id)
    {
        assert!(!manager.entity_exists(identifier), "Entity does not exist.");

        if self.ids.remove(&identifier)
        {
            self.one_selected_side.remove(&identifier);
        }
    }

    #[inline]
    fn clear(&mut self)
    {
        self.ids.clear();
        self.one_selected_side.clear();
        self.error_id = None;
    }

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

    #[inline]
    fn initialize_xtrusion(
        &mut self,
        manager: &mut EntitiesManager,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> Option<Status>
    {
        if !self.xtrusion_available()
        {
            if self.error_id.is_some()
            {
                _ = manager.test_operation_validity(|_| self.error_id);
            }

            return None;
        }

        let (xtrusion_side, normal, payload) = manager
            .selected_brushes_at_pos(cursor_pos, camera_scale)
            .iter()
            .find_map(|brush| brush.xtrusion_info(cursor_pos, camera_scale))?;

        let id = payload.id();
        let mut payloads = hv_vec![payload];

        let valid = manager.test_operation_validity(|manager| {
            manager
                .selected_brushes()
                .filter_set_with_predicate(id, |brush| brush.id())
                .map(|brush| (brush.id(), brush.matching_xtrusion_info(normal)))
                .find_map(|(id, result)| {
                    match result
                    {
                        XtrusionResult::None => (),
                        XtrusionResult::Invalid => return id.into(),
                        XtrusionResult::Valid(pl) => payloads.push(pl)
                    };

                    None
                })
        });

        if !valid
        {
            return None;
        }

        Some(Status::Xtrusion {
            mode: XtrusionMode::Xtrusion(payloads),
            normal,
            line: xtrusion_side,
            drag: XTrusionDrag::new()
        })
    }
}

//=======================================================================//

#[derive(Debug)]
pub(in crate::map::editor::state::core) struct SideTool(Status, BrushesWithSelectedSides);

impl SideTool
{
    #[inline]
    pub fn tool(drag_selection: DragArea) -> ActiveTool
    {
        ActiveTool::Side(SideTool(
            Status::Inactive(drag_selection),
            BrushesWithSelectedSides::new()
        ))
    }

    //==============================================================
    // Info

    #[inline]
    #[must_use]
    pub const fn ongoing_multi_frame_changes(&self) -> bool
    {
        !matches!(self.0, Status::Inactive(..) | Status::PreDrag(_) | Status::XtrusionUi)
    }

    #[inline]
    #[must_use]
    pub const fn drag_selection(&self) -> Option<DragArea>
    {
        Some(*return_if_no_match!(
            &self.0,
            Status::Inactive(drag_selection),
            drag_selection,
            None
        ))
    }

    #[inline]
    #[must_use]
    pub const fn intrusion(&self) -> bool
    {
        matches!(self.0, Status::Xtrusion {
            mode: XtrusionMode::Intrusion { .. },
            ..
        })
    }

    #[inline]
    #[must_use]
    fn cursor_pos(cursor: &Cursor) -> Vec2 { cursor.world() }

    #[inline]
    #[must_use]
    pub fn xtrusion_available(&self) -> bool { self.1.xtrusion_available() }

    //==============================================================
    // Update

    #[inline]
    pub fn disable_subtool(&mut self)
    {
        if matches!(self.0, Status::XtrusionUi)
        {
            self.0 = Status::default();
        }
    }

    #[inline]
    pub fn update(
        &mut self,
        bundle: &mut ToolUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        grid: Grid
    )
    {
        let cursor_pos = Self::cursor_pos(bundle.cursor);

        match &mut self.0
        {
            Status::Inactive(ds) =>
            {
                drag_area::update!(
                    ds,
                    cursor_pos,
                    inputs.left_mouse.pressed(),
                    {
                        if !inputs.left_mouse.just_pressed()
                        {
                            false
                        }
                        else if inputs.shift_pressed()
                        {
                            match Self::toggle_sides(
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
                                    return;
                                },
                                VertexesToggle::NonSelected => return
                            }
                        }
                        else
                        {
                            if Self::exclusively_select_sides(
                                manager,
                                edits_history,
                                &self.1,
                                cursor_pos,
                                bundle.camera.scale()
                            )
                            {
                                if inputs.alt_pressed()
                                {
                                    if let Some(s) = self.1.initialize_xtrusion(
                                        manager,
                                        cursor_pos,
                                        bundle.camera.scale()
                                    )
                                    {
                                        self.0 = s;
                                    }

                                    return;
                                }

                                self.0 = Status::PreDrag(cursor_pos);
                                return;
                            }

                            true
                        }
                    },
                    {
                        deselect_vertexes(manager, edits_history);
                    },
                    hull,
                    {
                        Self::select_sides_from_drag_selection(
                            manager,
                            edits_history,
                            &hull,
                            inputs.shift_pressed()
                        );
                    }
                );

                if inputs.back.just_pressed()
                {
                    // Side deletion.
                    Self::delete_selected_sides(bundle, manager, edits_history);
                    return;
                }

                if inputs.ctrl_pressed()
                {
                    return;
                }

                // Moving vertex with directional keys.
                let dir = return_if_none!(inputs.directional_keys_vector(grid.size()));
                let mut vxs_move = hv_vec![];

                if Self::move_sides(bundle, manager, edits_history, dir, &mut vxs_move)
                {
                    edits_history.vertexes_move(vxs_move);
                }
            },
            Status::PreDrag(pos) =>
            {
                if !inputs.left_mouse.pressed()
                {
                    self.0 = Status::Inactive(DragArea::default());
                    return;
                }

                if !bundle.cursor.moved()
                {
                    return;
                }

                self.0 = Status::Drag(
                    return_if_none!(Drag::try_new_initiated(*pos, bundle.cursor, grid)),
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
                        Self::move_sides(bundle, manager, edits_history, delta, cumulative_drag)
                    });
                }
            },
            Status::Xtrusion {
                mode,
                normal,
                line,
                drag
            } =>
            {
                match mode
                {
                    XtrusionMode::Xtrusion(_) =>
                    {
                        if inputs.left_mouse.pressed()
                        {
                            Self::attempt_xtrusion(
                                bundle, manager, mode, line, *normal, drag, grid
                            );
                            return;
                        }

                        self.0 = Status::default();
                    },
                    XtrusionMode::Intrusion {
                        payloads,
                        left_polygons,
                        right_polygons
                    } =>
                    {
                        Self::intrude_sides(
                            bundle,
                            manager,
                            payloads,
                            left_polygons,
                            right_polygons,
                            line,
                            drag,
                            grid
                        );

                        if !inputs.left_mouse.pressed()
                        {
                            self.finalize_intrusion(manager, edits_history);
                        }
                    },
                    XtrusionMode::Extrusion(polygons) =>
                    {
                        Self::extrude_sides(bundle, manager, polygons, line, drag, grid);

                        if !inputs.left_mouse.pressed()
                        {
                            self.finalize_extrusion(manager, edits_history);
                        }
                    }
                }
            },
            Status::XtrusionUi =>
            {
                if !inputs.left_mouse.just_pressed()
                {
                    return;
                }

                self.0 = return_if_none!(self.1.initialize_xtrusion(
                    manager,
                    cursor_pos,
                    bundle.camera.scale()
                ));
            }
        };
    }

    #[inline]
    fn exclusively_select_sides(
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        brushes_with_selected_sides: &BrushesWithSelectedSides,
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
                        brush.check_side_proximity_and_exclusively_select(cursor_pos, camera_scale)
                    )
                })
        {
            match result
            {
                SideSelectionResult::Selected => return true,
                SideSelectionResult::NotSelected(side, idx) =>
                {
                    id_vx_id = (id, side, idx).into();
                    break;
                },
                SideSelectionResult::None => ()
            };
        }

        let (id, side, idx) = return_if_none!(id_vx_id, false);

        edits_history.vertexes_selection_cluster(
            brushes_with_selected_sides
                .ids
                .iter()
                .filter_set_with_predicate(id, |id| **id)
                .filter_map(|id| {
                    let mut brush = manager.brush_mut(*id);
                    (!brush.hull().contains_point(side[0]) || !brush.hull().contains_point(side[1]))
                        .then(|| brush.deselect_vertexes().map(|idxs| (brush.id(), idxs)).unwrap())
                })
        );

        // Stash these right away.
        edits_history.vertexes_selection_cluster(
            manager
                .selected_brushes_mut()
                .filter_set_with_predicate(id, |brush| brush.id())
                .filter_map(|mut brush| {
                    brush
                        .try_exclusively_select_side(&side)
                        .map(|idxs| (brush.id(), idxs))
                })
                .chain(Some((id, idx)))
        );

        true
    }

    #[inline]
    #[must_use]
    fn toggle_sides(
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> VertexesToggle
    {
        let mut brushes = manager.selected_brushes_mut_at_pos(cursor_pos, camera_scale);
        let (side, selected) = return_if_none!(
            brushes.by_ref().find_map(|mut brush| {
                let (side, idx, selected) = return_if_none!(
                    brush.toggle_side_nearby_cursor_pos(cursor_pos, camera_scale),
                    None
                );

                edits_history.vertexes_selection(brush.id(), hv_vec![idx]);
                (side, selected).into()
            }),
            VertexesToggle::None
        );

        edits_history.vertexes_selection_cluster(brushes.filter_map(|mut brush| {
            brush.toggle_side(&side).map(|idx| (brush.id(), hv_vec![idx]))
        }));

        selected.into()
    }

    #[inline]
    fn move_sides(
        bundle: &ToolUpdateBundle,
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
                match brush.check_selected_sides_move(delta)
                {
                    VertexesMoveResult::None => (),
                    VertexesMoveResult::Invalid => return brush.id().into(),
                    VertexesMoveResult::Valid(pl) => move_payloads.push(pl)
                }

                None
            })
        });

        if !valid
        {
            return false;
        }

        assert!(!move_payloads.is_empty(), "No move payloads.");

        // Since everything went well confirm the move, store the vertexes and ids for
        // the overlap check.
        let mut moved_sides = hv_hash_set![];

        for payload in move_payloads
        {
            let id = payload.id();

            for [j, i] in payload.paired_moved_indexes().unwrap()
            {
                let brush = manager.brush(id);
                let vx_j = HashVec2(brush.vertex_at_index((*j).into()) + delta);
                let vx_i = HashVec2(brush.vertex_at_index((*i).into()) + delta);

                if moved_sides.contains(&(vx_j, vx_i))
                {
                    continue;
                }

                moved_sides.insert((vx_j, vx_i));

                for vx in [vx_j.0, vx_i.0]
                {
                    edits_history.vertexes_selection_cluster(
                        manager
                            .selected_brushes_mut_at_pos(vx, None)
                            .filter_set_with_predicate(id, |brush| brush.id())
                            .filter_map(|mut brush| {
                                brush
                                    .try_select_side(&[vx_j.0, vx_i.0])
                                    .map(|idx| (brush.id(), hv_vec![idx]))
                            })
                    );
                }
            }

            let mut brush = manager.brush_mut(id);
            let vx_move = brush.apply_vertexes_move_result(bundle.drawing_resources, payload);

            let mov = cumulative_move
                .iter_mut()
                .rev()
                .find_map(|(id, mov)| (*id == brush.id()).then_some(mov));

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

        true
    }

    #[inline]
    fn delete_selected_sides(
        bundle: &ToolUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory
    )
    {
        let mut payloads = hv_vec![];
        let valid = manager.test_operation_validity(|manager| {
            manager.selected_brushes().find_map(|brush| {
                match brush.check_selected_sides_deletion()
                {
                    SidesDeletionResult::None => (),
                    SidesDeletionResult::Invalid => return brush.id().into(),
                    SidesDeletionResult::Valid(p) => payloads.push(p)
                }

                None
            })
        });

        if !valid
        {
            return;
        }

        edits_history.sides_deletion_cluster(payloads.into_iter().map(|p| {
            let mut brush = manager.brush_mut(p.id());
            (brush.id(), brush.delete_selected_sides(bundle.drawing_resources, p))
        }));
    }

    #[inline]
    fn select_sides_from_drag_selection(
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        range: &Hull,
        shift_pressed: bool
    )
    {
        let func = if shift_pressed
        {
            Brush::select_sides_in_range
        }
        else
        {
            Brush::exclusively_select_sides_in_range
        };

        edits_history.vertexes_selection_cluster(
            manager
                .selected_brushes_intersect_range_mut(range)
                .filter_map(|mut brush| func(&mut brush, range).map(|idxs| (brush.id(), idxs)))
        );
    }

    #[inline]
    #[must_use]
    fn intrusion_polygons(
        bundle: &ToolUpdateBundle,
        manager: &mut EntitiesManager,
        payloads: &HvVec<XtrusionPayload>,
        delta: Vec2
    ) -> Option<(HvVec<ConvexPolygon>, HvVec<ConvexPolygon>)>
    {
        let mut left_polygons = hv_vec![capacity; payloads.len()];
        let mut right_polygons = hv_vec![capacity; payloads.len()];

        let valid = manager.test_operation_validity(|manager| {
            payloads.iter().find_map(|payload| {
                let id = payload.id();

                match payload.info().clip_polygon_at_intrusion_side(
                    bundle.drawing_resources,
                    manager.brush(id),
                    delta
                )
                {
                    None => id.into(),
                    Some([left, mut right]) =>
                    {
                        right.deselect_vertexes_no_indexes();
                        left_polygons.push(left);
                        right_polygons.push(right);
                        None
                    }
                }
            })
        });

        if !valid
        {
            return None;
        }

        Some((left_polygons, right_polygons))
    }

    #[inline]
    fn attempt_xtrusion(
        bundle: &ToolUpdateBundle,
        manager: &mut EntitiesManager,
        mode: &mut XtrusionMode,
        line: &[Vec2; 2],
        normal: Vec2,
        drag: &mut XTrusionDrag,
        grid: Grid
    )
    {
        drag.conditional_update(bundle.cursor, grid, line, |delta| {
            let XtrusionMode::Xtrusion(payloads) = mode
            else
            {
                panic!("Invalid side tool state for an xtrusion.")
            };
            let delta_against_normal = Self::xtrusion_delta_against_normal(normal, delta);

            let new_mode = {
                if delta_against_normal
                {
                    // Intrusion.
                    let (left_polygons, right_polygons) = return_if_none!(
                        Self::intrusion_polygons(bundle, manager, payloads, delta),
                        false
                    );

                    // Update the edits history.
                    XtrusionMode::Intrusion {
                        payloads: payloads.take_value(),
                        left_polygons,
                        right_polygons
                    }
                }
                else
                {
                    // Extrusion.
                    let mut polys = hv_vec![capacity; payloads.len()];

                    let valid = manager.test_operation_validity(|manager| {
                        // Generate the extrusion polygons.
                        payloads.take_value().into_iter().find_map(|payload| {
                            match payload.info().create_extrusion_polygon(
                                delta,
                                manager.brush(payload.id()).texture_settings()
                            )
                            {
                                Some(poly) =>
                                {
                                    polys.push((payload.id(), *payload.info(), poly));
                                    None
                                },
                                None => payload.id().into()
                            }
                        })
                    });

                    if !valid
                    {
                        return false;
                    }

                    XtrusionMode::Extrusion(polys)
                }
            };

            *mode = new_mode;
            true
        });
    }

    #[inline]
    fn xtrusion_delta_against_normal(normal: Vec2, delta: Vec2) -> bool { delta.dot(normal) < 0f32 }

    #[inline]
    fn intrude_sides(
        bundle: &ToolUpdateBundle,
        manager: &mut EntitiesManager,
        payloads: &HvVec<XtrusionPayload>,
        left_polygons: &mut HvVec<ConvexPolygon>,
        right_polygons: &mut HvVec<ConvexPolygon>,
        line: &[Vec2; 2],
        drag: &mut XTrusionDrag,
        grid: Grid
    )
    {
        drag.conditional_update(bundle.cursor, grid, line, |delta| {
            let (l_polys, r_polys) =
                return_if_none!(Self::intrusion_polygons(bundle, manager, payloads, delta), false);

            *left_polygons = l_polys;
            *right_polygons = r_polys;
            true
        });
    }

    #[inline]
    fn extrude_sides(
        bundle: &ToolUpdateBundle,
        manager: &mut EntitiesManager,
        polygons: &mut HvVec<(Id, XtrusionInfo, ConvexPolygon)>,
        line: &[Vec2; 2],
        drag: &mut XTrusionDrag,
        grid: Grid
    )
    {
        drag.conditional_update(bundle.cursor, grid, line, |delta| {
            let mut extruded_sides = Vec::with_capacity(polygons.len());

            let valid = manager.test_operation_validity(|_| {
                polygons.iter().find_map(|(id, info, poly)| {
                    match info.check_side_extrusion(poly, delta)
                    {
                        ExtrusionResult::Invalid => (*id).into(),
                        ExtrusionResult::Valid(pl) =>
                        {
                            extruded_sides.push(pl);
                            None
                        }
                    }
                })
            });

            if !valid
            {
                return false;
            }

            for (pl, poly) in extruded_sides
                .iter()
                .zip(polygons.iter_mut().map(|(_, _, poly)| poly))
            {
                XtrusionInfo::extrude_side(pl, poly);
            }

            true
        });
    }

    #[inline]
    fn finalize_intrusion(
        &mut self,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory
    )
    {
        let (payloads, left_polygons, right_polygons) = match_or_panic!(
            &mut self.0,
            Status::Xtrusion {
                mode: XtrusionMode::Intrusion {
                    payloads,
                    left_polygons,
                    right_polygons
                },
                ..
            },
            (payloads, left_polygons, right_polygons)
        );

        manager.spawn_brushes(
            left_polygons
                .take_value()
                .into_iter()
                .chain(right_polygons.take_value()),
            edits_history
        );

        for payload in payloads
        {
            manager.despawn_brush(payload.id(), edits_history, true);
        }

        self.0 = Status::default();
        self.1.clear();
    }

    #[inline]
    fn finalize_extrusion(
        &mut self,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory
    )
    {
        let polygons = match_or_panic!(
            &mut self.0,
            Status::Xtrusion {
                mode: XtrusionMode::Extrusion(polygons),
                ..
            },
            polygons
        );

        edits_history.vertexes_selection_cluster(
            manager
                .selected_brushes_mut()
                .filter_map(|mut brush| brush.deselect_vertexes().map(|idxs| (brush.id(), idxs)))
        );

        manager.deselect_selected_entities(edits_history);
        manager
            .spawn_brushes(polygons.take_value().into_iter().map(|(_, _, cp)| cp), edits_history);

        self.0 = Status::default();
        self.1.clear();
    }

    #[inline]
    pub fn update_selected_sides(&mut self, manager: &EntitiesManager, identifier: Id)
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

    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle, manager: &EntitiesManager, show_tooltips: bool)
    {
        #[inline]
        fn draw_selected_brushes(
            bundle: &mut DrawBundle,
            manager: &EntitiesManager,
            show_tooltips: bool
        )
        {
            for brush in manager.selected_brushes()
            {
                brush.draw_with_vertex_highlights(
                    bundle.window,
                    bundle.camera,
                    &mut bundle.drawer,
                    bundle.egui_context,
                    &VertexHighlightMode::Side,
                    show_tooltips
                );
            }
        }

        draw_non_selected_brushes(bundle, manager);

        match &self.0
        {
            Status::Inactive(ds) =>
            {
                draw_selected_brushes(bundle, manager, show_tooltips);

                if let Some(hull) = ds.hull()
                {
                    bundle.drawer.hull(&hull, Color::Hull);
                }
            },
            Status::Drag(..) | Status::PreDrag(_) | Status::XtrusionUi =>
            {
                draw_selected_brushes(bundle, manager, show_tooltips);
            },
            Status::Xtrusion { mode, .. } =>
            {
                match mode
                {
                    XtrusionMode::Xtrusion(_) =>
                    {
                        draw_selected_brushes(bundle, manager, show_tooltips);
                    },
                    XtrusionMode::Intrusion {
                        payloads,
                        left_polygons,
                        right_polygons
                    } =>
                    {
                        for brush in manager.selected_brushes()
                        {
                            let id = brush.id();

                            if payloads.iter().any(|p| p.id() == id)
                            {
                                continue;
                            }

                            brush.draw_with_vertex_highlights(
                                bundle.window,
                                bundle.camera,
                                &mut bundle.drawer,
                                bundle.egui_context,
                                &VertexHighlightMode::Side,
                                show_tooltips
                            );
                        }

                        for cp in left_polygons.iter().chain(right_polygons.iter())
                        {
                            cp.draw(bundle.camera, &mut bundle.drawer, Color::SelectedBrush);
                        }
                    },
                    XtrusionMode::Extrusion(polygons) =>
                    {
                        draw_selected_brushes(bundle, manager, show_tooltips);

                        for (_, _, cp) in polygons
                        {
                            cp.draw(bundle.camera, &mut bundle.drawer, Color::SelectedBrush);
                        }
                    }
                }
            },
        };
    }

    #[inline]
    pub fn draw_sub_tools(
        &mut self,
        ui: &mut egui::Ui,
        bundle: &StateUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        buttons: &mut ToolsButtons,
        tool_change_conditions: &ChangeConditions
    )
    {
        let extrusion_clicked =
            buttons.draw(ui, bundle, SubTool::SideXtrusion, tool_change_conditions, &self.0);
        let merge_clicked =
            buttons.draw(ui, bundle, SubTool::SideMerge, tool_change_conditions, &self.0);

        if merge_clicked
        {
            ActiveTool::merge_vertexes(manager, edits_history, true);
            return;
        }

        subtools_buttons!(self.0, (extrusion_clicked, Status::XtrusionUi, Status::XtrusionUi));
    }
}
