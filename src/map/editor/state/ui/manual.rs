//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;

use super::{window::Window, ToolsButtons, WindowCloser, WindowCloserInfo};
use crate::{map::editor::StateUpdateBundle, utils::misc::Toggle, HardcodedActions};

//=======================================================================//
// TYPES
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
    pub fn show(&mut self, bundle: &mut StateUpdateBundle, tools_buttons: &ToolsButtons)
    {
        let StateUpdateBundle {
            egui_context,
            key_inputs,
            ..
        } = bundle;

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
                /// Generates a section of the manual.
                macro_rules! manual_section {
                    ($name:literal, $(($command:literal, $exp:literal)),+) => {
                        ui.collapsing($name, |ui| {
                            ui.vertical(|ui| { $( show_explanation(ui, |ui| { ui.label($command); }, $exp); )+})
                        });
                        ui.separator();
                    };

                    (
                        $tool:ident,
                        $(($command:literal, $exp:literal)),+
                        $(, $(($subtool:ident, $sub_exp:literal)),+)?
                        $(, $tex_explanation:literal)?
                    ) => {
                        manual_section!(
                            no_separator,
                            $tool,
                            $(($command, $exp)),+
                            $(, $(($subtool, $sub_exp)),+)?
                            $(, $tex_explanation)?
                        );
                        ui.separator();
                    };

                    (
                        no_separator,
                        $tool:ident,
                        $(($command:literal, $exp:literal)),+
                        $(, $(($subtool:ident, $sub_exp:literal)),+)?
                        $(, $tex_explanation:literal)?
                    ) => {{
                        #[allow(unused_imports)]
                        use crate::map::editor::state::{ui::{Tool, SubTool}, core::tool::ToolInterface};

                        ui.collapsing(Tool::$tool.header(), |ui| {
                            ui.vertical(|ui| {
                                tools_buttons.image(ui, Tool::$tool);
                                $( show_explanation(ui, |ui| { ui.label($command); }, $exp); )+
                                $($( show_explanation(ui, |ui| tools_buttons.image(ui, SubTool::$subtool), $sub_exp); )+)?
                                $( show_explanation(ui, |ui| { ui.label("TEXTURE EDITING"); }, $tex_explanation); )?
                            })
                        });
                    }};
                }

                #[inline]
                fn show_explanation<F: FnOnce(&mut egui::Ui)>(ui: &mut egui::Ui, left: F, explanation: &str)
                {
                    ui.horizontal_wrapped(|ui| {
                        egui_extras::StripBuilder::new(ui)
                            .size(egui_extras::Size::exact(250f32))
                            .size(egui_extras::Size::remainder())
                            .horizontal(|mut strip| {
                                strip.cell(|ui| {
                                    left(ui);
                                });

                                strip.cell(|ui| {
                                    ui.label(explanation);
                                });
                            });
                    });
                }

                ui.spacing_mut().item_spacing = egui::vec2(0f32, 12f32);
                ui.add_space(8f32);

                hill_vacuum_proc_macros::generate_manual!();

                // You would think this does nothing, but it actually does something
                ui.add_space(0f32);
            }
        );
    }
}
