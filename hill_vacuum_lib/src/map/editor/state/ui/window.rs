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

#[must_use]
#[derive(Default)]
pub(in crate::map::editor::state::ui) struct Window
{
    open: bool,
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
    #[inline]
    pub const fn new() -> Self
    {
        Self {
            open: false,
            id:   None
        }
    }

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

    #[inline]
    #[must_use]
    pub fn is_open(&self) -> bool { self.open }

    #[inline]
    pub fn open(&mut self) { self.open = true; }

    #[inline]
    pub fn close(&mut self)
    {
        self.open = false;
        self.id = None;
    }

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
