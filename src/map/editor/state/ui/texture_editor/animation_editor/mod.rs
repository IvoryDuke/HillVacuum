mod instances_editor;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use hill_vacuum_shared::{match_or_panic, return_if_none};

use super::{Bundle, FIELD_NAME_WIDTH, MINUS_PLUS_WIDTH, SETTING_HEIGHT};
use crate::{
    map::{
        drawer::{
            animation::{
                overall_values::{
                    OverallAnimation,
                    UiOverallAnimation,
                    UiOverallAtlasAnimation,
                    UiOverallListAnimation,
                    UiOverallTiming
                },
                Animation,
                MoveUpDown
            },
            drawing_resources::DrawingResources,
            texture::{Texture, UiOverallTextureSettings}
        },
        editor::state::{
            clipboard::Clipboard,
            editor_state::InputsPresses,
            edits_history::EditsHistory,
            manager::EntitiesManager,
            ui::{
                overall_value_field::{OverallValueField, Response},
                texture_editor::{
                    animation_editor::instances_editor::InstancesEditor,
                    delete_button,
                    DELETE_BUTTON_WIDTH,
                    MINUS_PLUS_HEIGHT
                }
            }
        }
    },
    utils::{identifiers::EntityId, overall_value::UiOverallValue},
    INDEXES
};

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

/// The width of a label showing the index of an animation frame.
const INDEX_WIDTH: f32 = 10f32;
/// The width of the field on the left of the editor.
const LEFT_FIELD: f32 = INDEX_WIDTH + FIELD_NAME_WIDTH + 4f32;
/// The total width of the minus and plus of a [`MinusPlusOverallValueField`].
const MINUS_PLUS_TOTAL_WIDTH: f32 = super::MINUS_PLUS_TOTAL_WIDTH + 8f32;

//=======================================================================//
// MACROS
//
//=======================================================================//

/// The texture and time UI elements of a single list animation frame.
macro_rules! list_texture {
    (
        $response:ident,
        $strip:ident,
        $drawing_resources:ident,
        $clipboard:ident,
        $inputs:ident,
        $texture:ident,
        $time:ident,
        $index:expr,
        $texture_setter:expr,
        $time_setter:expr
    ) => {
        $response |=
            set_list_texture(
                &mut $strip,
                $drawing_resources,
                $clipboard,
                $inputs,
                $texture,
                $index,
                $texture_setter
            ) | set_list_time(&mut $strip, $clipboard, $inputs, $time, $index, $time_setter);
    };
}

use list_texture;

//=======================================================================//

/// Shows the UI elements to edit a single list animation frame.
macro_rules! edit_list_single_texture {
    (
        $builder:ident,
        $drawing_resources:ident,
        $clipboard:ident,
        $inputs:ident,
        $texture:ident,
        $time:ident,
        $index:expr,
        $field_width:ident,
        $texture_setter:expr,
        $time_setter:expr
    ) => {{
        use crate::map::editor::state::ui::texture_editor::animation_editor::{
            list_texture,
            set_list_texture,
            set_list_time,
            FIELD_NAME_WIDTH,
            INDEX_WIDTH
        };

        let mut response = Response::default();

        $builder
            .size(egui_extras::Size::exact(INDEX_WIDTH))
            .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
            .size(egui_extras::Size::exact($field_width))
            .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
            .size(egui_extras::Size::exact($field_width))
            .horizontal(|mut strip| {
                list_texture!(
                    response,
                    strip,
                    $drawing_resources,
                    $clipboard,
                    $inputs,
                    $texture,
                    $time,
                    $index,
                    $texture_setter,
                    $time_setter
                );
            });

        response
    }};
}

use edit_list_single_texture;

//=======================================================================//

/// Shows the UI elements to edit a list texture.
macro_rules! edit_list_texture {
    (
        $builder:ident,
        $drawing_resources:ident,
        $clipboard:ident,
        $inputs:ident,
        $texture:ident,
        $time:ident,
        $index:expr,
        $field_width:ident,
        $texture_setter:expr,
        $time_setter:expr,
        $move_up:expr,
        $move_down:expr,
        $remove:expr
    ) => {{
        use crate::map::editor::state::ui::texture_editor::animation_editor::{
            delete_button,
            list_texture,
            set_list_texture,
            set_list_time,
            DELETE_BUTTON_WIDTH,
            FIELD_NAME_WIDTH,
            INDEX_WIDTH,
            MINUS_PLUS_HEIGHT,
            MINUS_PLUS_TOTAL_WIDTH,
            MINUS_PLUS_WIDTH
        };

        let mut response = Response::default();

        $builder
            .size(egui_extras::Size::exact(INDEX_WIDTH))
            .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
            .size(egui_extras::Size::exact($field_width))
            .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
            .size(egui_extras::Size::exact($field_width))
            .size(egui_extras::Size::exact(MINUS_PLUS_TOTAL_WIDTH))
            .size(egui_extras::Size::exact(DELETE_BUTTON_WIDTH))
            .horizontal(|mut strip| {
                list_texture!(
                    response,
                    strip,
                    $drawing_resources,
                    $clipboard,
                    $inputs,
                    $texture,
                    $time,
                    $index,
                    $texture_setter,
                    $time_setter
                );

                strip.cell(|ui| {
                    use crate::map::editor::state::ui::minus_plus_buttons::{
                        DownUpButtons,
                        Response
                    };

                    ui.add_space(1f32);

                    #[allow(clippy::redundant_closure_call)]
                    match DownUpButtons::new((MINUS_PLUS_WIDTH, MINUS_PLUS_HEIGHT).into())
                        .show(ui, true)
                    {
                        Response::None => return,
                        Response::PlusClicked => $move_up($index),
                        Response::MinusClicked => $move_down($index)
                    };

                    response.value_changed = true;
                });

                strip.cell(|ui| {
                    ui.add_space(1f32);

                    if delete_button(ui)
                    {
                        response.value_changed = true;
                        #[allow(clippy::redundant_closure_call)]
                        $remove($index);
                    }
                });
            });

        response
    }};
}

use edit_list_texture;

//=======================================================================//

/// Shows the UI elements to edit the time of an atlas animation which has per-frame time.
macro_rules! set_atlas_per_frame_time {
    (
        $builder:ident,
        $clipboard:ident,
        $inputs:ident,
        $time:ident,
        $index:ident,
        $field_width:ident,
        $frame_time:expr,
        $move_up:expr,
        $move_down:expr
    ) => {{
        use crate::map::editor::state::ui::texture_editor::animation_editor::{
            set_atlas_time,
            FIELD_NAME_WIDTH,
            INDEX_WIDTH,
            MINUS_PLUS_HEIGHT,
            MINUS_PLUS_TOTAL_WIDTH,
            MINUS_PLUS_WIDTH
        };

        let mut response = Response::default();

        $builder
            .size(egui_extras::Size::exact(INDEX_WIDTH))
            .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
            .size(egui_extras::Size::exact($field_width))
            .size(egui_extras::Size::exact(MINUS_PLUS_TOTAL_WIDTH))
            .horizontal(|mut strip| {
                response |= set_atlas_time(
                    &mut strip,
                    $clipboard,
                    $inputs,
                    $time,
                    $index,
                    |index, time| {
                        #[allow(clippy::redundant_closure_call)]
                        $frame_time(index, time);
                    }
                );

                strip.cell(|ui| {
                    use crate::map::editor::state::ui::minus_plus_buttons::{
                        DownUpButtons,
                        Response
                    };

                    ui.add_space(1f32);

                    #[allow(clippy::redundant_closure_call)]
                    match DownUpButtons::new((MINUS_PLUS_WIDTH, MINUS_PLUS_HEIGHT).into())
                        .show(ui, true)
                    {
                        Response::None => return,
                        Response::MinusClicked => $move_down($index),
                        Response::PlusClicked => $move_up($index)
                    };

                    response.value_changed = true;
                });
            });

        response
    }};
}

use set_atlas_per_frame_time;

//=======================================================================//

/// Shows the UI elements concerning an atlas animation.
macro_rules! atlas {
    (
        $ui:ident,
        $atlas:ident,
        $drawing_resources:ident,
        $manager:ident,
        $clipboard:ident,
        $inputs:ident,
        $elapsed_time:expr,
        $field_width:ident,
        $x_partition:expr,
        $y_partition:expr,
        $len:expr,
        $timing:expr,
        $uniform_time:expr,
        $frame_time:expr,
        $move_up:expr,
        $move_down:expr
    ) => {{
        use crate::map::editor::state::ui::texture_editor::animation_editor::{
            set_atlas_timing,
            set_len,
            set_partition,
            set_single_atlas_time
        };

        let mut response = Response::default();

        egui_extras::StripBuilder::new($ui)
            .size(egui_extras::Size::exact(SETTING_HEIGHT))
            .size(egui_extras::Size::exact(SETTING_HEIGHT))
            .size(egui_extras::Size::exact(SETTING_HEIGHT))
            .vertical(|mut strip| {
                strip.strip(|builder| {
                    response |= set_partition(
                        builder,
                        $clipboard,
                        $inputs,
                        &mut $atlas.x,
                        "X partition",
                        $field_width,
                        $x_partition
                    );
                });

                strip.strip(|builder| {
                    response |= set_partition(
                        builder,
                        $clipboard,
                        $inputs,
                        &mut $atlas.y,
                        "Y partition",
                        $field_width,
                        $y_partition
                    );
                });

                strip.strip(|builder| {
                    response |=
                        set_len(builder, $clipboard, $inputs, &mut $atlas.len, $field_width, $len);
                });
            });

        $ui.separator();

        $ui.vertical(|ui| {
            ui.set_height(SETTING_HEIGHT);
            response.value_changed |= set_atlas_timing(ui, $timing);
        });

        match &mut $atlas.timing
        {
            UiOverallTiming::None => unreachable!(),
            UiOverallTiming::NonUniform => (),
            UiOverallTiming::Uniform(time) =>
            {
                egui_extras::StripBuilder::new($ui)
                    .size(egui_extras::Size::exact(SETTING_HEIGHT))
                    .vertical(|mut strip| {
                        strip.strip(|builder| {
                            response |= set_single_atlas_time(
                                builder,
                                $clipboard,
                                $inputs,
                                time,
                                0,
                                $field_width,
                                $uniform_time
                            );
                        });
                    });
            },
            UiOverallTiming::PerFrame(vec) =>
            {
                egui_extras::StripBuilder::new($ui)
                    .sizes(egui_extras::Size::exact(SETTING_HEIGHT), vec.len())
                    .vertical(|mut strip| {
                        if vec.len() == 1
                        {
                            strip.strip(|builder| {
                                response |= set_single_atlas_time(
                                    builder,
                                    $clipboard,
                                    $inputs,
                                    &mut vec[0],
                                    0,
                                    $field_width,
                                    $frame_time
                                );
                            });

                            return;
                        }

                        for (i, time) in vec.iter_mut().enumerate()
                        {
                            strip.strip(|builder| {
                                response |= set_atlas_per_frame_time!(
                                    builder,
                                    $clipboard,
                                    $inputs,
                                    time,
                                    i,
                                    $field_width,
                                    $frame_time,
                                    $move_up,
                                    $move_down
                                );
                            });
                        }
                    });
            }
        };

        response
    }};
}

use atlas;

//=======================================================================//

/// Shows the UI elements concerning a list animation.
macro_rules! list {
    (
        $ui:ident,
        $animation:ident,
        $drawing_resources:ident,
        $clipboard:ident,
        $inputs:ident,
        $field_width:ident,
        $texture:expr,
        $time:expr,
        $move_up:expr,
        $move_down:expr,
        $remove:expr,
        $push:expr
    ) => {{
        let mut response = Response::default();
        let (vec, texture_slot) =
            match_or_panic!($animation, UiOverallListAnimation::Uniform(vec, slot), (vec, slot));

        egui_extras::StripBuilder::new($ui)
            .sizes(egui_extras::Size::exact(SETTING_HEIGHT), vec.len() + 1)
            .vertical(|mut strip| {
                if vec.len() == 1
                {
                    let (texture, time) = &mut vec[0];

                    strip.strip(|builder| {
                        response |= edit_list_single_texture!(
                            builder,
                            $drawing_resources,
                            $clipboard,
                            $inputs,
                            texture,
                            time,
                            0,
                            $field_width,
                            $texture,
                            $time
                        );
                    });
                }
                else
                {
                    for (i, (name, time)) in vec.iter_mut().enumerate()
                    {
                        strip.strip(|builder| {
                            response |= edit_list_texture!(
                                builder,
                                $drawing_resources,
                                $clipboard,
                                $inputs,
                                name,
                                time,
                                i,
                                $field_width,
                                $texture,
                                $time,
                                $move_up,
                                $move_down,
                                $remove
                            );
                        });
                    }
                }

                strip.strip(|builder| {
                    use crate::map::editor::state::ui::texture_editor::animation_editor::new_list_texture;

                    response |= new_list_texture(
                        builder,
                        $drawing_resources,
                        $clipboard,
                        $inputs,
                        texture_slot,
                        vec.len(),
                        $field_width,
                        $push
                    );
                });
            });

        response
    }};
}

use list;

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The target of the animation edits.
#[derive(Default)]
pub(in crate::map::editor::state::ui::texture_editor) enum Target
{
    /// No target.
    #[default]
    None,
    /// The overall texture itself or a specific override.
    Texture(Option<(String, UiOverallAnimation)>),
    /// The selected brushes.
    Brushes
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// An UI editor to edit the animation of a texture or of the selected brushes.
#[derive(Default)]
pub(in crate::map::editor::state::ui) struct AnimationEditor
{
    /// The displayed animation.
    animation:                UiOverallAnimation,
    /// The target of the animation editing.
    pub target:               Target,
    /// Whever a texture animation update was scheduled.
    update_texture_animation: bool
}

impl AnimationEditor
{
    /// Whever the animation editor is open.
    #[inline]
    #[must_use]
    pub const fn is_open(&self) -> bool { !matches!(self.target, Target::None) }

    /// Closes the animation editor.
    #[inline]
    pub fn close(&mut self) { self.target = Target::None; }

    /// Opens the animation editor.
    #[inline]
    pub fn open(&mut self, target: Target) { self.target = target; }

    /// Whever a texture can be added to a list animation by clicking on a texture in the preview
    /// gallery.
    #[inline]
    #[must_use]
    pub fn can_add_textures_to_atlas(&self, overall_animation: &UiOverallAnimation) -> bool
    {
        match &self.target
        {
            Target::None => false,
            Target::Texture(over) =>
            {
                matches!(
                    over.as_ref().map_or(&self.animation, |(_, anim)| anim),
                    UiOverallAnimation::List(UiOverallListAnimation::Uniform(..))
                )
            },
            Target::Brushes =>
            {
                matches!(
                    overall_animation,
                    UiOverallAnimation::List(UiOverallListAnimation::Uniform(..))
                )
            }
        }
    }

    /// Whever the texture override is set.
    #[inline]
    #[must_use]
    pub const fn has_override(&self) -> bool { matches!(self.target, Target::Texture(Some(_))) }

    /// Returns the name of the texture override, if any.
    #[inline]
    #[must_use]
    pub const fn texture_override(&self) -> Option<&String>
    {
        match &self.target
        {
            Target::Texture(Some((name, _))) => Some(name),
            _ => None
        }
    }

    /// Sets the override of the texture whose animation is to be edited.
    #[inline]
    pub fn set_texture_override(&mut self, texture: &Texture)
    {
        self.target = Target::Texture(
            (
                texture.name().to_string(),
                UiOverallAnimation::from(OverallAnimation::from(texture.animation()))
            )
                .into()
        );
    }

    /// Schedules the update of the displayed texture animation.
    #[inline]
    pub fn schedule_texture_animation_update(&mut self) { self.update_texture_animation = true; }

    /// Adds a frame to a list animation.
    #[inline]
    pub fn push_list_animation_frame(
        &mut self,
        texture: &mut Texture,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        new_texture: &str
    )
    {
        match &mut self.target
        {
            Target::None => panic!("No list animation frame push target."),
            Target::Texture(over) =>
            {
                let animation = over
                    .as_mut()
                    .map(|(_, animation)| animation)
                    .unwrap_or(&mut self.animation);

                match_or_panic!(
                    animation,
                    UiOverallAnimation::List(UiOverallListAnimation::Uniform(_, slot)),
                    slot
                )
                .update(false, true, |name| {
                    edits_history.default_animation_list_new_frame(texture, new_texture);
                    texture
                        .animation_mut_set_dirty()
                        .get_list_animation_mut()
                        .push(new_texture);
                    name.into()
                });

                *animation = OverallAnimation::from(texture.animation()).into();
            },
            Target::Brushes =>
            {
                edits_history.list_animation_new_frame(
                    manager.selected_textured_brushes_mut().map(|mut brush| {
                        brush.push_list_animation_frame(new_texture);
                        brush.id()
                    }),
                    new_texture
                );
            }
        };
    }

    /// Updates the displayed animation from the overall texture settings.
    #[inline]
    pub fn update_from_overall_texture(
        &mut self,
        drawing_resources: &DrawingResources,
        texture: &UiOverallTextureSettings
    )
    {
        self.animation = match texture.name.uniform_value()
        {
            Some(name) =>
            {
                OverallAnimation::from(drawing_resources.texture_or_error(name).animation()).into()
            },
            None =>
            {
                self.close();
                UiOverallAnimation::None
            }
        };
    }

    /// Checks whever the sprites are within bounds.
    #[inline]
    #[must_use]
    fn check_sprites_within_bounds(
        drawing_resources: &DrawingResources,
        texture: &str,
        manager: &mut EntitiesManager
    ) -> bool
    {
        manager.test_operation_validity(|manager| {
            return_if_none!(manager.selected_brushes_with_texture_sprite_mut(texture), None)
                .find_map(|mut brush| {
                    (!brush.check_texture_within_bounds(drawing_resources)).then_some(brush.id())
                })
        })
    }

    /// UI elements to edit a list animation.
    #[inline]
    fn list(
        ui: &mut egui::Ui,
        bundle: &mut Bundle,
        texture: &mut Texture,
        animation: &mut UiOverallListAnimation,
        field_width: f32
    ) -> Response
    {
        let Bundle {
            drawing_resources,
            edits_history,
            clipboard,
            inputs,
            ..
        } = bundle;

        list!(
            ui,
            animation,
            drawing_resources,
            clipboard,
            inputs,
            field_width,
            |index, new_texture| {
                let prev = texture
                    .animation_mut_set_dirty()
                    .get_list_animation_mut()
                    .set_texture(index, new_texture)
                    .unwrap();

                edits_history.default_animation_list_texture(texture, index, &prev);
            },
            |index, time| {
                let prev = texture
                    .animation_mut_set_dirty()
                    .get_list_animation_mut()
                    .set_time(index, time)
                    .unwrap();

                edits_history.default_animation_list_time(texture, index, prev);
            },
            |index| {
                edits_history.default_animation_move_up(texture, index, false);
                texture
                    .animation_mut_set_dirty()
                    .get_list_animation_mut()
                    .move_up(index);
            },
            |index| {
                edits_history.default_animation_move_down(texture, index, false);
                texture
                    .animation_mut_set_dirty()
                    .get_list_animation_mut()
                    .move_down(index);
            },
            |index| {
                let (prev, time) = texture.animation().get_list_animation().frame(index);
                edits_history.default_animation_list_frame_removal(texture, index, prev, *time);
                texture
                    .animation_mut_set_dirty()
                    .get_list_animation_mut()
                    .remove(index);
            },
            |new_texture| {
                edits_history.default_animation_list_new_frame(texture, new_texture);
                texture
                    .animation_mut_set_dirty()
                    .get_list_animation_mut()
                    .push(new_texture);
            }
        )
    }

    /// UI elements to edit an atlas animation.
    #[inline]
    fn atlas(
        ui: &mut egui::Ui,
        bundle: &mut Bundle,
        texture: &mut Texture,
        atlas: &mut UiOverallAtlasAnimation,
        field_width: f32
    ) -> Response
    {
        let Bundle {
            drawing_resources,
            manager,
            edits_history,
            clipboard,
            inputs,
            ..
        } = bundle;

        /// Shows the UI elements to edit the x or y partitioning of the animation based on `xy`.
        macro_rules! xy_partition {
            ($xy:ident) => {
                paste::paste! {
                    |value| {
                        if !Self::check_sprites_within_bounds(
                            drawing_resources,
                            texture.name(),
                            manager
                        )
                        {
                            return false;
                        }

                        let prev = texture
                            .animation_mut_set_dirty()
                            .get_atlas_animation_mut()
                            .[< set_ $xy _partition >](value)
                            .unwrap();
                        edits_history.[< default_animation_atlas_ $xy >](texture, prev);
                        true
                    }
                }
            };
        }

        /// Moves the frame up or down based on `ud`.
        macro_rules! move_up_down {
            ($ud:ident) => {
                paste::paste! {
                    |index| {
                        edits_history.[< default_animation_move_ $ud >](texture, index, true);
                        texture.animation_mut_set_dirty().get_atlas_animation_mut().[< move_ $ud >](index);
                    }
                }
            };
        }

        atlas!(
            ui,
            atlas,
            drawing_resources,
            manager,
            clipboard,
            inputs,
            *elapsed_time,
            field_width,
            xy_partition!(x),
            xy_partition!(y),
            |len| {
                let atlas = texture.animation_mut_set_dirty().get_atlas_animation_mut();
                let len = len.min(atlas.max_len());

                texture
                    .animation_mut_set_dirty()
                    .get_atlas_animation_mut()
                    .set_len(len)
                    .map(|prev| {
                        edits_history.default_animation_atlas_len(texture, prev);
                        prev
                    })
            },
            |[uniform, per_frame], changed| {
                if uniform.clicked()
                {
                    if let Some(timing) = texture
                        .animation_mut_set_dirty()
                        .get_atlas_animation_mut()
                        .set_uniform()
                    {
                        edits_history.default_animation_atlas_timing(texture, timing);
                    }

                    *changed = true;
                }
                else if per_frame.clicked()
                {
                    if let Some(timing) = texture
                        .animation_mut_set_dirty()
                        .get_atlas_animation_mut()
                        .set_per_frame()
                    {
                        edits_history.default_animation_atlas_timing(texture, timing);
                    }

                    *changed = true;
                }

                if texture.animation().get_atlas_animation().is_uniform()
                {
                    uniform
                }
                else
                {
                    per_frame
                }
                .into()
            },
            |_, time| {
                let prev = texture
                    .animation_mut_set_dirty()
                    .get_atlas_animation_mut()
                    .set_uniform_time(time)
                    .unwrap();

                edits_history.default_animation_atlas_uniform_time(texture, prev);
            },
            |index, time| {
                let prev = texture
                    .animation_mut_set_dirty()
                    .get_atlas_animation_mut()
                    .set_frame_time(index, time)
                    .unwrap();

                edits_history.default_animation_atlas_frame_time(texture, index, prev);
            },
            move_up_down!(up),
            move_up_down!(down)
        )
    }

    /// Shows the texture animation editor.
    #[inline]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        bundle: &mut Bundle,
        overall_texture: &mut UiOverallTextureSettings,
        available_width: f32
    ) -> bool
    {
        if !self.is_open()
        {
            return false;
        }

        let field_width = (available_width -
            INDEX_WIDTH -
            FIELD_NAME_WIDTH * 2f32 -
            DELETE_BUTTON_WIDTH -
            MINUS_PLUS_TOTAL_WIDTH) /
            2f32 -
            10f32;

        let mut response = Response::default();

        match &mut self.target
        {
            Target::None => unreachable!(),
            Target::Texture(over) =>
            {
                /// Updates the overalll animation of the texture being edited.
                #[inline]
                fn update_animation(
                    over: &mut Option<(String, UiOverallAnimation)>,
                    overall_animation: &mut UiOverallAnimation,
                    selected_texture: &Texture
                )
                {
                    let animation = match over
                    {
                        Some((_, over)) => over,
                        None => overall_animation
                    };
                    *animation = OverallAnimation::from(selected_texture.animation()).into();
                }

                let Bundle {
                    drawing_resources,
                    manager,
                    edits_history,
                    ..
                } = bundle;

                let selected_texture = &mut unsafe {
                    std::ptr::from_mut(*drawing_resources)
                        .as_mut()
                        .unwrap()
                        .texture_mut(
                            over.as_ref()
                                .map(|(name, _)| name)
                                .or_else(|| overall_texture.name.uniform_value())
                                .unwrap()
                                .as_str()
                        )
                        .unwrap()
                };

                if std::mem::take(&mut self.update_texture_animation)
                {
                    update_animation(over, &mut self.animation, selected_texture);
                }

                ui.vertical(|ui| {
                    ui.set_height(SETTING_HEIGHT);

                    animation_pick(ui, |[none, list, atlas]| {
                        /// Checks whever an animation change is valid.
                        #[inline]
                        fn check_animation_change(
                            drawing_resources: &DrawingResources,
                            manager: &mut EntitiesManager,
                            edits_history: &mut EditsHistory,
                            texture: &mut Texture,
                            new_animation: Animation
                        )
                        {
                            let prev = std::mem::replace(texture.animation_mut(), new_animation);

                            if AnimationEditor::check_sprites_within_bounds(
                                drawing_resources,
                                texture.name(),
                                manager
                            )
                            {
                                _ = texture.animation_mut_set_dirty();
                                edits_history.default_animation(texture, prev);
                                return;
                            }

                            *texture.animation_mut() = prev;
                        }

                        if none.clicked()
                        {
                            check_animation_change(
                                drawing_resources,
                                manager,
                                edits_history,
                                selected_texture,
                                Animation::None
                            );
                        }
                        else if list.clicked()
                        {
                            let new_animation = Animation::list_animation(selected_texture.name());

                            check_animation_change(
                                drawing_resources,
                                manager,
                                edits_history,
                                selected_texture,
                                new_animation
                            );
                        }
                        else if atlas.clicked()
                        {
                            check_animation_change(
                                drawing_resources,
                                manager,
                                edits_history,
                                selected_texture,
                                Animation::atlas_animation()
                            );
                        };

                        match selected_texture.animation()
                        {
                            Animation::None => none,
                            Animation::List(_) => list,
                            Animation::Atlas(_) => atlas
                        }
                        .into()
                    });
                });

                ui.separator();

                match over.as_mut().map(|(_, anim)| anim).unwrap_or(&mut self.animation)
                {
                    UiOverallAnimation::NoSelection => unreachable!(),
                    UiOverallAnimation::NonUniform | UiOverallAnimation::None => (),
                    UiOverallAnimation::List(value) =>
                    {
                        response |= Self::list(ui, bundle, selected_texture, value, field_width);
                    },
                    UiOverallAnimation::Atlas(atlas) =>
                    {
                        response |= Self::atlas(ui, bundle, selected_texture, atlas, field_width);
                    }
                };

                if selected_texture.dirty()
                {
                    update_animation(over, &mut self.animation, selected_texture);
                }
            },
            Target::Brushes =>
            {
                response |=
                    InstancesEditor::show(ui, bundle, &mut overall_texture.animation, field_width);
            }
        };

        response.has_focus
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// UI element to pick the animation type of a texture.
#[inline]
fn animation_pick<F>(ui: &mut egui::Ui, mut f: F)
where
    F: FnMut([egui::Response; 3]) -> Option<egui::Response>
{
    egui_extras::StripBuilder::new(ui)
        .size(egui_extras::Size::exact(LEFT_FIELD))
        .size(egui_extras::Size::remainder())
        .horizontal(|mut strip| {
            strip.cell(|ui| {
                ui.label("Animation");
            });

            strip.cell(|ui| {
                ui.horizontal(|ui| {
                    return_if_none!(f([ui.button("None"), ui.button("List"), ui.button("Atlas")]))
                        .highlight();
                });
            });
        });
}

//=======================================================================//

/// UI element to set the partitioning of an atlas animation.
#[inline]
fn set_partition<F>(
    builder: egui_extras::StripBuilder,
    clipboard: &mut Clipboard,
    inputs: &InputsPresses,
    value: &mut UiOverallValue<u32>,
    label: &str,
    field_width: f32,
    mut f: F
) -> Response
where
    F: FnMut(u32) -> bool
{
    let mut response = Response::default();

    builder
        .size(egui_extras::Size::exact(LEFT_FIELD))
        .size(egui_extras::Size::exact(field_width))
        .horizontal(|mut strip| {
            strip.cell(|ui| {
                ui.label(label);
            });

            strip.cell(|ui| {
                response =
                    OverallValueField::show_always_enabled(ui, clipboard, inputs, value, |value| {
                        if value != 0 && f(value)
                        {
                            return value.into();
                        }

                        None
                    });
            });
        });

    response
}

//=======================================================================//

/// UI element to set the timing of an atlas animation.
#[inline]
fn set_atlas_timing<F>(ui: &mut egui::Ui, mut f: F) -> bool
where
    F: FnMut([egui::Response; 2], &mut bool) -> Option<egui::Response>
{
    let mut value_changed = false;

    egui_extras::StripBuilder::new(ui)
        .size(egui_extras::Size::exact(LEFT_FIELD))
        .size(egui_extras::Size::remainder())
        .horizontal(|mut strip| {
            strip.cell(|ui| {
                ui.label("Timing");
            });

            strip.cell(|ui| {
                ui.horizontal(|ui| {
                    return_if_none!(f(
                        [
                            ui.add(egui::Button::new("Uniform")),
                            ui.add(egui::Button::new("Per Frame"))
                        ],
                        &mut value_changed
                    ))
                    .highlight();
                });
            });
        });

    value_changed
}

//=======================================================================//

/// UI element to set the time of a frame of an atlas animation.
#[inline]
fn set_atlas_time<F>(
    strip: &mut egui_extras::Strip,
    clipboard: &mut Clipboard,
    inputs: &InputsPresses,
    value: &mut UiOverallValue<f32>,
    index: usize,
    mut f: F
) -> Response
where
    F: FnMut(usize, f32)
{
    let mut response = Response::default();

    strip.cell(|ui| {
        ui.label(INDEXES[index]);
    });

    strip.cell(|ui| {
        ui.label("Time");
    });

    strip.cell(|ui| {
        response = OverallValueField::show_always_enabled(ui, clipboard, inputs, value, |value| {
            if value > 0f32
            {
                f(index, value);
                return value.into();
            }

            None
        });
    });

    response
}

//=======================================================================//

/// UI element to set the amount of frames of an atlas animation.
#[inline]
fn set_len<F>(
    builder: egui_extras::StripBuilder,
    clipboard: &mut Clipboard,
    inputs: &InputsPresses,
    value: &mut UiOverallValue<usize>,
    field_width: f32,
    mut f: F
) -> Response
where
    F: FnMut(usize) -> Option<usize>
{
    let mut response = Response::default();

    builder
        .size(egui_extras::Size::exact(LEFT_FIELD))
        .size(egui_extras::Size::exact(field_width))
        .horizontal(|mut strip| {
            strip.cell(|ui| {
                ui.label("Length");
            });

            strip.cell(|ui| {
                response =
                    OverallValueField::show_always_enabled(ui, clipboard, inputs, value, |value| {
                        if value != 0
                        {
                            return f(value);
                        }

                        None
                    });
            });
        });

    response
}

//=======================================================================//

/// UI element to set the time of an atlas animation frame.
#[inline]
fn set_single_atlas_time<F>(
    builder: egui_extras::StripBuilder,
    clipboard: &mut Clipboard,
    inputs: &InputsPresses,
    value: &mut UiOverallValue<f32>,
    index: usize,
    field_width: f32,
    f: F
) -> Response
where
    F: FnMut(usize, f32)
{
    let mut response = Response::default();

    builder
        .size(egui_extras::Size::exact(INDEX_WIDTH))
        .size(egui_extras::Size::exact(FIELD_NAME_WIDTH))
        .size(egui_extras::Size::exact(field_width))
        .horizontal(|mut strip| {
            response |= set_atlas_time(&mut strip, clipboard, inputs, value, index, f);
        });

    response
}

//=======================================================================//

/// UI element to set the texture in a list animation.
#[inline]
fn set_list_texture<F>(
    strip: &mut egui_extras::Strip,
    drawing_resources: &DrawingResources,
    clipboard: &mut Clipboard,
    inputs: &InputsPresses,
    texture: &mut UiOverallValue<String>,
    index: usize,
    mut f: F
) -> Response
where
    F: FnMut(usize, &str)
{
    let mut response = Response::default();

    strip.cell(|ui| {
        ui.label(INDEXES[index]);
    });

    strip.cell(|ui| {
        ui.label("Texture");
    });

    strip.cell(|ui| {
        response |=
            OverallValueField::show_always_enabled(ui, clipboard, inputs, texture, |name| {
                if let Some(texture) = drawing_resources.texture(&name)
                {
                    f(index, texture.name());
                    return name.into();
                }

                None
            });
    });

    response
}

//=======================================================================//

/// Sets the time of a texture of a list animation.
#[inline]
fn set_list_time<F>(
    strip: &mut egui_extras::Strip,
    clipboard: &mut Clipboard,
    inputs: &InputsPresses,
    time: &mut UiOverallValue<f32>,
    index: usize,
    mut f: F
) -> Response
where
    F: FnMut(usize, f32)
{
    let mut response = Response::default();

    strip.cell(|ui| {
        ui.label("Time");
    });

    strip.cell(|ui| {
        response |= OverallValueField::show_always_enabled(ui, clipboard, inputs, time, |time| {
            if time > 0f32
            {
                f(index, time);
                return time.into();
            }

            None
        });
    });

    response
}

//=======================================================================//

/// UI element to add a new texture to a list animation.
#[inline]
fn new_list_texture<F>(
    builder: egui_extras::StripBuilder,
    drawing_resources: &DrawingResources,
    clipboard: &mut Clipboard,
    inputs: &InputsPresses,
    texture_slot: &mut UiOverallValue<String>,
    index: usize,
    field_width: f32,
    mut f: F
) -> Response
where
    F: FnMut(&str)
{
    let mut response = Response::default();

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
                response |= OverallValueField::show_always_enabled(
                    ui,
                    clipboard,
                    inputs,
                    texture_slot,
                    |name| {
                        if let Some(texture) = drawing_resources.texture(&name)
                        {
                            f(texture.name());
                            return name.into();
                        }

                        None
                    }
                );
            });
        });

    response
}
