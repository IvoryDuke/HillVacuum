//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use hill_vacuum_shared::NextValue;

use super::{INDEX_WIDTH, LEFT_FIELD, MINUS_PLUS_TOTAL_WIDTH};
use crate::{
    map::{
        drawer::animation::overall_values::{UiOverallAtlasAnimation, UiOverallTiming},
        editor::state::{
            clipboard::Clipboard,
            edits_history::EditsHistory,
            inputs_presses::InputsPresses,
            manager::EntitiesManager,
            ui::{
                overall_value_field::OverallValueField,
                texture_editor::{
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
fn set_atlas_per_frame_time<T, F, G, H>(
    builder: egui_extras::StripBuilder,
    manager: &mut EntitiesManager,
    clipboard: &mut Clipboard,
    edits_history: &mut EditsHistory,
    inputs: &InputsPresses,
    extra: &mut T,
    time: &mut UiOverallValue<f32>,
    index: usize,
    field_width: f32,
    frame_time: F,
    move_up: G,
    move_down: H
) where
    F: Fn(&mut EntitiesManager, &mut EditsHistory, &mut T, usize, f32),
    G: Fn(&mut EntitiesManager, &mut EditsHistory, &mut T, usize),
    H: Fn(&mut EntitiesManager, &mut EditsHistory, &mut T, usize)
{
    builder
        .size(egui_extras::Size::exact(INDEX_WIDTH))
        .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
        .size(egui_extras::Size::exact(field_width))
        .size(egui_extras::Size::exact(MINUS_PLUS_TOTAL_WIDTH))
        .horizontal(|mut strip| {
            set_atlas_time(
                &mut strip,
                manager,
                clipboard,
                inputs,
                edits_history,
                extra,
                time,
                index,
                frame_time
            );

            strip.cell(|ui| {
                use crate::map::editor::state::ui::minus_plus_buttons::{DownUpButtons, Response};

                ui.add_space(1f32);

                match DownUpButtons::new((MINUS_PLUS_WIDTH, MINUS_PLUS_HEIGHT).into())
                    .show(ui, true)
                {
                    Response::None => (),
                    Response::MinusClicked => move_down(manager, edits_history, extra, index),
                    Response::PlusClicked => move_up(manager, edits_history, extra, index)
                };
            });
        });
}

//=======================================================================//

/// UI element to set the partitioning of an atlas animation.
#[inline]
fn set_partition<T, F>(
    builder: egui_extras::StripBuilder,
    manager: &mut EntitiesManager,
    clipboard: &mut Clipboard,
    edits_history: &mut EditsHistory,
    inputs: &InputsPresses,
    extra: &mut T,
    value: &mut UiOverallValue<u32>,
    label: &str,
    field_width: f32,
    f: F
) where
    F: FnOnce(&mut EntitiesManager, &mut EditsHistory, &mut T, u32) -> bool
{
    builder
        .size(egui_extras::Size::exact(LEFT_FIELD))
        .size(egui_extras::Size::exact(field_width))
        .horizontal(|mut strip| {
            strip.cell(|ui| {
                ui.label(label);
            });

            strip.cell(|ui| {
                OverallValueField::show_always_enabled(ui, clipboard, inputs, value, |value| {
                    if value != 0 && f(manager, edits_history, extra, value)
                    {
                        return value.into();
                    }

                    None
                });
            });
        });
}

//=======================================================================//

/// UI element to set the timing of an atlas animation.
#[inline]
#[must_use]
fn set_atlas_timing<T, F>(
    ui: &mut egui::Ui,
    manager: &mut EntitiesManager,
    edits_history: &mut EditsHistory,
    extra: &mut T,
    f: F
) -> [egui::Response; 2]
where
    F: FnOnce(&mut EntitiesManager, &mut EditsHistory, &mut T, &[egui::Response; 2])
{
    let mut responses = [None, None];

    egui_extras::StripBuilder::new(ui)
        .size(egui_extras::Size::exact(LEFT_FIELD))
        .size(egui_extras::Size::remainder())
        .horizontal(|mut strip| {
            strip.cell(|ui| {
                ui.label("Timing");
            });

            strip.cell(|ui| {
                ui.horizontal(|ui| {
                    let rs = [
                        ui.add(egui::Button::new("Uniform")),
                        ui.add(egui::Button::new("Per Frame"))
                    ];
                    f(manager, edits_history, extra, &rs);

                    let mut rs = rs.into_iter();
                    responses = std::array::from_fn::<_, 2, _>(|_| rs.next_value().into());
                });
            });
        });

    let mut responses = responses.into_iter();
    [
        responses.next_value().unwrap(),
        responses.next_value().unwrap()
    ]
}

//=======================================================================//

/// UI element to set the time of a frame of an atlas animation.
#[inline]
fn set_atlas_time<T, F>(
    strip: &mut egui_extras::Strip,
    manager: &mut EntitiesManager,
    clipboard: &mut Clipboard,
    inputs: &InputsPresses,
    edits_history: &mut EditsHistory,
    extra: &mut T,
    value: &mut UiOverallValue<f32>,
    index: usize,
    f: F
) where
    F: FnOnce(&mut EntitiesManager, &mut EditsHistory, &mut T, usize, f32)
{
    strip.cell(|ui| {
        ui.label(INDEXES[index]);
    });

    strip.cell(|ui| {
        ui.label("Time");
    });

    strip.cell(|ui| {
        OverallValueField::show_always_enabled(ui, clipboard, inputs, value, |value| {
            if value > 0f32
            {
                f(manager, edits_history, extra, index, value);
                return value.into();
            }

            None
        });
    });
}

//=======================================================================//

/// UI element to set the amount of frames of an atlas animation.
#[inline]
fn set_atlas_len<T, F>(
    builder: egui_extras::StripBuilder,
    manager: &mut EntitiesManager,
    clipboard: &mut Clipboard,
    edits_history: &mut EditsHistory,
    inputs: &InputsPresses,
    extra: &mut T,
    value: &mut UiOverallValue<usize>,
    field_width: f32,
    f: F
) where
    F: FnOnce(&mut EntitiesManager, &mut EditsHistory, &mut T, usize) -> Option<usize>
{
    builder
        .size(egui_extras::Size::exact(LEFT_FIELD))
        .size(egui_extras::Size::exact(field_width))
        .horizontal(|mut strip| {
            strip.cell(|ui| {
                ui.label("Length");
            });

            strip.cell(|ui| {
                OverallValueField::show_always_enabled(ui, clipboard, inputs, value, |value| {
                    if value != 0
                    {
                        return f(manager, edits_history, extra, value);
                    }

                    None
                });
            });
        });
}

//=======================================================================//

/// UI element to set the time of an atlas animation frame.
#[inline]
fn set_single_atlas_time<T, F>(
    builder: egui_extras::StripBuilder,
    manager: &mut EntitiesManager,
    clipboard: &mut Clipboard,
    edits_history: &mut EditsHistory,
    inputs: &InputsPresses,
    extra: &mut T,
    value: &mut UiOverallValue<f32>,
    index: usize,
    field_width: f32,
    f: F
) where
    F: FnOnce(&mut EntitiesManager, &mut EditsHistory, &mut T, usize, f32)
{
    builder
        .size(egui_extras::Size::exact(INDEX_WIDTH))
        .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
        .size(egui_extras::Size::exact(field_width))
        .horizontal(|mut strip| {
            set_atlas_time(
                &mut strip,
                manager,
                clipboard,
                inputs,
                edits_history,
                extra,
                value,
                index,
                f
            );
        });
}

//=======================================================================//

#[allow(clippy::too_many_arguments)]
#[inline]
pub(in crate::map::editor::state::ui::texture_editor::animation_editor) fn atlas_editor<
    T,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M
>(
    ui: &mut egui::Ui,
    manager: &mut EntitiesManager,
    clipboard: &mut Clipboard,
    edits_history: &mut EditsHistory,
    inputs: &InputsPresses,
    extra: &mut T,
    atlas: &mut UiOverallAtlasAnimation,
    field_width: f32,
    x_partition: F,
    y_partition: G,
    len: H,
    timing: I,
    uniform_time: J,
    frame_time: K,
    move_up: L,
    move_down: M
) where
    F: FnOnce(&mut EntitiesManager, &mut EditsHistory, &mut T, u32) -> bool,
    G: FnOnce(&mut EntitiesManager, &mut EditsHistory, &mut T, u32) -> bool,
    H: FnOnce(&mut EntitiesManager, &mut EditsHistory, &mut T, usize) -> Option<usize>,
    I: FnOnce(&mut EntitiesManager, &mut EditsHistory, &mut T, &[egui::Response; 2]),
    J: FnOnce(&mut EntitiesManager, &mut EditsHistory, &mut T, usize, f32),
    K: Fn(&mut EntitiesManager, &mut EditsHistory, &mut T, usize, f32) + Copy,
    L: Fn(&mut EntitiesManager, &mut EditsHistory, &mut T, usize) + Copy,
    M: Fn(&mut EntitiesManager, &mut EditsHistory, &mut T, usize) + Copy
{
    egui_extras::StripBuilder::new(ui)
        .size(egui_extras::Size::exact(SETTING_HEIGHT))
        .size(egui_extras::Size::exact(SETTING_HEIGHT))
        .size(egui_extras::Size::exact(SETTING_HEIGHT))
        .vertical(|mut strip| {
            strip.strip(|builder| {
                set_partition(
                    builder,
                    manager,
                    clipboard,
                    edits_history,
                    inputs,
                    extra,
                    &mut atlas.x,
                    "X partition",
                    field_width,
                    x_partition
                );
            });

            strip.strip(|builder| {
                set_partition(
                    builder,
                    manager,
                    clipboard,
                    edits_history,
                    inputs,
                    extra,
                    &mut atlas.y,
                    "Y partition",
                    field_width,
                    y_partition
                );
            });

            strip.strip(|builder| {
                set_atlas_len(
                    builder,
                    manager,
                    clipboard,
                    edits_history,
                    inputs,
                    extra,
                    &mut atlas.len,
                    field_width,
                    len
                );
            });
        });

    ui.separator();

    ui.vertical(|ui| {
        ui.set_height(SETTING_HEIGHT);
        let [uniform, per_frame] = set_atlas_timing(ui, manager, edits_history, extra, timing);

        match &atlas.timing
        {
            UiOverallTiming::None => unreachable!(),
            UiOverallTiming::NonUniform => (),
            UiOverallTiming::Uniform(_) => _ = uniform.highlight(),
            UiOverallTiming::PerFrame(_) => _ = per_frame.highlight()
        };
    });

    match &mut atlas.timing
    {
        UiOverallTiming::None => unreachable!(),
        UiOverallTiming::NonUniform => (),
        UiOverallTiming::Uniform(time) =>
        {
            egui_extras::StripBuilder::new(ui)
                .size(egui_extras::Size::exact(SETTING_HEIGHT))
                .vertical(|mut strip| {
                    strip.strip(|builder| {
                        set_single_atlas_time(
                            builder,
                            manager,
                            clipboard,
                            edits_history,
                            inputs,
                            extra,
                            time,
                            0,
                            field_width,
                            uniform_time
                        );
                    });
                });
        },
        UiOverallTiming::PerFrame(vec) =>
        {
            egui_extras::StripBuilder::new(ui)
                .sizes(egui_extras::Size::exact(SETTING_HEIGHT), vec.len())
                .vertical(|mut strip| {
                    if vec.len() == 1
                    {
                        strip.strip(|builder| {
                            set_single_atlas_time(
                                builder,
                                manager,
                                clipboard,
                                edits_history,
                                inputs,
                                extra,
                                &mut vec[0],
                                0,
                                field_width,
                                frame_time
                            );
                        });

                        return;
                    }

                    for (i, time) in vec.iter_mut().enumerate()
                    {
                        strip.strip(|builder| {
                            set_atlas_per_frame_time(
                                builder,
                                manager,
                                clipboard,
                                edits_history,
                                inputs,
                                extra,
                                time,
                                i,
                                field_width,
                                frame_time,
                                move_up,
                                move_down
                            );
                        });
                    }
                });
        }
    };
}
