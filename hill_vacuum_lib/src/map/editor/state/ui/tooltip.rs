//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use shared::return_if_none;

use crate::map::editor::{state::core::tool::ToolInterface, StateUpdateBundle};

//=======================================================================//
// TYPES
//
//=======================================================================//

pub(in crate::map::editor::state::ui) struct Tooltip
{
    text:       String,
    spawn_time: Option<f32>,
    cursor_pos: egui::Pos2,
    working:    bool
}

impl Tooltip
{
    #[inline]
    #[must_use]
    pub const fn new() -> Self
    {
        Self {
            text:       String::new(),
            spawn_time: None,
            cursor_pos: egui::Pos2::new(f32::MAX, f32::MAX),
            working:    false
        }
    }

    #[inline]
    fn reset_timer(&mut self, elapsed_time: f32)
    {
        const TOOLTIP_SPAWN_INTERVAL: f32 = 0.75;
        self.spawn_time = (elapsed_time + TOOLTIP_SPAWN_INTERVAL).into();
    }

    #[inline]
    pub fn show(
        &mut self,
        bundle: &StateUpdateBundle,
        tool: impl ToolInterface,
        response: &egui::Response
    )
    {
        if response.clicked()
        {
            self.reset_timer(bundle.elapsed_time);
            return;
        }

        if !response.contains_pointer()
        {
            return;
        }

        let cursor_pos = return_if_none!(response.ctx.pointer_latest_pos());

        if cursor_pos == self.cursor_pos
        {
            self.working = true;

            if let Some(time) = self.spawn_time
            {
                if bundle.elapsed_time >= time
                {
                    self.text = tool.tooltip_label(&bundle.config.binds);
                    self.spawn_time = None;
                }
            }
            else
            {
                egui::show_tooltip_at_pointer(&response.ctx, tool.icon_file_name().into(), |ui| {
                    ui.label(&self.text);
                });
            }

            return;
        }

        self.working = false;
        self.cursor_pos = cursor_pos;
        self.reset_timer(bundle.elapsed_time);
    }
}
