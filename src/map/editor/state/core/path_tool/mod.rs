mod nodes_editor;
pub(in crate::map::editor::state::core) mod path_creation;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use glam::Vec2;
use hill_vacuum_shared::{match_or_panic, return_if_no_match, return_if_none};

use self::{nodes_editor::NodesEditor, path_creation::PathCreation};
use super::{
    cursor_delta::CursorDelta,
    item_selector::{ItemSelector, ItemsBeneathCursor},
    rect::{Rect, RectHighlightedEntity, RectTrait},
    tool::{
        ActiveTool,
        DisableSubtool,
        DragSelection,
        EnabledTool,
        OngoingMultiframeChange,
        SubTool
    }
};
use crate::{
    map::{
        drawer::{color::Color, drawing_resources::DrawingResources},
        editor::{
            cursor::Cursor,
            state::{
                core::{rect, tool::subtools_buttons},
                grid::Grid,
                inputs_presses::InputsPresses,
                manager::EntitiesManager,
                ui::{ToolsButtons, UiBundle}
            },
            DrawBundle,
            StateUpdateBundle,
            ToolUpdateBundle
        },
        hv_vec,
        path::{
            EditPath,
            IdNodesDeletionResult,
            IdNodesMoveResult,
            MovementSimulator,
            Moving,
            NodeSelectionResult,
            NodesMove,
            Path
        }
    },
    utils::{
        hull::Hull,
        identifiers::{EntityCenter, EntityId, Id},
        iterators::FilterSet,
        misc::{Camera, TakeValue, Toggle}
    },
    HvVec
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
    Inactive(RectHighlightedEntity<ItemBeneathCursor>),
    /// Preparing for dragging [`Node`]s.
    PreDrag(Vec2, Option<ItemBeneathCursor>),
    /// Dragging [`Node`]s.
    Drag(CursorDelta, HvVec<(Id, HvVec<NodesMove>)>),
    /// Editing an existing [`Path`].
    SingleEditing(Id, PathEditing),
    /// Attaching a [`Path`] to an entity.
    PathConnection(Option<Path>, Option<ItemBeneathCursor>),
    /// Simulating the entity movement.
    Simulation(HvVec<MovementSimulator>, bool),
    /// Starting a [`Path`] free draw from the UI.
    FreeDrawUi(Option<Id>),
    /// Starting a [`Node`] insertion from the UI.
    InsertNodeUi(Option<ItemBeneathCursor>)
}

impl Default for Status
{
    #[inline]
    #[must_use]
    fn default() -> Self { Self::Inactive(RectHighlightedEntity::default()) }
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
            Status::InsertNodeUi(_) => SubTool::PathInsertNode,
            Status::Simulation(..) => SubTool::PathSimulation,
            _ => return false
        }
    }
}

//=======================================================================/

/// The edits that can be done to a [`Path`].
#[derive(Debug)]
enum PathEditing
{
    /// Creating a new [`Path`].
    FreeDraw(PathCreation),
    /// Adding a [`Node`] to a [`Path`].
    InsertNode
    {
        /// The position of the [`Node`].
        pos:   Vec2,
        /// The index where the [`Node`] is inserted in the [`Path`].
        index: u8
    }
}

//=======================================================================//

/// The items that can be selected.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ItemBeneathCursor
{
    /// A selected moving entity.
    SelectedMoving(Id),
    /// An entity that could have a [`Path`].
    PossibleMoving(Id),
    /// A [`Path`] [`Node`].
    PathNode(Id, u8)
}

impl EntityId for ItemBeneathCursor
{
    #[inline]
    fn id(&self) -> Id { *self.id_as_ref() }

    #[inline]
    fn id_as_ref(&self) -> &Id
    {
        let (Self::SelectedMoving(id) | Self::PossibleMoving(id) | Self::PathNode(id, _)) = self;
        id
    }
}

//=======================================================================//

/// The items selector.
#[derive(Debug)]
struct Selector(ItemSelector<ItemBeneathCursor>);

impl Selector
{
    /// Returns a new [`Selector`].
    #[inline]
    #[must_use]
    fn new() -> Self
    {
        /// The selector function.
        #[inline]
        fn selector(
            _: &DrawingResources,
            manager: &EntitiesManager,
            cursor: &Cursor,
            _: &Grid,
            camera_scale: f32,
            items: &mut ItemsBeneathCursor<ItemBeneathCursor>
        )
        {
            let cursor_pos = cursor.world();

            for entity in manager.selected_movings_at_pos(cursor_pos, camera_scale).iter()
            {
                let id = entity.id();

                for (idx, selected) in entity.path_nodes_nearby_cursor_pos(cursor_pos, camera_scale)
                {
                    items.push(ItemBeneathCursor::PathNode(id, idx), selected);
                }
            }

            for brush in manager
                .selected_brushes_at_pos(cursor_pos, None)
                .iter()
                .filter(|brush| brush.attached().is_none() && brush.contains_point(cursor_pos))
            {
                let id = brush.id();

                if brush.has_path()
                {
                    items.push(ItemBeneathCursor::SelectedMoving(id), true);
                }
                else
                {
                    items.push(ItemBeneathCursor::PossibleMoving(id), false);
                }
            }

            for thing in manager
                .selected_things_at_pos(cursor_pos, None)
                .iter()
                .filter(|thing| thing.contains_point(cursor_pos))
            {
                let id = thing.id();

                if thing.has_path()
                {
                    items.push(ItemBeneathCursor::SelectedMoving(id), true);
                }
                else
                {
                    items.push(ItemBeneathCursor::PossibleMoving(id), false);
                }
            }
        }

        Self(ItemSelector::new(selector))
    }

    /// Returns the item beneth the cursor.
    #[inline]
    #[must_use]
    fn item_beneath_cursor(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &EntitiesManager,
        cursor: &Cursor,
        grid: &Grid,
        camera_scale: f32,
        inputs: &InputsPresses
    ) -> Option<ItemBeneathCursor>
    {
        self.0
            .item_beneath_cursor(drawing_resources, manager, cursor, grid, camera_scale, inputs)
    }
}

//=======================================================================//

/// The path tool.
#[derive(Debug)]
pub(in crate::map::editor::state::core) struct PathTool
{
    /// The state of the tool.
    status:       Status,
    /// The [`Node`]s parameters editor.
    nodes_editor: NodesEditor,
    /// The items selector.
    selector:     Selector
}

impl DisableSubtool for PathTool
{
    #[inline]
    fn disable_subtool(&mut self)
    {
        if matches!(
            self.status,
            Status::InsertNodeUi(_) |
                Status::FreeDrawUi(_) |
                Status::Simulation(..) |
                Status::SingleEditing(_, PathEditing::FreeDraw(..))
        )
        {
            self.status = Status::default();
        }
    }
}

impl OngoingMultiframeChange for PathTool
{
    #[inline]
    fn ongoing_multi_frame_change(&self) -> bool
    {
        matches!(
            self.status,
            Status::Drag(..) |
                Status::Simulation(..) |
                Status::SingleEditing(_, PathEditing::InsertNode { .. })
        )
    }
}

impl DragSelection for PathTool
{
    #[inline]
    fn drag_selection(&self) -> Option<Rect>
    {
        Rect::from(*return_if_no_match!(
            &self.status,
            Status::Inactive(drag_selection),
            drag_selection,
            None
        ))
        .into()
    }
}

impl PathTool
{
    /// Returns a new [`PathTool`].
    #[inline]
    #[must_use]
    fn new(drag_selection: Rect) -> Self
    {
        PathTool {
            status:       Status::Inactive(drag_selection.into()),
            nodes_editor: NodesEditor::default(),
            selector:     Selector::new()
        }
    }

    /// Returns an [`ActiveTool`] in its path tool variant.
    #[inline]
    pub fn tool(drag_selection: Rect) -> ActiveTool { ActiveTool::Path(Self::new(drag_selection)) }

    /// Returns an [`ActiveTool`] in its path tool variant in its [`Path`] attachment state.
    #[inline]
    pub fn path_connection(bundle: &ToolUpdateBundle, path: Path) -> ActiveTool
    {
        let mut tool = PathTool::new(Rect::default());
        let item_beneath_cursor = tool.selector.item_beneath_cursor(
            bundle.drawing_resources,
            bundle.manager,
            bundle.cursor,
            bundle.grid,
            bundle.camera.scale(),
            bundle.inputs
        );
        tool.status = Status::PathConnection(path.into(), item_beneath_cursor);

        ActiveTool::Path(tool)
    }

    //==============================================================
    // Info

    /// Whether copy/paste is available.
    #[inline]
    #[must_use]
    pub const fn copy_paste_available(&self) -> bool
    {
        matches!(
            self.status,
            Status::Inactive(..) | Status::InsertNodeUi(_) | Status::FreeDrawUi(_)
        )
    }

    /// Whether free draw is active.
    #[inline]
    #[must_use]
    pub const fn is_free_draw_active(&self) -> bool
    {
        matches!(self.status, Status::SingleEditing(_, PathEditing::FreeDraw(..)))
    }

    /// Whether the movement simulation is active.
    #[inline]
    #[must_use]
    pub const fn simulation_active(&self) -> bool { matches!(self.status, Status::Simulation(..)) }

    /// The cursor position to be used by the tool, if any.
    #[inline]
    #[must_use]
    const fn cursor_pos(status: &Status, cursor: &Cursor) -> Option<Vec2>
    {
        let value = match status
        {
            Status::PreDrag(..) | Status::Drag(..) | Status::Inactive(_) => cursor.world(),
            Status::SingleEditing(..) | Status::InsertNodeUi(_) => cursor.world_snapped(),
            _ => return None
        };

        Some(value)
    }

    /// Returns the [`Id`] of the selected moving entity beneath the cursor, if any.
    #[inline]
    #[must_use]
    pub fn selected_moving_beneath_cursor(&mut self, bundle: &StateUpdateBundle) -> Option<Id>
    {
        self.selector
            .item_beneath_cursor(
                bundle.drawing_resources,
                bundle.manager,
                bundle.cursor,
                bundle.grid,
                bundle.camera.scale(),
                bundle.inputs
            )
            .and_then(|item| {
                match item
                {
                    ItemBeneathCursor::SelectedMoving(id) => id.into(),
                    _ => None
                }
            })
    }

    /// Returns the [`Id`] of the entity that can have a [`Path`] beneath the cursor, if any.
    #[inline]
    #[must_use]
    pub fn possible_moving_beneath_cursor(&mut self, bundle: &StateUpdateBundle) -> Option<Id>
    {
        self.selector
            .item_beneath_cursor(
                bundle.drawing_resources,
                bundle.manager,
                bundle.cursor,
                bundle.grid,
                bundle.camera.scale(),
                bundle.inputs
            )
            .and_then(|item| {
                match item
                {
                    ItemBeneathCursor::PossibleMoving(id) => id.into(),
                    _ => None
                }
            })
    }

    //==============================================================
    // Update

    /// Updates the overall [`Node`] info.
    #[inline]
    pub fn update_overall_node(&mut self, manager: &EntitiesManager)
    {
        self.nodes_editor.update_overall_node(manager);
    }

    /// Updates the tool.
    #[inline]
    pub fn update(&mut self, bundle: &mut ToolUpdateBundle)
    {
        let item_beneath_cursor = self.selector.item_beneath_cursor(
            bundle.drawing_resources,
            bundle.manager,
            bundle.cursor,
            bundle.grid,
            bundle.camera.scale(),
            bundle.inputs
        );
        let cursor_pos = Self::cursor_pos(&self.status, bundle.cursor);

        match &mut self.status
        {
            Status::Inactive(ds) =>
            {
                rect::update!(
                    ds,
                    bundle.cursor.world(),
                    bundle.camera.scale(),
                    bundle.inputs.left_mouse.pressed(),
                    {
                        ds.set_highlighted_entity(item_beneath_cursor);

                        if !bundle.inputs.left_mouse.just_pressed()
                        {
                            false
                        }
                        else if bundle.inputs.alt_pressed()
                        {
                            // See if we should enable node insertion.
                            match return_if_none!(item_beneath_cursor)
                            {
                                ItemBeneathCursor::SelectedMoving(_) => (),
                                ItemBeneathCursor::PossibleMoving(id) =>
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
                            if bundle.inputs.shift_pressed()
                            {
                                if Self::toggle_node(bundle, id, idx)
                                {
                                    self.status =
                                        Status::PreDrag(cursor_pos.unwrap(), item_beneath_cursor);
                                    return;
                                }
                            }
                            else
                            {
                                Self::select_node(bundle, id, idx);
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
                        if bundle.edits_history.path_nodes_selection_cluster(
                            bundle
                                .manager
                                .selected_movings_mut(bundle.drawing_resources, bundle.grid)
                                .filter_map(|mut entity| {
                                    entity.deselect_path_nodes().map(|idxs| (entity.id(), idxs))
                                })
                        )
                        {
                            bundle.manager.schedule_overall_node_update();
                        }
                    },
                    hull,
                    {
                        // Select nodes.
                        Self::select_nodes_from_drag_selection(bundle, &hull);
                    }
                );

                if bundle.inputs.enter.just_pressed() &&
                    bundle.manager.selected_moving_amount() != 0
                {
                    // Initiate paths simulation.
                    self.enable_simulation(bundle.manager);
                    return;
                }

                if bundle.inputs.back.just_pressed()
                {
                    if Self::delete(bundle)
                    {
                        ds.set_highlighted_entity(self.selector.item_beneath_cursor(
                            bundle.drawing_resources,
                            bundle.manager,
                            bundle.cursor,
                            bundle.grid,
                            bundle.camera.scale(),
                            bundle.inputs
                        ));
                    }

                    return;
                }

                if bundle.inputs.ctrl_pressed()
                {
                    return;
                }

                // Moving vertex with directional keys.
                let dir =
                    return_if_none!(bundle.inputs.directional_keys_vector(bundle.grid.size()));
                let mut nodes_move = hv_vec![];
                Self::move_nodes(bundle, dir, &mut nodes_move);
                bundle.edits_history.path_nodes_move(nodes_move);
            },
            Status::PreDrag(pos, hgl_e) =>
            {
                *hgl_e = item_beneath_cursor;

                if !bundle.inputs.left_mouse.pressed()
                {
                    self.status = Status::Inactive((*hgl_e).into());
                    return;
                }

                if !bundle.cursor.moved()
                {
                    return;
                }

                self.status = Status::Drag(
                    return_if_none!(CursorDelta::try_new(bundle.cursor, bundle.grid, *pos)),
                    hv_vec![]
                );
                bundle.edits_history.start_multiframe_edit();
            },
            Status::Drag(drag, cumulative_drag) =>
            {
                if !bundle.inputs.left_mouse.pressed()
                {
                    bundle.edits_history.path_nodes_move(cumulative_drag.take_value());
                    bundle.edits_history.end_multiframe_edit();
                    self.status = Status::default();
                }
                else if bundle.cursor.moved()
                {
                    drag.conditional_update(bundle.cursor, bundle.grid, |delta| {
                        Self::move_nodes(bundle, delta, cumulative_drag)
                    });
                }
            },
            status @ Status::SingleEditing(..) =>
            {
                if !Self::single_editing(bundle, status)
                {
                    return;
                }

                self.status = Status::Inactive(
                    self.selector
                        .item_beneath_cursor(
                            bundle.drawing_resources,
                            bundle.manager,
                            bundle.cursor,
                            bundle.grid,
                            bundle.camera.scale(),
                            bundle.inputs
                        )
                        .into()
                );
            },
            Status::Simulation(simulators, paused) =>
            {
                if bundle.inputs.enter.just_pressed()
                {
                    paused.toggle();
                }

                if !*paused
                {
                    for sim in simulators
                    {
                        sim.update(bundle.manager.moving(sim.id()), bundle.delta_time);
                    }
                }
            },
            Status::FreeDrawUi(hgl_e) =>
            {
                match item_beneath_cursor
                {
                    Some(ItemBeneathCursor::PossibleMoving(id)) => *hgl_e = id.into(),
                    _ =>
                    {
                        *hgl_e = None;
                        return;
                    }
                };

                if bundle.inputs.left_mouse.just_pressed()
                {
                    self.status = Status::SingleEditing(
                        hgl_e.unwrap(),
                        PathEditing::FreeDraw(PathCreation::default())
                    );
                }
            },
            Status::InsertNodeUi(hgl_e) =>
            {
                *hgl_e = item_beneath_cursor;

                if !bundle.inputs.left_mouse.just_pressed()
                {
                    return;
                }

                let (id, idx) = return_if_no_match!(
                    *hgl_e,
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
                        ItemBeneathCursor::PossibleMoving(_) | ItemBeneathCursor::SelectedMoving(_)
                    )
                )
                {
                    *hgl_e = None;
                    return;
                }

                let item_beneath_cursor = item_beneath_cursor.unwrap();
                *hgl_e = item_beneath_cursor.into();

                if !bundle.inputs.left_mouse.just_pressed()
                {
                    return;
                }

                match item_beneath_cursor
                {
                    ItemBeneathCursor::SelectedMoving(id) =>
                    {
                        bundle.manager.replace_selected_path(
                            bundle.drawing_resources,
                            bundle.edits_history,
                            bundle.grid,
                            id,
                            std::mem::take(path).unwrap()
                        );
                    },
                    ItemBeneathCursor::PossibleMoving(id) =>
                    {
                        bundle.manager.create_path(
                            bundle.drawing_resources,
                            bundle.edits_history,
                            bundle.grid,
                            id,
                            std::mem::take(path).unwrap()
                        );
                    },
                    ItemBeneathCursor::PathNode(..) => unreachable!()
                };

                self.status = Status::Inactive(Some(item_beneath_cursor).into());
            }
        };
    }

    /// Returns a [`Status`] to add a [`Node`] to the [`Path`] of the entity with [`Id`]
    /// `identifier`.
    #[inline]
    #[must_use]
    const fn add_node_status(cursor: &Cursor, identifier: Id, index: u8) -> Status
    {
        Status::SingleEditing(identifier, PathEditing::InsertNode {
            index: index + 1,
            pos:   cursor.world_snapped()
        })
    }

    /// Updates the tool after a post undo/redo despawn.
    #[inline]
    pub fn undo_redo_despawn(&mut self, manager: &EntitiesManager, identifier: Id)
    {
        assert!(!manager.is_selected_moving(identifier), "Brush is not a selected platform.");

        match &mut self.status
        {
            Status::Inactive(ds) if ds.has_highlighted_entity() =>
            {
                if ds.highlighted_entity().unwrap().id() == identifier
                {
                    ds.set_highlighted_entity(None);
                }
            },
            _ => ()
        };
    }

    /// Toggles the [`Node`] of the entity with [`Id`] `identifier` at `index`.
    #[inline]
    #[must_use]
    fn toggle_node(bundle: &mut ToolUpdateBundle, identifier: Id, index: u8) -> bool
    {
        let selected = bundle
            .manager
            .moving_mut(bundle.drawing_resources, bundle.grid, identifier)
            .toggle_path_node_at_index(usize::from(index));
        bundle.manager.schedule_overall_node_update();
        bundle.edits_history.path_nodes_selection(identifier, hv_vec![index]);
        selected
    }

    /// Selects the [`Node`] of the entity with [`Id`] `identifier` at `index`.
    #[inline]
    fn select_node(bundle: &mut ToolUpdateBundle, identifier: Id, index: u8)
    {
        match bundle
            .manager
            .moving_mut(bundle.drawing_resources, bundle.grid, identifier)
            .exclusively_select_path_node_at_index(usize::from(index))
        {
            NodeSelectionResult::Selected => return,
            NodeSelectionResult::NotSelected(idxs) =>
            {
                bundle.edits_history.path_nodes_selection(identifier, idxs);
            }
        };

        bundle.manager.schedule_overall_node_update();
    }

    /// Selects the [`Node`]s within `range`.
    #[inline]
    fn select_nodes_from_drag_selection(bundle: &mut ToolUpdateBundle, range: &Hull)
    {
        let func = if bundle.inputs.shift_pressed()
        {
            EditPath::select_path_nodes_in_range
        }
        else
        {
            EditPath::exclusively_select_path_nodes_in_range
        };

        if bundle.edits_history.path_nodes_selection_cluster(
            bundle
                .manager
                .selected_movings_mut(bundle.drawing_resources, bundle.grid)
                .filter_map(|mut entity| func(&mut *entity, range).map(|vxs| (entity.id(), vxs)))
        )
        {
            bundle.manager.schedule_overall_node_update();
        }
    }

    /// Moves the selected [`Node`]s. Returns whether it was possible.
    #[inline]
    fn move_nodes(
        bundle: &mut ToolUpdateBundle,
        delta: Vec2,
        cumulative_move: &mut HvVec<(Id, HvVec<NodesMove>)>
    ) -> bool
    {
        let mut move_payloads = hv_vec![];

        for moving in bundle.manager.selected_moving()
        {
            match moving.check_selected_path_nodes_move(delta)
            {
                IdNodesMoveResult::None => (),
                IdNodesMoveResult::Invalid => return false,
                IdNodesMoveResult::Valid(pl) => move_payloads.push(pl)
            };
        }

        assert!(!move_payloads.is_empty(), "Move payloads is empty.");

        for payload in move_payloads
        {
            let id = payload.id();
            let mut moving = bundle.manager.moving_mut(bundle.drawing_resources, bundle.grid, id);
            let nodes_move = moving.apply_selected_path_nodes_move(payload);

            let mov = cumulative_move
                .iter_mut()
                .rev()
                .find_map(|(id, mov)| (*id == moving.id()).then_some(mov));

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

    /// Updates the editing of a single entity. Returns whether the editing was concluded.
    #[inline]
    #[must_use]
    fn single_editing(bundle: &mut ToolUpdateBundle, status: &mut Status) -> bool
    {
        let cursor_pos = Self::cursor_pos(status, bundle.cursor).unwrap();
        let (id, editing) =
            match_or_panic!(status, Status::SingleEditing(id, editing), (*id, editing));

        match editing
        {
            PathEditing::FreeDraw(path) =>
            {
                if bundle.inputs.enter.just_pressed()
                {
                    bundle.manager.create_path(
                        bundle.drawing_resources,
                        bundle.edits_history,
                        bundle.grid,
                        id,
                        return_if_none!(path.path(), false)
                    );
                    return true;
                }

                if bundle.inputs.right_mouse.just_pressed()
                {
                    path.remove(
                        bundle.edits_history,
                        cursor_pos,
                        bundle.manager.moving(id).center(),
                        bundle.camera.scale()
                    );
                }
                else if bundle.inputs.left_mouse.just_pressed()
                {
                    path.push(bundle.edits_history, cursor_pos, bundle.manager.moving(id).center());
                }
            },
            PathEditing::InsertNode { index, pos } =>
            {
                *pos = cursor_pos;
                let mut moving =
                    bundle.manager.moving_mut(bundle.drawing_resources, bundle.grid, id);

                if bundle.inputs.left_mouse.pressed()
                {
                    return false;
                }

                if moving.try_insert_path_node_at_index(*pos, *index as usize)
                {
                    bundle.edits_history.path_node_insertion(id, *pos, *index);
                }

                return true;
            }
        };

        false
    }

    /// Deletes the selected [`Node`]s or [`Path`]s depending on whether alt is pressed.
    #[inline]
    #[must_use]
    fn delete(bundle: &mut ToolUpdateBundle) -> bool
    {
        let ToolUpdateBundle {
            drawing_resources,
            manager,
            edits_history,
            inputs,
            grid,
            ..
        } = bundle;

        // Delete paths.
        if inputs.alt_pressed()
        {
            manager.remove_selected_paths(drawing_resources, edits_history, grid);
            return true;
        }

        // Delete all selected nodes if that's fine.
        let mut payloads = hv_vec![];

        for moving in manager.selected_moving()
        {
            match moving.check_selected_nodes_deletion()
            {
                IdNodesDeletionResult::None => continue,
                IdNodesDeletionResult::Invalid => return false,
                IdNodesDeletionResult::Valid(payload) => payloads.push(payload)
            };
        }

        if payloads.is_empty()
        {
            return false;
        }

        edits_history.path_nodes_deletion_cluster(payloads.into_iter().map(|p| {
            (
                p.id(),
                manager
                    .moving_mut(drawing_resources, grid, p.id())
                    .remove_selected_path_nodes(p)
            )
        }));

        true
    }

    /// Deletes the free draw [`Node`] at `index`.
    #[inline]
    pub fn delete_free_draw_path_node(&mut self, manager: &EntitiesManager, index: usize)
    {
        let (id, path) = match_or_panic!(
            &mut self.status,
            Status::SingleEditing(id, PathEditing::FreeDraw(path)),
            (*id, path)
        );

        path.remove_index(index, manager.moving(id).center());
    }

    /// Inserts a free draw [`Node`] at position `p` and `index`.
    #[inline]
    pub fn insert_free_draw_path_node(&mut self, manager: &EntitiesManager, p: Vec2, index: usize)
    {
        let (id, path) = match_or_panic!(
            &mut self.status,
            Status::SingleEditing(id, PathEditing::FreeDraw(path)),
            (*id, path)
        );

        path.insert_at_index(p, index, manager.moving(id).center());
    }

    /// Enables the movement simulation.
    #[inline]
    pub fn enable_simulation(&mut self, manager: &EntitiesManager)
    {
        if self.nodes_editor.interacting() || manager.selected_moving_amount() == 0
        {
            return;
        }

        if matches!(self.status, Status::Simulation(..))
        {
            return;
        }

        self.status = Status::Simulation(manager.selected_movement_simulators(), false);
    }

    //==============================================================
    // Draw

    /// Draws the tool.
    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle)
    {
        let DrawBundle {
            window,
            drawer,
            camera,
            cursor,
            things_catalog,
            manager,
            ..
        } = bundle;

        let brushes = manager.brushes();

        /// Draws the entities except `filters`.
        macro_rules! draw_entities {
            ($($filters:expr)?) => {{
                for entity in manager.visible_paths(window, camera, drawer.grid()).iter()
                    $(.filter_set_with_predicate($filters, |entity| entity.id()))?
                {
                    let id = entity.id();

                    if manager.is_selected_moving(id)
                    {
                        entity.draw_path(
                            window,
                            camera,
                            drawer
                        );
                    }
                    else
                    {
                        entity.draw_semitransparent_path(drawer);
                    }
                }

                for brush in manager.visible_brushes(window, camera, drawer.grid()).iter()
                    $(.filter_set_with_predicate($filters, |brush| brush.id()))?
                {
                    let id = brush.id();

                    if manager.is_selected_moving(id) || is_attached_to_selected_moving(manager, id)
                    {
                        brush.draw_selected(camera, drawer);
                    }
                    else if manager.is_selected(id) && brush.no_path_nor_attached()
                    {
                        brush.draw_non_selected(camera, drawer);
                    }
                    else
                    {
                        brush.draw_opaque(camera, drawer);
                    }
                }

                for thing in manager.visible_things(window, camera, drawer.grid()).iter()
                    $(.filter_set_with_predicate($filters, |thing| thing.id()))?
                {
                    let id = thing.id();

                    if manager.is_selected_moving(id)
                    {
                        thing.draw_selected(
                            window,
                            camera,
                            drawer,
                            things_catalog
                        );
                    }
                    else if manager.is_selected(id)
                    {
                        thing.draw_non_selected(
                            window,
                            camera,
                            drawer,
                            things_catalog
                        );
                    }
                    else
                    {
                        thing.draw_opaque(
                            window,
                            camera,
                            drawer,
                            things_catalog
                        );
                    }
                }
            }};
        }

        /// Draws the entities highlighting `hgl_e`.
        macro_rules! draw_entities_with_highlight {
            ($hgl_e:expr) => {
                if $hgl_e.is_none()
                {
                    draw_entities!();
                }
                else
                {
                    let hgl_e = $hgl_e.unwrap();

                    match hgl_e
                    {
                        ItemBeneathCursor::SelectedMoving(id) =>
                        {
                            manager.moving(id).draw_highlighted_with_path_nodes(
                                window,
                                camera,
                                brushes,
                                things_catalog,
                                drawer
                            );
                        },
                        ItemBeneathCursor::PossibleMoving(id) =>
                        {
                            if manager.is_thing(id)
                            {
                                manager.thing(id).draw_highlighted_non_selected(
                                    window,
                                    camera,
                                    drawer,
                                    things_catalog
                                );
                            }
                            else
                            {
                                manager.brush(id).draw_highlighted_non_selected(camera, drawer);
                            }
                        },
                        ItemBeneathCursor::PathNode(id, idx) =>
                        {
                            manager.moving(id).draw_with_highlighted_path_node(
                                window,
                                camera,
                                brushes,
                                things_catalog,
                                drawer,
                                usize::from(idx)
                            );
                        }
                    };

                    draw_entities!(hgl_e.id());
                }
            };
        }

        if matches!(self.status, Status::SingleEditing(..))
        {
            drawer.square_highlight(
                PathTool::cursor_pos(&self.status, cursor).unwrap(),
                Color::CursorPolygon
            );
        }

        match &self.status
        {
            Status::Inactive(ds) =>
            {
                draw_entities_with_highlight!(ds.highlighted_entity());
                drawer.hull(&return_if_none!(ds.hull()), Color::Hull);
            },
            Status::PreDrag(_, hgl_e) | Status::InsertNodeUi(hgl_e) =>
            {
                draw_entities_with_highlight!(*hgl_e);
            },
            Status::Drag(..) => draw_entities!(),
            Status::SingleEditing(id, editing) =>
            {
                match editing
                {
                    PathEditing::FreeDraw(path) =>
                    {
                        let center = if manager.is_thing(*id)
                        {
                            let thing = manager.thing(*id);
                            thing.draw_highlighted_selected(window, camera, drawer, things_catalog);
                            thing.center()
                        }
                        else
                        {
                            let brush = manager.brush(*id);
                            brush.draw_highlighted_selected(camera, drawer);
                            brush.center()
                        };

                        path.draw_with_knot(window, camera, drawer, center);
                    },
                    PathEditing::InsertNode { pos, index } =>
                    {
                        manager.moving(*id).draw_with_path_node_addition(
                            window,
                            camera,
                            brushes,
                            things_catalog,
                            drawer,
                            *pos,
                            *index as usize
                        );
                    }
                }

                draw_entities!(*id);
            },
            Status::Simulation(simulators, _) =>
            {
                for simulator in simulators
                {
                    manager.moving(simulator.id()).draw_movement_simulation(
                        window,
                        camera,
                        brushes,
                        things_catalog,
                        drawer,
                        simulator
                    );
                }

                for moving in manager
                    .visible_paths(window, camera, drawer.grid())
                    .iter()
                    .filter(|moving| !is_moving(manager, moving.id()))
                {
                    moving.draw_semitransparent_path(drawer);
                }

                for brush in manager
                    .visible_brushes(window, camera, drawer.grid())
                    .iter()
                    .filter(|brush| !is_moving(manager, brush.id()))
                {
                    brush.draw_opaque(camera, drawer);
                }

                for brush in manager
                    .visible_sprites(window, camera, drawer.grid())
                    .iter()
                    .filter(|brush| !is_moving(manager, brush.id()))
                {
                    brush.draw_sprite(drawer, Color::OpaqueEntity, false);
                }

                for brush in manager
                    .visible_anchors(window, camera, drawer.grid())
                    .iter()
                    .filter(|brush| !is_moving(manager, brush.id()))
                {
                    brush.draw_anchors(brushes, drawer);
                }

                for thing in manager
                    .visible_things(window, camera, drawer.grid())
                    .iter()
                    .filter(|thing| !is_moving(manager, thing.id()))
                {
                    thing.draw_opaque(window, camera, drawer, things_catalog);
                }
            },
            Status::FreeDrawUi(hgl_e) =>
            {
                if let Some(hgl_e) = hgl_e
                {
                    if manager.is_thing(*hgl_e)
                    {
                        manager.thing(*hgl_e).draw_highlighted_non_selected(
                            window,
                            camera,
                            drawer,
                            things_catalog
                        );
                    }
                    else
                    {
                        manager.brush(*hgl_e).draw_highlighted_non_selected(camera, drawer);
                    }

                    draw_entities!(*hgl_e);
                }
                else
                {
                    draw_entities!();
                }
            },
            Status::PathConnection(path, hgl_e) =>
            {
                path.as_ref().unwrap().draw_no_tooltips(drawer, cursor.world());
                draw_entities_with_highlight!(hgl_e);
            }
        };
    }

    /// Draws the UI.
    #[inline]
    pub fn ui(&mut self, ui: &mut egui::Ui, bundle: &mut UiBundle)
    {
        self.nodes_editor
            .show(ui, bundle, matches!(self.status, Status::Simulation(..)));
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
        subtools_buttons!(
            self.status,
            ui,
            bundle,
            buttons,
            (
                PathFreeDraw,
                Status::FreeDrawUi(None),
                Status::FreeDrawUi(_),
                Status::InsertNodeUi(_) | Status::Simulation(..)
            ),
            (
                PathInsertNode,
                Status::InsertNodeUi(None),
                Status::InsertNodeUi(_),
                Status::FreeDrawUi(_) | Status::Simulation(..)
            )
        );

        if !buttons.draw(ui, bundle, SubTool::PathSimulation, &self.status)
        {
            return;
        }

        match &self.status
        {
            Status::Inactive(..) | Status::FreeDrawUi(_) | Status::InsertNodeUi(_) =>
            {
                self.nodes_editor.force_simulation(bundle);
                self.enable_simulation(bundle.manager);
            },
            Status::Simulation(..) => self.status = Status::default(),
            _ => ()
        };
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Whether the brush with [`Id`] `identifier` is attached to a selected moving brush.
#[inline]
#[must_use]
fn is_attached_to_selected_moving(manager: &EntitiesManager, identifier: Id) -> bool
{
    match manager.brush(identifier).attached()
    {
        Some(id) => manager.is_selected_moving(id),
        None => false
    }
}

//=======================================================================//

/// Whether the entity with [`Id`] `identifier` moves.
#[inline]
#[must_use]
fn is_moving(manager: &EntitiesManager, identifier: Id) -> bool
{
    let sel_mov = manager.is_selected_moving(identifier);

    if manager.is_thing(identifier)
    {
        return sel_mov;
    }

    sel_mov || is_attached_to_selected_moving(manager, identifier)
}
