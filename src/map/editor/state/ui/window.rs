//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;

use crate::utils::misc::Toggle;

//=======================================================================//
// TYPES
//
//=======================================================================//

/// A struct to keep track of the status of UI windows.
#[must_use]
#[derive(Default)]
pub(in crate::map::editor::state::ui) struct Window
{
    /// Whever the window is open.
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

    /// Whever the window is open.
    #[inline(always)]
    #[must_use]
    pub const fn is_open(&self) -> bool { self.open }

    /// Opens the window.
    #[inline(always)]
    pub fn open(&mut self) { self.open = true; }

    /// Checks whever the window should be opened.
    /// Returns whever it is currently open.
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
    pub fn show<F, R>(
        &mut self,
        egui_context: &egui::Context,
        window: egui::Window,
        mut f: F
    ) -> Option<R>
    where
        F: FnMut(&mut egui::Ui) -> R
    {
        window
            .open(&mut self.open)
            .show(egui_context, |ui| f(ui))
            .and_then(|inner| {
                self.id = inner.response.layer_id.into();
                inner.inner
            })
    }
}
