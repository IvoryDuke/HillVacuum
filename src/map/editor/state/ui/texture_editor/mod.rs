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
    overall_value_field::{MinusPlusOverallValueField, MinusPlusUiOverallValue, OverallValueField},
    window::Window,
    ActuallyLostFocus,
    UiBundle,
    WindowCloser,
    WindowCloserInfo
};
use crate::{
    config::controls::bind::Bind,
    map::{
        drawer::{
            drawing_resources::{DrawingResources, TextureMaterials},
            overall_values::{OverallTextureSettings, UiOverallTextureSettings},
            texture::Texture
        },
        editor::state::{
            edits_history::EditsHistory,
            format_texture_preview,
            grid::Grid,
            manager::{EntitiesManager, TextureResult},
            ui::{minus_plus_buttons::MinusPlusButtons, texture_per_row}
        }
    },
    utils::{
        identifiers::EntityId,
        misc::Toggle,
        overall_value::{OverallValue, OverallValueInterface, OverallValueToUi, UiOverallValue}
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

macro_rules! scale_offset_setters {
    ($(($value:ident, $($xy:ident),+)),+) => { paste::paste! {$($(
        #[inline]
        fn [< $value _ $xy _setter >](
            drawing_resources: &DrawingResources,
            manager: &mut EntitiesManager,
            edits_history: &mut EditsHistory,
            grid: &Grid,
            value: f32
        ) -> bool
        {
            let valid = manager.test_operation_validity(|manager| {
                manager
                    .selected_textured_brushes_mut(drawing_resources, grid)
                    .find_map(|mut brush| {
                        (!brush.[< check_texture_ $value _ $xy >](drawing_resources, grid, value))
                            .then_some(brush.id())
                    })
            });

            if !valid
            {
                return false;
            }

            edits_history.[< texture_ $value _ $xy _cluster >](
                manager
                    .selected_textured_brushes_mut(drawing_resources, grid)
                    .filter_map(|mut brush| {
                        brush.[< set_texture_ $value _ $xy >](value).map(|prev| (brush.id(), prev))
                    })
            );

            manager.schedule_outline_update();
            true
        }
    )+)+}};
}

//=======================================================================//

macro_rules! height_parallax_scroll_setters {
    ($($value:ident, $t:ty),+) => { paste::paste! {$(
        #[inline]
        fn [< $value _setter >](
            drawing_resources: &DrawingResources,
            manager: &mut EntitiesManager,
            edits_history: &mut EditsHistory,
            grid: &Grid,
            value: $t
        ) -> bool
        {
            edits_history.[< texture_ $value _cluster>](
                manager
                    .selected_textured_brushes_mut(drawing_resources, grid)
                    .filter_map(|mut brush| {
                        brush.[< set_texture_ $value >](value).map(|prev| (brush.id(), prev))
                    })
            );

            true
        }
    )+}};
}

//=======================================================================//

/// Creates the definition for the scale, offset, scroll, and parallax texture settings functions.
macro_rules! scale_offset_scroll_parallax {
    ($((
        $value:ident,
        $label:literal,
        $step:literal,
        $clamp:expr
        $(, $default_if_none:literal)?
    )),+) => { paste::paste! { $(
        #[inline]
        fn [< set_ $value >](
            &mut self,
            strip: egui_extras::StripBuilder,
            bundle: &mut UiBundle,
            field_width: f32
        )
        {
            /// The label of the x value.
            const X_LABEL: &str = concat!($label, " X");
            /// The label of the y value.
            const Y_LABEL: &str = concat!($label, " Y");
            /// The padding before the y label.
            const Y_LABEL_PADDING: f32 = 8f32;

            strip
                .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
                .size(egui_extras::Size::exact(field_width))
                .size(egui_extras::Size::exact(MINUS_PLUS_TOTAL_WIDTH))
                .size(egui_extras::Size::exact(FIELD_NAME_WIDTH + Y_LABEL_PADDING))
                .size(egui_extras::Size::exact(field_width))
                .size(egui_extras::Size::exact(MINUS_PLUS_TOTAL_WIDTH))
                .horizontal(|mut strip| {
                    strip.cell(|ui| { ui.label(X_LABEL); });

                    let value = &mut self.overall_texture.[< $value _x >];
                    $(
                        let _ = $default_if_none;
                        let value = match value
                        {
                            Some(value) => value,
                            None => &mut UiOverallValue::none()
                        };
                    )?

                    Self::minus_plus_textedit(
                        &mut strip,
                        bundle,
                        value,
                        $step,
                        $clamp,
                        Self::[< $value _x_setter >]
                    );

                    strip.cell(|ui| {
                        ui.add_space(Y_LABEL_PADDING);
                        ui.label(Y_LABEL);
                    });

                    let value = &mut self.overall_texture.[< $value _y >];
                    $(
                        let _ = $default_if_none;
                        let value = match value
                        {
                            Some(value) => value,
                            None => &mut UiOverallValue::none()
                        };
                    )?

                    Self::minus_plus_textedit(
                        &mut strip,
                        bundle,
                        value,
                        $step,
                        $clamp,
                        Self::[< $value _y_setter >]
                    );
                });
        }
    )+ }};
}

//=======================================================================//

/// Creates the definition for the angle and height texture settings functions.
macro_rules! angle_height {
    ($((
        $value:ident,
        $label:literal,
        $t:ty,
        $clamp:expr
    )),+) => { paste::paste! {$(
        #[inline]
        fn [< set_ $value >](
            &mut self,
            strip: egui_extras::StripBuilder,
            bundle: &mut UiBundle,
            field_width: f32
        )
        {
            strip
                .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
                .size(egui_extras::Size::exact(field_width))
                .size(egui_extras::Size::exact(MINUS_PLUS_TOTAL_WIDTH))
                .horizontal(|mut strip| {
                    #[allow(clippy::cast_precision_loss)]
                    const ONE: $t = 1 as $t;

                    strip.cell(|ui| { ui.label($label); });

                    Self::minus_plus_textedit(
                        &mut strip,
                        bundle,
                        &mut self.overall_texture.$value,
                        ONE,
                        $clamp,
                        Self::[< $value _setter >]
                    );
                });
        }
    )+}};
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

#[must_use]
struct SizeFilter
{
    filter:  String,
    value:   Option<u32>,
    buttons: MinusPlusButtons
}

impl Default for SizeFilter
{
    #[inline]
    fn default() -> Self
    {
        Self {
            filter:  String::new(),
            value:   None,
            buttons: MinusPlusButtons::new(egui::Vec2::new(MINUS_PLUS_WIDTH, MINUS_PLUS_HEIGHT))
        }
    }
}

impl SizeFilter
{
    #[inline]
    fn show(&mut self, ui: &mut egui::Ui, bundle: &mut UiBundle) -> bool
    {
        use super::minus_plus_buttons::Response;

        let response =
            bundle
                .clipboard
                .copy_paste_text_editor(bundle.inputs, ui, &mut self.filter, 60f32);

        self.value = match self.filter.parse::<u32>().ok()
        {
            Some(v) =>
            {
                if v == 0
                {
                    self.filter.clear();
                    None
                }
                else
                {
                    v.into()
                }
            },
            None => None
        };

        match self.buttons.show(ui, true)
        {
            Response::None => (),
            Response::PlusClicked =>
            {
                match &mut self.value
                {
                    Some(v) => *v += 1,
                    None => self.value = 1.into()
                };

                self.filter = self.value.unwrap().to_string();
            },
            Response::MinusClicked =>
            {
                if let Some(v) = &mut self.value
                {
                    if *v == 1
                    {
                        self.value = None;
                        self.filter.clear();
                    }
                    else
                    {
                        *v -= 1;
                        self.filter = v.to_string();
                    }
                }
            }
        };

        response.has_focus() || response.actually_lost_focus()
    }
}

//=======================================================================//

/// The core of the texture editor.
#[derive(Default)]
struct Innards
{
    name_filter:      String,
    width_filter:     SizeFilter,
    height_filter:    SizeFilter,
    /// The overall texture.
    overall_texture:  UiOverallTextureSettings,
    /// The editor of the texture animation.
    animation_editor: AnimationEditor
}

impl Innards
{
    scale_offset_setters!((scale, x, y), (offset, x, y));

    height_parallax_scroll_setters!(
        height, i8, parallax_x, f32, parallax_y, f32, scroll_x, f32, scroll_y, f32
    );

    scale_offset_scroll_parallax!(
        (scale, "Scale", 0.5, |scale, step| {
            if scale == 0f32
            {
                return step;
            }

            scale
        }),
        (offset, "Offset", 1f32, no_clamp),
        (scroll, "Scroll", 1f32, no_clamp, 0),
        (parallax, "Parallax", 0.05, no_clamp, 0)
    );

    angle_height!(
        (angle, "Angle", f32, |angle, _| angle.rem_euclid(360f32)),
        (height, "Height", i8, |height, _| {
            height.clamp(*TEXTURE_HEIGHT_RANGE.start(), *TEXTURE_HEIGHT_RANGE.end())
        })
    );

    /// Assigns a texture to the selected brushes, if possible.
    #[inline]
    fn assign_texture(
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        texture: &str
    ) -> bool
    {
        match manager.set_selected_brushes_texture(drawing_resources, edits_history, grid, texture)
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
    fn selected_texture(&mut self, ui: &mut egui::Ui, bundle: &mut UiBundle)
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

    #[inline]
    fn minus_plus_textedit<T, F, C>(
        strip: &mut egui_extras::Strip,
        bundle: &mut UiBundle,
        value: &mut UiOverallValue<T>,
        step: T,
        clamp: C,
        mut f: F
    ) where
        T: MinusPlusUiOverallValue,
        C: Fn(T, T) -> T,
        F: FnMut(&DrawingResources, &mut EntitiesManager, &mut EditsHistory, &Grid, T) -> bool
    {
        let UiBundle {
            clipboard,
            inputs,
            manager,
            edits_history,
            drawing_resources,
            grid,
            ..
        } = bundle;

        MinusPlusOverallValueField::new((MINUS_PLUS_WIDTH, MINUS_PLUS_HEIGHT).into()).show(
            strip,
            clipboard,
            inputs,
            value,
            step,
            clamp,
            |value| f(drawing_resources, manager, edits_history, grid, value).then_some(value)
        );
    }

    /// Draws the UI elements of the texture editor.
    #[inline]
    fn texture_settings(&mut self, ui: &mut egui::Ui, bundle: &mut UiBundle, available_width: f32)
    {
        egui_extras::StripBuilder::new(ui)
            .sizes(egui_extras::Size::exact(SETTING_HEIGHT), 9)
            .vertical(|mut strip| {
                let plus_minus_field_width =
                    available_width / 2f32 - 11.5 - (FIELD_NAME_WIDTH + MINUS_PLUS_TOTAL_WIDTH);

                strip.strip(|strip| {
                    self.set_texture(strip, bundle, available_width);
                });

                for func in [Self::set_offset, Self::set_scale]
                {
                    strip.strip(|strip| {
                        func(self, strip, bundle, plus_minus_field_width);
                    });
                }

                strip.strip(|strip| {
                    self.set_scroll(strip, bundle, plus_minus_field_width);
                });

                strip.strip(|strip| {
                    self.set_angle(strip, bundle, plus_minus_field_width);
                });

                strip.strip(|strip| {
                    self.set_height(strip, bundle, plus_minus_field_width);
                });

                strip.strip(|strip| {
                    self.set_sprite(strip, bundle);
                });

                strip.strip(|strip| {
                    self.set_parallax(strip, bundle, plus_minus_field_width);
                });

                strip.strip(|strip| {
                    Self::settings(strip, bundle);
                });
            });
    }

    #[inline]
    fn settings(strip: egui_extras::StripBuilder, bundle: &mut UiBundle)
    {
        let UiBundle {
            drawing_resources,
            manager,
            edits_history,
            grid,
            settings,
            ..
        } = bundle;

        strip.size(egui_extras::Size::remainder()).horizontal(|mut strip| {
            strip.cell(|ui| {
                for (label, setting) in [
                    ("Show scroll  ", &mut settings.scroll_enabled),
                    ("Show parallax  ", &mut settings.parallax_enabled)
                ]
                {
                    ui.label(label);
                    _ = ui.add(egui::Checkbox::without_text(setting));
                    ui.add_space(5f32);
                }

                if ui.button("Reset").clicked()
                {
                    edits_history.texture_reset_cluster(
                        manager
                            .selected_textured_brushes_mut(drawing_resources, grid)
                            .map(|mut brush| (brush.id(), brush.reset_texture()))
                    );
                }
            });
        });
    }

    /// Selects the mode of the texture editor.
    #[inline]
    fn mode_selector(&mut self, ui: &mut egui::Ui, manager: &EntitiesManager)
    {
        ui.horizontal(|ui| {
            ui.label("Mode");

            let settings = ui.button("Texture settings");
            let texture_atlas = ui.add_enabled(
                self.overall_texture.name.uniform_value().is_some() ||
                    self.animation_editor.has_override(),
                egui::Button::new("Texture animation")
            );
            let selected_textured = manager.selected_textured_amount();
            let brushes_atlas = ui.add_enabled(
                selected_textured != 0 && selected_textured == manager.selected_brushes_amount(),
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

    #[inline]
    fn textures_gallery(&mut self, ui: &mut egui::Ui, bundle: &mut UiBundle)
    {
        #[inline]
        fn gallery<'a, F, G>(
            ui: &mut egui::Ui,
            drawing_resources: &'a DrawingResources,
            textures_per_row: usize,
            filter: Option<F>,
            mut click_func: G
        ) where
            F: Fn(&&'a TextureMaterials) -> bool,
            G: FnMut(&Texture, &egui::Response)
        {
            let mut textures = drawing_resources.ui_textures(filter);

            while ui
                .horizontal(|ui| {
                    for _ in 0..textures_per_row
                    {
                        let texture_materials = match textures.next()
                        {
                            Some(t) => t,
                            None =>
                            {
                                ui.add_space(ui.available_width());
                                return false;
                            }
                        };

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

                            click_func(texture, &response);

                            ui.vertical_centered(|ui| {
                                ui.add(egui::Label::new(texture.label()).wrap());
                            });
                        });
                    }

                    ui.add_space(ui.available_width());
                    true
                })
                .inner
            {}
        }

        #[inline]
        #[must_use]
        fn name_filter(
            texture: &TextureMaterials,
            n: Option<&str>,
            _: Option<u32>,
            _: Option<u32>
        ) -> bool
        {
            texture.texture().name().contains(n.unwrap())
        }

        #[inline]
        #[must_use]
        fn width_filter(
            texture: &TextureMaterials,
            _: Option<&str>,
            w: Option<u32>,
            _: Option<u32>
        ) -> bool
        {
            texture.texture().size().x == w.unwrap()
        }

        #[inline]
        #[must_use]
        fn height_filter(
            texture: &TextureMaterials,
            _: Option<&str>,
            _: Option<u32>,
            h: Option<u32>
        ) -> bool
        {
            texture.texture().size().y == h.unwrap()
        }

        macro_rules! filter_gen {
            ($(($($f:ident),+)),+) => { paste::paste! {$(
                #[inline]
                #[must_use]
                fn [< $($f)_+_filter >](
                    texture: &TextureMaterials,
                    n: Option<&str>,
                    w: Option<u32>,
                    h: Option<u32>
                ) -> bool
                {
                    $([< $f _filter >](texture, n, w, h)) && +
                }
            )+}};
        }

        filter_gen!((width, height), (name, width), (name, height), (name, width, height));

        let UiBundle {
            drawing_resources,
            manager,
            edits_history,
            grid,
            ..
        } = bundle;

        let n_filter = (!self.name_filter.is_empty()).then_some(self.name_filter.as_str());
        let w_filter = self.width_filter.value;
        let h_filter = self.height_filter.value;

        let filter = if n_filter.is_none() && h_filter.is_none() && w_filter.is_none()
        {
            None
        }
        else
        {
            let filter = match (n_filter, w_filter, h_filter)
            {
                (None, None, None) => unreachable!(),
                (None, None, Some(_)) => height_filter,
                (None, Some(_), None) => width_filter,
                (None, Some(_), Some(_)) => width_height_filter,
                (Some(_), None, None) => name_filter,
                (Some(_), None, Some(_)) => name_height_filter,
                (Some(_), Some(_), None) => name_width_filter,
                (Some(_), Some(_), Some(_)) => name_width_height_filter
            };

            (move |texture: &&TextureMaterials| filter(texture, n_filter, w_filter, h_filter))
                .into()
        };

        let textures_per_row = texture_per_row(ui, TEXTURE_GALLERY_PREVIEW_FRAME_SIDE);

        if self
            .animation_editor
            .can_add_textures_to_list(&self.overall_texture.animation)
        {
            let mut clicked_texture = None;

            gallery(ui, drawing_resources, textures_per_row, filter, |texture, response| {
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
                drawing_resources,
                manager,
                edits_history,
                grid,
                &self.overall_texture,
                return_if_none!(clicked_texture).as_str()
            );

            return;
        }

        gallery(ui, drawing_resources, textures_per_row, filter, |texture, response| {
            if response.clicked()
            {
                _ = Innards::assign_texture(
                    drawing_resources,
                    manager,
                    edits_history,
                    grid,
                    texture.name()
                );
            }
            else if response.secondary_clicked()
            {
                self.animation_editor.set_texture_override(texture);
            }
        });
    }

    /// Shows the texture editor.
    #[inline]
    fn show(&mut self, ui: &mut egui::Ui, bundle: &mut UiBundle)
    {
        const X_SPACING: f32 = 2f32;

        #[inline]
        fn line_section<F>(ui: &mut egui::Ui, f: F)
        where
            F: FnOnce(&mut egui::Ui)
        {
            ui.vertical(|ui| {
                ui.set_height(SETTING_HEIGHT);
                f(ui);
            });
            ui.separator();
        }

        line_section(ui, |ui| self.mode_selector(ui, bundle.manager));

        ui.horizontal(|ui| {
            ui.set_height(250f32);

            ui.vertical(|ui| {
                self.selected_texture(ui, bundle);
            });

            let spacing = ui.spacing_mut();
            spacing.item_spacing.x = X_SPACING;
            spacing.slider_width = SLIDER_WIDTH;
            let available_width = ui.available_width();

            if self.animation_editor.is_open()
            {
                ui.vertical(|ui| {
                    self.animation_editor.show(
                        ui,
                        bundle,
                        &mut self.overall_texture,
                        available_width
                    );
                });

                return;
            }

            self.texture_settings(ui, bundle, available_width);
        });

        ui.separator();

        line_section(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = X_SPACING;

                ui.label("Name filter");
                ui.add_space(2f32);
                let mut has_focus = bundle
                    .clipboard
                    .copy_paste_text_editor(
                        bundle.inputs,
                        ui,
                        &mut self.name_filter,
                        ui.available_width() - 382f32
                    )
                    .has_focus();

                ui.add_space(2f32);
                ui.label("Width filter");
                ui.add_space(2f32);
                has_focus |= self.width_filter.show(ui, bundle);

                ui.add_space(2f32);
                ui.label("Height filter");
                ui.add_space(2f32);
                has_focus | self.height_filter.show(ui, bundle)
            });
        });

        ui.vertical(|ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.textures_gallery(ui, bundle));
        });
    }

    /// Sets the texture of the selected brushes.
    #[inline]
    fn set_texture(
        &mut self,
        mut strip: egui_extras::StripBuilder,
        bundle: &mut UiBundle,
        available_width: f32
    )
    {
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
                OverallValueField::show_always_enabled(
                    ui,
                    bundle.clipboard,
                    bundle.inputs,
                    &mut self.overall_texture.name,
                    |value| {
                        let UiBundle {
                            drawing_resources,
                            manager,
                            edits_history,
                            grid,
                            ..
                        } = bundle;

                        if let Some(texture) = drawing_resources.texture(&value)
                        {
                            Self::assign_texture(
                                drawing_resources,
                                manager,
                                edits_history,
                                grid,
                                texture.name()
                            )
                            .then_some(value);
                        }

                        None
                    }
                );
            });

            if !del
            {
                return;
            }

            strip.cell(|ui| {
                if delete_button(ui)
                {
                    bundle.manager.remove_selected_textures(
                        bundle.drawing_resources,
                        bundle.edits_history,
                        bundle.grid
                    );
                    bundle.manager.schedule_outline_update();
                }
            });
        });
    }

    /// Sets the sprite value of the selected textures.
    #[inline]
    fn set_sprite(&mut self, strip: egui_extras::StripBuilder, bundle: &mut UiBundle)
    {
        strip
            .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
            .size(egui_extras::Size::remainder())
            .horizontal(|mut strip| {
                let UiBundle {
                    drawing_resources,
                    manager,
                    edits_history,
                    grid,
                    ..
                } = bundle;

                strip.cell(|ui| {
                    ui.label("Sprite");
                });

                if !manager.any_selected_brushes() ||
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

                    manager.set_sprite(drawing_resources, edits_history, grid, value);
                    manager.schedule_outline_update();
                });
            });
    }

    #[allow(unused_mut)]
    #[inline]
    fn angle_setter(
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        value: f32
    ) -> bool
    {
        let valid = manager.test_operation_validity(|manager| {
            manager
                .selected_textured_brushes_mut(drawing_resources, grid)
                .find_map(|mut brush| {
                    (!brush.check_texture_angle(drawing_resources, grid, value))
                        .then_some(brush.id())
                })
        });

        if !valid
        {
            return false;
        }

        edits_history.texture_angle_cluster(
            manager
                .selected_textured_brushes_mut(drawing_resources, grid)
                .filter_map(|mut brush| {
                    brush
                        .set_texture_angle(drawing_resources, grid, value)
                        .map(|prev| (brush.id(), prev))
                })
        );

        manager.schedule_outline_update();
        true
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
        #[inline]
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

        self.innards.overall_texture = match brushes.next()
        {
            Some(brush) =>
            {
                let mut t = OverallTextureSettings::from(brush.texture_settings());
                _ = brushes.any(|brush| t.stack(&brush.texture_settings()));
                t
            },
            None => OverallTextureSettings::none()
        }
        .ui();

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
    pub fn show(&mut self, egui_context: &egui::Context, bundle: &mut UiBundle) -> bool
    {
        /// The minimum texture editor size.
        const WINDOW_MIN_SIZE: f32 = 742f32;

        if !self.window.check_open(
            !bundle.inputs.alt_pressed() &&
                !bundle.inputs.ctrl_pressed() &&
                Bind::TextureEditor.just_pressed(bundle.key_inputs, &bundle.config.binds)
        )
        {
            return false;
        }

        self.window
            .show(
                egui_context,
                egui::Window::new("Texture Editor")
                    .min_width(WINDOW_MIN_SIZE)
                    .min_height(300f32)
                    .default_height(WINDOW_MIN_SIZE),
                |ui| {
                    self.innards.show(ui, bundle);
                }
            )
            .unwrap_or_default()
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// A function that does nothing, used for a macro.
#[inline]
#[must_use]
const fn no_clamp(value: f32, _: f32) -> f32 { value }

//=======================================================================//

/// Shows a delete button and returns whether it was pressed.
#[inline]
#[must_use]
fn delete_button(ui: &mut egui::Ui) -> bool
{
    ui.add(
        egui::Button::new("\u{00D7}").min_size((MINUS_PLUS_WIDTH + 1f32, MINUS_PLUS_HEIGHT).into())
    )
    .clicked()
}
