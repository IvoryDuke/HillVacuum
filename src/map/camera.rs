//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::{transform::components::Transform, window::Window};
use glam::{Vec2, Vec3};

use super::editor::state::{
    grid::Grid,
    ui::{ui_camera_displacement, ui_left_space, ui_right_space, ui_size, ui_top_space}
};
use crate::utils::{hull::Hull, misc::Camera};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

impl Camera for Transform
{
    #[inline]
    fn pos(&self) -> Vec2 { self.translation.truncate() }

    #[inline]
    fn scale(&self) -> f32 { self.scale.x }

    #[inline]
    fn viewport(&self, window: &Window, grid: &Grid) -> Hull
    {
        const VISIBILITY_PADDING: f32 = 64f32;

        let (half_width, half_height) = self.scaled_window_half_sizes(window);
        let viewport = Hull::new(
            self.translation.y + half_height,
            self.translation.y - half_height,
            self.translation.x - half_width,
            self.translation.x + half_width
        )
        .unwrap()
        .transformed(|vx| grid.point_projection(vx));

        Hull::new(
            viewport.top() - ui_top_space() * self.scale(),
            viewport.bottom(),
            viewport.left() + ui_left_space() * self.scale(),
            viewport.right() - ui_right_space() * self.scale()
        )
        .unwrap()
        .bumped(VISIBILITY_PADDING * self.scale())
    }

    #[inline]
    fn set_pos(&mut self, pos: Vec2) { self.translation = pos.extend(0f32); }

    #[inline]
    fn translate(&mut self, delta: Vec2) { self.translation += delta.extend(0f32); }

    #[inline]
    fn change_scale(&mut self, units: f32) -> f32
    {
        let prev_scale = self.scale();
        self.scale = Vec3::splat((self.scale() - units * 0.125).clamp(0.125, 20f32));
        prev_scale
    }

    #[inline]
    fn zoom(&mut self, units: f32)
    {
        let prev_scale = self.change_scale(units);
        self.translate(-ui_camera_displacement(self.scale() - prev_scale));
    }

    #[inline]
    fn scale_viewport_to_hull(&mut self, window: &Window, grid: &Grid, hull: &Hull, padding: f32)
    {
        let hull = &hull.transformed(|vx| grid.transform_point(vx));

        let ui_size = ui_size();
        scale_viewport(
            self,
            (window.width() - ui_size.x, window.height() - ui_size.y),
            hull,
            padding
        );

        self.set_pos(hull.center() - ui_camera_displacement(self.scale()));
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
    let (size, win_size) =
        if width < height { (height, window_sizes.1) } else { (width, window_sizes.0) };

    camera.scale = Vec3::splat((size + double_padding / camera.scale()) / win_size);
}

//=======================================================================//

/// Returns the position the engine camera should assume to be in the center of the portion of the
/// screen in which the map is visible.
#[inline]
#[must_use]
pub(in crate::map) fn init_camera_transform() -> Transform
{
    Transform::from_translation((-ui_camera_displacement(1f32)).extend(0f32))
}
