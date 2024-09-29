//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::{input::ButtonInput, prelude::KeyCode};
use bevy_egui::egui;

use super::{window::Window, ToolsButtons, WindowCloser, WindowCloserInfo};
use crate::{utils::misc::Toggle, HardcodedActions};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The manual window.
#[derive(Default)]
pub(in crate::map::editor::state::ui) struct Manual(Window);

impl Toggle for Manual
{
    #[inline]
    fn toggle(&mut self) { self.0.toggle() }
}

impl WindowCloserInfo for Manual
{
    #[inline]
    fn window_closer(&self) -> Option<WindowCloser>
    {
        /// Calls the close function.
        #[inline]
        fn close(manual: &mut Manual) { manual.0.close() }

        self.0
            .layer_id()
            .map(|id| WindowCloser::Manual(id, close as fn(&mut Self)))
    }
}

impl Manual
{
    /// Shows the manual window.
    #[inline]
    pub fn show(
        &mut self,
        egui_context: &egui::Context,
        key_inputs: &ButtonInput<KeyCode>,
        tools_buttons: &ToolsButtons
    )
    {
        if !self.0.check_open(HardcodedActions::ToggleManual.pressed(key_inputs))
        {
            return;
        }

        self.0.show(
            egui_context,
            egui::Window::new("Manual")
                .vscroll(true)
                .min_width(400f32)
                .default_width(800f32)
                .min_height(300f32)
                .default_height(600f32),
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0f32, 12f32);
                ui.add_space(8f32);

                hill_vacuum_proc_macros::generate_manual!();

                // You would think this does nothing, but it actually does something
                ui.add_space(0f32);
            }
        );
    }
}
