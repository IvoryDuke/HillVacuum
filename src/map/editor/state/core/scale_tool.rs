//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::Vec2;
use bevy_egui::egui;
use hill_vacuum_shared::return_if_none;

use super::{tool::OngoingMultiframeChange, ActiveTool};
use crate::{
    map::{
        brush::convex_polygon::{ConvexPolygon, ScaleInfo},
        drawer::texture::TextureInterface,
        editor::{
            cursor_pos::Cursor,
            state::{
                core::draw_selected_and_non_selected_brushes,
                editor_state::{edit_target, InputsPresses, TargetSwitch, ToolsSettings},
                edits_history::EditsHistory,
                grid::Grid,
                manager::EntitiesManager
            },
            DrawBundle,
            ToolUpdateBundle
        },
        hv_vec,
        HvVec
    },
    utils::{
        hull::{Corner, Flip, Hull, ScaleResult},
        identifiers::{EntityId, Id},
        math::AroundEqual,
        misc::{Camera, TakeValue}
    }
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The state of the tool.
#[derive(Debug)]
enum Status
{
    /// Scaling with the keyboard.
    Keyboard,
    /// Scaling with mouse drag.
    Drag(HvVec<(Id, ConvexPolygon)>, Vec2, Hull),
    /// Scaling textures with mouse drag.
    DragTextures(HvVec<(Id, (f32, f32))>, Vec2, Hull)
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The scale tool.
#[derive(Debug)]
pub(in crate::map::editor::state::core) struct ScaleTool
{
    /// The state of the tool.
    status:          Status,
    /// The outline of the tool.
    outline:         Hull,
    /// The selected [`Corner`] of the outline.
    selected_corner: Corner
}

impl OngoingMultiframeChange for ScaleTool
{
    #[inline]
    fn ongoing_multi_frame_change(&self) -> bool { matches!(self.status, Status::Drag(..)) }
}

impl ScaleTool
{
    /// The maximum texture scale step.
    const MAX_TEXTURES_SCALE_INTERVAL: f32 = 2f32;
    /// The minimum texture scale step.
    const MIN_TEXTURE_SCALE_INTERVAL: f32 = 0.1;
    /// How much the texture scale is increased by pressing the UI buttons.
    const SCALE_INTERVAL_STEP: f32 = 0.05;

    /// Returns an [`ActiveTool`] in its scale variant.
    #[inline]
    pub fn tool(manager: &EntitiesManager, grid: Grid, settings: &ToolsSettings) -> ActiveTool
    {
        ActiveTool::Scale(ScaleTool {
            status:          Status::Keyboard,
            outline:         Self::outline(manager, grid, settings).unwrap(),
            selected_corner: Corner::TopLeft
        })
    }

    //==============================================================
    // Info

    /// The cursor position used by the tool.
    #[inline]
    #[must_use]
    const fn cursor_pos(cursor: &Cursor) -> Vec2 { cursor.world_snapped() }

    //==============================================================
    // Update

    /// Updates the tool.
    #[inline]
    pub fn update(
        &mut self,
        bundle: &mut ToolUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        grid: Grid,
        settings: &mut ToolsSettings
    )
    {
        let ToolUpdateBundle { camera, cursor, .. } = bundle;

        match &mut self.status
        {
            Status::Keyboard =>
            {
                if !settings.entity_editing()
                {
                    if inputs.plus.just_pressed()
                    {
                        settings.texture_scale_interval = (settings.texture_scale_interval +
                            Self::SCALE_INTERVAL_STEP)
                            .min(Self::MAX_TEXTURES_SCALE_INTERVAL);
                    }
                    else if inputs.minus.just_pressed()
                    {
                        settings.texture_scale_interval = (settings.texture_scale_interval -
                            Self::SCALE_INTERVAL_STEP)
                            .max(Self::MIN_TEXTURE_SCALE_INTERVAL);
                    }
                }

                if inputs.tab.just_pressed()
                {
                    if inputs.alt_pressed()
                    {
                        self.selected_corner = self.selected_corner.previous();
                    }
                    else
                    {
                        self.selected_corner = self.selected_corner.next();
                    }
                }

                if let Some(dir) = inputs.directional_keys_vector(grid.size())
                {
                    self.keyboard_scale(bundle, manager, edits_history, grid, settings, dir);
                }
                else if inputs.left_mouse.just_pressed()
                {
                    self.check_scale_vertex_proximity(
                        Self::cursor_pos(cursor),
                        settings,
                        camera.scale()
                    );
                }
            },
            Status::Drag(backup_polygons, start_pos, hull) =>
            {
                let cursor_pos = Self::cursor_pos(cursor);

                Self::scale_brushes(
                    bundle,
                    manager,
                    hull,
                    &mut self.selected_corner,
                    cursor_pos,
                    backup_polygons,
                    settings.texture_editing()
                );

                if inputs.left_mouse.pressed()
                {
                    return;
                }

                if !cursor_pos.around_equal_narrow(start_pos) && !backup_polygons.is_empty()
                {
                    edits_history.polygon_edit_cluster(backup_polygons.take_value().into_iter());
                }

                self.finalize_drag_scale(manager, grid, settings);
            },
            Status::DragTextures(backup_scales, start_pos, hull) =>
            {
                let cursor_pos = Self::cursor_pos(cursor);

                Self::scale_textures(
                    bundle,
                    manager,
                    hull,
                    &mut self.selected_corner,
                    cursor_pos,
                    backup_scales
                );

                if inputs.left_mouse.pressed()
                {
                    return;
                }

                if !cursor_pos.around_equal_narrow(start_pos) && !backup_scales.is_empty()
                {
                    edits_history
                        .texture_scale_flip_cluster(backup_scales.take_value().into_iter());
                }

                self.finalize_drag_scale(manager, grid, settings);
            }
        };
    }

    /// Scales with the keyboard inputs.
    #[inline]
    fn keyboard_scale(
        &mut self,
        bundle: &mut ToolUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: Grid,
        settings: &ToolsSettings,
        mut dir: Vec2
    )
    {
        let new_corner_position = self.outline.corner_vertex(self.selected_corner) + dir;

        edit_target!(
            settings.target_switch(),
            |scale_texture| {
                let mut backup_polygons = hv_vec![];

                Self::scale_brushes(
                    bundle,
                    manager,
                    &mut self.outline,
                    &mut self.selected_corner,
                    new_corner_position,
                    &mut backup_polygons,
                    scale_texture
                );

                if !backup_polygons.is_empty()
                {
                    edits_history.polygon_edit_cluster(backup_polygons.take_value().into_iter());
                }
            },
            {
                let ToolUpdateBundle {
                    drawing_resources, ..
                } = bundle;

                if dir.x != 0f32
                {
                    dir.x = dir.x.signum() * settings.texture_scale_interval;
                }

                if dir.y != 0f32
                {
                    dir.y = dir.y.signum() * settings.texture_scale_interval;
                }

                let dir = match self.selected_corner
                {
                    Corner::TopRight => dir,
                    Corner::TopLeft => Vec2::new(-dir.x, dir.y),
                    Corner::BottomLeft => Vec2::new(-dir.x, -dir.y),
                    Corner::BottomRight => Vec2::new(dir.x, -dir.y)
                };

                let valid = manager.test_operation_validity(|manager| {
                    manager.selected_textured_brushes_mut().find_map(|mut brush| {
                        let texture = brush.texture_settings().unwrap();
                        let scale_x = texture.scale_x() + dir.x;
                        let scale_y = texture.scale_y() + dir.y;

                        (!brush.check_texture_scale_x(drawing_resources, scale_x) ||
                            !brush.check_texture_scale_y(drawing_resources, scale_y))
                        .then_some(brush.id())
                    })
                });

                if !valid
                {
                    return;
                }

                for mut brush in manager.selected_textured_brushes_mut()
                {
                    let texture = brush.texture_settings().unwrap();
                    let scale_x = texture.scale_x() + dir.x;
                    let scale_y = texture.scale_y() + dir.y;

                    _ = brush.set_texture_scale_x(drawing_resources, scale_x);
                    _ = brush.set_texture_scale_y(drawing_resources, scale_y);
                }

                edits_history.texture_scale_delta(manager.selected_textured_ids().copied(), dir);
                self.update_outline(manager, grid, settings);
            }
        );
    }

    /// Scales the selected [`Brush`]es.
    #[inline]
    fn scale_brushes(
        bundle: &mut ToolUpdateBundle,
        manager: &mut EntitiesManager,
        hull: &mut Hull,
        selected_corner: &mut Corner,
        new_corner_position: Vec2,
        backup_polygons: &mut HvVec<(Id, ConvexPolygon)>,
        scale_texture: bool
    )
    {
        let (new_hull, payloads) = match hull.scaled(selected_corner, new_corner_position)
        {
            ScaleResult::None => return,
            ScaleResult::Scale(new_hull) =>
            {
                let info = ScaleInfo::new(hull, &new_hull).unwrap();
                let mut payloads = hv_vec![capacity; manager.selected_brushes_amount()];

                let valid = manager.test_operation_validity(|manager| {
                    manager.selected_brushes_mut().find_map(|mut brush| {
                        use crate::map::brush::ScaleResult;

                        match brush.check_scale(bundle.drawing_resources, &info, scale_texture)
                        {
                            ScaleResult::Invalid => brush.id().into(),
                            ScaleResult::Valid(payload) =>
                            {
                                payloads.push(payload);
                                None
                            }
                        }
                    })
                });

                if !valid
                {
                    return;
                }

                (new_hull, payloads)
            },
            ScaleResult::Flip(flip_queue, new_hull) =>
            {
                let info = ScaleInfo::new(&hull.flipped(flip_queue.iter().copied()), &new_hull)
                    .unwrap_or(ScaleInfo::identity(hull));

                let mut payloads = hv_vec![capacity; manager.selected_brushes_amount()];

                let valid = manager.test_operation_validity(|manager| {
                    manager.selected_brushes_mut().find_map(|mut brush| {
                        use crate::map::brush::ScaleResult;

                        match brush.check_flip_scale(
                            bundle.drawing_resources,
                            &info,
                            &flip_queue,
                            scale_texture
                        )
                        {
                            ScaleResult::Invalid => brush.id().into(),
                            ScaleResult::Valid(payload) =>
                            {
                                payloads.push(payload);
                                None
                            }
                        }
                    })
                });

                if !valid
                {
                    return;
                }

                (new_hull, payloads)
            }
        };

        *hull = new_hull;

        if backup_polygons.is_empty()
        {
            backup_polygons
                .extend(manager.selected_brushes().map(|brush| (brush.id(), brush.polygon())));
        }

        for payload in payloads
        {
            manager
                .brush_mut(payload.id())
                .set_scale_coordinates(bundle.drawing_resources, payload);
        }
    }

    /// Scales the textures of the selected [`Brush`]es and returns the new outline [`Hull`].
    #[inline]
    fn scale_textures(
        bundle: &mut ToolUpdateBundle,
        manager: &mut EntitiesManager,
        hull: &mut Hull,
        selected_corner: &mut Corner,
        new_corner_position: Vec2,
        backup_scales: &mut HvVec<(Id, (f32, f32))>
    ) -> Option<Hull>
    {
        let ToolUpdateBundle {
            drawing_resources, ..
        } = bundle;

        let result = hull.scaled(selected_corner, new_corner_position);
        let new_hull = match &result
        {
            ScaleResult::None => return None,
            ScaleResult::Scale(new_hull) | ScaleResult::Flip(_, new_hull) => *new_hull
        };
        let info = ScaleInfo::new(hull, &new_hull).unwrap();
        let multi = Vec2::new(info.width_multi(), info.height_multi());

        let valid = manager.test_operation_validity(|manager| {
            manager.selected_textured_brushes_mut().find_map(|mut brush| {
                let texture = brush.texture_settings().unwrap();
                let scale_x = texture.scale_x() * multi.x;
                let scale_y = texture.scale_y() * multi.y;

                (!brush.check_texture_scale_x(drawing_resources, scale_x) ||
                    !brush.check_texture_scale_y(drawing_resources, scale_y))
                .then_some(brush.id())
            })
        });

        if !valid
        {
            return None;
        }

        if backup_scales.is_empty()
        {
            for brush in manager.selected_textured_brushes()
            {
                let texture = brush.texture_settings().unwrap();
                backup_scales.push((brush.id(), (texture.scale_x(), texture.scale_y())));
            }
        }

        *hull = new_hull;

        for mut brush in manager.selected_textured_brushes_mut()
        {
            let texture = brush.texture_settings().unwrap();
            let scale_x = texture.scale_x() * multi.x;
            let scale_y = texture.scale_y() * multi.y;

            _ = brush.set_texture_scale_x(drawing_resources, scale_x);
            _ = brush.set_texture_scale_y(drawing_resources, scale_y);
        }

        if let ScaleResult::Flip(flip_queue, _) = result
        {
            for flip in flip_queue
            {
                match flip
                {
                    Flip::Above(_) | Flip::Below(_) =>
                    {
                        for mut brush in manager.selected_textured_brushes_mut()
                        {
                            brush.flip_scale_y(drawing_resources);
                        }
                    },
                    Flip::Left(_) | Flip::Right(_) =>
                    {
                        for mut brush in manager.selected_textured_brushes_mut()
                        {
                            brush.flip_texture_scale_x(drawing_resources);
                        }
                    }
                };
            }
        }

        new_hull.into()
    }

    /// Checks whever there is an outline vertex near the cursor.
    #[inline]
    fn check_scale_vertex_proximity(
        &mut self,
        cursor_pos: Vec2,
        settings: &ToolsSettings,
        camera_scale: f32
    )
    {
        self.selected_corner =
            return_if_none!(self.outline.nearby_corner(cursor_pos, camera_scale));

        self.status = if settings.entity_editing()
        {
            Status::Drag(hv_vec![], cursor_pos, self.outline)
        }
        else
        {
            Status::DragTextures(hv_vec![], cursor_pos, self.outline)
        };
    }

    /// Returns the outline of the tool, if any.
    #[inline]
    #[must_use]
    fn outline(manager: &EntitiesManager, grid: Grid, settings: &ToolsSettings) -> Option<Hull>
    {
        match settings.target_switch()
        {
            TargetSwitch::Entity => manager.selected_brushes_hull(),
            TargetSwitch::Both =>
            {
                manager
                    .selected_brushes_hull()
                    .unwrap()
                    .merged(&manager.selected_textured_brushes_hull().unwrap())
                    .into()
            },
            TargetSwitch::Texture => manager.selected_textured_brushes_hull()
        }
        .map(|hull| grid.snap_hull(&hull))
    }

    /// Updates the outline of the tool.
    #[inline]
    pub fn update_outline(
        &mut self,
        manager: &EntitiesManager,
        grid: Grid,
        settings: &ToolsSettings
    )
    {
        if !self.ongoing_multi_frame_change()
        {
            self.outline = Self::outline(manager, grid, settings).unwrap();
        }
    }

    /// Finalizes a mouse drag scale.
    #[inline]
    pub fn finalize_drag_scale(
        &mut self,
        manager: &EntitiesManager,
        grid: Grid,
        settings: &ToolsSettings
    )
    {
        self.status = Status::Keyboard;
        self.update_outline(manager, grid, settings);
    }

    //==============================================================
    // Draw

    /// Draws the tool.
    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle, manager: &EntitiesManager)
    {
        draw_selected_and_non_selected_brushes!(bundle, manager);

        let DrawBundle { drawer, .. } = bundle;

        match &self.status
        {
            Status::Keyboard =>
            {
                drawer.hull_with_corner_highlights(
                    &self.outline,
                    self.selected_corner,
                    Color::Hull,
                    Color::ToolCursor
                );
            },
            Status::Drag(_, _, hull) | Status::DragTextures(_, _, hull) =>
            {
                drawer.hull_with_corner_highlights(
                    hull,
                    self.selected_corner,
                    Color::Hull,
                    Color::ToolCursor
                );
            }
        };
    }

    /// Draws the UI of the tool.
    #[inline]
    pub fn ui(&mut self, ui: &mut egui::Ui, settings: &mut ToolsSettings)
    {
        ui.label(egui::RichText::new("SCALE TOOL"));
        settings.ui(ui, !self.ongoing_multi_frame_change());
        ui.label(egui::RichText::new("Corner:"));

        ui.horizontal_wrapped(|ui| {
            if let Status::Drag { .. } = self.status
            {
                ui.add_enabled(false, egui::Button::new(egui::RichText::new("Top left")));
                ui.add_enabled(false, egui::Button::new(egui::RichText::new("Top right")));
                ui.add_enabled(false, egui::Button::new(egui::RichText::new("Bottom left")));
                ui.add_enabled(false, egui::Button::new(egui::RichText::new("Bottom right")));

                return;
            }

            let top_left = ui.button(egui::RichText::new("Top left"));
            let top_right = ui.button(egui::RichText::new("Top right"));
            let bottom_right = ui.button(egui::RichText::new("Bottom right"));
            let bottom_left = ui.button(egui::RichText::new("Bottom left"));

            for b in [&top_left, &top_right, &bottom_left, &bottom_right]
            {
                b.surrender_focus();
            }

            if top_left.clicked()
            {
                self.selected_corner = Corner::TopLeft;
            }
            else if top_right.clicked()
            {
                self.selected_corner = Corner::TopRight;
            }
            else if bottom_left.clicked()
            {
                self.selected_corner = Corner::BottomLeft;
            }
            else if bottom_right.clicked()
            {
                self.selected_corner = Corner::BottomRight;
            }

            match self.selected_corner
            {
                Corner::TopRight => top_right.highlight(),
                Corner::TopLeft => top_left.highlight(),
                Corner::BottomLeft => bottom_left.highlight(),
                Corner::BottomRight => bottom_right.highlight()
            };
        });

        if settings.entity_editing()
        {
            return;
        }

        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Interval:"));
            ui.add(
                egui::Slider::new(
                    &mut settings.texture_scale_interval,
                    Self::MIN_TEXTURE_SCALE_INTERVAL..=Self::MAX_TEXTURES_SCALE_INTERVAL
                )
                .step_by(f64::from(Self::SCALE_INTERVAL_STEP))
                .drag_value_speed(f64::from(Self::SCALE_INTERVAL_STEP))
                .show_value(false)
                .text_color(egui::Color32::WHITE)
            );
            ui.label(egui::RichText::new(format!("{:.2}", settings.texture_scale_interval)));
        });
    }
}
