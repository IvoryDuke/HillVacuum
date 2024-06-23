mod animation_editor;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use hill_vacuum_shared::{return_if_none, TEXTURE_HEIGHT_RANGE};

use self::animation_editor::{AnimationEditor, Target};
use super::{
    checkbox::CheckBox,
    overall_value_field::{MinusPlusOverallValueField, OverallValueField},
    window::Window,
    WindowCloser,
    WindowCloserInfo
};
use crate::{
    config::controls::bind::Bind,
    map::{
        drawer::{
            drawing_resources::{DrawingResources, TextureMaterials},
            texture::{OverallTextureSettings, Texture, UiOverallTextureSettings}
        },
        editor::{
            state::{
                clipboard::Clipboard,
                editor_state::{InputsPresses, ToolsSettings},
                edits_history::EditsHistory,
                format_texture_preview,
                manager::{EntitiesManager, TextureResult},
                ui::textures_gallery
            },
            StateUpdateBundle
        }
    },
    utils::{
        identifiers::EntityId,
        misc::Toggle,
        overall_value::{OverallValue, OverallValueInterface, OverallValueToUi}
    }
};

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

/// The height of the area of the UI dedicated to the texture settings.
const SETTING_HEIGHT: f32 = 25f32;
/// The size of the side of the texture previews in the texture list.
const TEXTURE_GALLERY_PREVIEW_FRAME_SIDE: f32 = 128f32;
/// The width of the name of the field.
const FIELD_NAME_WIDTH: f32 = 70f32;
/// The slider width.
const SLIDER_WIDTH: f32 = FIELD_NAME_WIDTH * 4f32;
/// The width of the [`MinusPlusOverallValueField`] plus or minus.
const MINUS_PLUS_WIDTH: f32 = 16f32;
/// The total width of the [`MinusPlusOverallValueField`] plus and minus.
const MINUS_PLUS_TOTAL_WIDTH: f32 = MINUS_PLUS_WIDTH * 2f32;
/// The height of the minus and plus of a [`MinusPlusOverallValueField`].
const MINUS_PLUS_HEIGHT: f32 = 19f32;
/// The width of a delete button.
const DELETE_BUTTON_WIDTH: f32 = 12f32;

//=======================================================================//
// MACROS
//
//=======================================================================//

/// Generates the function surrounding a [`MinusPlusOverallValueField`] related to a texture
/// setting.
macro_rules! plus_minus_textedit {
    (
        $self:ident,
        $bundle:ident,
        $t:ty,
        $value:ident,
        $step:expr,
        $strip:ident,
        $clamp:expr
        $(, $drawing_resources:ident)?
        $(, $return_if_none:literal)?
    ) => {{ paste::paste! {
        #[inline]
        fn set(
            value: $t,
            manager: &mut EntitiesManager,
            edits_history: &mut EditsHistory
            $(, $drawing_resources: &DrawingResources)?
        ) -> bool
        {
            $(
                let valid = manager.test_operation_validity(|manager| {
                    manager.selected_textured_brushes_mut().find_map(|mut brush| {
                        (!brush.[< check_texture_ $value >]($drawing_resources, value)).then_some(brush.id())
                    })
                });

                if !valid
                {
                    return false;
                }
            )?

            edits_history.[< texture_ $value _cluster >](
                manager.selected_textured_brushes_mut().filter_map(|mut brush| {
                    brush.[< set_texture_ $value >]($($drawing_resources, )? value).map(|prev| (brush.id(), prev))
                })
            );

            manager.schedule_outline_update();
            true
        }

        let Bundle {
            clipboard,
            inputs,
            manager,
            edits_history,
            $($drawing_resources,)?
            ..
        } = $bundle;

        let value = &mut $self.overall_texture.$value;
        $(
            let _ = $return_if_none;
            let value = return_if_none!(value);
        )?

        MinusPlusOverallValueField::new((MINUS_PLUS_WIDTH, MINUS_PLUS_HEIGHT).into())
            .show(
                &mut $strip,
                clipboard,
                inputs,
                value,
                $step,
                $clamp,
                |value| {
                    set(
                        value,
                        manager,
                        edits_history
                        $(, $drawing_resources)?
                    ).then_some(value)
                }
            ).has_focus
    }}};
}

//=======================================================================//

/// Creates the definition for the scale, offset, scroll, and parallax texture settings functions.
macro_rules! scale_offset_scroll_parallax {
    ($((
        $value:ident,
        $label:literal,
        $step:literal,
        $clamp:expr
        $(, $drawing_resources:ident)?
        $(, $return_if_none:literal)?
    )),+) => { paste::paste! { $(
        #[inline]
        fn [< set_ $value >](
            &mut self,
            strip: egui_extras::StripBuilder,
            bundle: &mut Bundle,
            field_width: f32
        ) -> bool
        {
            /// The label of the x value.
            const X_LABEL: &str = concat!($label, " X");
            /// The label of the y value.
            const Y_LABEL: &str = concat!($label, " Y");
            /// The padding before the y label.
            const Y_LABEL_PADDING: f32 = 8f32;

            $(
                let _ = $return_if_none;

                if self.overall_texture.[< $value _x >].is_none()
                {
                    return false;
                }
            )?

            let mut has_focus = false;

            strip
                .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
                .size(egui_extras::Size::exact(field_width))
                .size(egui_extras::Size::exact(MINUS_PLUS_TOTAL_WIDTH))
                .size(egui_extras::Size::exact(FIELD_NAME_WIDTH + Y_LABEL_PADDING))
                .size(egui_extras::Size::exact(field_width))
                .size(egui_extras::Size::exact(MINUS_PLUS_TOTAL_WIDTH))
                .horizontal(|mut strip| {
                    strip.cell(|ui| { ui.label(X_LABEL); });

                    has_focus = plus_minus_textedit!(
                        self,
                        bundle,
                        f32,
                        [< $value _x >],
                        $step,
                        strip,
                        $clamp
                        $(, $drawing_resources)?
                        $(, $return_if_none)?
                    );

                    strip.cell(|ui| {
                        ui.add_space(Y_LABEL_PADDING);
                        ui.label(Y_LABEL);
                    });

                    has_focus |= plus_minus_textedit!(
                        self,
                        bundle,
                        f32,
                        [< $value _y >],
                        $step,
                        strip,
                        $clamp
                        $(, $drawing_resources)?
                        $(, $return_if_none)?
                    );
                });

            has_focus
        }
    )+ }};
}

//=======================================================================//

/// Creates the definition for the angle and height texture settings functions.
macro_rules! angle_and_height {
    ($(($value:ident, $label:literal, $t:ty, $bound:expr $(, $drawing_resources:ident)?)),+) => { paste::paste! { $(
        #[inline]
        fn [< set_ $value >](
            &mut self,
            strip: egui_extras::StripBuilder,
            bundle: &mut Bundle,
            field_width: f32
        ) -> bool
        {
            let mut has_focus = false;

            strip
                .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
                .size(egui_extras::Size::exact(field_width))
                .size(egui_extras::Size::exact(MINUS_PLUS_TOTAL_WIDTH))
                .horizontal(|mut strip| {
                    #[allow(clippy::cast_precision_loss)]
                    const ONE: $t = 1 as $t;

                    strip.cell(|ui| { ui.label($label); });

                    has_focus = plus_minus_textedit!(
                        self,
                        bundle,
                        $t,
                        $value,
                        ONE,
                        strip,
                        $bound
                        $(, $drawing_resources)?
                    );
                });

            has_focus
        }
    )+ }};
}

//=======================================================================//

/// Creates the definition for a toggle of a texture setting.
macro_rules! toggle {
    ($(($value:ident, $label:literal)),+) => { paste::paste! { $(
        #[inline]
        fn [< toggle_ $value >](
            &mut self,
            strip: egui_extras::StripBuilder,
            settings: &mut ToolsSettings
        )
        {
            strip
                .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
                .size(egui_extras::Size::remainder())
                .horizontal(|mut strip| {
                    strip.cell(|ui| {
                        ui.label($label);
                    });

                    strip.cell(|ui| {
                        _ = ui.add(
                            egui::Checkbox::without_text(&mut settings.[< $value _enabled >])
                        );
                    });
                });
        }
    )+ }};
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// A bundle of references to data necessary for the texture editor.
#[allow(clippy::missing_docs_in_private_items)]
struct Bundle<'a>
{
    drawing_resources: &'a mut DrawingResources,
    manager:           &'a mut EntitiesManager,
    edits_history:     &'a mut EditsHistory,
    clipboard:         &'a mut Clipboard,
    inputs:            &'a InputsPresses,
    settings:          &'a mut ToolsSettings
}

//=======================================================================//

/// The core of the texture editor.
#[derive(Default)]
struct Innards
{
    /// The overall texture.
    overall_texture:  UiOverallTextureSettings,
    /// The editor of the texture animation.
    animation_editor: AnimationEditor
}

impl Innards
{
    scale_offset_scroll_parallax!(
        (
            scale,
            "Scale",
            0.5,
            |scale, step| {
                if scale == 0f32
                {
                    return step;
                }

                scale
            },
            drawing_resources
        ),
        (offset, "Offset", 1f32, no_clamp, drawing_resources),
        (scroll, "Scroll", 1f32, no_clamp),
        (parallax, "Parallax", 0.05, no_clamp, 0)
    );

    angle_and_height!(
        (
            angle,
            "Angle",
            f32,
            |angle: f32, _| {
                let mut angle = angle.floor().rem_euclid(360f32);

                if angle < 0f32
                {
                    angle += 360f32;
                }

                angle
            },
            drawing_resources
        ),
        (height, "Height", i8, |height: i8, _| {
            height.clamp(*TEXTURE_HEIGHT_RANGE.start(), *TEXTURE_HEIGHT_RANGE.end())
        })
    );

    toggle!((scroll, "Scroll"), (parallax, "Parallax"));

    /// Assigns a texture to the selected brushes, if possible.
    #[inline]
    fn assign_texture(
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        texture: &str
    ) -> bool
    {
        match manager.set_selected_brushes_texture(drawing_resources, edits_history, texture)
        {
            TextureResult::Invalid => false,
            TextureResult::Valid => true,
            TextureResult::ValidRefreshOutline =>
            {
                manager.schedule_outline_update();
                true
            }
        }
    }

    /// The name of the texture being edited, if any.
    #[inline]
    #[must_use]
    fn selected_texture_name(&self) -> Option<&String>
    {
        self.animation_editor
            .texture_override()
            .or_else(|| self.overall_texture.name.uniform_value())
    }

    /// Draws the selected texture.
    #[inline]
    fn selected_texture(&mut self, ui: &mut egui::Ui, bundle: &mut Bundle)
    {
        /// The side of the texture preview.
        const TEXTURE_PREVIEW_FRAME_SIDE: f32 = 224f32;

        ui.set_width(TEXTURE_PREVIEW_FRAME_SIDE);
        let texture = bundle
            .drawing_resources
            .egui_texture(return_if_none!(self.selected_texture_name()));
        format_texture_preview!(Image, ui, texture.0, texture.1, TEXTURE_PREVIEW_FRAME_SIDE);
        ui.vertical_centered(|ui| ui.label(texture.2));
    }

    /// Draws the UI elements of the texture editor.
    #[inline]
    fn texture_settings(
        &mut self,
        ui: &mut egui::Ui,
        bundle: &mut Bundle,
        available_width: f32
    ) -> bool
    {
        let mut has_focus = false;

        egui_extras::StripBuilder::new(ui)
            .sizes(egui_extras::Size::exact(SETTING_HEIGHT), 10)
            .vertical(|mut strip| {
                let plus_minus_field_width =
                    available_width / 2f32 - 11.5 - (FIELD_NAME_WIDTH + MINUS_PLUS_TOTAL_WIDTH);

                strip.strip(|strip| {
                    has_focus = self.set_texture(strip, bundle, available_width);
                });

                for func in [Self::set_offset, Self::set_scale]
                {
                    strip.strip(|strip| {
                        has_focus |= func(self, strip, bundle, plus_minus_field_width);
                    });
                }

                strip.strip(|strip| {
                    has_focus |= self.set_scroll(strip, bundle, plus_minus_field_width);
                });
                strip.strip(|strip| {
                    self.toggle_scroll(strip, bundle.settings);
                });

                strip.strip(|strip| {
                    has_focus |= self.set_angle(strip, bundle, plus_minus_field_width);
                });

                strip.strip(|strip| {
                    has_focus |= self.set_height(strip, bundle, plus_minus_field_width);
                });

                strip.strip(|strip| {
                    self.set_sprite(strip, bundle);
                });

                strip.strip(|strip| {
                    self.toggle_parallax(strip, bundle.settings);
                });

                strip.strip(|strip| {
                    has_focus |= self.set_parallax(strip, bundle, plus_minus_field_width);
                });
            });

        has_focus
    }

    /// Selects the mode of the texture editor.
    #[inline]
    fn mode_selector(&mut self, ui: &mut egui::Ui, manager: &EntitiesManager)
    {
        ui.horizontal(|ui| {
            ui.label("Mode");

            let settings = ui.button("Texture settings");
            let texture_atlas = ui.add_enabled(
                !self.overall_texture.name.is_none() || self.animation_editor.has_override(),
                egui::Button::new("Texture animation")
            );
            let brushes_atlas = ui.add_enabled(
                manager.selected_textured_amount() != 0,
                egui::Button::new("Selected brushes animation")
            );

            if settings.clicked()
            {
                self.animation_editor.close();
            }
            else if texture_atlas.clicked()
            {
                self.animation_editor.open(Target::Texture(None));
            }
            else if brushes_atlas.clicked()
            {
                self.animation_editor.open(Target::Brushes);
            }

            match self.animation_editor.target
            {
                Target::None => settings,
                Target::Texture(_) => texture_atlas,
                Target::Brushes => brushes_atlas
            }
            .highlight();
        });
    }

    /// Shows the texture editor.
    #[inline]
    fn show(&mut self, ui: &mut egui::Ui, bundle: &mut Bundle) -> bool
    {
        ui.vertical(|ui| {
            ui.set_height(SETTING_HEIGHT);
            self.mode_selector(ui, bundle.manager);
            ui.separator();
        });

        let response = ui
            .horizontal(|ui| {
                ui.vertical(|ui| {
                    self.selected_texture(ui, bundle);
                });

                let spacing = ui.spacing_mut();
                spacing.item_spacing.x = 2f32;
                spacing.slider_width = SLIDER_WIDTH;
                let available_width = ui.available_width();

                if self.animation_editor.is_open()
                {
                    return ui
                        .vertical(|ui| {
                            self.animation_editor.show(
                                ui,
                                bundle,
                                &mut self.overall_texture,
                                available_width
                            )
                        })
                        .inner;
                }

                self.texture_settings(ui, bundle, available_width)
            })
            .inner;

        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            /// Draws the button to be clicked to pick a texture.
            #[inline]
            fn texture_preview<F>(
                ui: &mut egui::Ui,
                texture_materials: &TextureMaterials,
                mut f: F
            ) -> egui::Response
            where
                F: FnMut(&Texture, &egui::Response)
            {
                ui.vertical(|ui| {
                    ui.set_width(TEXTURE_GALLERY_PREVIEW_FRAME_SIDE);

                    let texture = texture_materials.texture();
                    let response = format_texture_preview!(
                        ImageButton,
                        ui,
                        texture_materials.egui_id(),
                        texture.size(),
                        TEXTURE_GALLERY_PREVIEW_FRAME_SIDE
                    );

                    f(texture, &response);

                    ui.vertical_centered(|ui| {
                        ui.add(egui::Label::new(texture.label()).wrap(true));
                    });
                    response
                })
                .inner
            }

            let Bundle {
                drawing_resources,
                manager,
                edits_history,
                ..
            } = bundle;

            /// Draws the gallery of loaded textures.
            macro_rules! gallery {
                ($f:expr) => {
                    textures_gallery!(
                        ui,
                        TEXTURE_GALLERY_PREVIEW_FRAME_SIDE,
                        |textures_per_row| { drawing_resources.chunked_textures(textures_per_row) },
                        match self.overall_texture.name.uniform_value()
                        {
                            Some(name) => drawing_resources.texture_index(name),
                            None => None
                        },
                        |ui, texture| { texture_preview(ui, texture, $f) },
                        |ui: &mut egui::Ui, textures| {
                            ui.horizontal(|ui| {
                                for texture_materials in textures
                                {
                                    texture_preview(ui, texture_materials, $f);
                                }

                                ui.add_space(ui.available_width());
                            });
                        }
                    );
                };
            }

            if self
                .animation_editor
                .can_add_textures_to_atlas(&self.overall_texture.animation)
            {
                let mut clicked_texture = None;

                gallery!(|texture, response| {
                    if response.clicked()
                    {
                        clicked_texture = texture.name().to_owned().into();
                    }
                    else if response.secondary_clicked()
                    {
                        self.animation_editor.set_texture_override(texture);
                    }
                });

                self.animation_editor.push_list_animation_frame(
                    &mut drawing_resources
                        .texture_mut(self.selected_texture_name().unwrap().as_str())
                        .unwrap(),
                    manager,
                    edits_history,
                    return_if_none!(clicked_texture).as_str()
                );

                return;
            }

            gallery!(|texture, response| {
                if response.clicked()
                {
                    _ = Innards::assign_texture(
                        drawing_resources,
                        manager,
                        edits_history,
                        texture.name()
                    );
                }
                else if response.secondary_clicked()
                {
                    self.animation_editor.set_texture_override(texture);
                }
            });
        });

        response
    }

    /// Sets the texture of the selected brushes.
    #[inline]
    fn set_texture(
        &mut self,
        mut strip: egui_extras::StripBuilder,
        bundle: &mut Bundle,
        available_width: f32
    ) -> bool
    {
        let mut has_focus = false;
        let del = !self.overall_texture.name.is_none();

        if del
        {
            strip = strip
                .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
                .size(egui_extras::Size::exact(
                    available_width - FIELD_NAME_WIDTH - DELETE_BUTTON_WIDTH - 12f32
                ))
                .size(egui_extras::Size::exact(DELETE_BUTTON_WIDTH));
        }
        else
        {
            strip = strip
                .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
                .size(egui_extras::Size::remainder());
        }

        strip.horizontal(|mut strip| {
            strip.cell(|ui| {
                ui.label("Name");
            });

            strip.cell(|ui| {
                has_focus = OverallValueField::show_always_enabled(
                    ui,
                    bundle.clipboard,
                    bundle.inputs,
                    &mut self.overall_texture.name,
                    |value| {
                        let Bundle {
                            drawing_resources,
                            manager,
                            edits_history,
                            ..
                        } = bundle;

                        if let Some(texture) = drawing_resources.texture(&value)
                        {
                            return Self::assign_texture(
                                drawing_resources,
                                manager,
                                edits_history,
                                texture.name()
                            )
                            .then(|| value.clone());
                        }

                        None
                    }
                )
                .has_focus;
            });

            if !del
            {
                return;
            }

            strip.cell(|ui| {
                if delete_button(ui)
                {
                    bundle.manager.remove_selected_textures(bundle.edits_history);
                    bundle.manager.schedule_outline_update();
                }
            });
        });

        has_focus
    }

    /// Sets the sprite value of the selected textures.
    #[inline]
    fn set_sprite(&mut self, strip: egui_extras::StripBuilder, bundle: &mut Bundle)
    {
        strip
            .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
            .size(egui_extras::Size::remainder())
            .horizontal(|mut strip| {
                strip.cell(|ui| {
                    ui.label("Sprite");
                });

                if !bundle.manager.any_selected_brushes() ||
                    matches!(self.overall_texture.sprite, OverallValue::None)
                {
                    strip.cell(|ui| {
                        ui.add_enabled(false, egui::Checkbox::without_text(&mut false));
                    });

                    return;
                }

                strip.cell(|ui| {
                    let value =
                        return_if_none!(CheckBox::show(ui, &self.overall_texture.sprite, |v| *v));

                    bundle.manager.set_sprite(
                        bundle.drawing_resources,
                        bundle.edits_history,
                        value
                    );
                    bundle.manager.schedule_outline_update();
                });
            });
    }
}

//=======================================================================//

/// The texture editor window.
pub(in crate::map::editor::state::ui) struct TextureEditor
{
    /// The data of the window.
    window:  Window,
    /// The core of the editor.
    innards: Innards
}

impl Default for TextureEditor
{
    #[inline]
    #[must_use]
    fn default() -> Self
    {
        Self {
            window:  Window::new(),
            innards: Innards::default()
        }
    }
}

impl Toggle for TextureEditor
{
    #[inline]
    fn toggle(&mut self)
    {
        if self.window.is_open()
        {
            self.window.close();
            return;
        }

        self.window.open();
    }
}

impl WindowCloserInfo for TextureEditor
{
    #[inline]
    fn window_closer(&self) -> Option<WindowCloser>
    {
        /// Calls the window close function.
        #[inline(always)]
        fn close(ed: &mut TextureEditor) { ed.window.close(); }

        self.window
            .layer_id()
            .map(|id| WindowCloser::TextureEditor(id, close as fn(&mut Self)))
    }
}

impl TextureEditor
{
    /// Updates the overall texture.
    #[inline]
    pub fn update_overall_texture(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &EntitiesManager
    )
    {
        let mut brushes = manager.selected_brushes();
        let mut overall_texture = match brushes.next()
        {
            Some(brush) => OverallTextureSettings::from(brush.texture_settings()),
            None => OverallTextureSettings::none()
        };

        _ = brushes.any(|brush| overall_texture.stack(&brush.texture_settings()));

        self.innards.overall_texture = overall_texture.ui();
        self.innards
            .animation_editor
            .update_from_overall_texture(drawing_resources, &self.innards.overall_texture);
    }

    /// Schedules the update of the texture animation.
    #[inline]
    pub fn schedule_texture_animation_update(&mut self)
    {
        self.innards.animation_editor.schedule_texture_animation_update();
    }

    /// Draws the texture editor.
    #[inline]
    #[must_use]
    pub fn show(
        &mut self,
        bundle: &mut StateUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        clipboard: &mut Clipboard,
        inputs: &InputsPresses,
        settings: &mut ToolsSettings
    ) -> bool
    {
        /// The minimum texture editor size.
        const WINDOW_MIN_SIZE: f32 = 742f32;

        if !self.window.check_open(
            !inputs.alt_pressed() &&
                !inputs.ctrl_pressed() &&
                Bind::TextureEditor.just_pressed(bundle.key_inputs, &bundle.config.binds)
        )
        {
            return false;
        }

        let StateUpdateBundle {
            egui_context,
            drawing_resources,
            ..
        } = bundle;

        let mut bundle = Bundle {
            drawing_resources,
            manager,
            edits_history,
            clipboard,
            inputs,
            settings
        };

        self.window
            .show(
                egui_context,
                egui::Window::new("Texture Editor")
                    .min_width(WINDOW_MIN_SIZE)
                    .min_height(300f32)
                    .default_height(WINDOW_MIN_SIZE),
                |ui| self.innards.show(ui, &mut bundle)
            )
            .map_or(false, |response| response)
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// A function that does nothing, used for a macro.
#[inline(always)]
#[must_use]
const fn no_clamp(value: f32, _: f32) -> f32 { value }

//=======================================================================//

/// Shows a delete button and returns whever it was pressed.
#[inline]
#[must_use]
fn delete_button(ui: &mut egui::Ui) -> bool
{
    ui.add(
        egui::Button::new("\u{00D7}").min_size((MINUS_PLUS_WIDTH + 1f32, MINUS_PLUS_HEIGHT).into())
    )
    .clicked()
}
