//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::KeyCode;
use bevy_egui::egui;
use shared::return_if_no_match;

use super::{window::Window, WindowCloser, WindowCloserInfo};
use crate::{
    config::{controls::bind::Bind, Config},
    map::editor::StateUpdateBundle,
    utils::misc::{Blinker, Toggle}
};

//=======================================================================//
// TYPES
//
//=======================================================================//

#[derive(Default)]
enum BindEdit
{
    #[default]
    None,
    Some(Bind, Blinker)
}

impl BindEdit
{
    const BLINK_INTERVAL: f32 = 0.75;
    const BLINK_OFF: &'static str = " ";
    const BLINK_ON: &'static str = "_";

    #[inline]
    #[must_use]
    const fn being_edited(&self) -> bool { matches!(self, Self::Some(..)) }

    #[inline]
    fn initialize(&mut self, bind: Bind)
    {
        *self = Self::Some(bind, Blinker::new(Self::BLINK_INTERVAL));
    }

    #[inline]
    #[must_use]
    fn update(&mut self, delta_time: f32) -> Option<(Bind, &'static str)>
    {
        let (b, blinker) = return_if_no_match!(self, Self::Some(b, blinker), (b, blinker), None);
        Some((*b, if blinker.update(delta_time) { Self::BLINK_ON } else { Self::BLINK_OFF }))
    }

    #[inline]
    fn reset(&mut self) { *self = BindEdit::default(); }
}

//=======================================================================//

#[derive(Default)]
pub(in crate::map::editor::state::ui) struct ControlsWindow
{
    window:    Window,
    bind_edit: BindEdit
}

impl Toggle for ControlsWindow
{
    #[inline]
    fn toggle(&mut self)
    {
        if self.window.is_open()
        {
            self.bind_edit.reset();
            self.window.close();
            return;
        }

        self.window.open();
    }
}

impl WindowCloserInfo for ControlsWindow
{
    #[inline]
    fn window_closer(&self) -> Option<WindowCloser>
    {
        #[inline]
        fn close(controls: &mut ControlsWindow)
        {
            if !controls.bind_edit.being_edited()
            {
                controls.window.close();
            }

            controls.bind_edit.reset();
        }

        self.window
            .layer_id()
            .map(|id| WindowCloser::Controls((id, close as fn(&mut Self))))
    }
}

impl ControlsWindow
{
    #[inline]
    #[must_use]
    pub fn show(&mut self, bundle: &mut StateUpdateBundle) -> bool
    {
        if !self.window.is_open()
        {
            return false;
        }

        let StateUpdateBundle {
            delta_time,
            key_inputs,
            egui_context,
            config: Config { binds, .. },
            ..
        } = bundle;

        let bind_was_being_edited = self.bind_edit.being_edited();

        self.window.show(
            egui_context,
            egui::Window::new("Controls")
                .vscroll(true)
                .collapsible(false)
                .resizable(false),
            |ui| {
                ui.set_width(250f32);
                ui.visuals_mut().faint_bg_color = egui::Color32::from_gray(35);

                egui::Grid::new("binds_grid")
                    .num_columns(2)
                    .spacing([40f32, 4f32])
                    .striped(true)
                    .show(ui, |ui| {
                        #[inline]
                        fn bind_button(
                            ui: &mut egui::Ui,
                            label: &'static str,
                            keycode: &'static str
                        ) -> egui::Response
                        {
                            ui.label(label);
                            let response =
                                ui.add(egui::Button::new(keycode).min_size([100f32, 0f32].into()));
                            ui.end_row();

                            response
                        }

                        let mut iter = Bind::iter();

                        if let Some((b, blink)) = self.bind_edit.update(*delta_time)
                        {
                            for bind in iter.by_ref()
                            {
                                if bind == b
                                {
                                    for k in key_inputs.get_just_pressed()
                                    {
                                        if bind.set_bind(*k, binds)
                                        {
                                            self.bind_edit.reset();
                                            break;
                                        }
                                    }

                                    bind_button(ui, bind.label(), blink);
                                    break;
                                }

                                bind_button(ui, bind.label(), bind.keycode_str(binds));
                            }

                            for bind in iter
                            {
                                bind_button(ui, bind.label(), bind.keycode_str(binds));
                            }

                            return;
                        }

                        for bind in iter
                        {
                            let response = bind_button(ui, bind.label(), bind.keycode_str(binds));

                            if response.clicked()
                            {
                                self.bind_edit.initialize(bind);
                            }
                            else if response.hovered() &&
                                key_inputs.just_pressed(KeyCode::Backspace)
                            {
                                bind.unbind(binds);
                            }
                        }
                    });
            }
        );

        bind_was_being_edited
    }
}
