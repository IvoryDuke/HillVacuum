//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::window::Window;
use bevy_egui::egui;
use glam::Vec2;
use hill_vacuum_shared::{return_if_no_match, return_if_none};

use super::{
    cursor_delta::CursorDelta,
    draw_selected_and_non_selected_brushes,
    tool::{ActiveTool, DisableSubtool, EnabledTool, OngoingMultiframeChange, SubTool}
};
use crate::{
    map::{
        drawer::{color::Color, drawing_resources::DrawingResources},
        editor::{
            cursor::Cursor,
            state::{
                clipboard::{
                    prop::{Prop, PropScreenshotTimer},
                    Clipboard,
                    PROP_SCREENSHOT_SIZE
                },
                core::{bottom_panel, tool::subtools_buttons},
                edits_history::EditsHistory,
                grid::Grid,
                manager::EntitiesManager,
                ui::{centered_window, ToolsButtons, UiBundle}
            },
            DrawBundle,
            ToolUpdateBundle
        },
        thing::{catalog::ThingsCatalog, ThingInterface}
    },
    utils::hull::Hull,
    INDEXES
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The state of the tool.
#[must_use]
enum Status
{
    /// Inactive.
    Inactive(()),
    /// Setting the pivot of the new [`Prop`].
    SetPivot(Hull),
    /// Waiting for the [`Prop`] screenshot to be taken.
    PropCreationScreenshot(PropScreenshotTimer, Prop),
    /// Creating a [`Prop`] from the UI.
    PropCreationUi(Prop),
    /// Preparing to spawn the quick [`Prop`].
    QuickPropSetup,
    /// Spawning copies of a [`Prop`].
    Paint(PaintingProp, CursorDelta)
}

impl Default for Status
{
    #[inline]
    fn default() -> Self { Self::Inactive(()) }
}

impl EnabledTool for Status
{
    type Item = SubTool;

    #[inline]
    fn is_tool_enabled(&self, tool: Self::Item) -> bool
    {
        tool == match self
        {
            Status::SetPivot(_) |
            Status::PropCreationScreenshot(..) |
            Status::PropCreationUi(..) => SubTool::PaintCreation,
            Status::QuickPropSetup => SubTool::PaintQuick,
            _ => return false
        }
    }
}

//=======================================================================//

/// The source of the [`Prop`] to draw.
#[derive(Clone, Copy)]
enum PaintingProp
{
    /// Quick [`Prop`].
    Quick,
    /// Slotted [`Prop`].
    Slotted
}

#[allow(clippy::missing_docs_in_private_items)]
type PropSpawnFunc = fn(
    &mut Clipboard,
    &DrawingResources,
    &ThingsCatalog,
    &mut EntitiesManager,
    &mut EditsHistory,
    &Grid,
    Vec2
) -> bool;

impl PaintingProp
{
    /// The function that executes a [`Prop`] copy spawn.
    #[inline]
    #[must_use]
    fn spawn_func(self) -> PropSpawnFunc
    {
        match self
        {
            Self::Quick => Clipboard::spawn_quick_prop,
            Self::Slotted => Clipboard::spawn_selected_prop
        }
    }
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The paint tool.
pub(in crate::map::editor::state::core) struct PaintTool
{
    /// The slot where to store the created [`Prop`].
    slot:                    String,
    /// The state of the tool.
    status:                  Status,
    /// The maximum height of the bottom panel.
    max_bottom_panel_height: f32
}

impl DisableSubtool for PaintTool
{
    #[inline]
    fn disable_subtool(&mut self)
    {
        if !matches!(self.status, Status::Inactive(()) | Status::Paint(..))
        {
            self.status = Status::default();
        }
    }
}

impl OngoingMultiframeChange for PaintTool
{
    #[inline]
    fn ongoing_multi_frame_change(&self) -> bool { matches!(self.status, Status::Paint(..)) }
}

impl PaintTool
{
    /// Returns an [`ActiveTool`] in its paint tool variant.
    #[inline]
    pub fn tool() -> ActiveTool
    {
        ActiveTool::Paint(PaintTool {
            slot:                    String::new(),
            status:                  Status::default(),
            max_bottom_panel_height: 0f32
        })
    }

    /// Returns the cursor position used by the tool.
    #[inline]
    #[must_use]
    const fn cursor_pos(cursor: &Cursor) -> Vec2 { cursor.world_snapped() }

    #[inline]
    #[must_use]
    fn quick_spawn(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        manager: &mut EntitiesManager,
        clipboard: &mut Clipboard,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        cursor_pos: Vec2
    ) -> bool
    {
        if clipboard.spawn_quick_prop(
            drawing_resources,
            things_catalog,
            manager,
            edits_history,
            grid,
            cursor_pos
        )
        {
            self.status = Status::Paint(PaintingProp::Quick, CursorDelta::new(cursor_pos));
            return true;
        }

        false
    }

    /// Updates the tool.
    #[allow(clippy::cast_precision_loss)]
    #[inline]
    pub fn update(&mut self, bundle: &mut ToolUpdateBundle)
    {
        let ToolUpdateBundle {
            images,
            user_textures,
            prop_cameras,
            paint_tool_camera,
            cursor,
            drawing_resources,
            things_catalog,
            inputs,
            clipboard,
            manager,
            edits_history,
            grid,
            ..
        } = bundle;

        let cursor_pos = Self::cursor_pos(cursor);

        match &mut self.status
        {
            Status::Inactive(()) =>
            {
                if inputs.back.just_pressed()
                {
                    clipboard.delete_selected_prop(prop_cameras);
                }

                if inputs.enter.just_pressed() && manager.any_selected_entities()
                {
                    self.status =
                        Status::SetPivot(Self::outline(things_catalog, manager, grid).unwrap());
                }

                if !inputs.left_mouse.just_pressed()
                {
                    return;
                }

                if inputs.alt_pressed() &&
                    self.quick_spawn(
                        drawing_resources,
                        things_catalog,
                        manager,
                        clipboard,
                        edits_history,
                        grid,
                        cursor_pos
                    )
                {
                    return;
                }

                if clipboard.spawn_selected_prop(
                    drawing_resources,
                    things_catalog,
                    manager,
                    edits_history,
                    grid,
                    cursor_pos
                )
                {
                    self.status =
                        Status::Paint(PaintingProp::Slotted, CursorDelta::new(cursor_pos));
                }
            },
            Status::SetPivot(hull) =>
            {
                if !inputs.left_mouse.just_pressed() || !hull.contains_point(cursor_pos)
                {
                    return;
                }

                let mut prop = Prop::new(
                    drawing_resources,
                    things_catalog,
                    grid,
                    manager.selected_entities(),
                    cursor_pos,
                    None
                );

                Clipboard::assign_camera_to_prop(
                    images,
                    paint_tool_camera,
                    user_textures,
                    *drawing_resources,
                    things_catalog,
                    grid,
                    &mut prop
                );

                self.status = Status::PropCreationScreenshot(PropScreenshotTimer::new(None), prop);
            },
            Status::PropCreationScreenshot(timer, prop) =>
            {
                if timer.update(prop_cameras)
                {
                    self.status = Status::PropCreationUi(std::mem::take(prop));
                    paint_tool_camera.0.is_active = false;
                }
            },
            Status::PropCreationUi(prop) =>
            {
                if !inputs.enter.just_pressed()
                {
                    return;
                }

                if self.slot.is_empty()
                {
                    clipboard.create_quick_prop(std::mem::take(prop));
                    self.status = Status::QuickPropSetup;
                    return;
                }

                if let Ok(slot) = self.slot.parse()
                {
                    clipboard.insert_prop(std::mem::take(prop), slot);
                    self.status = Status::default();
                }

                self.slot.clear();
            },
            Status::QuickPropSetup =>
            {
                if !inputs.left_mouse.just_pressed()
                {
                    return;
                }

                _ = self.quick_spawn(
                    drawing_resources,
                    things_catalog,
                    manager,
                    clipboard,
                    edits_history,
                    grid,
                    cursor_pos
                );
            },
            Status::Paint(prop, drag) =>
            {
                if cursor.moved()
                {
                    drag.update(cursor, grid, |_| {
                        prop.spawn_func()(
                            clipboard,
                            drawing_resources,
                            things_catalog,
                            manager,
                            edits_history,
                            grid,
                            cursor_pos
                        );
                    });
                }

                if !inputs.left_mouse.pressed()
                {
                    self.status = Status::default();
                }
            }
        };
    }

    /// Returns the selected entities' outline.
    #[inline]
    #[must_use]
    fn outline(
        things_catalog: &ThingsCatalog,
        manager: &EntitiesManager,
        grid: &Grid
    ) -> Option<Hull>
    {
        Hull::from_hulls_iter(
            manager
                .selected_brushes()
                .map(|brush| {
                    let mut hull = brush.polygon_hull();

                    if let Some(pivot) = brush.sprite_pivot()
                    {
                        hull = Hull::from_points([hull.top_right(), hull.bottom_left(), pivot]);
                    }

                    if let Some(p_hull) = brush.path_hull()
                    {
                        hull = hull.merged(&p_hull);
                    }

                    hull
                })
                .chain(
                    manager
                        .selected_things()
                        .map(|thing| thing.thing_hull(things_catalog))
                )
        )
        .map(|hull| grid.snap_hull(&hull))
    }

    /// Updates the selected entities' outline.
    #[inline]
    pub fn update_outline(
        &mut self,
        things_catalog: &ThingsCatalog,
        manager: &EntitiesManager,
        grid: &Grid
    )
    {
        *return_if_no_match!(&mut self.status, Status::SetPivot(hull), hull) =
            return_if_none!(Self::outline(things_catalog, manager, grid));
    }

    /// Draws the UI.
    #[allow(clippy::cast_precision_loss)]
    #[inline]
    pub fn ui(&mut self, egui_context: &egui::Context, bundle: &mut UiBundle)
    {
        /// The size of the frame of the [`Prop`]s previews.
        const PREVIEW_SIZE: egui::Vec2 = egui::Vec2::new(
            PROP_SCREENSHOT_SIZE.x as f32 * 0.4f32,
            PROP_SCREENSHOT_SIZE.y as f32 * 0.4f32
        );

        let UiBundle {
            window, clipboard, ..
        } = bundle;

        if let Status::PropCreationUi(prop) = &self.status
        {
            self.prop_creation_window(window, egui_context, clipboard, prop.screenshot());
        }

        if clipboard.props_amount() == 0
        {
            return;
        }

        let clicked = return_if_none!(bottom_panel(
            egui_context,
            "props",
            &mut self.max_bottom_panel_height,
            PREVIEW_SIZE,
            clipboard.selected_prop_index(),
            clipboard.ui_iter(),
            |ui, texture| {
                (
                    ui.vertical(|ui| {
                        let response =
                            ui.add(egui::ImageButton::new((texture.tex_id, PREVIEW_SIZE)));
                        ui.label(INDEXES[texture.index]);
                        response
                    })
                    .inner,
                    texture.index
                )
            }
        ));
        clipboard.set_selected_prop_index(clicked);
    }

    /// Draws the prop creation window.
    #[allow(clippy::cast_precision_loss)]
    #[inline]
    fn prop_creation_window(
        &mut self,
        window: &Window,
        egui_context: &egui::Context,
        clipboard: &Clipboard,
        texture: egui::TextureId
    )
    {
        /// The size of the frame of the new [`Prop`] screenshot.
        const PROP_SNAPSHOT_FRAME: egui::Vec2 =
            egui::Vec2::new(PROP_SCREENSHOT_SIZE.x as f32, PROP_SCREENSHOT_SIZE.y as f32);

        let response = centered_window(window, "Prop Creation")
            .default_width(300f32)
            .show(egui_context, |ui| {
                ui.vertical_centered(|ui| {
                    let response = ui.horizontal(|ui| {
                        ui.add_space(8f32);
                        ui.label("Slot number (press Enter to confirm):");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.slot).desired_width(f32::INFINITY)
                        )
                        .has_focus()
                    });

                    if clipboard.props_amount() == 0
                    {
                        ui.label("No used slots");
                    }
                    else
                    {
                        ui.label(format!(
                            "Currently used slots: 0 to {}",
                            clipboard.props_amount() - 1
                        ));
                    }

                    ui.image((texture, PROP_SNAPSHOT_FRAME));

                    response.inner
                })
                .inner
            })
            .unwrap();

        egui_context.move_to_top(response.response.layer_id);
    }

    /// Draws the tool.
    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle)
    {
        draw_selected_and_non_selected_brushes!(bundle);
        bundle
            .drawer
            .square_highlight(Self::cursor_pos(bundle.cursor), Color::ToolCursor);

        match &self.status
        {
            Status::SetPivot(hull) => bundle.drawer.hull(hull, Color::Hull),
            Status::PropCreationScreenshot(_, prop) => prop.draw(bundle, None),
            _ => ()
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
        subtools_buttons!(
            self.status,
            ui,
            bundle,
            buttons,
            (
                PaintCreation,
                Status::SetPivot(
                    Self::outline(bundle.things_catalog, bundle.manager, bundle.grid).unwrap()
                ),
                Status::SetPivot(_) |
                    Status::PropCreationScreenshot(..) |
                    Status::PropCreationUi(..),
                Status::QuickPropSetup
            ),
            (
                PaintQuick,
                Status::QuickPropSetup,
                Status::QuickPropSetup,
                Status::SetPivot(_) |
                    Status::PropCreationScreenshot(..) |
                    Status::PropCreationUi(..)
            )
        );
    }
}
