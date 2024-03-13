//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::borrow::Cow;

use bevy::prelude::{ButtonInput, KeyCode, Vec2};
use bevy_egui::egui;
use proc_macros::{EnumFromUsize, EnumIter, EnumSize, SubToolEnum, ToolEnum};
use shared::{continue_if_none, match_or_panic, return_if_no_match, return_if_none, NextValue};

use super::{
    clip_tool::ClipTool,
    drag_area::DragArea,
    draw_selected_and_non_selected_things,
    draw_tool::{cursor_polygon::Status, DrawTool},
    entity_tool::EntityTool,
    flip_tool::FlipTool,
    map_preview::MapPreviewTool,
    paint_tool::PaintTool,
    path_tool::PathTool,
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
        drawer::{color::Color, drawing_resources::DrawingResources},
        editor::{
            state::{
                clipboard::Clipboard,
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
        hv_vec,
        thing::catalog::ThingsCatalog,
        HvVec
    },
    utils::{
        identifiers::{EntityId, Id},
        iterators::FilterSet,
        math::polygons::convex_hull,
        misc::FromToStr
    }
};

//=======================================================================//
// MACROS
//
//=======================================================================//

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

    /// Whever the tool can be enabled.
    #[must_use]
    fn change_conditions_met(self, change_conditions: &ChangeConditions) -> bool;

    /// Whever the tool is a subtool.
    #[must_use]
    fn subtool(self) -> bool;

    #[must_use]
    fn index(self) -> usize;
}

//=======================================================================//

pub(in crate::map::editor::state) trait EnabledTool
{
    type Item: ToolInterface;

    #[must_use]
    fn is_tool_enabled(&self, tool: Self::Item) -> bool;
}

//=======================================================================//

#[derive(PartialEq)]
enum Snap
{
    None,
    Brushes,
    Vertexes,
    Sides,
    PathNodes
}

type SnapFunc = fn(&mut Brush, &DrawingResources, Grid) -> Option<HvVec<(HvVec<u8>, Vec2)>>;

impl Snap
{
    #[inline]
    #[must_use]
    fn new(active_tool: &ActiveTool, manager: &EntitiesManager, alt_pressed: bool) -> Self
    {
        if active_tool.ongoing_multi_frame_changes() ||
            alt_pressed ||
            manager.selected_brushes_amount() == 0
        {
            return Self::None;
        }

        match active_tool
        {
            ActiveTool::Vertex(_) => Self::Vertexes,
            ActiveTool::Side(_) => Self::Sides,
            ActiveTool::Path(_) => Self::PathNodes,
            _ => Self::Brushes
        }
    }

    #[inline]
    #[must_use]
    const fn method(self) -> Option<SnapFunc>
    {
        match self
        {
            Self::None => None,
            Self::Brushes => Some(Brush::snap_vertexes),
            Self::Vertexes => Some(Brush::snap_selected_vertexes),
            Self::Sides => Some(Brush::snap_selected_sides),
            Self::PathNodes => Some(Brush::snap_selected_path_nodes)
        }
    }
}

//=======================================================================//

#[must_use]
#[derive(Clone, Copy, Default)]
pub(in crate::map::editor::state) enum EditingTarget
{
    #[default]
    Other,
    Draw,
    BrushFreeDraw(Status),
    Thing,
    Vertexes,
    Sides,
    Subtract,
    Path,
    PathFreeDraw
}

impl EditingTarget
{
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
            ActiveTool::Subtract(_) => Self::Subtract,
            ActiveTool::Zoom(_) => prev_value,
            _ => Self::Other
        }
    }

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

impl ActiveTool
{
    //==============================================================
    // Info

    #[inline]
    #[must_use]
    pub const fn ongoing_multi_frame_changes(&self) -> bool
    {
        match self
        {
            Self::Entity(t) => t.ongoing_multi_frame_changes(),
            Self::Clip(t) => t.ongoing_multi_frame_changes(),
            Self::Draw(t) => t.ongoing_multi_frame_changes(),
            Self::Path(t) => t.ongoing_multi_frame_changes(),
            Self::Rotate(t) => t.ongoing_multi_frame_changes(),
            Self::Scale(t) => t.ongoing_multi_frame_changes(),
            Self::Shear(t) => t.ongoing_multi_frame_changes(),
            Self::Side(t) => t.ongoing_multi_frame_changes(),
            Self::Vertex(t) => t.ongoing_multi_frame_changes(),
            Self::Paint(t) => t.ongoing_multi_frame_changes(),
            _ => false
        }
    }

    #[inline]
    pub fn editing_target(&self, prev_value: EditingTarget) -> EditingTarget
    {
        EditingTarget::new(self, prev_value)
    }

    #[inline]
    fn drag_selection(&self) -> DragArea
    {
        match &self
        {
            Self::Entity(t) => t.drag_selection(),
            Self::Subtract(t) => return t.drag_selection(),
            Self::Zoom(t) => return t.drag_selection(),
            Self::Vertex(t) => t.drag_selection(),
            Self::Side(t) => t.drag_selection(),
            Self::Path(t) => t.drag_selection(),
            _ => None
        }
        .unwrap_or_default()
    }

    #[inline]
    #[must_use]
    pub const fn path_simulation_active(&self) -> bool
    {
        return_if_no_match!(self, Self::Path(t), t, false).simulation_active()
    }

    #[inline]
    #[must_use]
    pub const fn entity_tool(&self) -> bool { matches!(self, Self::Entity(_)) }

    #[inline]
    #[must_use]
    pub const fn texture_tool(&self) -> bool
    {
        matches!(self, Self::Entity(_) | Self::Scale(_) | Self::Rotate(_) | Self::Flip(_))
    }

    #[inline]
    #[must_use]
    pub fn split_available(&self) -> bool
    {
        return_if_no_match!(self, Self::Vertex(t), t, false).split_available()
    }

    #[inline]
    #[must_use]
    fn xtrusion_available(&self) -> bool
    {
        return_if_no_match!(self, Self::Side(t), t, false).xtrusion_available()
    }

    #[inline]
    #[must_use]
    pub const fn map_preview(&self) -> bool { matches!(self, Self::MapPreview { .. }) }

    //==============================================================
    // Copy/Paste

    #[inline]
    #[must_use]
    pub const fn copy_paste_available(&self) -> bool
    {
        match self
        {
            Self::Draw(_) | Self::Zoom(_) | Self::MapPreview { .. } => false,
            Self::Shatter(_) | Self::Subtract(_) | Self::Flip(_) | Self::Thing(_) => true,
            Self::Entity(t) => !t.ongoing_multi_frame_changes(),
            Self::Vertex(t) => !t.ongoing_multi_frame_changes(),
            Self::Side(t) => !t.ongoing_multi_frame_changes(),
            Self::Clip(t) => !t.ongoing_multi_frame_changes(),
            Self::Scale(t) => !t.ongoing_multi_frame_changes(),
            Self::Shear(t) => !t.ongoing_multi_frame_changes(),
            Self::Rotate(t) => !t.ongoing_multi_frame_changes(),
            Self::Path(t) => t.copy_paste_available(),
            Self::Paint(t) => !t.ongoing_multi_frame_changes()
        }
    }

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
                return_if_none!(t.path_beneath_cursor(bundle, manager, inputs))
            );

            return;
        }

        clipboard.copy(manager.selected_entities());
    }

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
                    manager,
                    edits_history,
                    return_if_none!(t.path_beneath_cursor(bundle, manager, inputs))
                );

                return;
            },
            Self::Entity(t) => t.remove_highlighted_entity(),
            _ => ()
        };

        clipboard.copy(manager.selected_entities());
        manager.despawn_selected_entities(edits_history);
        manager.schedule_outline_update();
    }

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
                manager,
                edits_history,
                return_if_none!(t.path_beneath_cursor(bundle, manager, inputs))
            );

            return;
        }

        if !clipboard.has_copy_data()
        {
            return;
        }

        if let Self::Vertex(_) | Self::Side(_) = self
        {
            for mut brush in manager.selected_brushes_mut()
            {
                brush.deselect_vertexes_no_indexes();
            }
        }

        manager.deselect_selected_entities(edits_history);
        clipboard.paste(bundle, manager, edits_history, bundle.cursor.world_snapped());
        manager.schedule_outline_update();
    }

    #[inline]
    pub fn update_outline(
        &mut self,
        manager: &EntitiesManager,
        settings: &ToolsSettings,
        grid: Grid
    )
    {
        match self
        {
            Self::Shear(t) => t.update_outline(manager, grid),
            Self::Scale(t) => t.update_outline(manager, grid, settings),
            Self::Rotate(t) => t.update_pivot(manager, settings),
            Self::Flip(t) => t.update_outline(manager, grid),
            Self::Paint(t) => t.update_outline(manager, grid),
            _ => ()
        };
    }

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

    #[inline]
    pub fn update_overall_node(&mut self, manager: &EntitiesManager)
    {
        return_if_no_match!(self, Self::Path(t), t).update_overall_node(manager);
    }

    #[inline]
    pub fn update_overall_thing_info(&mut self, manager: &EntitiesManager)
    {
        return_if_no_match!(self, Self::Thing(t), t).update_overall_thing_info(manager);
    }

    //==============================================================
    // Undo/Redo

    #[inline]
    #[must_use]
    pub const fn select_all_available(&self) -> bool { !self.ongoing_multi_frame_changes() }

    #[inline]
    pub fn select_all(
        &mut self,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        settings: &ToolsSettings,
        grid: Grid
    )
    {
        assert!(self.select_all_available(), "Select all is not available.");

        match self
        {
            Self::Subtract(t) => t.select_non_selected_brushes(manager, edits_history),
            Self::Vertex(_) | Self::Side(_) =>
            {
                edits_history.vertexes_selection_cluster(
                    manager.selected_brushes_mut().filter_map(|mut brush| {
                        brush.select_all_vertexes().map(|idxs| (brush.id(), idxs))
                    })
                );
            },
            Self::Path(_) =>
            {
                edits_history.path_nodes_selection_cluster(
                    manager.selected_platforms_mut().filter_map(|mut brush| {
                        brush.select_all_path_nodes().map(|idxs| (brush.id(), idxs))
                    })
                );
            },
            _ => manager.select_all_entities(edits_history)
        };

        self.update_outline(manager, settings, grid);
    }

    //==============================================================
    // Undo/Redo

    #[inline]
    #[must_use]
    pub const fn undo_redo_available(&self) -> bool { !self.ongoing_multi_frame_changes() }

    //==============================================================
    // Update

    #[inline]
    pub fn toggle_map_preview(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &EntitiesManager
    )
    {
        *self = match self
        {
            Self::MapPreview(t) => std::mem::take(t.prev_tool()),
            _ => MapPreviewTool::tool(drawing_resources, self, manager)
        };
    }

    #[inline]
    pub fn disable_subtool(&mut self)
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
                t.update(bundle, manager, inputs, edits_history, grid, settings);
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
                    *self = EntityTool::tool(DragArea::default());
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
                t.update(bundle, manager, inputs, edits_history, settings, grid);
            },
            Self::Zoom(t) =>
            {
                *self = std::mem::take(return_if_none!(t.update(bundle, inputs)));
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

    #[inline]
    pub fn change(
        &mut self,
        tool: Tool,
        bundle: &StateUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        settings: &ToolsSettings,
        grid: Grid,
        tool_change_conditions: &ChangeConditions,
        alt_pressed: bool
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
                self.snap_tool(
                    bundle.drawing_resources,
                    manager,
                    edits_history,
                    settings,
                    grid,
                    alt_pressed
                );
                return;
            },
            Tool::Zoom => ZoomTool::tool(self.drag_selection(), self),
            Tool::Subtract => SubtractTool::tool(self.drag_selection()),
            Tool::Clip => ClipTool::tool(),
            Tool::Shatter => ShatterTool::tool(),
            Tool::Hollow =>
            {
                self.hollow_tool(bundle, manager, edits_history, grid);
                return;
            },
            Tool::Scale => ScaleTool::tool(manager, grid, settings),
            Tool::Shear => ShearTool::tool(manager, grid),
            Tool::Rotate => RotateTool::tool(manager, settings),
            Tool::Flip => FlipTool::tool(manager),
            Tool::Intersection =>
            {
                self.intersection_tool(manager, edits_history, settings, grid);
                return;
            },
            Tool::Merge =>
            {
                self.merge_tool(manager, edits_history, alt_pressed);
                return;
            },
            Tool::Path => PathTool::tool(self.drag_selection()),
            Tool::Paint => PaintTool::tool(),
            Tool::Thing => ThingTool::tool(manager)
        };
    }

    #[inline]
    pub fn snap_tool(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        settings: &ToolsSettings,
        grid: Grid,
        alt_pressed: bool
    )
    {
        let snap = Snap::new(self, manager, alt_pressed);
        let mut snapped = false;

        if let Snap::PathNodes = snap
        {
            for mut brush in manager.selected_platforms_mut()
            {
                edits_history.path_nodes_snap(
                    brush.id(),
                    continue_if_none!(brush.snap_selected_path_nodes(drawing_resources, grid))
                );
            }
        }
        else
        {
            let method = return_if_none!(snap.method());

            for mut brush in manager.selected_brushes_mut()
            {
                edits_history.vertexes_snap(
                    brush.id(),
                    continue_if_none!((method)(&mut brush, drawing_resources, grid))
                );
                snapped = true;
            }

            if snapped
            {
                self.update_outline(manager, settings, grid);
            }
        }
    }

    #[inline]
    fn hollow_tool(
        &mut self,
        bundle: &StateUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: Grid
    )
    {
        let mut wall_brushes = hv_vec![capacity; manager.selected_brushes_amount()];
        let valid = manager.test_operation_validity(|manager| {
            manager.selected_brushes().find_map(|brush| {
                match brush.hollow(bundle.drawing_resources, grid.size_f32())
                {
                    Some(polys) =>
                    {
                        wall_brushes.extend(polys);
                        None
                    },
                    None => brush.id().into()
                }
            })
        });

        if !valid
        {
            return;
        }

        if wall_brushes.is_empty()
        {
            return;
        }

        self.instantaneous_tools_fallback(manager, edits_history);
        manager.replace_selected_brushes(wall_brushes.into_iter(), edits_history);
    }

    #[inline]
    fn intersection_tool(
        &mut self,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        settings: &ToolsSettings,
        grid: Grid
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
                manager.despawn_selected_brushes(edits_history);
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

        manager.despawn_selected_brushes(edits_history);

        // Spawn the intersection brush.
        if success
        {
            self.instantaneous_tools_fallback(manager, edits_history);
            manager.spawn_brushes(Some(intersection_polygon).into_iter(), edits_history);
        }

        self.update_outline(manager, settings, grid);
    }

    #[inline]
    pub fn merge_vertexes(
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        sides: bool
    )
    {
        let mut brushes_with_selected_sides = 0;
        let mut vertexes = Vec::with_capacity(manager.selected_brushes_amount());

        if sides
        {
            for vxs in manager.selected_brushes().filter_map(Brush::selected_sides_vertexes)
            {
                vertexes.extend(vxs);
                brushes_with_selected_sides += 1;
            }
        }
        else
        {
            for vxs in manager.selected_brushes().filter_map(Brush::selected_vertexes)
            {
                vertexes.extend(vxs);
                brushes_with_selected_sides += 1;
            }
        }

        if brushes_with_selected_sides > 1 && vertexes.len() > 2
        {
            manager.deselect_selected_entities(edits_history);
            manager.spawn_brush(
                ConvexPolygon::from(
                    hv_vec![collect; convex_hull(std::mem::take(&mut vertexes).into_iter())]
                ),
                edits_history
            );
        }
    }

    #[inline]
    fn merge_tool(
        &mut self,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        alt_pressed: bool
    )
    {
        if alt_pressed
        {
            match self
            {
                Self::Vertex(_) =>
                {
                    Self::merge_vertexes(manager, edits_history, false);
                    return;
                },
                Self::Side(_) =>
                {
                    Self::merge_vertexes(manager, edits_history, true);
                    return;
                },
                _ => ()
            };
        }

        // Place all vertexes of the selected brushes in one vector.
        let mut vertexes = Vec::with_capacity(manager.selected_brushes_amount() * 3);

        for vxs in manager.selected_brushes().map(Brush::vertexes)
        {
            for vx in vxs
            {
                if !vertexes.contains(&vx)
                {
                    vertexes.push(vx);
                }
            }
        }

        // Spawn and despawn.
        self.instantaneous_tools_fallback(manager, edits_history);

        manager.replace_selected_brushes(
            Some(ConvexPolygon::from(hv_vec![collect; convex_hull(vertexes.into_iter())]))
                .into_iter(),
            edits_history
        );
    }

    #[inline]
    fn instantaneous_tools_fallback(
        &mut self,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory
    )
    {
        if let Self::Draw(t) = self
        {
            t.despawn_drawn_brushes(manager, edits_history);
        }
    }

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
                if t.ongoing_multi_frame_changes()
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
                if manager.selected_entities_amount() != 0 || clipboard.props_amount() != 0
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

    #[inline]
    pub fn draw(
        &self,
        bundle: &mut DrawBundle,
        manager: &EntitiesManager,
        settings: &ToolsSettings,
        show_tooltips: bool
    )
    {
        match self
        {
            Self::Zoom(t) =>
            {
                t.draw(bundle);
                Self::draw_tool(&t.previous_active_tool, bundle, manager, settings, show_tooltips);
            },
            _ => Self::draw_tool(self, bundle, manager, settings, show_tooltips)
        };
    }

    #[inline]
    pub fn draw_map_preview(&self, bundle: &mut DrawBundleMapPreview, manager: &EntitiesManager)
    {
        match_or_panic!(self, Self::MapPreview(t), t).draw(bundle, manager);
    }

    #[inline]
    fn draw_tool(
        tool: &Self,
        bundle: &mut DrawBundle,
        manager: &EntitiesManager,
        settings: &ToolsSettings,
        show_tooltips: bool
    )
    {
        #[inline]
        fn sprites_and_anchors(bundle: &mut DrawBundle, manager: &EntitiesManager)
        {
            let DrawBundle {
                window,
                drawer,
                camera,
                ..
            } = bundle;

            let brushes = manager.brushes();

            for brush in manager.visible_anchors(window, camera).iter()
            {
                brush.draw_anchors(brushes, drawer);
            }

            for brush in manager.visible_sprite_highlights(window, camera).iter()
            {
                brush.draw_sprite_highlight(drawer);
            }

            for brush in manager.visible_sprites(window, camera).iter()
            {
                let color = if manager.is_selected(brush.id())
                {
                    Color::SelectedBrush
                }
                else
                {
                    Color::NonSelectedBrush
                };

                brush.draw_sprite(drawer, color);
            }
        }

        // Things
        match tool
        {
            ActiveTool::Entity(_) | ActiveTool::Thing(_) => (),
            ActiveTool::Paint(_) =>
            {
                draw_selected_and_non_selected_things!(bundle, manager);
            },
            _ =>
            {
                for thing in manager.visible_things(bundle.window, bundle.camera).iter()
                {
                    thing.draw_opaque(&mut bundle.drawer, bundle.things_catalog);
                }
            }
        };

        // Brushes
        match tool
        {
            Self::Draw(t) => t.draw(bundle, manager, show_tooltips),
            Self::Entity(t) => t.draw(bundle, manager, settings, show_tooltips),
            Self::Vertex(t) => t.draw(bundle, manager, show_tooltips),
            Self::Side(t) => t.draw(bundle, manager, show_tooltips),
            Self::Clip(t) => t.draw(bundle, manager),
            Self::Shatter(t) => t.draw(bundle, manager),
            Self::Subtract(t) => t.draw(bundle, manager),
            Self::Scale(t) => t.draw(bundle, manager),
            Self::Shear(t) => t.draw(bundle, manager),
            Self::Rotate(t) => t.draw(bundle, manager),
            Self::Flip(t) => t.draw(bundle, manager),
            Self::Path(t) =>
            {
                if !t.simulation_active()
                {
                    sprites_and_anchors(bundle, manager);
                }

                t.draw(bundle, manager, show_tooltips);
                return;
            },
            Self::Paint(t) => t.draw(bundle, manager),
            Self::Thing(t) => t.draw(bundle, manager),
            _ => unreachable!()
        };

        // Non simulation common stuff.
        for brush in manager.visible_paths(bundle.window, bundle.camera).iter()
        {
            brush.draw_semitransparent_path(&mut bundle.drawer);
        }

        sprites_and_anchors(bundle, manager);
    }

    #[inline]
    #[must_use]
    pub fn bottom_panel(
        &mut self,
        bundle: &mut StateUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        clipboard: &mut Clipboard
    ) -> bool
    {
        match self
        {
            Self::Paint(t) => return t.ui(bundle, clipboard),
            Self::Thing(t) =>
            {
                t.bottom_panel(bundle, manager, inputs, edits_history);
            },
            _ => ()
        };

        false
    }

    #[inline]
    #[must_use]
    pub fn ui(
        &mut self,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        clipboard: &mut Clipboard,
        ui: &mut egui::Ui,
        settings: &mut ToolsSettings
    ) -> bool
    {
        #[inline]
        #[must_use]
        fn draw_ui(
            tool: &mut ActiveTool,
            manager: &mut EntitiesManager,
            inputs: &InputsPresses,
            edits_history: &mut EditsHistory,
            clipboard: &mut Clipboard,
            ui: &mut egui::Ui,
            settings: &mut ToolsSettings
        ) -> bool
        {
            match tool
            {
                ActiveTool::Thing(t) =>
                {
                    return t.left_panel(ui, manager, inputs, edits_history, clipboard, settings);
                },
                ActiveTool::Entity(t) => t.ui(ui, settings),
                ActiveTool::Rotate(t) => t.ui(ui, settings),
                ActiveTool::Draw(t) => t.ui(ui, settings),
                ActiveTool::Clip(t) => t.ui(ui),
                ActiveTool::Scale(t) => t.ui(ui, settings),
                ActiveTool::Shear(t) => t.ui(ui),
                ActiveTool::Flip(_) => FlipTool::ui(ui, settings),
                ActiveTool::Path(t) => return t.ui(manager, edits_history, clipboard, inputs, ui),
                ActiveTool::Zoom(tool) =>
                {
                    return draw_ui(
                        tool.previous_active_tool.as_mut(),
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

            false
        }

        ui.separator();
        ui.style_mut().spacing.slider_width = 60f32;
        draw_ui(self, manager, inputs, edits_history, clipboard, ui, settings)
    }

    #[inline]
    pub fn draw_sub_tools(
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
            Self::Entity(t) => t.draw_sub_tools(ui, bundle, buttons, tool_change_conditions),
            Self::Thing(t) => t.draw_sub_tools(ui, bundle, buttons, tool_change_conditions),
            Self::Vertex(t) =>
            {
                t.draw_sub_tools(
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
                t.draw_sub_tools(
                    ui,
                    bundle,
                    manager,
                    edits_history,
                    buttons,
                    tool_change_conditions
                );
            },
            Self::Clip(t) => t.draw_sub_tools(ui, bundle, buttons, tool_change_conditions),
            Self::Rotate(t) => t.draw_sub_tools(ui, bundle, buttons, tool_change_conditions),
            Self::Path(t) =>
            {
                t.draw_sub_tools(
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
                t.draw_sub_tools(ui, bundle, manager, grid, buttons, tool_change_conditions);
            },
            _ => ()
        };
    }
}

//=======================================================================//

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
    #[inline]
    #[must_use]
    pub fn just_pressed(self, key_inputs: &ButtonInput<KeyCode>, binds: &BindsKeyCodes) -> bool
    {
        self.bind().just_pressed(key_inputs, binds)
    }

    #[inline]
    #[must_use]
    pub fn keycode(self, binds: &BindsKeyCodes) -> Option<KeyCode> { self.bind().keycode(binds) }

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
}

//=======================================================================//

#[derive(EnumIter, EnumSize, SubToolEnum, Clone, Copy, PartialEq)]
pub(in crate::map::editor::state) enum SubTool
{
    EntityDragSpawn,
    EntityAnchor,
    VertexInsert,
    VertexMerge,
    VertexSplit,
    VertexPolygonToPath,
    SideXtrusion,
    SideMerge,
    ClipSide,
    RotatePivot,
    PathFreeDraw,
    PathAddNode,
    PathSimulation,
    PaintCreation,
    PaintQuick,
    ThingChange
}

impl SubTool
{
    #[inline]
    #[must_use]
    fn key_combo(self, binds: &BindsKeyCodes) -> Cow<str>
    {
        match self
        {
            Self::EntityDragSpawn | Self::SideXtrusion | Self::RotatePivot =>
            {
                Cow::Borrowed("Alt + directional key, or mouse drag")
            },
            Self::VertexMerge | Self::SideMerge =>
            {
                Cow::Owned(format!("Alt + {}", Tool::Merge.keycode_str(binds)))
            },
            Self::ClipSide => Cow::Borrowed("Alt"),
            Self::EntityAnchor => Cow::Borrowed("Right mouse click"),
            Self::VertexInsert | Self::PathFreeDraw | Self::PathAddNode | Self::ThingChange =>
            {
                Cow::Borrowed("Alt + left mouse click")
            },
            Self::PathSimulation | Self::VertexSplit | Self::PaintCreation =>
            {
                Cow::Borrowed("Enter")
            },
            Self::VertexPolygonToPath | Self::PaintQuick => Cow::Borrowed("Shift + Enter")
        }
    }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

pub(in crate::map::editor::state) struct ChangeConditions
{
    ongoing_multi_frame_changes:  bool,
    ctrl_pressed:                 bool,
    space_pressed:                bool,
    alt_pressed:                  bool,
    vertex_rounding_availability: bool,
    path_simulation_active:       bool,
    quick_prop:                   bool,
    split_available:              bool,
    xtrusion_available:           bool,
    things_catalog_empty:         bool,
    no_props:                     bool,
    brushes_amount:               usize,
    selected_brushes_amount:      usize,
    things_amount:                usize,
    selected_things_amount:       usize,
    selected_paths_amount:        usize,
    settings:                     ToolsSettings
}

impl ChangeConditions
{
    #[inline]
    pub fn new(
        inputs: &InputsPresses,
        clipboard: &Clipboard,
        core: &Core,
        things_catalog: &ThingsCatalog,
        manager: &EntitiesManager,
        settings: &ToolsSettings
    ) -> Self
    {
        let alt_pressed = inputs.alt_pressed();

        Self {
            ongoing_multi_frame_changes: core.active_tool.ongoing_multi_frame_changes(),
            ctrl_pressed: inputs.ctrl_pressed(),
            space_pressed: inputs.space.pressed(),
            alt_pressed,
            vertex_rounding_availability: Snap::new(&core.active_tool, manager, alt_pressed) !=
                Snap::None,
            path_simulation_active: core.active_tool.path_simulation_active(),
            quick_prop: clipboard.has_quick_prop(),
            split_available: core.active_tool.split_available(),
            xtrusion_available: core.active_tool.xtrusion_available(),
            selected_brushes_amount: manager.selected_brushes_amount(),
            brushes_amount: manager.brushes_amount(),
            selected_paths_amount: manager.selected_platforms_amount(),
            settings: *settings,
            things_catalog_empty: things_catalog.is_empty(),
            things_amount: manager.things_amount(),
            selected_things_amount: manager.selected_things_amount(),
            no_props: clipboard.no_props()
        }
    }
}
