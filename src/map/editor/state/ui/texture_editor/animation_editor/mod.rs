mod atlas;
mod instances_editor;
mod list;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use atlas::atlas_editor;
use bevy_egui::egui;
use hill_vacuum_shared::{match_or_panic, return_if_none};

use super::{UiBundle, FIELD_NAME_WIDTH, SETTING_HEIGHT};
use crate::{
    map::{
        drawer::{
            animation::{
                overall_values::{
                    OverallAnimation,
                    UiOverallAnimation,
                    UiOverallAtlasAnimation,
                    UiOverallListAnimation
                },
                Animation,
                MoveUpDown
            },
            drawing_resources::DrawingResources,
            overall_values::UiOverallTextureSettings,
            texture::Texture
        },
        editor::state::{
            edits_history::EditsHistory,
            grid::Grid,
            manager::EntitiesManager,
            ui::texture_editor::{
                animation_editor::instances_editor::InstancesEditor,
                DELETE_BUTTON_WIDTH
            }
        }
    },
    utils::{
        identifiers::EntityId,
        misc::{ReplaceValue, TakeValue},
        overall_value::OverallValueToUi
    }
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
// STRUCTS
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
    /// Whether a texture animation update was scheduled.
    update_texture_animation: bool
}

impl AnimationEditor
{
    /// Whether the animation editor is open.
    #[inline]
    #[must_use]
    pub const fn is_open(&self) -> bool { !matches!(self.target, Target::None) }

    /// Closes the animation editor.
    #[inline]
    pub fn close(&mut self) { self.target = Target::None; }

    /// Opens the animation editor.
    #[inline]
    pub fn open(&mut self, target: Target) { self.target = target; }

    /// Whether a texture can be added to a list animation by clicking on a texture in the preview
    /// gallery.
    #[inline]
    #[must_use]
    pub fn can_add_textures_to_list(&self, overall_animation: &UiOverallAnimation) -> bool
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

    /// Whether the texture override is set.
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
        drawing_resources: &mut DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        overall_texture: &UiOverallTextureSettings,
        new_texture: &str
    )
    {
        match &mut self.target
        {
            Target::None => panic!("No list animation frame push target."),
            Target::Texture(over) =>
            {
                let texture = &mut drawing_resources
                    .texture_mut(
                        over.as_ref()
                            .map(|(name, _)| name)
                            .or_else(|| overall_texture.name.uniform_value())
                            .unwrap()
                            .as_str()
                    )
                    .unwrap();
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
                    manager.selected_textured_brushes_mut(drawing_resources, grid).map(
                        |mut brush| {
                            brush.push_list_animation_frame(new_texture);
                            brush.id()
                        }
                    ),
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
                OverallAnimation::from(drawing_resources.texture_or_error(name).animation()).ui()
            },
            None =>
            {
                self.close();
                UiOverallAnimation::None
            }
        };
    }

    /// Checks whether the sprites are within bounds.
    #[inline]
    #[must_use]
    fn check_sprites_within_bounds(
        drawing_resources: &DrawingResources,
        grid: &Grid,
        texture: &str,
        manager: &mut EntitiesManager
    ) -> bool
    {
        manager.test_operation_validity(|manager| {
            return_if_none!(
                manager.selected_brushes_with_sprites_mut(drawing_resources, grid, texture),
                None
            )
            .find_map(|mut brush| {
                (!brush.check_texture_within_bounds(drawing_resources, grid)).then_some(brush.id())
            })
        })
    }

    /// UI elements to edit a list animation.
    #[inline]
    fn list(
        ui: &mut egui::Ui,
        bundle: &mut UiBundle,
        texture: &mut Texture,
        animation: &mut UiOverallListAnimation,
        field_width: f32
    )
    {
        let UiBundle {
            drawing_resources,
            edits_history,
            clipboard,
            inputs,
            ..
        } = bundle;

        list::list_editor(
            ui,
            drawing_resources,
            clipboard,
            edits_history,
            inputs,
            texture,
            animation,
            field_width,
            |edits_history, texture, index, new_texture| {
                let prev = return_if_none!(texture
                    .animation_mut_set_dirty()
                    .get_list_animation_mut()
                    .set_texture(index, new_texture));

                edits_history.default_animation_list_texture(texture, index, &prev);
            },
            |edits_history, texture, index, time| {
                let prev = return_if_none!(texture
                    .animation_mut_set_dirty()
                    .get_list_animation_mut()
                    .set_time(index, time));

                edits_history.default_animation_list_time(texture, index, prev);
            },
            |edits_history, texture, index| {
                edits_history.default_animation_move_up(texture, index, false);
                texture
                    .animation_mut_set_dirty()
                    .get_list_animation_mut()
                    .move_up(index);
            },
            |edits_history, texture, index| {
                edits_history.default_animation_move_down(texture, index, false);
                texture
                    .animation_mut_set_dirty()
                    .get_list_animation_mut()
                    .move_down(index);
            },
            |edits_history, texture, index| {
                let (prev, time) = texture.animation().get_list_animation().frame(index);
                edits_history.default_animation_list_frame_removal(texture, index, prev, *time);
                texture
                    .animation_mut_set_dirty()
                    .get_list_animation_mut()
                    .remove(index);
            },
            |edits_history, texture, new_texture| {
                edits_history.default_animation_list_new_frame(texture, new_texture);
                texture
                    .animation_mut_set_dirty()
                    .get_list_animation_mut()
                    .push(new_texture);
            }
        );
    }

    /// UI elements to edit an atlas animation.
    #[inline]
    fn atlas(
        ui: &mut egui::Ui,
        bundle: &mut UiBundle,
        texture: &mut Texture,
        atlas: &mut UiOverallAtlasAnimation,
        field_width: f32
    )
    {
        let UiBundle {
            drawing_resources,
            manager,
            edits_history,
            clipboard,
            inputs,
            grid,
            ..
        } = bundle;

        /// Shows the UI elements to edit the x or y partitioning of the animation based on `xy`.
        macro_rules! xy_partition {
            ($xy:ident) => {
                paste::paste! {
                    |manager, edits_history, texture, value| {
                        if !Self::check_sprites_within_bounds(
                            drawing_resources,
                            grid,
                            texture.name(),
                            manager
                        )
                        {
                            return false;
                        }

                        let prev = return_if_none!(
                            texture
                                .animation_mut_set_dirty()
                                .get_atlas_animation_mut()
                                .[< set_ $xy _partition >](value),
                            false
                        );
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
                    |_, edits_history, texture, index| {
                        edits_history.[< default_animation_move_ $ud >](texture, index, true);
                        texture.animation_mut_set_dirty().get_atlas_animation_mut().[< move_ $ud >](index);
                    }
                }
            };
        }

        atlas_editor(
            ui,
            manager,
            clipboard,
            edits_history,
            inputs,
            texture,
            atlas,
            field_width,
            xy_partition!(x),
            xy_partition!(y),
            |_, edits_history, texture, len| {
                let atlas = texture.animation_mut_set_dirty().get_atlas_animation_mut();
                let len = len.min(atlas.max_len());

                texture
                    .animation_mut_set_dirty()
                    .get_atlas_animation_mut()
                    .set_len(len)
                    .inspect(|prev| {
                        edits_history.default_animation_atlas_len(texture, *prev);
                    })
            },
            |_, edits_history, texture, [uniform, per_frame]| {
                if uniform.clicked()
                {
                    if let Some(timing) = texture
                        .animation_mut_set_dirty()
                        .get_atlas_animation_mut()
                        .set_uniform()
                    {
                        edits_history.default_animation_atlas_timing(texture, timing);
                    }
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
                }
            },
            |_, edits_history, texture, _, time| {
                let prev = return_if_none!(texture
                    .animation_mut_set_dirty()
                    .get_atlas_animation_mut()
                    .set_uniform_time(time));

                edits_history.default_animation_atlas_uniform_time(texture, prev);
            },
            |_, edits_history, texture, index, time| {
                let prev = return_if_none!(texture
                    .animation_mut_set_dirty()
                    .get_atlas_animation_mut()
                    .set_frame_time(index, time));

                edits_history.default_animation_atlas_frame_time(texture, index, prev);
            },
            move_up_down!(up),
            move_up_down!(down)
        );
    }

    /// Shows the texture animation editor.
    #[inline]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        bundle: &mut UiBundle,
        overall_texture: &mut UiOverallTextureSettings,
        available_width: f32
    )
    {
        if !self.is_open()
        {
            return;
        }

        let field_width = (available_width -
            INDEX_WIDTH -
            FIELD_NAME_WIDTH * 2f32 -
            DELETE_BUTTON_WIDTH -
            MINUS_PLUS_TOTAL_WIDTH) /
            2f32 -
            10f32;

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

                let UiBundle {
                    drawing_resources,
                    manager,
                    edits_history,
                    grid,
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

                if self.update_texture_animation.take_value()
                {
                    update_animation(over, &mut self.animation, selected_texture);
                }

                let ui_animation =
                    over.as_mut().map(|(_, anim)| anim).unwrap_or(&mut self.animation);

                ui.vertical(|ui| {
                    ui.set_height(SETTING_HEIGHT);

                    animation_pick(ui, |[none, list, atlas]| {
                        /// Checks whether an animation change is valid.
                        #[inline]
                        fn check_animation_change(
                            drawing_resources: &DrawingResources,
                            manager: &mut EntitiesManager,
                            edits_history: &mut EditsHistory,
                            grid: &Grid,
                            texture: &mut Texture,
                            ui_animation: &mut UiOverallAnimation,
                            new_animation: Animation
                        )
                        {
                            let prev = texture.animation_mut().replace_value(new_animation);

                            if AnimationEditor::check_sprites_within_bounds(
                                drawing_resources,
                                grid,
                                texture.name(),
                                manager
                            )
                            {
                                _ = texture.animation_mut_set_dirty();
                                *ui_animation = OverallAnimation::from(texture.animation()).ui();
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
                                grid,
                                selected_texture,
                                ui_animation,
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
                                grid,
                                selected_texture,
                                ui_animation,
                                new_animation
                            );
                        }
                        else if atlas.clicked()
                        {
                            check_animation_change(
                                drawing_resources,
                                manager,
                                edits_history,
                                grid,
                                selected_texture,
                                ui_animation,
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

                egui::ScrollArea::vertical().show(ui, |ui| {
                    match ui_animation
                    {
                        UiOverallAnimation::NoSelection => unreachable!(),
                        UiOverallAnimation::NonUniform | UiOverallAnimation::None => (),
                        UiOverallAnimation::List(value) =>
                        {
                            Self::list(ui, bundle, selected_texture, value, field_width);
                        },
                        UiOverallAnimation::Atlas(atlas) =>
                        {
                            Self::atlas(ui, bundle, selected_texture, atlas, field_width);
                        }
                    };
                });

                if selected_texture.dirty()
                {
                    update_animation(over, &mut self.animation, selected_texture);
                }
            },
            Target::Brushes =>
            {
                InstancesEditor::show(ui, bundle, &mut overall_texture.animation, field_width);
            }
        };
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// UI element to pick the animation type of a texture.
#[inline]
fn animation_pick<F>(ui: &mut egui::Ui, f: F)
where
    F: FnOnce([egui::Response; 3]) -> Option<egui::Response>
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
