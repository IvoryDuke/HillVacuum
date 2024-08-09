//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;

use super::{window::Window, WindowCloser, WindowCloserInfo};
use crate::{
    config::{controls::bind::Bind, Config},
    map::editor::{
        state::{core::Core, edits_history::EditsHistory},
        StateUpdateBundle
    },
    utils::misc::Toggle
};

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The manual window.
#[derive(Default)]
pub(in crate::map::editor::state::ui) struct EditsHistoryWindow(Window);

impl Toggle for EditsHistoryWindow
{
    #[inline]
    fn toggle(&mut self) { self.0.toggle() }
}

impl WindowCloserInfo for EditsHistoryWindow
{
    #[inline]
    fn window_closer(&self) -> Option<WindowCloser>
    {
        /// Calls the close function.
        #[inline]
        fn close(window: &mut EditsHistoryWindow) { window.0.close() }

        self.0
            .layer_id()
            .map(|id| WindowCloser::EditsHistory(id, close as fn(&mut Self)))
    }
}

impl EditsHistoryWindow
{
    #[inline]
    pub fn show(
        &mut self,
        bundle: &mut StateUpdateBundle,
        core: &Core,
        edits_history: &EditsHistory
    ) -> Option<usize>
    {
        let StateUpdateBundle {
            egui_context,
            key_inputs,
            config: Config { binds, .. },
            ..
        } = bundle;

        if !self.0.check_open(Bind::EditsHistory.just_pressed(key_inputs, binds))
        {
            return None;
        }

        let mut clicked = None;

        self.0
            .show(egui_context, egui::Window::new("Edits History").vscroll(true), |ui| {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                    clicked = edits_history.show(ui, core);
                });
            });

        clicked
    }
}
