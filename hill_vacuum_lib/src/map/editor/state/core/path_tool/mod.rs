mod nodes_editor;
pub(in crate::map::editor::state::core) mod path_creation;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::Vec2;
use bevy_egui::egui;
use shared::{match_or_panic, return_if_no_match, return_if_none};

use self::{nodes_editor::NodesEditor, path_creation::PathCreation};
use super::{
    drag::Drag,
    drag_area::{DragArea, DragAreaHighlightedEntity, DragAreaTrait},
    item_selector::{ItemSelector, ItemsBeneathCursor},
    tool::{ActiveTool, ChangeConditions, EnabledTool, SubTool}
};
use crate::{
    map::{
        brush::{
            path::{MovementSimulator, NodeSelectionResult, NodesMove, Path},
            Brush,
            NodesDeletionResult,
            NodesMoveResult
        },
        drawer::color::Color,
        editor::{
            cursor_pos::Cursor,
            state::{
                clipboard::Clipboard,
                core::{
                    drag_area,
                    is_anchored_to_selected_platform,
                    is_moving_brush,
                    tool::subtools_buttons
                },
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
        hv_vec,
        HvVec
    },
    utils::{
        hull::Hull,
        identifiers::{EntityId, Id},
        iterators::FilterSet,
        misc::{Camera, TakeValue, Toggle}
    }
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[derive(Debug)]
enum Status
{
    Inactive(DragAreaHighlightedEntity<ItemBeneathCursor>),
    PreDrag(Vec2, Option<ItemBeneathCursor>),
    Drag(Drag, HvVec<(Id, HvVec<NodesMove>)>),
    SingleEditing(Id, PathEditing),
    PathConnection(Option<Path>, Option<ItemBeneathCursor>),
    Simulation(HvVec<MovementSimulator>, bool),
    FreeDrawUi(Option<Id>),
    AddNodeUi
}

impl Default for Status
{
    #[inline]
    #[must_use]
    fn default() -> Self { Self::Inactive(DragAreaHighlightedEntity::default()) }
}

impl EnabledTool for Status
{
    type Item = SubTool;

    #[inline]
    fn is_tool_enabled(&self, tool: Self::Item) -> bool
    {
        tool == match self
        {
            Status::FreeDrawUi(_) => SubTool::PathFreeDraw,
            Status::AddNodeUi => SubTool::PathAddNode,
            Status::Simulation(..) => SubTool::PathSimulation,
            _ => return false
        }
    }
}

//=======================================================================/

#[derive(Debug)]
enum PathEditing
{
    FreeDraw(PathCreation),
    AddNode
    {
        pos: Vec2,
        idx: u8
    }
}

//=======================================================================//

#[derive(Clone, Copy, Debug, PartialEq)]
enum ItemBeneathCursor
{
    SelectedPlatform(Id),
    PossiblePlatform(Id),
    PathNode(Id, u8)
}

impl EntityId for ItemBeneathCursor
{
    #[inline]
    fn id(&self) -> Id { *self.id_as_ref() }

    #[inline]
    fn id_as_ref(&self) -> &Id
    {
        let (ItemBeneathCursor::SelectedPlatform(id) |
        ItemBeneathCursor::PossiblePlatform(id) |
        ItemBeneathCursor::PathNode(id, _)) = self;
        id
    }
}

//=======================================================================//

#[derive(Debug)]
struct Selector(ItemSelector<ItemBeneathCursor>);

impl Selector
{
    #[inline]
    #[must_use]
    fn new() -> Self { Self(ItemSelector::new(Self::selector)) }

    #[inline]
    fn selector(
        manager: &EntitiesManager,
        cursor_pos: Vec2,
        camera_scale: f32,
        items: &mut ItemsBeneathCursor<ItemBeneathCursor>
    )
    {
        for brush in manager.selected_paths_at_pos(cursor_pos, camera_scale).iter()
        {
            let id = brush.id();

            for (idx, selected) in brush.path_nodes_nearby_cursor_pos(cursor_pos, camera_scale)
            {
                items.push(ItemBeneathCursor::PathNode(id, idx), selected);
            }
        }

        for brush in manager
            .selected_brushes_at_pos(cursor_pos, None)
            .iter()
            .filter(|brush| brush.anchored().is_none() && brush.contains_point(cursor_pos))
        {
            let id = brush.id();

            if manager.is_platform(id)
            {
                items.push(ItemBeneathCursor::SelectedPlatform(id), true);
            }
            else if brush.no_motor_nor_anchored()
            {
                items.push(ItemBeneathCursor::PossiblePlatform(id), false);
            }
        }
    }

    #[inline]
    #[must_use]
    fn item_beneath_cursor(
        &mut self,
        manager: &EntitiesManager,
        cursor: &Cursor,
        camera_scale: f32,
        inputs: &InputsPresses
    ) -> Option<ItemBeneathCursor>
    {
        self.0.item_beneath_cursor(manager, cursor, camera_scale, inputs)
    }
}

//=======================================================================//

#[derive(Debug)]
pub(in crate::map::editor::state::core) struct PathTool
{
    status:       Status,
    nodes_editor: NodesEditor,
    selector:     Selector
}

impl PathTool
{
    #[inline]
    #[must_use]
    fn new(drag_selection: DragArea) -> Self
    {
        PathTool {
            status:       Status::Inactive(drag_selection.into()),
            nodes_editor: NodesEditor::default(),
            selector:     Selector::new()
        }
    }

    #[inline]
    pub fn tool(drag_selection: DragArea) -> ActiveTool
    {
        ActiveTool::Path(Self::new(drag_selection))
    }

    #[inline]
    pub fn path_connection(
        bundle: &ToolUpdateBundle,
        manager: &EntitiesManager,
        inputs: &InputsPresses,
        path: Path
    ) -> ActiveTool
    {
        let mut tool = PathTool::new(DragArea::default());
        let item_beneath_cursor = tool.selector.item_beneath_cursor(
            manager,
            bundle.cursor,
            bundle.camera.scale(),
            inputs
        );
        tool.status = Status::PathConnection(path.into(), item_beneath_cursor);

        ActiveTool::Path(tool)
    }

    //==============================================================
    // Info

    #[inline]
    #[must_use]
    pub const fn ongoing_multi_frame_changes(&self) -> bool
    {
        matches!(
            self.status,
            Status::Drag(..) |
                Status::Simulation(..) |
                Status::SingleEditing(_, PathEditing::AddNode { .. })
        )
    }

    #[inline]
    #[must_use]
    pub const fn copy_paste_available(&self) -> bool
    {
        matches!(
            self.status,
            Status::Inactive(..) | Status::AddNodeUi | Status::FreeDrawUi(_) | Status::PreDrag(..)
        )
    }

    #[inline]
    #[must_use]
    pub fn drag_selection(&self) -> Option<DragArea>
    {
        Some(
            (*return_if_no_match!(
                &self.status,
                Status::Inactive(drag_selection),
                drag_selection,
                None
            ))
            .into()
        )
    }

    #[inline]
    #[must_use]
    pub const fn is_free_draw_active(&self) -> bool
    {
        matches!(self.status, Status::SingleEditing(_, PathEditing::FreeDraw(..)))
    }

    #[inline]
    #[must_use]
    pub const fn simulation_active(&self) -> bool { matches!(self.status, Status::Simulation(..)) }

    #[inline]
    #[must_use]
    const fn cursor_pos(status: &Status, cursor: &Cursor) -> Option<Vec2>
    {
        let value = match status
        {
            Status::PreDrag(..) | Status::Drag(..) | Status::Inactive(_) => cursor.world(),
            Status::SingleEditing(..) | Status::AddNodeUi => cursor.world_snapped(),
            _ => return None
        };

        Some(value)
    }

    #[inline]
    #[must_use]
    fn cursor_color(&self, cursor: &Cursor) -> Option<(Vec2, Color)>
    {
        matches!(self.status, Status::SingleEditing(..))
            .then(|| (Self::cursor_pos(&self.status, cursor).unwrap(), Color::CursorPolygon))
    }

    #[inline]
    #[must_use]
    pub fn path_beneath_cursor(
        &mut self,
        bundle: &StateUpdateBundle,
        manager: &EntitiesManager,
        inputs: &InputsPresses
    ) -> Option<Id>
    {
        self.selector
            .item_beneath_cursor(manager, bundle.cursor, bundle.camera.scale(), inputs)
            .and_then(|item| {
                match item
                {
                    ItemBeneathCursor::SelectedPlatform(id) => id.into(),
                    _ => None
                }
            })
    }

    //==============================================================
    // Update

    #[inline]
    pub fn disable_subtool(&mut self)
    {
        if matches!(
            self.status,
            Status::AddNodeUi |
                Status::FreeDrawUi(_) |
                Status::Simulation(..) |
                Status::SingleEditing(_, PathEditing::FreeDraw(..))
        )
        {
            self.status = Status::default();
        }
    }

    #[inline]
    pub fn update_overall_node(&mut self, manager: &EntitiesManager)
    {
        self.nodes_editor.update_overall_node(manager);
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
        let item_beneath_cursor = self.selector.item_beneath_cursor(
            manager,
            bundle.cursor,
            bundle.camera.scale(),
            inputs
        );
        let cursor_pos = Self::cursor_pos(&self.status, bundle.cursor);

        match &mut self.status
        {
            Status::Inactive(ds) =>
            {
                drag_area::update!(
                    ds,
                    bundle.cursor.world(),
                    inputs.left_mouse.pressed(),
                    {
                        ds.set_highlighted_entity(item_beneath_cursor);

                        if !inputs.left_mouse.just_pressed()
                        {
                            false
                        }
                        else if inputs.alt_pressed()
                        {
                            // See if we should enable node insertion.
                            match return_if_none!(item_beneath_cursor)
                            {
                                ItemBeneathCursor::SelectedPlatform(_) => (),
                                ItemBeneathCursor::PossiblePlatform(id) =>
                                {
                                    self.status = Status::SingleEditing(
                                        id,
                                        PathEditing::FreeDraw(PathCreation::default())
                                    );
                                },
                                ItemBeneathCursor::PathNode(id, idx) =>
                                {
                                    self.status = Self::add_node_status(bundle.cursor, id, idx);
                                }
                            };

                            return;
                        }
                        else if let Some(ItemBeneathCursor::PathNode(id, idx)) =
                            item_beneath_cursor
                        {
                            if inputs.shift_pressed()
                            {
                                if Self::toggle_node(manager, edits_history, id, idx)
                                {
                                    self.status =
                                        Status::PreDrag(cursor_pos.unwrap(), item_beneath_cursor);
                                    return;
                                }
                            }
                            else
                            {
                                Self::select_node(manager, edits_history, id, idx);
                                self.status =
                                    Status::PreDrag(cursor_pos.unwrap(), item_beneath_cursor);
                                return;
                            }

                            false
                        }
                        else
                        {
                            true
                        }
                    },
                    {
                        // Deselect selected nodes.
                        edits_history.path_nodes_selection_cluster(
                            manager.selected_platforms_mut().filter_map(|mut brush| {
                                brush.deselect_path_nodes().map(|idxs| (brush.id(), idxs))
                            })
                        );
                    },
                    hull,
                    {
                        // Select nodes.
                        Self::select_nodes_from_drag_selection(
                            manager,
                            edits_history,
                            &hull,
                            inputs.shift_pressed()
                        );
                    }
                );

                if inputs.enter.just_pressed() && manager.selected_platforms_amount() != 0
                {
                    // Initiate platforms simulation.
                    self.enable_simulation(manager);
                    return;
                }

                if inputs.back.just_pressed()
                {
                    Self::delete(manager, inputs, edits_history);
                    return;
                }

                if inputs.ctrl_pressed()
                {
                    return;
                }

                // Moving vertex with directional keys.
                let dir = return_if_none!(inputs.directional_keys_vector(grid.size()));
                let mut nodes_move = hv_vec![];
                Self::move_nodes(manager, dir, &mut nodes_move);
                edits_history.path_nodes_move(nodes_move);
            },
            Status::PreDrag(pos, hgl_e) =>
            {
                *hgl_e = item_beneath_cursor;

                if !inputs.left_mouse.pressed()
                {
                    self.status = Status::Inactive((*hgl_e).into());
                    return;
                }

                if !bundle.cursor.moved()
                {
                    return;
                }

                self.status = Status::Drag(
                    return_if_none!(Drag::try_new_initiated(*pos, bundle.cursor, grid)),
                    hv_vec![]
                );
                edits_history.start_multiframe_edit();
            },
            Status::Drag(drag, cumulative_drag) =>
            {
                if !inputs.left_mouse.pressed()
                {
                    edits_history.path_nodes_move(cumulative_drag.take_value());
                    edits_history.end_multiframe_edit();
                    self.status = Status::default();
                }
                else if bundle.cursor.moved()
                {
                    drag.conditional_update(bundle.cursor, grid, |delta| {
                        Self::move_nodes(manager, delta, cumulative_drag)
                    });
                }
            },
            status @ Status::SingleEditing(..) =>
            {
                if !Self::single_editing(bundle, manager, status, inputs, edits_history)
                {
                    return;
                }

                self.status = Status::Inactive(
                    self.selector
                        .item_beneath_cursor(manager, bundle.cursor, bundle.camera.scale(), inputs)
                        .into()
                );
            },
            Status::Simulation(simulators, paused) =>
            {
                if inputs.enter.just_pressed()
                {
                    paused.toggle();
                }

                if !*paused
                {
                    for sim in simulators
                    {
                        sim.update(manager.brush(sim.id()), bundle.delta_time);
                    }
                }
            },
            Status::FreeDrawUi(hgl_e) =>
            {
                match item_beneath_cursor
                {
                    Some(ItemBeneathCursor::PossiblePlatform(id)) => *hgl_e = id.into(),
                    _ =>
                    {
                        *hgl_e = None;
                        return;
                    }
                };

                if inputs.left_mouse.just_pressed()
                {
                    self.status = Status::SingleEditing(
                        hgl_e.unwrap(),
                        PathEditing::FreeDraw(PathCreation::default())
                    );
                }
            },
            Status::AddNodeUi =>
            {
                if !inputs.left_mouse.just_pressed()
                {
                    return;
                }

                let (id, idx) = return_if_no_match!(
                    item_beneath_cursor,
                    Some(ItemBeneathCursor::PathNode(id, idx)),
                    (id, idx)
                );

                self.status = Self::add_node_status(bundle.cursor, id, idx);
            },
            Status::PathConnection(path, hgl_e) =>
            {
                if !matches!(
                    item_beneath_cursor,
                    Some(
                        ItemBeneathCursor::PossiblePlatform(_) |
                            ItemBeneathCursor::SelectedPlatform(_)
                    )
                )
                {
                    *hgl_e = None;
                    return;
                }

                let item_beneath_cursor = item_beneath_cursor.unwrap();
                *hgl_e = item_beneath_cursor.into();

                if !inputs.left_mouse.just_pressed()
                {
                    return;
                }

                match item_beneath_cursor
                {
                    ItemBeneathCursor::SelectedPlatform(id) =>
                    {
                        manager.replace_selected_motor(
                            id,
                            edits_history,
                            std::mem::take(path).unwrap()
                        );
                    },
                    ItemBeneathCursor::PossiblePlatform(id) =>
                    {
                        manager.create_motor(id, std::mem::take(path).unwrap(), edits_history);
                    },
                    ItemBeneathCursor::PathNode(..) => unreachable!()
                };

                self.status = Status::Inactive(Some(item_beneath_cursor).into());
            }
        };
    }

    #[inline]
    #[must_use]
    fn add_node_status(cursor: &Cursor, identifier: Id, index: u8) -> Status
    {
        Status::SingleEditing(identifier, PathEditing::AddNode {
            idx: index + 1,
            pos: cursor.world_snapped()
        })
    }

    #[inline]
    pub fn undo_redo_despawn(&mut self, manager: &EntitiesManager, identifier: Id)
    {
        assert!(!manager.is_selected_platform(identifier), "Brush is not a selected platform.");

        match &mut self.status
        {
            Status::Inactive(ds) if ds.has_highlighted_entity() =>
            {
                if ds.highlighted_entity().unwrap().id() == identifier
                {
                    ds.set_highlighted_entity(None);
                }
            },
            Status::SingleEditing(id, _) =>
            {
                if identifier == *id
                {
                    self.status = Status::default();
                }
            },
            _ => ()
        };
    }

    #[inline]
    #[must_use]
    fn toggle_node(
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        identifier: Id,
        index: u8
    ) -> bool
    {
        let selected = manager
            .brush_mut(identifier)
            .toggle_path_node_at_index(index as usize);
        edits_history.path_nodes_selection(identifier, hv_vec![index]);
        selected
    }

    #[inline]
    fn select_node(
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        identifier: Id,
        index: u8
    )
    {
        match manager
            .brush_mut(identifier)
            .exclusively_select_path_node_at_index(index as usize)
        {
            NodeSelectionResult::Selected => (),
            NodeSelectionResult::NotSelected(idxs) =>
            {
                edits_history.path_nodes_selection(identifier, idxs);
            }
        };
    }

    #[inline]
    fn select_nodes_from_drag_selection(
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        range: &Hull,
        shift_pressed: bool
    )
    {
        let func = if shift_pressed
        {
            Brush::select_path_nodes_in_range
        }
        else
        {
            Brush::exclusively_select_path_nodes_in_range
        };

        edits_history.path_nodes_selection_cluster(
            manager
                .selected_paths_intersect_range_mut(range)
                .filter_map(|mut brush| func(&mut brush, range).map(|vxs| (brush.id(), vxs)))
        );
    }

    #[inline]
    fn move_nodes(
        manager: &mut EntitiesManager,
        delta: Vec2,
        cumulative_move: &mut HvVec<(Id, HvVec<NodesMove>)>
    ) -> bool
    {
        let mut move_payloads = hv_vec![];

        for brush in manager.selected_platforms()
        {
            match brush.check_selected_path_nodes_move(delta)
            {
                NodesMoveResult::None => (),
                NodesMoveResult::Invalid => return false,
                NodesMoveResult::Valid(pl) => move_payloads.push(pl)
            };
        }

        assert!(!move_payloads.is_empty(), "Move payloads is empty.");

        for payload in move_payloads
        {
            let id = payload.id();
            let mut brush = manager.brush_mut(id);
            let nodes_move = brush.apply_selected_path_nodes_move(payload);

            let mov = cumulative_move
                .iter_mut()
                .rev()
                .find_map(|(id, mov)| (*id == brush.id()).then_some(mov));

            match mov
            {
                Some(mov) =>
                {
                    if !mov.last_mut().unwrap().merge(&nodes_move)
                    {
                        mov.push(nodes_move);
                    }
                },
                None => cumulative_move.push((id, hv_vec![nodes_move]))
            };
        }

        true
    }

    #[inline]
    #[must_use]
    fn single_editing(
        bundle: &ToolUpdateBundle,
        manager: &mut EntitiesManager,
        status: &mut Status,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory
    ) -> bool
    {
        let cursor_pos = Self::cursor_pos(status, bundle.cursor).unwrap();
        let (id, editing) =
            match_or_panic!(status, Status::SingleEditing(id, editing), (*id, editing));

        match editing
        {
            PathEditing::FreeDraw(path) =>
            {
                if inputs.enter.just_pressed()
                {
                    manager.create_motor(id, return_if_none!(path.path(), false), edits_history);
                    return true;
                }

                if inputs.right_mouse.just_pressed()
                {
                    path.remove(
                        edits_history,
                        cursor_pos,
                        manager.brush(id).center(),
                        bundle.camera.scale()
                    );
                }
                else if inputs.left_mouse.just_pressed()
                {
                    path.push(edits_history, cursor_pos, manager.brush(id).center());
                }
            },
            PathEditing::AddNode { idx, pos } =>
            {
                *pos = cursor_pos;
                let mut brush = manager.brush_mut(id);

                if inputs.left_mouse.pressed()
                {
                    return false;
                }

                if !brush.try_insert_path_node_at_index(*pos, *idx as usize)
                {
                    return true;
                }

                edits_history.path_node_insertion(id, *pos, *idx);
            }
        };

        false
    }

    #[inline]
    fn delete(
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory
    )
    {
        // Delete motors.
        if inputs.alt_pressed()
        {
            manager.remove_selected_motors(edits_history);
            return;
        }

        // Delete all selected nodes if that's fine.
        let mut payloads = hv_vec![];

        for brush in manager.selected_platforms()
        {
            match brush.check_selected_nodes_deletion()
            {
                NodesDeletionResult::None => continue,
                NodesDeletionResult::Invalid => return,
                NodesDeletionResult::Valid(payload) => payloads.push(payload)
            };
        }

        if payloads.is_empty()
        {
            return;
        }

        edits_history.path_nodes_deletion_cluster(
            payloads
                .into_iter()
                .map(|p| (p.id(), manager.brush_mut(p.id()).delete_selected_path_nodes(p)))
        );
    }

    #[inline]
    pub fn delete_free_draw_path_node(&mut self, manager: &EntitiesManager, index: usize)
    {
        let (id, path) = match_or_panic!(
            &mut self.status,
            Status::SingleEditing(id, PathEditing::FreeDraw(path)),
            (*id, path)
        );

        path.remove_index(index, manager.brush(id).center());
    }

    #[inline]
    pub fn insert_free_draw_path_node(&mut self, manager: &EntitiesManager, p: Vec2, index: usize)
    {
        let (id, path) = match_or_panic!(
            &mut self.status,
            Status::SingleEditing(id, PathEditing::FreeDraw(path)),
            (*id, path)
        );

        path.insert_at_index(p, index, manager.brush(id).center());
    }

    #[inline]
    pub fn enable_simulation(&mut self, manager: &EntitiesManager)
    {
        if self.nodes_editor.interacting() || manager.selected_platforms_amount() == 0
        {
            return;
        }

        if matches!(self.status, Status::Simulation(..))
        {
            return;
        }

        self.status = Status::Simulation(manager.movement_simulators(), false);
    }

    //==============================================================
    // Draw

    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle, manager: &EntitiesManager, show_tooltips: bool)
    {
        let DrawBundle {
            window,
            egui_context,
            drawer,
            camera,
            cursor,
            ..
        } = bundle;

        let brushes = manager.brushes();

        macro_rules! draw_brushes {
            ($($filters:expr)?) => {{
                for brush in manager.visible_paths(window, camera).iter()
                    $(.filter_set_with_predicate($filters, |brush| brush.id()))?
                {
                    let id = brush.id();

                    if manager.is_selected_platform(id)
                    {
                        brush.draw_path(
                            window,
                            camera,
                            egui_context,
                            drawer,
                            show_tooltips
                        );
                    }
                    else
                    {
                        brush.draw_semitransparent_path(drawer);
                    }
                }

                for brush in manager.visible_brushes(window, camera).iter()
                    $(.filter_set_with_predicate($filters, |brush| brush.id()))?
                {
                    let id = brush.id();

                    if manager.is_selected_platform(id) || is_anchored_to_selected_platform!(manager, id)
                    {
                        brush.draw_selected(camera, drawer);
                    }
                    else if manager.is_selected(id) && brush.no_motor_nor_anchored()
                    {
                        brush.draw_non_selected(camera, drawer);
                    }
                    else
                    {
                        brush.draw_with_color(camera, drawer, Color::OpaqueBrush);
                    }
                }
            }};
        }

        macro_rules! draw_brushes_with_highlight {
            ($hgl_e:expr) => {
                if $hgl_e.is_none()
                {
                    draw_brushes!();
                }
                else
                {
                    let hgl_e = $hgl_e.unwrap();

                    match hgl_e
                    {
                        ItemBeneathCursor::SelectedPlatform(id) =>
                        {
                            manager.brush(id).draw_highlighted_with_path_nodes(
                                window,
                                camera,
                                egui_context,
                                brushes,
                                drawer,
                                show_tooltips
                            );
                        },
                        ItemBeneathCursor::PossiblePlatform(id) =>
                        {
                            manager.brush(id).draw_highlighted_non_selected(camera, drawer);
                        },
                        ItemBeneathCursor::PathNode(id, idx) =>
                        {
                            manager.brush(id).draw_with_highlighted_path_node(
                                window,
                                camera,
                                egui_context,
                                brushes,
                                drawer,
                                idx as usize,
                                show_tooltips
                            );
                        }
                    };

                    draw_brushes!(hgl_e.id());
                }
            };
        }

        if let Some((pos, color)) = self.cursor_color(cursor)
        {
            drawer.square_highlight(pos, color);
        }

        match &self.status
        {
            Status::Inactive(ds) =>
            {
                draw_brushes_with_highlight!(ds.highlighted_entity());
                drawer.hull(&return_if_none!(ds.hull()), Color::Hull);
            },
            Status::PreDrag(_, hgl_e) =>
            {
                draw_brushes_with_highlight!(*hgl_e);
            },
            Status::Drag(..) | Status::AddNodeUi =>
            {
                draw_brushes!();
            },
            Status::SingleEditing(id, editing) =>
            {
                match editing
                {
                    PathEditing::FreeDraw(path) =>
                    {
                        let brush = manager.brush(*id);
                        brush.draw_highlighted_selected(camera, drawer);
                        path.draw_with_knot(
                            window,
                            camera,
                            egui_context,
                            drawer,
                            show_tooltips,
                            brush.center()
                        );
                    },
                    PathEditing::AddNode { pos, idx } =>
                    {
                        manager.brush(*id).draw_with_path_node_addition(
                            window,
                            camera,
                            egui_context,
                            brushes,
                            drawer,
                            *pos,
                            *idx as usize,
                            show_tooltips
                        );
                    }
                }

                draw_brushes!(*id);
            },
            Status::Simulation(simulators, _) =>
            {
                for simulator in simulators
                {
                    manager.brush(simulator.id()).draw_movement_simulation(
                        window,
                        camera,
                        egui_context,
                        brushes,
                        drawer,
                        show_tooltips,
                        simulator
                    );
                }

                for brush in manager
                    .visible_paths(window, camera)
                    .iter()
                    .filter(|brush| !is_moving_brush!(manager, brush.id()))
                {
                    brush.draw_semitransparent_path(drawer);
                }

                for brush in manager
                    .visible_brushes(window, camera)
                    .iter()
                    .filter(|brush| !is_moving_brush!(manager, brush.id()))
                {
                    brush.draw_with_color(camera, drawer, Color::OpaqueBrush);
                }

                for brush in manager
                    .visible_sprite_highlights(window, camera)
                    .iter()
                    .filter(|brush| !is_moving_brush!(manager, brush.id()))
                {
                    brush.draw_sprite_highlight(drawer);
                }

                for brush in manager
                    .visible_sprites(window, camera)
                    .iter()
                    .filter(|brush| !is_moving_brush!(manager, brush.id()))
                {
                    brush.draw_sprite(drawer, Color::OpaqueBrush);
                }

                let brushes = manager.brushes();

                for brush in manager
                    .visible_anchors(window, camera)
                    .iter()
                    .filter(|brush| !is_moving_brush!(manager, brush.id()))
                {
                    brush.draw_anchors(brushes, drawer);
                }
            },
            Status::FreeDrawUi(hgl_e) =>
            {
                if let Some(hgl_e) = hgl_e
                {
                    manager.brush(*hgl_e).draw_highlighted_non_selected(camera, drawer);
                    draw_brushes!(*hgl_e);
                }
                else
                {
                    draw_brushes!();
                }
            },
            Status::PathConnection(path, hgl_e) =>
            {
                path.as_ref().unwrap().draw(
                    window,
                    camera,
                    egui_context,
                    drawer,
                    cursor.world(),
                    false
                );

                draw_brushes_with_highlight!(hgl_e);
            }
        };
    }

    #[inline]
    #[must_use]
    pub fn ui(
        &mut self,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        clipboard: &mut Clipboard,
        inputs: &InputsPresses,
        ui: &mut egui::Ui
    ) -> bool
    {
        self.nodes_editor.update(
            manager,
            edits_history,
            clipboard,
            inputs,
            ui,
            matches!(self.status, Status::Simulation(..))
        )
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
        subtools_buttons!(
            self.status,
            ui,
            bundle,
            buttons,
            tool_change_conditions,
            (
                PathFreeDraw,
                Status::FreeDrawUi(None),
                Status::FreeDrawUi(_),
                Status::AddNodeUi | Status::Simulation(..)
            ),
            (
                PathAddNode,
                Status::AddNodeUi,
                Status::AddNodeUi,
                Status::FreeDrawUi(_) | Status::Simulation(..)
            )
        );

        if !buttons.draw(ui, bundle, SubTool::PathSimulation, tool_change_conditions, &self.status)
        {
            return;
        }

        match &self.status
        {
            Status::Inactive(..) | Status::FreeDrawUi(_) | Status::AddNodeUi =>
            {
                self.nodes_editor.force_simulation(manager, edits_history);
                self.enable_simulation(manager);
            },
            Status::Simulation(..) => self.status = Status::default(),
            _ => ()
        };
    }
}
