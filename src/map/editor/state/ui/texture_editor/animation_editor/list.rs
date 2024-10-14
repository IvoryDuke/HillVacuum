//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use hill_vacuum_shared::match_or_panic;

use super::{INDEX_WIDTH, MINUS_PLUS_TOTAL_WIDTH};
use crate::{
    map::{
        drawer::{
            animation::overall_values::UiOverallListAnimation,
            drawing_resources::DrawingResources
        },
        editor::state::{
            clipboard::Clipboard,
            edits_history::EditsHistory,
            inputs_presses::InputsPresses,
            ui::{
                overall_value_field::OverallValueField,
                texture_editor::{
                    delete_button,
                    DELETE_BUTTON_WIDTH,
                    FIELD_NAME_WIDTH,
                    MINUS_PLUS_HEIGHT,
                    MINUS_PLUS_WIDTH,
                    SETTING_HEIGHT
                }
            }
        }
    },
    utils::overall_value::UiOverallValue,
    INDEXES
};

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

#[allow(clippy::too_many_arguments)]
#[inline]
fn list_texture<T, F, G>(
    strip: &mut egui_extras::Strip,
    drawing_resources: &DrawingResources,
    clipboard: &mut Clipboard,
    edits_history: &mut EditsHistory,
    inputs: &InputsPresses,
    extra: &mut T,
    texture: &mut UiOverallValue<String>,
    time: &mut UiOverallValue<f32>,
    index: usize,
    f: F,
    g: G
) where
    F: Fn(&mut EditsHistory, &mut T, usize, &str),
    G: Fn(&mut EditsHistory, &mut T, usize, f32)
{
    set_list_texture(
        strip,
        drawing_resources,
        clipboard,
        edits_history,
        inputs,
        extra,
        texture,
        index,
        f
    );
    set_list_time(strip, clipboard, edits_history, inputs, extra, time, index, g);
}

//=======================================================================//

#[allow(clippy::too_many_arguments)]
#[inline]
fn edit_list_single_texture<T, F, G>(
    builder: egui_extras::StripBuilder,
    drawing_resources: &DrawingResources,
    clipboard: &mut Clipboard,
    edits_history: &mut EditsHistory,
    inputs: &InputsPresses,
    extra: &mut T,
    texture: &mut UiOverallValue<String>,
    time: &mut UiOverallValue<f32>,
    index: usize,
    field_width: f32,
    f: F,
    g: G
) where
    F: Fn(&mut EditsHistory, &mut T, usize, &str),
    G: Fn(&mut EditsHistory, &mut T, usize, f32)
{
    builder
        .size(egui_extras::Size::exact(INDEX_WIDTH))
        .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
        .size(egui_extras::Size::exact(field_width))
        .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
        .size(egui_extras::Size::exact(field_width))
        .horizontal(|mut strip| {
            list_texture(
                &mut strip,
                drawing_resources,
                clipboard,
                edits_history,
                inputs,
                extra,
                texture,
                time,
                index,
                f,
                g
            );
        });
}

//=======================================================================//

#[allow(clippy::too_many_arguments)]
#[inline]
fn edit_list_texture<T, F, G, H, I, J>(
    builder: egui_extras::StripBuilder,
    drawing_resources: &DrawingResources,
    clipboard: &mut Clipboard,
    edits_history: &mut EditsHistory,
    inputs: &InputsPresses,
    extra: &mut T,
    texture: &mut UiOverallValue<String>,
    time: &mut UiOverallValue<f32>,
    index: usize,
    field_width: f32,
    texture_setter: F,
    time_setter: G,
    move_up: H,
    move_down: I,
    remove: J
) where
    F: Fn(&mut EditsHistory, &mut T, usize, &str),
    G: Fn(&mut EditsHistory, &mut T, usize, f32),
    H: Fn(&mut EditsHistory, &mut T, usize),
    I: Fn(&mut EditsHistory, &mut T, usize),
    J: Fn(&mut EditsHistory, &mut T, usize)
{
    builder
        .size(egui_extras::Size::exact(INDEX_WIDTH))
        .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
        .size(egui_extras::Size::exact(field_width))
        .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
        .size(egui_extras::Size::exact(field_width))
        .size(egui_extras::Size::exact(MINUS_PLUS_TOTAL_WIDTH))
        .size(egui_extras::Size::exact(DELETE_BUTTON_WIDTH))
        .horizontal(|mut strip| {
            list_texture(
                &mut strip,
                drawing_resources,
                clipboard,
                edits_history,
                inputs,
                extra,
                texture,
                time,
                index,
                texture_setter,
                time_setter
            );

            strip.cell(|ui| {
                use crate::map::editor::state::ui::minus_plus_buttons::{DownUpButtons, Response};

                ui.add_space(1f32);

                match DownUpButtons::new((MINUS_PLUS_WIDTH, MINUS_PLUS_HEIGHT).into())
                    .show(ui, true)
                {
                    Response::None => (),
                    Response::PlusClicked => move_up(edits_history, extra, index),
                    Response::MinusClicked => move_down(edits_history, extra, index)
                };
            });

            strip.cell(|ui| {
                ui.add_space(1f32);

                if delete_button(ui)
                {
                    remove(edits_history, extra, index);
                }
            });
        });
}

//=======================================================================//

/// UI element to set the texture in a list animation.
#[inline]
fn set_list_texture<F, T>(
    strip: &mut egui_extras::Strip,
    drawing_resources: &DrawingResources,
    clipboard: &mut Clipboard,
    edits_history: &mut EditsHistory,
    inputs: &InputsPresses,
    extra: &mut T,
    texture: &mut UiOverallValue<String>,
    index: usize,
    f: F
) where
    F: Fn(&mut EditsHistory, &mut T, usize, &str)
{
    strip.cell(|ui| {
        ui.label(INDEXES[index]);
    });

    strip.cell(|ui| {
        ui.label("Texture");
    });

    strip.cell(|ui| {
        OverallValueField::show_always_enabled(ui, clipboard, inputs, texture, |name| {
            if let Some(texture) = drawing_resources.texture(&name)
            {
                f(edits_history, extra, index, texture.name());
                return name.into();
            }

            None
        });
    });
}

//=======================================================================//

/// Sets the time of a texture of a list animation.
#[inline]
fn set_list_time<F, T>(
    strip: &mut egui_extras::Strip,
    clipboard: &mut Clipboard,
    edits_history: &mut EditsHistory,
    inputs: &InputsPresses,
    extra: &mut T,
    time: &mut UiOverallValue<f32>,
    index: usize,
    f: F
) where
    F: Fn(&mut EditsHistory, &mut T, usize, f32)
{
    strip.cell(|ui| {
        ui.label("Time");
    });

    strip.cell(|ui| {
        OverallValueField::show_always_enabled(ui, clipboard, inputs, time, |time| {
            if time > 0f32
            {
                f(edits_history, extra, index, time);
                return time.into();
            }

            None
        });
    });
}

//=======================================================================//

/// UI element to add a new texture to a list animation.
#[inline]
fn new_list_texture<F, T>(
    builder: egui_extras::StripBuilder,
    drawing_resources: &DrawingResources,
    clipboard: &mut Clipboard,
    edits_history: &mut EditsHistory,
    inputs: &InputsPresses,
    extra: &mut T,
    texture_slot: &mut UiOverallValue<String>,
    index: usize,
    field_width: f32,
    f: F
) where
    F: Fn(&mut EditsHistory, &mut T, &str)
{
    builder
        .size(egui_extras::Size::exact(INDEX_WIDTH))
        .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
        .size(egui_extras::Size::exact(field_width))
        .horizontal(|mut strip| {
            strip.cell(|ui| {
                ui.label(INDEXES[index]);
            });

            strip.cell(|ui| {
                ui.label("Texture");
            });

            strip.cell(|ui| {
                OverallValueField::show_always_enabled(
                    ui,
                    clipboard,
                    inputs,
                    texture_slot,
                    |name| {
                        if let Some(texture) = drawing_resources.texture(&name)
                        {
                            f(edits_history, extra, texture.name());
                            return name.into();
                        }

                        None
                    }
                );
            });
        });
}

//=======================================================================//

#[allow(clippy::too_many_arguments)]
#[inline]
pub(in crate::map::editor::state::ui::texture_editor::animation_editor) fn list_editor<
    T,
    F,
    G,
    H,
    I,
    J,
    K
>(
    ui: &mut egui::Ui,
    drawing_resources: &DrawingResources,
    clipboard: &mut Clipboard,
    edits_history: &mut EditsHistory,
    inputs: &InputsPresses,
    extra: &mut T,
    animation: &mut UiOverallListAnimation,
    field_width: f32,
    texture_setter: F,
    time_setter: G,
    move_up: H,
    move_down: I,
    remove: J,
    push: K
) where
    F: Fn(&mut EditsHistory, &mut T, usize, &str) + Copy,
    G: Fn(&mut EditsHistory, &mut T, usize, f32) + Copy,
    H: Fn(&mut EditsHistory, &mut T, usize) + Copy,
    I: Fn(&mut EditsHistory, &mut T, usize) + Copy,
    J: Fn(&mut EditsHistory, &mut T, usize) + Copy,
    K: Fn(&mut EditsHistory, &mut T, &str)
{
    let (vec, texture_slot) =
        match_or_panic!(animation, UiOverallListAnimation::Uniform(vec, slot), (vec, slot));

    egui_extras::StripBuilder::new(ui)
        .sizes(egui_extras::Size::exact(SETTING_HEIGHT), vec.len() + 1)
        .vertical(|mut strip| {
            if vec.len() == 1
            {
                let (texture, time) = &mut vec[0];

                strip.strip(|builder| {
                    edit_list_single_texture(
                        builder,
                        drawing_resources,
                        clipboard,
                        edits_history,
                        inputs,
                        extra,
                        texture,
                        time,
                        0,
                        field_width,
                        texture_setter,
                        time_setter
                    );
                });
            }
            else
            {
                for (i, (name, time)) in vec.iter_mut().enumerate()
                {
                    strip.strip(|builder| {
                        edit_list_texture(
                            builder,
                            drawing_resources,
                            clipboard,
                            edits_history,
                            inputs,
                            extra,
                            name,
                            time,
                            i,
                            field_width,
                            texture_setter,
                            time_setter,
                            move_up,
                            move_down,
                            remove
                        );
                    });
                }
            }

            strip.strip(|builder| {
                new_list_texture(
                    builder,
                    drawing_resources,
                    clipboard,
                    edits_history,
                    inputs,
                    extra,
                    texture_slot,
                    vec.len(),
                    field_width,
                    push
                );
            });
        });
}
