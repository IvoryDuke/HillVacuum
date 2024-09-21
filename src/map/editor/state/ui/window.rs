//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;

use super::IsFocused;
use crate::utils::misc::Toggle;

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// A struct to keep track of the status of UI windows.
#[must_use]
#[derive(Default)]
pub(in crate::map::editor::state::ui) struct Window
{
    /// Whether the window is open.
    open: bool,
    /// The [`LayerId`] of the window, if it is open.
    id:   Option<egui::LayerId>
}

impl Toggle for Window
{
    #[inline]
    fn toggle(&mut self)
    {
        self.open.toggle();

        if !self.open
        {
            self.id = None;
        }
    }
}

impl Window
{
    /// Returns a new [`Window`].
    #[inline]
    pub const fn new() -> Self
    {
        Self {
            open: false,
            id:   None
        }
    }

    /// The [`LayerId`] of the window.
    #[inline]
    #[must_use]
    pub const fn layer_id(&self) -> Option<egui::LayerId>
    {
        if self.open
        {
            self.id
        }
        else
        {
            None
        }
    }

    /// Whether the window is open.
    #[inline]
    #[must_use]
    pub const fn is_open(&self) -> bool { self.open }

    /// Opens the window.
    #[inline]
    pub fn open(&mut self) { self.open = true; }

    /// Checks whether the window should be opened.
    /// Returns whether it is currently open.
    #[inline]
    #[must_use]
    pub fn check_open(&mut self, keys_pressed: bool) -> bool
    {
        if keys_pressed
        {
            self.open();
        }
        else if !self.is_open()
        {
            return false;
        }

        true
    }

    /// Closes the window.
    #[inline]
    pub fn close(&mut self)
    {
        self.open = false;
        self.id = None;
    }

    /// If open, Shows the window and updates the [`LayerId`].
    #[inline]
    pub fn show<F>(
        &mut self,
        egui_context: &egui::Context,
        window: egui::Window,
        f: F
    ) -> Option<bool>
    where
        F: FnOnce(&mut egui::Ui)
    {
        window
            .open(&mut self.open)
            .show(egui_context, |ui| {
                f(ui);
                ui.is_focused()
            })
            .and_then(|inner| {
                self.id = inner.response.layer_id.into();
                inner.inner
            })
    }
}
