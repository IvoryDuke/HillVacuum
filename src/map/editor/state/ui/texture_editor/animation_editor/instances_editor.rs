//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use hill_vacuum_shared::NextValue;

use super::animation_pick;
use crate::{
    map::{
        brush::Brush,
        drawer::{
            animation::{
                overall_values::{
                    UiOverallAnimation,
                    UiOverallAtlasAnimation,
                    UiOverallListAnimation
                },
                Animation
            },
            texture::Texture
        },
        editor::state::ui::{
            overall_value_field::OverallValueField,
            texture_editor::{
                animation_editor::{atlas::atlas_editor, list::list_editor},
                UiBundle,
                SETTING_HEIGHT
            }
        }
    },
    utils::identifiers::EntityId
};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The editor of the selected brushes texture [`Animation`].
pub(in crate::map::editor::state::ui::texture_editor::animation_editor) struct InstancesEditor;

impl InstancesEditor
{
    /// UI elements to edit a list animation.
    #[inline]
    fn list(
        ui: &mut egui::Ui,
        bundle: &mut UiBundle,
        animation: &mut UiOverallListAnimation,
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

        list_editor(
            ui,
            drawing_resources,
            clipboard,
            edits_history,
            inputs,
            manager,
            animation,
            field_width,
            |edits_history, manager, index, texture| {
                let name = manager
                    .selected_textured_brushes()
                    .next_value()
                    .texture_list_animation_frame(index)
                    .0
                    .clone();

                edits_history.list_animation_texture(
                    manager.selected_textured_brushes_mut(drawing_resources, grid).map(
                        |mut brush| {
                            _ = brush.set_list_animation_texture(index, texture);
                            brush.id()
                        }
                    ),
                    index,
                    name
                );
            },
            |edits_history, manager, index, time| {
                let prev = manager
                    .selected_textured_brushes()
                    .next_value()
                    .texture_list_animation_frame(index)
                    .1;

                edits_history.list_animation_time(
                    manager.selected_textured_brushes_mut(drawing_resources, grid).map(
                        |mut brush| {
                            _ = brush.set_texture_list_animation_time(index, time);
                            brush.id()
                        }
                    ),
                    index,
                    prev
                );
            },
            |edits_history, manager, index| {
                edits_history.animation_move_up(
                    manager.selected_textured_brushes_mut(drawing_resources, grid).map(
                        |mut brush| {
                            brush.move_up_list_animation_frame(index);
                            brush.id()
                        }
                    ),
                    index,
                    false
                );
            },
            |edits_history, manager, index| {
                edits_history.animation_move_down(
                    manager.selected_textured_brushes_mut(drawing_resources, grid).map(
                        |mut brush| {
                            brush.move_down_list_animation_frame(index);
                            brush.id()
                        }
                    ),
                    index,
                    false
                );
            },
            |edits_history, manager, index| {
                let (texture, time) = manager
                    .selected_textured_brushes()
                    .next_value()
                    .texture_list_animation_frame(index)
                    .clone();

                edits_history.list_animation_frame_removal(
                    manager.selected_textured_brushes_mut(drawing_resources, grid).map(
                        |mut brush| {
                            brush.remove_list_animation_frame(index);
                            brush.id()
                        }
                    ),
                    index,
                    texture,
                    time
                );
            },
            |edits_history, manager, texture| {
                edits_history.list_animation_new_frame(
                    manager.selected_textured_brushes_mut(drawing_resources, grid).map(
                        |mut brush| {
                            brush.push_list_animation_frame(texture);
                            brush.id()
                        }
                    ),
                    texture
                );
            }
        );
    }

    /// UI elements to edit an atlas animation.
    #[inline]
    fn atlas(
        ui: &mut egui::Ui,
        bundle: &mut UiBundle,
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
                    |manager, edits_history, _, value| {
                        let valid = manager.test_operation_validity(|manager| {
                            manager.selected_brushes_with_sprite_mut(drawing_resources, grid).find_map(|mut brush| {
                                (!brush.[< check_atlas_animation_ $xy _partition >](
                                    drawing_resources,
                                    grid,
                                    value
                                )).then_some(brush.id())
                            })
                        });

                        if !valid
                        {
                            return false;
                        }

                        edits_history.[< atlas_ $xy _cluster >](
                            manager.selected_textured_brushes_mut(drawing_resources, grid).filter_map(|mut brush| {
                                brush
                                    .[< set_texture_atlas_animation_ $xy _partition >](
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
                    |manager, edits_history, _, index| {
                        edits_history.[< animation_move_ $ud >](
                            manager.selected_textured_brushes_mut(drawing_resources, grid).map(|mut brush| {
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

        atlas_editor(
            ui,
            manager,
            clipboard,
            edits_history,
            inputs,
            &mut (),
            atlas,
            field_width,
            xy_partition!(x),
            xy_partition!(y),
            |manager, edits_history, _, len| {
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
                    manager
                        .selected_textured_brushes_mut(drawing_resources, grid)
                        .filter_map(|mut brush| {
                            brush
                                .set_texture_atlas_animation_len(len)
                                .map(|prev| (brush.id(), prev))
                        })
                );

                len.into()
            },
            |manager, edits_history, _, [uniform, per_frame]| {
                if uniform.clicked()
                {
                    edits_history.atlas_timing_cluster(
                        manager
                            .selected_textured_brushes_mut(drawing_resources, grid)
                            .filter_map(|mut brush| {
                                brush
                                    .set_atlas_animation_uniform_timing()
                                    .map(|timing| (brush.id(), timing))
                            })
                    );
                }
                else if per_frame.clicked()
                {
                    edits_history.atlas_timing_cluster(
                        manager
                            .selected_textured_brushes_mut(drawing_resources, grid)
                            .filter_map(|mut brush| {
                                brush
                                    .set_atlas_animation_per_frame_timing()
                                    .map(|timing| (brush.id(), timing))
                            })
                    );
                }
            },
            |manager, edits_history, _, _, time| {
                edits_history.atlas_uniform_time_cluster(
                    manager
                        .selected_textured_brushes_mut(drawing_resources, grid)
                        .filter_map(|mut brush| {
                            brush
                                .set_texture_atlas_animation_uniform_time(time)
                                .map(|prev| (brush.id(), prev))
                        })
                );
            },
            |manager, edits_history, _, index, time| {
                edits_history.atlas_frame_time_cluster(
                    manager
                        .selected_textured_brushes_mut(drawing_resources, grid)
                        .filter_map(|mut brush| {
                            brush
                                .set_texture_atlas_animation_frame_time(index, time)
                                .map(|prev| (brush.id(), (index, prev)))
                        })
                );
            },
            move_up_down!(up),
            move_up_down!(down)
        );
    }

    /// Shows the UI elements of the editor.
    #[inline]
    pub fn show(
        ui: &mut egui::Ui,
        bundle: &mut UiBundle,
        overall_animation: &mut UiOverallAnimation,
        field_width: f32
    )
    {
        ui.vertical(|ui| {
            ui.set_height(SETTING_HEIGHT);
            animation_pick(ui, |[none, list, atlas]| {
                #[inline]
                fn anim_change<F>(bundle: &mut UiBundle, new: &Animation, f: F)
                where
                    F: Fn(&mut Brush) -> Animation
                {
                    let valid = bundle.manager.test_operation_validity(|manager| {
                        manager
                            .selected_brushes_with_sprite_mut(bundle.drawing_resources, bundle.grid)
                            .find_map(|mut brush| {
                                (!brush.check_texture_animation_change(
                                    bundle.drawing_resources,
                                    bundle.grid,
                                    new
                                ))
                                .then_some(brush.id())
                            })
                    });

                    if !valid
                    {
                        return;
                    }

                    bundle.edits_history.animation_cluster(
                        bundle
                            .manager
                            .selected_textured_brushes_mut(bundle.drawing_resources, bundle.grid)
                            .map(|mut brush| (brush.id(), f(&mut brush)))
                    );
                }

                if none.clicked()
                {
                    anim_change(bundle, &Animation::None, |brush| {
                        brush.set_texture_animation(Animation::None)
                    });
                }
                else if list.clicked()
                {
                    anim_change(bundle, &Animation::None, Brush::generate_list_animation);
                }
                else if atlas.clicked()
                {
                    anim_change(bundle, &Animation::atlas_animation(), |brush| {
                        brush.set_texture_animation(Animation::atlas_animation())
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

        egui::ScrollArea::vertical().show(ui, |ui| {
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
                            OverallValueField::show_always_enabled(
                                ui,
                                bundle.clipboard,
                                bundle.inputs,
                                slot,
                                |value| {
                                    if let Some(texture) =
                                        bundle.drawing_resources.texture(&value).map(Texture::name)
                                    {
                                        for mut brush in
                                            bundle.manager.selected_textured_brushes_mut(
                                                bundle.drawing_resources,
                                                bundle.grid
                                            )
                                        {
                                            bundle.edits_history.animation(
                                                brush.id(),
                                                brush.set_texture_list_animation(texture)
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
                            Self::list(ui, bundle, list, field_width);
                        }
                    };
                },
                UiOverallAnimation::Atlas(atlas) =>
                {
                    Self::atlas(ui, bundle, atlas, field_width);
                }
            };
        });
    }
}
