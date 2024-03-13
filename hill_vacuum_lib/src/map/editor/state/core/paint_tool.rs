//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::{prelude::Vec2, window::Window};
use bevy_egui::egui;
use shared::{return_if_no_match, return_if_none};

use super::{
    bottom_area,
    drag::Drag,
    draw_selected_and_non_selected_brushes,
    tool::{ActiveTool, ChangeConditions, EnabledTool, SubTool}
};
use crate::{
    map::editor::{
        cursor_pos::Cursor,
        state::{
            clipboard::{Clipboard, Prop, PropScreenshotTimer, PROP_SCREENSHOT_SIZE},
            core::tool::subtools_buttons,
            editor_state::InputsPresses,
            edits_history::EditsHistory,
            grid::Grid,
            manager::EntitiesManager,
            ui::{centered_window, textures_gallery, ToolsButtons}
        },
        DrawBundle,
        StateUpdateBundle,
        ToolUpdateBundle
    },
    utils::hull::Hull,
    INDEXES
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[derive(Debug)]
enum Status
{
    Inactive(()),
    SetPivot(Hull),
    PropCreationScreenshot(PropScreenshotTimer, Prop),
    PropCreationUi(Prop),
    QuickPropSetup,
    Paint(PaintingProp, Drag)
}

impl Default for Status
{
    #[inline]
    #[must_use]
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

#[derive(Debug, Clone, Copy)]
enum PaintingProp
{
    Quick,
    Slotted
}

type PropSpawnFunc =
    fn(&mut Clipboard, &ToolUpdateBundle, &mut EntitiesManager, &mut EditsHistory, Vec2);

impl PaintingProp
{
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
// TYPES
//
//=======================================================================//

#[derive(Debug)]
pub(in crate::map::editor::state::core) struct PaintTool
{
    slot:          String,
    status:        Status,
    max_ui_height: f32
}

impl PaintTool
{
    #[inline]
    pub fn tool() -> ActiveTool
    {
        ActiveTool::Paint(PaintTool {
            slot:          String::new(),
            status:        Status::default(),
            max_ui_height: 0f32
        })
    }

    #[inline]
    #[must_use]
    pub const fn ongoing_multi_frame_changes(&self) -> bool
    {
        matches!(self.status, Status::Paint(..))
    }

    #[inline]
    pub fn disable_subtool(&mut self)
    {
        if !matches!(self.status, Status::Inactive(()) | Status::Paint(..))
        {
            self.status = Status::default();
        }
    }

    #[inline]
    #[must_use]
    const fn cursor_pos(cursor: &Cursor) -> Vec2 { cursor.world_snapped() }

    #[allow(clippy::cast_precision_loss)]
    #[inline]
    pub fn update(
        &mut self,
        bundle: &mut ToolUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        clipboard: &mut Clipboard,
        grid: Grid
    )
    {
        let ToolUpdateBundle {
            images,
            user_textures,
            prop_cameras,
            paint_tool_camera,
            cursor,
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

                if inputs.enter.just_pressed() && manager.selected_entities_amount() != 0
                {
                    self.status = Status::SetPivot(Self::outline(manager, grid).unwrap());
                }

                if !inputs.left_mouse.just_pressed() || clipboard.selected_prop_index().is_none()
                {
                    return;
                }

                clipboard.spawn_selected_prop(bundle, manager, edits_history, cursor_pos);
                self.status = Status::Paint(PaintingProp::Slotted, Drag::new(cursor_pos));
            },
            Status::SetPivot(hull) =>
            {
                if !inputs.left_mouse.just_pressed() || !hull.contains_point(cursor_pos)
                {
                    return;
                }

                let mut prop = Prop::new(manager.selected_entities(), cursor_pos, None);
                Clipboard::assign_camera_to_prop(
                    images,
                    paint_tool_camera,
                    user_textures,
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

                clipboard.spawn_quick_prop(
                    bundle,
                    manager,
                    edits_history,
                    bundle.cursor.world_snapped()
                );
                self.status = Status::Paint(PaintingProp::Quick, Drag::new(cursor_pos));
            },
            Status::Paint(prop, drag) =>
            {
                if cursor.moved()
                {
                    drag.update(cursor, grid, |_| {
                        prop.spawn_func()(clipboard, bundle, manager, edits_history, cursor_pos);
                    });
                }

                if !inputs.left_mouse.pressed()
                {
                    self.status = Status::default();
                }
            }
        };
    }

    #[inline]
    #[must_use]
    fn outline(manager: &EntitiesManager, grid: Grid) -> Option<Hull>
    {
        manager.selected_entities_hull().map(|hull| grid.snap_hull(&hull))
    }

    #[inline]
    pub fn update_outline(&mut self, manager: &EntitiesManager, grid: Grid)
    {
        *return_if_no_match!(&mut self.status, Status::SetPivot(hull), hull) =
            return_if_none!(Self::outline(manager, grid));
    }

    #[allow(clippy::cast_precision_loss)]
    #[inline]
    #[must_use]
    pub fn ui(&mut self, bundle: &mut StateUpdateBundle, clipboard: &mut Clipboard) -> bool
    {
        const PREVIEW_SIZE: egui::Vec2 = egui::Vec2::new(
            PROP_SCREENSHOT_SIZE.x as f32 * 0.4f32,
            PROP_SCREENSHOT_SIZE.y as f32 * 0.4f32
        );

        let StateUpdateBundle {
            window,
            egui_context,
            ..
        } = bundle;

        let focused = if let Status::PropCreationUi(prop) = &self.status
        {
            self.prop_creation_window(window, egui_context, clipboard, prop.screenshot())
        }
        else
        {
            false
        };

        if clipboard.props_amount() == 0
        {
            return focused;
        }

        if let Some(clicked) = bottom_area!(
            self,
            egui_context,
            clipboard,
            "props",
            prop,
            PREVIEW_SIZE.y + 28f32,
            PREVIEW_SIZE,
            |ui: &mut egui::Ui, texture: (usize, egui::TextureId), frame| {
                ui.vertical(|ui| {
                    let response = ui.add(egui::ImageButton::new((texture.1, frame)));
                    ui.label(INDEXES[texture.0]);
                    response
                })
                .inner
            }
        )
        {
            clipboard.set_selected_prop_index(clicked);
        }

        focused
    }

    #[allow(clippy::cast_precision_loss)]
    #[inline]
    #[must_use]
    fn prop_creation_window(
        &mut self,
        window: &Window,
        egui_context: &mut egui::Context,
        clipboard: &Clipboard,
        texture: egui::TextureId
    ) -> bool
    {
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
        response.inner.unwrap()
    }

    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle, manager: &EntitiesManager)
    {
        draw_selected_and_non_selected_brushes!(bundle, manager);
        bundle
            .drawer
            .square_highlight(Self::cursor_pos(bundle.cursor), Color::ToolCursor);

        match &self.status
        {
            Status::SetPivot(hull) => bundle.drawer.hull(hull, Color::Hull),
            Status::PropCreationScreenshot(_, prop) => prop.draw(bundle, manager, None),
            _ => ()
        };
    }

    #[inline]
    pub fn draw_sub_tools(
        &mut self,
        ui: &mut egui::Ui,
        bundle: &StateUpdateBundle,
        manager: &EntitiesManager,
        grid: Grid,
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
                PaintCreation,
                Status::SetPivot(Self::outline(manager, grid).unwrap()),
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
