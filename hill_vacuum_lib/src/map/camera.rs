//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::cmp::Ordering;

use bevy::prelude::{Transform, Vec2, Vec3, Window};

use super::editor::state::ui::{
    ui_camera_displacement,
    ui_left_space,
    ui_right_space,
    ui_size,
    ui_top_space
};
use crate::utils::{hull::Hull, misc::Camera};

//=======================================================================//
// TYPES
//
//=======================================================================//

impl Camera for Transform
{
    #[inline]
    fn pos(&self) -> Vec2 { self.translation.truncate() }

    #[inline]
    fn scale(&self) -> f32 { self.scale.x }

    #[inline]
    fn viewport_ui_constricted(&self, window: &Window) -> Hull
    {
        let viewport = self.viewport(window);

        Hull::new(
            viewport.top() - ui_top_space() * self.scale(),
            viewport.bottom(),
            viewport.left() + ui_left_space() * self.scale(),
            viewport.right() - ui_right_space() * self.scale()
        )
    }

    #[inline]
    fn viewport(&self, window: &Window) -> Hull
    {
        let (half_width, half_height) = self.scaled_window_half_sizes(window);

        Hull::new(
            self.translation.y + half_height,
            self.translation.y - half_height,
            self.translation.x - half_width,
            self.translation.x + half_width
        )
    }

    #[inline]
    fn set_pos(&mut self, pos: Vec2) { self.translation = pos.extend(0f32); }

    #[inline]
    fn translate(&mut self, delta: Vec2) { self.translation += delta.extend(0f32); }

    #[inline]
    fn change_scale(&mut self, units: f32) -> f32
    {
        let prev_scale = self.scale();
        self.scale = Vec3::splat((self.scale() - units * 0.125).clamp(0.125, 5f32));
        prev_scale
    }

    #[inline]
    fn zoom(&mut self, units: f32)
    {
        let prev_scale = self.change_scale(units);
        self.translate(-ui_camera_displacement() * (self.scale() - prev_scale));
    }

    #[inline]
    fn scale_viewport_ui_constricted_to_hull(&mut self, window: &Window, hull: &Hull, padding: f32)
    {
        let ui_size = ui_size();
        scale_viewport(
            self,
            (window.width() - ui_size.x, window.height() - ui_size.y),
            hull,
            padding
        );

        self.set_pos(hull.center() - ui_camera_displacement() * self.scale());
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Scales the viewport to fit `hull`.
#[inline]
pub fn scale_viewport(camera: &mut Transform, window_sizes: (f32, f32), hull: &Hull, padding: f32)
{
    let double_padding = padding * 2f32;
    let (width, height) = (hull.width(), hull.height());
    let size_extension = |size: f32, win_size: f32, win_mul: f32| {
        double_padding + f32::max(0f32, size + double_padding - win_size * camera.scale()) * win_mul
    };

    match (width / height).total_cmp(&(window_sizes.0 / window_sizes.1))
    {
        Ordering::Less =>
        {
            let height =
                height + size_extension(width, window_sizes.0, window_sizes.1 / window_sizes.0);
            camera.scale = Vec3::splat(height / window_sizes.1);
        },
        Ordering::Equal =>
        {
            camera.scale = Vec3::splat((height + double_padding) / window_sizes.1);
        },
        Ordering::Greater =>
        {
            let width =
                width + size_extension(height, window_sizes.1, window_sizes.0 / window_sizes.1);
            camera.scale = Vec3::splat(width / window_sizes.0);
        }
    };
}

//=======================================================================//

/// Returns the position the engine camera should assume to be in the center of the portion of the
/// screen in which the map is visible.
#[inline]
#[must_use]
pub(in crate::map) fn init_camera_transform() -> Transform
{
    Transform::from_translation((-ui_camera_displacement()).extend(0f32))
}
