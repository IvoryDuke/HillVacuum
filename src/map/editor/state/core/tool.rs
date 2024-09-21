//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::input::{keyboard::KeyCode, ButtonInput};
use bevy_egui::egui;
use glam::Vec2;
use hill_vacuum_proc_macros::{EnumFromUsize, EnumIter, EnumSize, SubToolEnum, ToolEnum};
use hill_vacuum_shared::{match_or_panic, return_if_no_match, return_if_none, NextValue};

use super::{
    clip_tool::ClipTool,
    draw_selected_and_non_selected_things,
    draw_tool::{cursor_polygon::FreeDrawStatus, DrawTool},
    entity_tool::EntityTool,
    flip_tool::FlipTool,
    map_preview::MapPreviewTool,
    paint_tool::PaintTool,
    path_tool::PathTool,
    rect::Rect,
    rotate_tool::RotateTool,
    scale_tool::ScaleTool,
    shatter_tool::ShatterTool,
    shear_tool::ShearTool,
    side_tool::SideTool,
    subtract_tool::SubtractTool,
    thing_tool::ThingTool,
    vertex_tool::VertexTool,
    zoom_tool::ZoomTool,
    Core
};
use crate::{
    config::controls::{bind::Bind, BindsKeyCodes},
    map::{
        brush::{convex_polygon::ConvexPolygon, Brush},
        drawer::drawing_resources::DrawingResources,
        editor::{
            state::{
                clipboard::Clipboard,
                core::{deselect_vertexes, draw_selected_and_non_selected_sprites},
                editor_state::{InputsPresses, ToolsSettings},
                edits_history::EditsHistory,
                grid::Grid,
                manager::EntitiesManager,
                ui::ToolsButtons
            },
            DrawBundle,
            DrawBundleMapPreview,
            StateUpdateBundle,
            ToolUpdateBundle
        },
        thing::catalog::ThingsCatalog
    },
    utils::{
        collections::hv_vec,
        identifiers::{EntityId, Id},
        iterators::FilterSet,
        math::{polygons::convex_hull, HashVec2},
        misc::FromToStr
    },
    HvHashSet
};

//=======================================================================//
// MACROS
//
//=======================================================================//

/// Draws the subtool buttons.
macro_rules! subtools_buttons {
    (
        $status:expr,
        $ui:ident,
        $bundle:ident,
        $buttons:ident,
        $change_conditions:expr,
        $(($subtool:ident, $value:expr, $disable:pat $(, $enable:pat)?)),+
    ) => {$({
        let clicked =
            $buttons.draw($ui, $bundle, SubTool::$subtool, $change_conditions, &$status);
        subtools_buttons!($status, (clicked, $value, $disable $(, $enable)?));
    })+};

    (
        $status:expr,
        $(($clicked:ident, $value:expr, $disable:pat $(, $enable:pat)?)),+
    ) => {$({
        if $clicked
        {
            match &$status
            {
                #[allow(clippy::unnested_or_patterns)]
                Status::Inactive(..) $(| $enable)? => $status = $value,
                $disable => $status = Status::default(),
                #[allow(unreachable_patterns)]
                _ => ()
            };
        }
    })+};
}

pub(in crate::map::editor::state::core) use subtools_buttons;

//=======================================================================//
// TRAITS
//
//=======================================================================//

/// A trait for tools.
pub(in crate::map::editor::state) trait ToolInterface
where
    Self: Copy + PartialEq
{
    /// The text to be used in UI elements.
    #[must_use]
    fn label(self) -> &'static str;

    /// The header text of the manual section.
    #[must_use]
    fn header(self) -> &'static str;

    /// The name of the icon file.
    #[must_use]
    fn icon_file_name(self) -> &'static str;

    /// The text to be displayed in the UI tooltip when the tool icon is being hovered.
    #[must_use]
    fn tooltip_label(self, binds: &BindsKeyCodes) -> String;

    /// Whether the tool can be enabled.
    #[must_use]
    fn change_conditions_met(self, change_conditions: &ChangeConditions) -> bool;

    /// Whether the tool is a subtool.
    #[must_use]
    fn subtool(self) -> bool;

    /// The index associated with the tool.
    #[must_use]
    fn index(self) -> usize;
}

//=======================================================================//

/// A trait to return whether the tool is enabled.
pub(in crate::map::editor::state) trait EnabledTool
{
    /// The tool to check if it's enabled.
    type Item: ToolInterface;

    /// Whether the tool associated with `tool` is enabled.
    #[must_use]
    fn is_tool_enabled(&self, tool: Self::Item) -> bool;
}

//=======================================================================//

/// A trait to disable the subtool of the active tool.
pub(in crate::map::editor::state) trait DisableSubtool
{
    /// Disables the active subtool, if any.
    fn disable_subtool(&mut self);
}

//=======================================================================//

/// A trait to return whether a tool has any ongoing multiframe changes.
pub(in crate::map::editor::state) trait OngoingMultiframeChange
{
    /// Whether there are any ongoing multiframe changes.
    #[must_use]
    fn ongoing_multi_frame_change(&self) -> bool;
}

//=======================================================================//

/// A trait to return the drag selection of the tool, if any.
pub(in crate::map::editor::state::core) trait DragSelection
{
    /// Returns the [`Rect`] describing the tool's drag selection, if any.
    fn drag_selection(&self) -> Option<Rect>;
}

//=======================================================================//

/// The type of entities snap to execute.
#[allow(clippy::missing_docs_in_private_items)]
#[derive(PartialEq)]
enum Snap
{
    None,
    Entities,
    Things,
    Brushes,
    Vertexes,
    Sides,
    PathNodes
}

impl Snap
{
    /// Returns a new [`Snap`].
    #[inline]
    #[must_use]
    fn new(active_tool: &ActiveTool, manager: &EntitiesManager) -> Self
    {
        if active_tool.ongoing_multi_frame_change() || !manager.any_selected_brushes()
        {
            return Self::None;
        }

        match active_tool
        {
            ActiveTool::Entity(_) => Self::Entities,
            ActiveTool::Thing(_) => Self::Things,
            ActiveTool::Vertex(_) => Self::Vertexes,
            ActiveTool::Side(_) => Self::Sides,
            ActiveTool::Path(_) => Self::PathNodes,
            _ => Self::Brushes
        }
    }
}

//=======================================================================//

/// The map element being edited by the active tool.
#[allow(clippy::missing_docs_in_private_items)]
#[must_use]
#[derive(Clone, Copy, Default)]
pub(in crate::map::editor::state) enum EditingTarget
{
    #[default]
    Other,
    Draw,
    BrushFreeDraw(FreeDrawStatus),
    Thing,
    Vertexes,
    Sides,
    Subtractees,
    Path,
    PathFreeDraw
}

impl EditingTarget
{
    /// Returns a new [`EditingTarget`].
    #[inline]
    const fn new(active_tool: &ActiveTool, prev_value: Self) -> Self
    {
        match active_tool
        {
            ActiveTool::Draw(t) =>
            {
                match t.free_draw_status()
                {
                    Some(s) => Self::BrushFreeDraw(s),
                    None => Self::Draw
                }
            },
            ActiveTool::Thing(_) => Self::Thing,
            ActiveTool::Vertex(t) =>
            {
                if t.is_free_draw_active()
                {
                    Self::PathFreeDraw
                }
                else
                {
                    Self::Vertexes
                }
            },
            ActiveTool::Side(_) => Self::Sides,
            ActiveTool::Path(t) =>
            {
                if t.is_free_draw_active()
                {
                    Self::PathFreeDraw
                }
                else
                {
                    Self::Path
                }
            },
            ActiveTool::Subtract(_) => Self::Subtractees,
            ActiveTool::Zoom(_) | ActiveTool::MapPreview(_) => prev_value,
            _ => Self::Other
        }
    }

    /// Whether the change of [`EditingTarget`] requires certain edits to be purged from the
    /// [`EditsHistory`].
    #[inline]
    #[must_use]
    pub fn requires_tool_edits_purge(self, prev_value: Self) -> bool
    {
        match (prev_value, self)
        {
            (Self::Other, _) | (Self::Draw | Self::BrushFreeDraw(_), Self::BrushFreeDraw(_)) =>
            {
                false
            },
            _ => core::mem::discriminant(&self) != core::mem::discriminant(&prev_value)
        }
    }
}

//=======================================================================//

/// The currently active tool.
#[allow(clippy::missing_docs_in_private_items)]
#[must_use]
#[derive(Debug)]
pub(in crate::map::editor::state::core) enum ActiveTool
{
    Draw(DrawTool),
    Entity(EntityTool),
    Vertex(VertexTool),
    Side(SideTool),
    Clip(ClipTool),
    Shatter(ShatterTool),
    Subtract(SubtractTool),
    Scale(ScaleTool),
    Shear(ShearTool),
    Rotate(RotateTool),
    Flip(FlipTool),
    Zoom(ZoomTool),
    Path(PathTool),
    Paint(PaintTool),
    Thing(ThingTool),
    MapPreview(MapPreviewTool)
}

impl Default for ActiveTool
{
    #[inline]
    fn default() -> Self { Self::Draw(DrawTool::default()) }
}

impl EnabledTool for ActiveTool
{
    type Item = Tool;

    #[inline]
    fn is_tool_enabled(&self, tool: Self::Item) -> bool
    {
        tool == match self
        {
            Self::Draw(t) => return t.is_tool_enabled(tool),
            Self::Entity(_) => Tool::Entity,
            Self::Vertex(_) => Tool::Vertex,
            Self::Side(_) => Tool::Side,
            Self::Clip(_) => Tool::Clip,
            Self::Shatter(_) => Tool::Shatter,
            Self::Subtract(_) => Tool::Subtract,
            Self::Scale(_) => Tool::Scale,
            Self::Shear(_) => Tool::Shear,
            Self::Rotate(_) => Tool::Rotate,
            Self::Flip(_) => Tool::Flip,
            Self::Zoom(_) => Tool::Zoom,
            Self::Path(_) => Tool::Path,
            Self::Paint(_) => Tool::Paint,
            Self::Thing(_) => Tool::Thing,
            Self::MapPreview { .. } => return false
        }
    }
}

impl DisableSubtool for ActiveTool
{
    #[inline]
    fn disable_subtool(&mut self)
    {
        match self
        {
            Self::Draw(t) => t.disable_subtool(),
            Self::Thing(t) => t.disable_subtool(),
            Self::Entity(t) => t.disable_subtool(),
            Self::Vertex(t) => t.disable_subtool(),
            Self::Side(t) => t.disable_subtool(),
            Self::Clip(t) => t.disable_subtool(),
            Self::Rotate(t) => t.disable_subtool(),
            Self::Path(t) => t.disable_subtool(),
            Self::Paint(t) => t.disable_subtool(),
            _ => ()
        };
    }
}

impl OngoingMultiframeChange for ActiveTool
{
    #[inline]
    fn ongoing_multi_frame_change(&self) -> bool
    {
        match self
        {
            Self::Entity(t) => t.ongoing_multi_frame_change(),
            Self::Clip(t) => t.ongoing_multi_frame_change(),
            Self::Draw(t) => t.ongoing_multi_frame_change(),
            Self::Path(t) => t.ongoing_multi_frame_change(),
            Self::Rotate(t) => t.ongoing_multi_frame_change(),
            Self::Scale(t) => t.ongoing_multi_frame_change(),
            Self::Shear(t) => t.ongoing_multi_frame_change(),
            Self::Side(t) => t.ongoing_multi_frame_change(),
            Self::Vertex(t) => t.ongoing_multi_frame_change(),
            Self::Paint(t) => t.ongoing_multi_frame_change(),
            _ => false
        }
    }
}

impl ActiveTool
{
    //==============================================================
    // Info

    /// The [`EditingTarget`] associated with the current tool.
    #[inline]
    pub const fn editing_target(&self, prev_value: EditingTarget) -> EditingTarget
    {
        EditingTarget::new(self, prev_value)
    }

    /// The current drag selection.
    #[inline]
    fn drag_selection(&self) -> Rect
    {
        match &self
        {
            Self::Entity(t) => t.drag_selection(),
            Self::Subtract(t) => t.drag_selection(),
            Self::Zoom(t) => t.drag_selection(),
            Self::Vertex(t) => t.drag_selection(),
            Self::Side(t) => t.drag_selection(),
            Self::Path(t) => t.drag_selection(),
            _ => None
        }
        .unwrap_or_default()
    }

    /// Whether the simulation of the moving entities is active.
    #[inline]
    #[must_use]
    pub const fn path_simulation_active(&self) -> bool
    {
        return_if_no_match!(self, Self::Path(t), t, false).simulation_active()
    }

    /// Whether the entity tool is active.
    #[inline]
    #[must_use]
    pub const fn entity_tool(&self) -> bool { matches!(self, Self::Entity(_)) }

    /// Whether a tool with texture editing capabilities is available.
    #[inline]
    #[must_use]
    pub const fn texture_tool(&self) -> bool
    {
        matches!(self, Self::Entity(_) | Self::Scale(_) | Self::Rotate(_) | Self::Flip(_))
    }

    /// Whether the vertexes merge is available.
    #[inline]
    #[must_use]
    pub const fn vx_merge_available(&self) -> bool
    {
        match self
        {
            Self::Vertex(t) => t.vx_merge_available(),
            Self::Side(t) => t.vx_merge_available(),
            _ => false
        }
    }

    /// Whether the split subtoon is available.
    #[inline]
    #[must_use]
    pub fn split_available(&self) -> bool
    {
        return_if_no_match!(self, Self::Vertex(t), t, false).split_available()
    }

    /// Whether the x-trusion subtool is available.
    #[inline]
    #[must_use]
    fn xtrusion_available(&self) -> bool
    {
        return_if_no_match!(self, Self::Side(t), t, false).xtrusion_available()
    }

    /// Whether map preview is active.
    #[inline]
    #[must_use]
    pub const fn map_preview(&self) -> bool { matches!(self, Self::MapPreview { .. }) }

    //==============================================================
    // Copy/Paste

    /// Whether copy/paste is available.
    #[inline]
    #[must_use]
    pub fn copy_paste_available(&self) -> bool
    {
        match self
        {
            Self::Draw(_) | Self::Zoom(_) | Self::MapPreview { .. } => false,
            Self::Shatter(_) | Self::Subtract(_) | Self::Flip(_) | Self::Thing(_) => true,
            Self::Entity(t) => !t.ongoing_multi_frame_change(),
            Self::Vertex(t) => !t.ongoing_multi_frame_change(),
            Self::Side(t) => !t.ongoing_multi_frame_change(),
            Self::Clip(t) => !t.ongoing_multi_frame_change(),
            Self::Scale(t) => !t.ongoing_multi_frame_change(),
            Self::Shear(t) => !t.ongoing_multi_frame_change(),
            Self::Rotate(t) => !t.ongoing_multi_frame_change(),
            Self::Path(t) => t.copy_paste_available(),
            Self::Paint(t) => !t.ongoing_multi_frame_change()
        }
    }

    /// Copies the selected entities.
    #[inline]
    pub fn copy(
        &mut self,
        bundle: &StateUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        clipboard: &mut Clipboard
    )
    {
        assert!(self.copy_paste_available(), "Copy is not available.");

        if let Self::Path(t) = self
        {
            clipboard.copy_platform_path(
                manager,
                return_if_none!(t.selected_moving_beneath_cursor(bundle, manager, inputs))
            );

            return;
        }

        clipboard.copy(bundle.drawing_resources, manager.selected_entities());
    }

    /// Cuts the selected entities.
    #[inline]
    pub fn cut(
        &mut self,
        bundle: &StateUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        clipboard: &mut Clipboard,
        edits_history: &mut EditsHistory
    )
    {
        assert!(self.copy_paste_available(), "Cut is not available.");

        match self
        {
            Self::Path(t) =>
            {
                clipboard.cut_platform_path(
                    bundle.drawing_resources,
                    manager,
                    edits_history,
                    return_if_none!(t.selected_moving_beneath_cursor(bundle, manager, inputs))
                );

                return;
            },
            Self::Entity(t) => t.remove_highlighted_entity(),
            _ => ()
        };

        clipboard.copy(bundle.drawing_resources, manager.selected_entities());
        manager.despawn_selected_entities(bundle.drawing_resources, edits_history);
        manager.schedule_outline_update();
    }

    /// Pastes the copied entities.
    #[inline]
    pub fn paste(
        &mut self,
        bundle: &StateUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        clipboard: &mut Clipboard,
        edits_history: &mut EditsHistory
    )
    {
        assert!(self.copy_paste_available(), "Paste is not available.");

        if let Self::Path(t) = self
        {
            clipboard.paste_platform_path(
                bundle.drawing_resources,
                manager,
                edits_history,
                return_if_none!(t.possible_moving_beneath_cursor(bundle, manager, inputs))
            );

            return;
        }

        if !clipboard.has_copy_data()
        {
            return;
        }

        if let Self::Vertex(_) | Self::Side(_) = self
        {
            deselect_vertexes(bundle.drawing_resources, manager, edits_history);
        }

        manager.deselect_selected_entities(edits_history);
        clipboard.paste(bundle, manager, edits_history);
        manager.schedule_outline_update();
    }

    #[inline]
    pub fn duplicate(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        clipboard: &mut Clipboard,
        edits_history: &mut EditsHistory,
        delta: Vec2
    )
    {
        assert!(self.copy_paste_available(), "Duplicate is not available.");

        if let Self::Vertex(_) | Self::Side(_) = self
        {
            deselect_vertexes(drawing_resources, manager, edits_history);
        }

        clipboard.duplicate(drawing_resources, manager, edits_history, delta);
        manager.schedule_outline_update();
    }

    /// Updates the outline of certain tools.
    #[inline]
    pub fn update_outline(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &EntitiesManager,
        grid: Grid,
        settings: &ToolsSettings
    )
    {
        match self
        {
            Self::Shear(t) => t.update_outline(manager, grid),
            Self::Scale(t) => t.update_outline(drawing_resources, manager, grid, settings),
            Self::Flip(t) => t.update_outline(manager, grid),
            Self::Paint(t) => t.update_outline(drawing_resources, manager, grid),
            _ => ()
        };
    }

    /// Updates the stored info concerning the selected vertexes.
    #[inline]
    pub fn update_selected_vertexes<'a>(
        &mut self,
        manager: &EntitiesManager,
        ids: impl Iterator<Item = &'a Id>
    )
    {
        match self
        {
            ActiveTool::Vertex(t) =>
            {
                for id in ids
                {
                    t.update_selected_vertexes(manager, *id);
                }
            },
            ActiveTool::Side(t) =>
            {
                for id in ids
                {
                    t.update_selected_sides(manager, *id);
                }
            },
            _ => ()
        };
    }

    /// Updates the overall UI [`Path`] [`Node`].
    #[inline]
    pub fn update_overall_node(&mut self, manager: &EntitiesManager)
    {
        return_if_no_match!(self, Self::Path(t), t).update_overall_node(manager);
    }

    //==============================================================
    // Undo/Redo

    /// Whether it is possible to select all the entities.
    #[inline]
    #[must_use]
    pub fn select_all_available(&self) -> bool { !self.ongoing_multi_frame_change() }

    /// Selects all the entities.
    #[inline]
    pub fn select_all(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: Grid,
        settings: &ToolsSettings
    )
    {
        assert!(self.select_all_available(), "Select all is not available.");

        match self
        {
            Self::Subtract(t) => t.select_non_selected_brushes(manager, edits_history),
            Self::Vertex(_) | Self::Side(_) =>
            {
                edits_history.vertexes_selection_cluster(
                    manager
                        .selected_brushes_mut(drawing_resources)
                        .filter_map(|mut brush| {
                            brush.select_all_vertexes().map(|idxs| (brush.id(), idxs))
                        })
                );
            },
            Self::Path(_) =>
            {
                if edits_history.path_nodes_selection_cluster(
                    manager
                        .selected_movings_mut(drawing_resources)
                        .filter_map(|mut brush| {
                            brush.select_all_path_nodes().map(|idxs| (brush.id(), idxs))
                        })
                )
                {
                    manager.schedule_overall_node_update();
                }
            },
            _ => manager.select_all_entities(edits_history)
        };

        self.update_outline(drawing_resources, manager, grid, settings);
    }

    //==============================================================
    // Undo/Redo

    /// Whether undo/redo is avauilable.
    #[inline]
    #[must_use]
    pub fn undo_redo_available(&self) -> bool { !self.ongoing_multi_frame_change() }

    //==============================================================
    // Update

    /// Toggles the map preview.
    #[inline]
    pub fn toggle_map_preview(&mut self, bundle: &StateUpdateBundle, manager: &EntitiesManager)
    {
        *self = match self
        {
            Self::MapPreview(t) => std::mem::take(t.prev_tool()),
            _ => MapPreviewTool::tool(bundle, self, manager)
        };
    }

    /// Updates the tool.
    #[inline]
    pub fn update(
        &mut self,
        bundle: &mut ToolUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        clipboard: &mut Clipboard,
        grid: Grid,
        settings: &mut ToolsSettings
    )
    {
        match self
        {
            Self::Draw(t) => t.update(bundle, manager, inputs, edits_history, settings),
            Self::Entity(t) =>
            {
                t.update(bundle, manager, inputs, edits_history, clipboard, grid, settings);
            },
            Self::Vertex(t) =>
            {
                let path = return_if_none!(t.update(bundle, manager, inputs, edits_history, grid));
                *self = PathTool::path_connection(bundle, manager, inputs, path);
            },
            Self::Side(t) => t.update(bundle, manager, inputs, edits_history, grid),
            Self::Clip(t) => t.update(bundle, manager, inputs, edits_history),
            Self::Shatter(t) => t.update(bundle, manager, inputs, edits_history),
            Self::Subtract(t) =>
            {
                if t.update(bundle, manager, inputs, edits_history)
                {
                    *self = EntityTool::tool(Rect::default());
                    self.update(bundle, manager, inputs, edits_history, clipboard, grid, settings);
                }
            },
            Self::Scale(t) => t.update(bundle, manager, inputs, edits_history, grid, settings),
            Self::Shear(t) => t.update(bundle, manager, inputs, edits_history, grid),
            Self::Rotate(t) =>
            {
                t.update(bundle, manager, inputs, edits_history, settings, grid.size());
            },
            Self::Flip(t) =>
            {
                t.update(bundle, manager, inputs, edits_history, grid, settings);
            },
            Self::Zoom(t) =>
            {
                *self = std::mem::take(return_if_none!(t.update(bundle, inputs, grid)));
            },
            Self::Path(t) => t.update(bundle, manager, inputs, edits_history, grid),
            Self::Paint(t) =>
            {
                t.update(bundle, manager, inputs, edits_history, clipboard, grid);
            },
            Self::Thing(t) =>
            {
                t.update(bundle, manager, inputs, edits_history, settings);
            },
            Self::MapPreview(t) => t.update(bundle, manager)
        };
    }

    /// Changes the tool if requested.
    #[inline]
    pub fn change(
        &mut self,
        tool: Tool,
        bundle: &StateUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        inputs: &InputsPresses,
        grid: Grid,
        settings: &ToolsSettings,
        tool_change_conditions: &ChangeConditions
    )
    {
        // Safety check.
        assert!(
            tool.change_conditions_met(tool_change_conditions),
            "Requested tool change to unavailable tool {tool:?}"
        );
        assert!(
            !edits_history.multiframe_edit(),
            "Requested tool change during multiframe edit."
        );

        if matches!((&*self, tool), (Self::Zoom(_), Tool::Zoom))
        {
            return;
        }

        // Tool change.
        *self = match tool
        {
            Tool::Square => DrawTool::square(self, bundle.cursor),
            Tool::Triangle => DrawTool::triangle(self, bundle.cursor),
            Tool::Circle => DrawTool::circle(self, bundle.cursor, settings),
            Tool::FreeDraw => DrawTool::free(self),
            Tool::Entity => EntityTool::tool(self.drag_selection()),
            Tool::Vertex => VertexTool::tool(self.drag_selection()),
            Tool::Side => SideTool::tool(self.drag_selection()),
            Tool::Snap =>
            {
                self.snap_tool(bundle.drawing_resources, manager, edits_history, grid, settings);
                return;
            },
            Tool::Zoom => ZoomTool::tool(self.drag_selection(), self),
            Tool::Subtract => SubtractTool::tool(self.drag_selection()),
            Tool::Clip => ClipTool::tool(),
            Tool::Shatter => ShatterTool::tool(),
            Tool::Hollow =>
            {
                Self::hollow_tool(bundle.drawing_resources, manager, edits_history, grid);
                return;
            },
            Tool::Scale => ScaleTool::tool(bundle.drawing_resources, manager, grid, settings),
            Tool::Shear => ShearTool::tool(manager, grid),
            Tool::Rotate => RotateTool::tool(manager, settings),
            Tool::Flip => FlipTool::tool(manager, grid),
            Tool::Intersection =>
            {
                self.intersection_tool(bundle, manager, edits_history, grid, settings);
                return;
            },
            Tool::Merge =>
            {
                self.merge_tool(bundle, manager, edits_history, inputs);
                return;
            },
            Tool::Path => PathTool::tool(self.drag_selection()),
            Tool::Paint => PaintTool::tool(),
            Tool::Thing => ThingTool::tool()
        };
    }

    /// Snaps the selected entities and [`Path`]s to the `grid` based on `grid` and the currently
    /// selected tool.
    #[inline]
    pub fn snap_tool(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: Grid,
        settings: &ToolsSettings
    )
    {
        /// Snap the selected brushes to the grid.
        #[inline]
        #[must_use]
        fn snap_brushes(
            drawing_resources: &DrawingResources,
            manager: &mut EntitiesManager,
            edits_history: &mut EditsHistory,
            grid: Grid
        ) -> bool
        {
            manager
                .selected_brushes_mut(drawing_resources)
                .fold(false, |acc, mut brush| {
                    edits_history
                        .vertexes_snap(brush.id(), return_if_none!(brush.snap_vertexes(grid), acc));

                    true
                })
        }

        /// Snap the selected [`ThingInstances`]s to the grid.
        #[inline]
        #[must_use]
        fn snap_things(
            manager: &mut EntitiesManager,
            edits_history: &mut EditsHistory,
            grid: Grid
        ) -> bool
        {
            manager.selected_things_mut().fold(false, |acc, mut thing| {
                edits_history.thing_move(thing.id(), return_if_none!(thing.snap(grid), acc));

                true
            })
        }

        let snapped = match Snap::new(self, manager)
        {
            Snap::None => false,
            Snap::Entities =>
            {
                snap_brushes(drawing_resources, manager, edits_history, grid) |
                    snap_things(manager, edits_history, grid)
            },
            Snap::Things => snap_things(manager, edits_history, grid),
            Snap::Brushes => snap_brushes(drawing_resources, manager, edits_history, grid),
            Snap::Vertexes =>
            {
                manager
                    .selected_brushes_mut(drawing_resources)
                    .fold(false, |acc, mut brush| {
                        edits_history.vertexes_snap(
                            brush.id(),
                            return_if_none!(brush.snap_selected_vertexes(grid), acc)
                        );

                        true
                    })
            },
            Snap::Sides =>
            {
                manager
                    .selected_brushes_mut(drawing_resources)
                    .fold(false, |acc, mut brush| {
                        edits_history.vertexes_snap(
                            brush.id(),
                            return_if_none!(brush.snap_selected_sides(grid), acc)
                        );

                        true
                    })
            },
            Snap::PathNodes =>
            {
                manager
                    .selected_movings_mut(drawing_resources)
                    .fold(false, |acc, mut moving| {
                        edits_history.path_nodes_snap(
                            moving.id(),
                            return_if_none!(moving.snap_selected_path_nodes(grid), acc)
                        );

                        true
                    })
            },
        };

        if snapped
        {
            self.update_outline(drawing_resources, manager, grid, settings);
        }
    }

    /// Replaces each selected brushes with four others.
    /// These four brushes create a room with wall thickness equal to the grid size as big as the
    /// brush they replaced. If it's not possible to create rooms for all the brushes the
    /// process will be aborted.
    #[inline]
    fn hollow_tool(
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: Grid
    )
    {
        let mut wall_brushes = hv_vec![];
        let valid = manager.test_operation_validity(|manager| {
            manager.selected_brushes().find_map(|brush| {
                match brush.hollow(grid.size_f32())
                {
                    Some(result) =>
                    {
                        wall_brushes.push(result);
                        None
                    },
                    None => brush.id().into()
                }
            })
        });

        if !valid || wall_brushes.is_empty()
        {
            return;
        }

        for result in wall_brushes
        {
            _ = manager.replace_brush_with_partition(
                drawing_resources,
                edits_history,
                result.walls.into_iter(),
                result.id,
                |brush| brush.set_polygon(result.main)
            );
        }

        edits_history.override_edit_tag("Brushes hollow");
    }

    /// Generates the brush that represents the intersection between all the selected ones, if
    /// any. All selected brushes are despawned.
    #[inline]
    fn intersection_tool(
        &mut self,
        bundle: &StateUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: Grid,
        settings: &ToolsSettings
    )
    {
        let (mut intersection_polygon, filters) = {
            // Get the first intersection.
            let mut iter = manager.selected_brushes_ids();
            let id_1 = *iter.next_value();
            let id_2 = *iter.next_value();
            drop(iter);

            let intersection = manager.brush(id_1).intersection(manager.brush(id_2));

            if let Some(cp) = intersection
            {
                (cp, [id_1, id_2])
            }
            else
            {
                manager.despawn_selected_brushes(bundle.drawing_resources, edits_history);
                return;
            }
        };

        // Intersect the polygon with all the other brushes.
        let mut success = true;

        for id in manager.selected_brushes_ids().copied().filter_set(filters)
        {
            if !manager.brush(id).intersect(&mut intersection_polygon)
            {
                success = false;
                break;
            }
        }

        // Spawn the intersection brush.
        self.draw_tool_despawn(
            bundle.drawing_resources,
            manager,
            edits_history,
            |manager, edits_history| {
                manager.despawn_selected_brushes(bundle.drawing_resources, edits_history);

                if success
                {
                    manager.spawn_brushes(
                        bundle.drawing_resources,
                        Some(intersection_polygon).into_iter(),
                        edits_history,
                        bundle.default_properties.brushes.instance()
                    );
                }
            }
        );

        self.update_outline(bundle.drawing_resources, manager, grid, settings);
    }

    /// Merges all selected vertexes.
    #[inline]
    pub fn merge_vertexes(
        bundle: &StateUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        sides: bool
    )
    {
        let mut vertexes = HvHashSet::new();

        if sides
        {
            for vxs in manager.selected_brushes().filter_map(Brush::selected_sides_vertexes)
            {
                vertexes.extend(vxs.map(HashVec2));
            }
        }
        else
        {
            for vxs in manager.selected_brushes().filter_map(Brush::selected_vertexes)
            {
                vertexes.extend(vxs.map(HashVec2));
            }
        }

        if vertexes.len() < 3
        {
            return;
        }

        let vertexes = return_if_none!(convex_hull(vertexes));
        manager.deselect_selected_entities(edits_history);
        manager.spawn_brush(
            bundle.drawing_resources,
            ConvexPolygon::from(hv_vec![collect; vertexes]),
            edits_history,
            bundle.default_properties.brushes.instance()
        );
    }

    /// Executes a vertexes merge based on the active tool.
    #[inline]
    fn merge_tool(
        &mut self,
        bundle: &StateUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        inputs: &InputsPresses
    )
    {
        if inputs.alt_pressed()
        {
            match self
            {
                Self::Vertex(_) =>
                {
                    Self::merge_vertexes(bundle, manager, edits_history, false);
                    return;
                },
                Self::Side(_) =>
                {
                    Self::merge_vertexes(bundle, manager, edits_history, true);
                    return;
                },
                _ => ()
            };
        }

        // Place all vertexes of the selected brushes in one vector.
        let mut vertexes = HvHashSet::new();
        let mut brushes = manager.selected_brushes();

        let mut texture = {
            let first = brushes.next_value();
            let second = brushes.next_value();

            for brush in manager.selected_brushes()
            {
                vertexes.extend(brush.vertexes().map(HashVec2));
            }

            match (first.texture_settings(), second.texture_settings())
            {
                (Some(t_1), Some(t_2)) if *t_1 == *t_2 => t_1.clone().into(),
                _ => None
            }
        };

        while texture.is_some()
        {
            let brush = match brushes.next()
            {
                Some(brush) => brush,
                None => break
            };

            match brush.texture_settings()
            {
                Some(tex) if *tex == *texture.as_ref().unwrap() => (),
                _ => texture = None
            };
        }

        for brush in brushes
        {
            vertexes.extend(brush.vertexes().map(HashVec2));
        }

        self.draw_tool_despawn(
            bundle.drawing_resources,
            manager,
            edits_history,
            |manager, edits_history| {
                let mut poly =
                    ConvexPolygon::from(hv_vec![collect; convex_hull(vertexes).unwrap()]);

                if let Some(texture) = texture
                {
                    poly.set_texture_settings(texture);
                }

                manager.replace_selected_brushes(
                    bundle.drawing_resources,
                    Some(poly).into_iter(),
                    edits_history,
                    bundle.default_properties.brushes.instance()
                );
            }
        );
    }

    /// Executes the despawn of the drawn brushes if the draw tool is active.
    #[inline]
    fn draw_tool_despawn<F>(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        f: F
    ) where
        F: FnOnce(&mut EntitiesManager, &mut EditsHistory)
    {
        if let Self::Draw(t) = self
        {
            t.despawn_drawn_brushes(drawing_resources, manager, edits_history);
        }

        f(manager, edits_history);
    }

    /// Forcefully disables a tool and replaces it with another if certain circumstances are met.
    #[inline]
    pub fn fallback(&mut self, manager: &EntitiesManager, clipboard: &Clipboard)
    {
        let tool = match self
        {
            Self::Zoom(t) => &mut t.previous_active_tool,
            _ => self
        };

        match tool
        {
            Self::Draw(..) | Self::MapPreview(_) | Self::Thing(_) => return,
            Self::Entity(_) =>
            {
                if manager.entities_amount() == 0
                {
                    *tool = Self::default();
                }

                return;
            },
            Self::Clip(t) =>
            {
                if t.ongoing_multi_frame_change()
                {
                    return;
                }
            },
            Self::Side(t) =>
            {
                if t.intrusion()
                {
                    return;
                }
            },
            Self::Paint(_) =>
            {
                if manager.any_selected_entities() || clipboard.props_amount() != 0
                {
                    return;
                }
            },
            Self::Path(_) =>
            {
                if manager.any_selected_entities()
                {
                    return;
                }
            },
            Self::Zoom(_) => unreachable!(),
            _ => ()
        };

        if manager.brushes_amount() == 0
        {
            *tool = Self::default();
            return;
        }

        let selected_brushes_amount = manager.selected_brushes_amount();

        match tool
        {
            Self::Subtract(_) =>
            {
                if selected_brushes_amount == 1
                {
                    return;
                }
            },
            _ =>
            {
                if selected_brushes_amount != 0
                {
                    return;
                }
            }
        };

        *tool = Self::Entity(EntityTool::default());
    }

    //==============================================================
    // Draw

    /// Draws the tool.
    #[inline]
    pub fn draw(
        &self,
        bundle: &mut DrawBundle,
        manager: &EntitiesManager,
        grid: Grid,
        settings: &ToolsSettings
    )
    {
        /// Draws the tool.
        #[inline]
        fn draw_tool(
            tool: &ActiveTool,
            bundle: &mut DrawBundle,
            manager: &EntitiesManager,
            grid: Grid,
            settings: &ToolsSettings
        )
        {
            #[inline]
            fn paths(bundle: &mut DrawBundle, manager: &EntitiesManager)
            {
                let brushes = manager.brushes();

                for brush in manager
                    .visible_anchors(bundle.window, bundle.camera, bundle.drawer.grid())
                    .iter()
                {
                    brush.draw_anchors(brushes, &mut bundle.drawer);
                }

                for brush in manager
                    .visible_paths(bundle.window, bundle.camera, bundle.drawer.grid())
                    .iter()
                {
                    brush.draw_semitransparent_path(&mut bundle.drawer);
                }
            }

            // Things
            match tool
            {
                ActiveTool::Entity(_) | ActiveTool::Thing(_) | ActiveTool::Path(_) => (),
                ActiveTool::Paint(_) =>
                {
                    draw_selected_and_non_selected_things!(bundle, manager);
                },
                _ =>
                {
                    for thing in manager
                        .visible_things(bundle.window, bundle.camera, bundle.drawer.grid())
                        .iter()
                    {
                        thing.draw_opaque(
                            bundle.window,
                            bundle.camera,
                            &mut bundle.drawer,
                            bundle.things_catalog
                        );
                    }
                }
            };

            // Brushes
            match tool
            {
                ActiveTool::Draw(t) => t.draw(bundle, manager),
                ActiveTool::Entity(t) =>
                {
                    t.draw(bundle, manager, settings);
                    paths(bundle, manager);
                    return;
                },
                ActiveTool::Vertex(t) => t.draw(bundle, manager),
                ActiveTool::Side(t) => t.draw(bundle, manager),
                ActiveTool::Clip(t) => t.draw(bundle, manager),
                ActiveTool::Shatter(t) => t.draw(bundle, manager),
                ActiveTool::Subtract(t) => t.draw(bundle, manager),
                ActiveTool::Scale(t) => t.draw(bundle, manager),
                ActiveTool::Shear(t) => t.draw(bundle, manager),
                ActiveTool::Rotate(t) => t.draw(bundle, manager),
                ActiveTool::Flip(t) => t.draw(bundle, manager),
                ActiveTool::Path(t) =>
                {
                    t.draw(bundle, manager);

                    if t.simulation_active()
                    {
                        return;
                    }
                },
                ActiveTool::Paint(t) => t.draw(bundle, manager, grid),
                ActiveTool::Thing(t) => t.draw(bundle, manager),
                _ => unreachable!()
            };

            // Paths and sprites.
            paths(bundle, manager);
            draw_selected_and_non_selected_sprites!(bundle, manager, false);
        }

        match self
        {
            Self::Zoom(t) =>
            {
                t.draw(bundle);
                draw_tool(&t.previous_active_tool, bundle, manager, grid, settings);
            },
            _ => draw_tool(self, bundle, manager, grid, settings)
        };
    }

    /// Draws the map preview.
    #[inline]
    pub fn draw_map_preview(&self, bundle: &mut DrawBundleMapPreview, manager: &EntitiesManager)
    {
        match_or_panic!(self, Self::MapPreview(t), t).draw(bundle, manager);
    }

    /// Draws the UI bottom panel.
    #[inline]
    pub fn bottom_panel(
        &mut self,
        bundle: &mut StateUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        clipboard: &mut Clipboard
    )
    {
        match self
        {
            Self::Paint(t) => t.ui(bundle, clipboard),
            Self::Thing(t) =>
            {
                t.bottom_panel(bundle, manager, inputs, edits_history);
            },
            _ => ()
        };
    }

    /// Draws the tool UI.
    #[inline]
    pub fn ui(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        clipboard: &mut Clipboard,
        ui: &mut egui::Ui,
        settings: &mut ToolsSettings
    )
    {
        /// Same as above.
        #[inline]
        fn draw_ui(
            tool: &mut ActiveTool,
            drawing_resources: &DrawingResources,
            manager: &mut EntitiesManager,
            inputs: &InputsPresses,
            edits_history: &mut EditsHistory,
            clipboard: &mut Clipboard,
            ui: &mut egui::Ui,
            settings: &mut ToolsSettings
        )
        {
            match tool
            {
                ActiveTool::Thing(_) => ThingTool::left_panel(ui, settings),
                ActiveTool::Entity(t) => t.ui(ui, settings),
                ActiveTool::Rotate(t) => t.ui(ui, settings),
                ActiveTool::Draw(t) => t.ui(ui, settings),
                ActiveTool::Clip(t) => t.ui(ui),
                ActiveTool::Scale(t) => t.ui(ui, settings),
                ActiveTool::Shear(t) => t.ui(ui),
                ActiveTool::Flip(_) => FlipTool::ui(ui, settings),
                ActiveTool::Path(t) =>
                {
                    t.ui(drawing_resources, manager, edits_history, clipboard, inputs, ui);
                },
                ActiveTool::Zoom(tool) =>
                {
                    draw_ui(
                        tool.previous_active_tool.as_mut(),
                        drawing_resources,
                        manager,
                        inputs,
                        edits_history,
                        clipboard,
                        ui,
                        settings
                    );
                },
                _ => ()
            };
        }

        ui.separator();
        ui.style_mut().spacing.slider_width = 60f32;
        draw_ui(self, drawing_resources, manager, inputs, edits_history, clipboard, ui, settings);
    }

    /// Draws the subtool.
    #[inline]
    pub fn draw_subtools(
        &mut self,
        ui: &mut egui::Ui,
        bundle: &StateUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: Grid,
        buttons: &mut ToolsButtons,
        tool_change_conditions: &ChangeConditions
    )
    {
        match self
        {
            Self::Entity(t) => t.draw_subtools(ui, bundle, buttons, tool_change_conditions),
            Self::Thing(t) => t.draw_subtools(ui, bundle, buttons, tool_change_conditions),
            Self::Vertex(t) =>
            {
                t.draw_subtools(
                    ui,
                    bundle,
                    manager,
                    edits_history,
                    buttons,
                    tool_change_conditions
                );
            },
            Self::Side(t) =>
            {
                t.draw_subtools(
                    ui,
                    bundle,
                    manager,
                    edits_history,
                    buttons,
                    tool_change_conditions
                );
            },
            Self::Clip(t) => t.draw_subtools(ui, bundle, buttons, tool_change_conditions),
            Self::Rotate(t) => t.draw_subtools(ui, bundle, buttons, tool_change_conditions),
            Self::Path(t) =>
            {
                t.draw_subtools(
                    ui,
                    bundle,
                    manager,
                    edits_history,
                    buttons,
                    tool_change_conditions
                );
            },
            Self::Paint(t) =>
            {
                t.draw_subtools(ui, bundle, manager, grid, buttons, tool_change_conditions);
            },
            _ => ()
        };
    }
}

//=======================================================================//

#[allow(clippy::missing_docs_in_private_items)]
#[derive(ToolEnum, Clone, Copy, PartialEq, EnumIter, EnumSize, EnumFromUsize, Debug)]
pub(in crate::map::editor::state) enum Tool
{
    Square,
    Triangle,
    Circle,
    FreeDraw,
    Thing,
    Entity,
    Vertex,
    Side,
    Snap,
    Clip,
    Shatter,
    Hollow,
    Scale,
    Shear,
    Rotate,
    Flip,
    Intersection,
    Merge,
    Subtract,
    Path,
    Zoom,
    Paint
}

impl Tool
{
    /// Whether the bind associated with the tool was pressed.
    #[inline]
    #[must_use]
    pub fn just_pressed(self, key_inputs: &ButtonInput<KeyCode>, binds: &BindsKeyCodes) -> bool
    {
        self.bind().just_pressed(key_inputs, binds)
    }

    /// Returns the [`KeyCode`] to enable the tool, if any.
    #[inline]
    #[must_use]
    pub const fn keycode(self, binds: &BindsKeyCodes) -> Option<KeyCode>
    {
        self.bind().keycode(binds)
    }

    /// Returns a `str` representing this `Tool`'s associated `Keycode`.
    #[inline]
    #[must_use]
    pub fn keycode_str(self, binds: &BindsKeyCodes) -> &'static str
    {
        match self.keycode(binds)
        {
            Some(key) => key.to_str(),
            None => ""
        }
    }

    #[inline]
    #[must_use]
    const fn conditions_met(self, change_conditions: &ChangeConditions) -> bool
    {
        if change_conditions.ongoing_multi_frame_change ||
            change_conditions.ctrl_pressed ||
            change_conditions.space_pressed
        {
            return false;
        }

        match self
        {
            Self::Square | Self::Triangle | Self::Circle | Self::FreeDraw | Self::Zoom => true,
            Self::Thing =>
            {
                !change_conditions.things_catalog_empty ||
                    change_conditions.selected_things_amount != 0
            },
            Self::Entity => change_conditions.brushes_amount + change_conditions.things_amount > 0,
            Self::Paint =>
            {
                change_conditions.selected_brushes_amount + change_conditions.selected_things_amount >
                    0 ||
                    !change_conditions.no_props
            },
            Self::Vertex |
            Self::Side |
            Self::Clip |
            Self::Shatter |
            Self::Scale |
            Self::Shear |
            Self::Rotate |
            Self::Flip |
            Self::Hollow => change_conditions.selected_brushes_amount != 0,
            Self::Path =>
            {
                change_conditions.selected_platforms_amount != 0 ||
                    change_conditions.any_selected_possible_platforms
            },
            Self::Snap => change_conditions.vertex_rounding_availability,
            Self::Merge | Self::Intersection => change_conditions.selected_brushes_amount > 1,
            Self::Subtract =>
            {
                change_conditions.selected_brushes_amount == 1 &&
                    change_conditions.brushes_amount > 1
            },
        }
    }
}

//=======================================================================//

/// The subtools.
#[derive(EnumIter, EnumSize, SubToolEnum, Clone, Copy, PartialEq)]
pub(in crate::map::editor::state) enum SubTool
{
    /// Entity tool drag spawn.
    EntityDragSpawn,
    /// Vertex tool new vertex insert.
    VertexInsert,
    /// Vertex tool merge.
    VertexMerge,
    /// Vertex tool split.
    VertexSplit,
    /// Vertex tool polygon to path.
    VertexPolygonToPath,
    /// Side tool x-trusion.
    SideXtrusion,
    /// Side tool merge.
    SideMerge,
    /// Clip tool clip with brush side.
    ClipSide,
    /// Rotate tool move pivot.
    RotatePivot,
    /// Path tool free draw.
    PathFreeDraw,
    /// Path tool add node.
    PathInsertNode,
    /// Path tool movement simulation.
    PathSimulation,
    /// Pain tool prop creation.
    PaintCreation,
    /// Paint tool quick prop.
    PaintQuick,
    /// Thing tool thing change.
    ThingChange
}

impl SubTool
{
    #[inline]
    #[must_use]
    const fn conditions_met(self, change_conditions: &ChangeConditions) -> bool
    {
        if let Self::PathSimulation = self
        {
            return (change_conditions.path_simulation_active ||
                self.tool().conditions_met(change_conditions)) &&
                change_conditions.selected_platforms_amount != 0;
        }

        if !self.tool().conditions_met(change_conditions)
        {
            return false;
        }

        match self
        {
            Self::ThingChange => change_conditions.selected_things_amount != 0,
            Self::EntityDragSpawn | Self::PaintCreation =>
            {
                change_conditions.selected_brushes_amount + change_conditions.selected_things_amount !=
                    0
            },
            Self::VertexSplit => change_conditions.split_available,
            Self::VertexPolygonToPath => Tool::Path.conditions_met(change_conditions),
            Self::SideXtrusion => change_conditions.xtrusion_available,
            Self::PaintQuick => change_conditions.quick_prop,
            Self::VertexMerge | Self::SideMerge => change_conditions.vx_merge_available,
            Self::VertexInsert | Self::PathFreeDraw | Self::PathInsertNode | Self::RotatePivot =>
            {
                true
            },
            Self::ClipSide => change_conditions.selected_brushes_amount > 1,
            Self::PathSimulation => unreachable!()
        }
    }
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// A collection of information required to determine which tools can be enabled.
#[allow(clippy::missing_docs_in_private_items)]
pub(in crate::map::editor::state) struct ChangeConditions
{
    ongoing_multi_frame_change: bool,
    ctrl_pressed: bool,
    space_pressed: bool,
    vertex_rounding_availability: bool,
    path_simulation_active: bool,
    quick_prop: bool,
    vx_merge_available: bool,
    split_available: bool,
    xtrusion_available: bool,
    things_catalog_empty: bool,
    no_props: bool,
    brushes_amount: usize,
    selected_brushes_amount: usize,
    things_amount: usize,
    selected_things_amount: usize,
    selected_platforms_amount: usize,
    any_selected_possible_platforms: bool
}

impl ChangeConditions
{
    /// Returns a new [`ChangeConditions`].
    #[inline]
    pub fn new(
        inputs: &InputsPresses,
        clipboard: &Clipboard,
        core: &Core,
        things_catalog: &ThingsCatalog,
        manager: &EntitiesManager
    ) -> Self
    {
        Self {
            ongoing_multi_frame_change: core.active_tool.ongoing_multi_frame_change(),
            ctrl_pressed: inputs.ctrl_pressed(),
            space_pressed: inputs.space.pressed(),
            vertex_rounding_availability: Snap::new(&core.active_tool, manager) != Snap::None,
            path_simulation_active: core.active_tool.path_simulation_active(),
            quick_prop: clipboard.has_quick_prop(),
            vx_merge_available: core.active_tool.vx_merge_available(),
            split_available: core.active_tool.split_available(),
            xtrusion_available: core.active_tool.xtrusion_available(),
            selected_brushes_amount: manager.selected_brushes_amount(),
            brushes_amount: manager.brushes_amount(),
            selected_platforms_amount: manager.selected_moving_amount(),
            any_selected_possible_platforms: manager.any_selected_possible_moving(),
            things_catalog_empty: things_catalog.is_empty(),
            things_amount: manager.things_amount(),
            selected_things_amount: manager.selected_things_amount(),
            no_props: clipboard.no_props()
        }
    }
}
