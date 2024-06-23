//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use hill_vacuum_shared::{match_or_panic, NextValue};

use super::animation_pick;
use crate::{
    map::{
        brush::Brush,
        drawer::{
            animation::{
                overall_values::{
                    UiOverallAnimation,
                    UiOverallAtlasAnimation,
                    UiOverallListAnimation,
                    UiOverallTiming
                },
                Animation
            },
            texture::Texture
        },
        editor::state::ui::{
            overall_value_field::{OverallValueField, Response},
            texture_editor::{
                animation_editor::{
                    edit_list_single_texture,
                    edit_list_texture,
                    set_atlas_per_frame_time
                },
                Bundle,
                SETTING_HEIGHT
            }
        }
    },
    utils::identifiers::EntityId
};

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The editor of the selected [`Brush`]es texture [`Animation`].
pub(in crate::map::editor::state::ui::texture_editor::animation_editor) struct InstancesEditor;

impl InstancesEditor
{
    /// UI elements to edit a list animation.
    #[inline]
    fn list(
        ui: &mut egui::Ui,
        bundle: &mut Bundle,
        animation: &mut UiOverallListAnimation,
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

        super::list!(
            ui,
            animation,
            drawing_resources,
            clipboard,
            inputs,
            field_width,
            |index, texture| {
                let name = manager
                    .selected_textured_brushes()
                    .next_value()
                    .texture_list_animation_frame(index)
                    .0
                    .clone();

                edits_history.list_animation_texture(
                    manager.selected_textured_brushes_mut().map(|mut brush| {
                        _ = brush.set_list_animation_texture(index, texture);
                        brush.id()
                    }),
                    index,
                    name
                );
            },
            |index, time| {
                let prev = manager
                    .selected_textured_brushes()
                    .next_value()
                    .texture_list_animation_frame(index)
                    .1;

                edits_history.list_animation_time(
                    manager.selected_textured_brushes_mut().map(|mut brush| {
                        _ = brush.set_texture_list_animation_time(index, time);
                        brush.id()
                    }),
                    index,
                    prev
                );
            },
            |index| {
                edits_history.animation_move_up(
                    manager.selected_textured_brushes_mut().map(|mut brush| {
                        brush.move_up_list_animation_frame(index);
                        brush.id()
                    }),
                    index,
                    false
                );
            },
            |index| {
                edits_history.animation_move_down(
                    manager.selected_textured_brushes_mut().map(|mut brush| {
                        brush.move_down_list_animation_frame(index);
                        brush.id()
                    }),
                    index,
                    false
                );
            },
            |index| {
                let (texture, time) = manager
                    .selected_textured_brushes()
                    .next_value()
                    .texture_list_animation_frame(index)
                    .clone();

                edits_history.list_animation_frame_removal(
                    manager.selected_textured_brushes_mut().map(|mut brush| {
                        brush.remove_list_animation_frame(index);
                        brush.id()
                    }),
                    index,
                    texture,
                    time
                );
            },
            |texture| {
                edits_history.list_animation_new_frame(
                    manager.selected_textured_brushes_mut().map(|mut brush| {
                        brush.push_list_animation_frame(texture);
                        brush.id()
                    }),
                    texture
                );
            }
        )
    }

    /// UI elements to edit an atlas animation.
    #[inline]
    fn atlas(
        ui: &mut egui::Ui,
        bundle: &mut Bundle,
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
                        let valid = manager.test_operation_validity(|manager| {
                            manager.selected_brushes_with_sprite_mut().find_map(|mut brush| {
                                (!brush.[< check_atlas_animation_ $xy _partition >](
                                    drawing_resources,
                                    value
                                )).then_some(brush.id())
                            })
                        });

                        if !valid
                        {
                            return false;
                        }

                        edits_history.[< atlas_ $xy _cluster >](
                            manager.selected_textured_brushes_mut().filter_map(|mut brush| {
                                brush
                                    .[< set_texture_atlas_animation_ $xy _partition >](
                                        drawing_resources,
                                        value
                                    )
                                    .map(|value| (brush.id(), value))
                            })
                        );

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
                        edits_history.[< animation_move_ $ud >](
                            manager.selected_textured_brushes_mut().map(|mut brush| {
                                brush.[< move_ $ud _atlas_animation_frame_time >](index);
                                brush.id()
                            }),
                            index,
                            true
                        );
                    }
                }
            };
        }

        super::atlas!(
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
                let valid = manager.test_operation_validity(|manager| {
                    manager.selected_textured_brushes().find_map(|brush| {
                        (brush.texture_atlas_animation_max_len() < len).then_some(brush.id())
                    })
                });

                if !valid
                {
                    return None;
                }

                edits_history.atlas_len_cluster(
                    manager.selected_textured_brushes_mut().filter_map(|mut brush| {
                        brush
                            .set_texture_atlas_animation_len(len)
                            .map(|prev| (brush.id(), prev))
                    })
                );

                len.into()
            },
            |[uniform, per_frame], _| {
                if uniform.clicked()
                {
                    edits_history.atlas_timing_cluster(
                        manager.selected_textured_brushes_mut().filter_map(|mut brush| {
                            brush
                                .set_atlas_animation_uniform_timing()
                                .map(|timing| (brush.id(), timing))
                        })
                    );
                }
                else if per_frame.clicked()
                {
                    edits_history.atlas_timing_cluster(
                        manager.selected_textured_brushes_mut().filter_map(|mut brush| {
                            brush
                                .set_atlas_animation_per_frame_timing()
                                .map(|timing| (brush.id(), timing))
                        })
                    );
                }

                match &atlas.timing
                {
                    UiOverallTiming::None => unreachable!(),
                    UiOverallTiming::NonUniform => None,
                    UiOverallTiming::Uniform(_) => uniform.into(),
                    UiOverallTiming::PerFrame(_) => per_frame.into()
                }
            },
            |_, time| {
                edits_history.atlas_uniform_time_cluster(
                    manager.selected_textured_brushes_mut().filter_map(|mut brush| {
                        brush
                            .set_texture_atlas_animation_uniform_time(time)
                            .map(|prev| (brush.id(), prev))
                    })
                );
            },
            |index, time| {
                edits_history.atlas_frame_time_cluster(
                    manager.selected_textured_brushes_mut().filter_map(|mut brush| {
                        brush
                            .set_texture_atlas_animation_frame_time(index, time)
                            .map(|prev| (brush.id(), (index, prev)))
                    })
                );
            },
            move_up_down!(up),
            move_up_down!(down)
        )
    }

    /// Shows the UI elements of the editor.
    #[inline]
    pub fn show(
        ui: &mut egui::Ui,
        bundle: &mut Bundle,
        overall_animation: &mut UiOverallAnimation,
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

        let mut response = Response::default();

        ui.vertical(|ui| {
            ui.set_height(SETTING_HEIGHT);
            animation_pick(ui, |[none, list, atlas]| {
                /// Changes the [`Animations`] to the clicked one.
                macro_rules! anim_change {
                    ($new:expr, $f:expr) => {{
                        let new = &$new;
                        let valid = manager.test_operation_validity(|manager| {
                            manager.selected_brushes_with_sprite_mut().find_map(|mut brush| {
                                (!brush.check_texture_animation_change(drawing_resources, new))
                                    .then_some(brush.id())
                            })
                        });

                        #[allow(clippy::redundant_closure_call)]
                        if valid
                        {
                            edits_history.animation_cluster(
                                manager
                                    .selected_textured_brushes_mut()
                                    .map(|mut brush| (brush.id(), $f(&mut brush)))
                            );
                        }
                    }};
                }

                if none.clicked()
                {
                    anim_change!(Animation::None, |brush: &mut Brush| {
                        brush.set_texture_animation(drawing_resources, Animation::None)
                    });
                }
                else if list.clicked()
                {
                    anim_change!(Animation::None, |brush: &mut Brush| {
                        brush.generate_list_animation(drawing_resources)
                    });
                }
                else if atlas.clicked()
                {
                    anim_change!(Animation::atlas_animation(), |brush: &mut Brush| {
                        brush.set_texture_animation(drawing_resources, Animation::atlas_animation())
                    });
                }

                match overall_animation
                {
                    UiOverallAnimation::NoSelection => unreachable!(),
                    UiOverallAnimation::NonUniform => None,
                    UiOverallAnimation::None => none.into(),
                    UiOverallAnimation::List(_) => list.into(),
                    UiOverallAnimation::Atlas { .. } => atlas.into()
                }
            });
        });

        ui.separator();

        match overall_animation
        {
            UiOverallAnimation::NoSelection => unreachable!(),
            UiOverallAnimation::NonUniform | UiOverallAnimation::None => (),
            UiOverallAnimation::List(animation) =>
            {
                match animation
                {
                    UiOverallListAnimation::NonUniform(slot) =>
                    {
                        response |= OverallValueField::show_always_enabled(
                            ui,
                            clipboard,
                            inputs,
                            slot,
                            |value| {
                                if let Some(texture) =
                                    drawing_resources.texture(&value).map(Texture::name)
                                {
                                    for mut brush in manager.selected_textured_brushes_mut()
                                    {
                                        edits_history.animation(
                                            brush.id(),
                                            brush.set_texture_list_animation(
                                                drawing_resources,
                                                texture
                                            )
                                        );
                                    }

                                    return value.into();
                                }

                                None
                            }
                        );
                    },
                    list @ UiOverallListAnimation::Uniform(..) =>
                    {
                        response |= Self::list(ui, bundle, list, field_width);
                    }
                };
            },
            UiOverallAnimation::Atlas(atlas) =>
            {
                response |= Self::atlas(ui, bundle, atlas, field_width);
            }
        };

        response
    }
}
