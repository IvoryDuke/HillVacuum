//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::input::keyboard::KeyCode;
use bevy_egui::egui;
use hill_vacuum_shared::return_if_no_match;
use is_executable::IsExecutable;

use super::{window::Window, UiBundle, WindowCloserInfo};
use crate::{
    config::{controls::bind::Bind, Config},
    map::editor::state::{grid::Grid, ui::WindowCloser},
    utils::misc::{Blinker, Toggle}
};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// Info about the bind being edited.
#[derive(Default)]
enum BindEdit
{
    /// Inactive
    #[default]
    None,
    /// Working
    Some(Bind, Blinker)
}

impl BindEdit
{
    /// The duration time of a blink.
    const BLINK_INTERVAL: f32 = 0.75;
    /// The blink off string.
    const BLINK_OFF: &'static str = " ";
    /// The blink on string.
    const BLINK_ON: &'static str = "_";

    /// Whether a bind is being edited.
    #[inline]
    #[must_use]
    const fn being_edited(&self) -> bool { matches!(self, Self::Some(..)) }

    /// Starts a bind edit.
    #[inline]
    fn initialize(&mut self, bind: Bind)
    {
        *self = Self::Some(bind, Blinker::new(Self::BLINK_INTERVAL));
    }

    /// Updates `self` and returns the [`Bind`] being edited and the string to show in place of the
    /// keyboard key.
    #[inline]
    #[must_use]
    fn update(&mut self, delta_time: f32) -> Option<(Bind, &'static str)>
    {
        let (b, blinker) = return_if_no_match!(self, Self::Some(b, blinker), (b, blinker), None);
        Some((*b, if blinker.update(delta_time) { Self::BLINK_ON } else { Self::BLINK_OFF }))
    }

    /// Resets the bind edit.
    #[inline]
    fn reset(&mut self) { *self = BindEdit::default(); }
}

//=======================================================================//

/// The settings window.
#[derive(Default)]
pub(in crate::map::editor::state::ui) struct SettingsWindow
{
    /// The window data.
    window:    Window,
    /// Data concerning the bind being edited.
    bind_edit: BindEdit
}

impl Toggle for SettingsWindow
{
    #[inline]
    fn toggle(&mut self)
    {
        if self.window.is_open()
        {
            self.bind_edit.reset();
        }

        self.window.toggle();
    }
}

impl WindowCloserInfo for SettingsWindow
{
    #[inline]
    fn window_closer(&self) -> Option<WindowCloser>
    {
        /// Calls the window close.
        #[inline]
        fn close(controls: &mut SettingsWindow)
        {
            controls.bind_edit.reset();
            controls.window.close();
        }

        self.window
            .layer_id()
            .map(|id| WindowCloser::Settings(id, close as fn(&mut Self)))
    }
}

impl SettingsWindow
{
    /// Shows the settings window.
    #[inline]
    #[must_use]
    pub fn show(&mut self, egui_context: &egui::Context, bundle: &mut UiBundle) -> bool
    {
        let UiBundle {
            images,
            prop_cameras,
            key_inputs,
            user_textures,
            delta_time,
            config:
                Config {
                    binds,
                    colors,
                    exporter,
                    ..
                },
            drawing_resources,
            things_catalog,
            manager,
            clipboard,
            inputs,
            grid,
            ..
        } = bundle;

        if grid.changed_requires_update() &&
            (!inputs.left_mouse.pressed() || !self.window.is_open())
        {
            manager.rebuild_sprite_quad_tree(drawing_resources, grid);
            clipboard.queue_all_props_screenshots(
                images,
                prop_cameras,
                user_textures,
                drawing_resources,
                things_catalog,
                grid
            );

            grid.set_updated();
        }

        if !self.window.check_open(Bind::Settings.just_pressed(key_inputs, binds))
        {
            return false;
        }

        if inputs.esc.just_pressed() && self.bind_edit.being_edited()
        {
            self.bind_edit.reset();
        }

        let bind_was_being_edited = self.bind_edit.being_edited();

        self.window.show(
            egui_context,
            egui::Window::new("Settings")
                .vscroll(true)
                .collapsible(true)
                .max_width(250f32),
            |ui| {
                /// Shows a button to redefine a keyboard bind.
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

                egui::Grid::new("settings_grid")
                    .num_columns(2)
                    .spacing([40f32, 4f32])
                    .striped(true)
                    .show(ui, |ui| {
                        // Grid.
                        ui.label("GRID");
                        ui.end_row();

                        ui.label("Skew");
                        let mut skew = grid.skew();

                        if ui.add(egui::Slider::new(&mut skew, Grid::SKEW_RANGE)).changed()
                        {
                            grid.set_skew(skew);
                        }

                        ui.end_row();

                        ui.label("Angle");
                        let mut angle = grid.angle();

                        if ui.add(egui::Slider::new(&mut angle, Grid::ANGLE_RANGE)).changed()
                        {
                            grid.set_angle(angle);
                        }

                        ui.end_row();

                        // Keyboard binds.
                        ui.label("CONTROLS");
                        ui.end_row();

                        match self.bind_edit.update(*delta_time)
                        {
                            Some((b, blink)) =>
                            {
                                let mut iter = Bind::iter();

                                for bind in &mut iter
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
                            },
                            None =>
                            {
                                for bind in Bind::iter()
                                {
                                    let response =
                                        bind_button(ui, bind.label(), bind.keycode_str(binds));

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
                            }
                        };

                        if ui.button("Reset to default").clicked()
                        {
                            binds.reset();
                        }
                        ui.end_row();

                        ui.label("");
                        ui.end_row();

                        // Colors.
                        ui.label("COLORS");
                        ui.end_row();

                        colors.show(bundle.materials, ui);

                        if ui.button("Reset to default").clicked()
                        {
                            colors.reset(bundle.materials);
                        }
                        ui.end_row();

                        ui.label("");
                        ui.end_row();

                        // Exporter.
                        ui.label("EXPORTER");
                        ui.end_row();

                        if ui.button("Pick exporter").clicked()
                        {
                            match rfd::FileDialog::new()
                                .set_directory(std::env::current_dir().unwrap())
                                .set_title("Pick exporter")
                                .pick_file()
                            {
                                Some(file) if file.is_executable() => *exporter = file.into(),
                                _ => ()
                            }
                        }

                        let label = match exporter
                        {
                            Some(path) => path.file_stem().unwrap().to_str().unwrap(),
                            None => ""
                        };

                        ui.label(label);
                        ui.end_row();
                    });
            }
        );

        bind_was_being_edited
    }
}
