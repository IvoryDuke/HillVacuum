//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use glam::Vec2;
use hill_vacuum_shared::return_if_none;

use crate::{
    map::{
        drawer::color::Color,
        editor::{cursor::Cursor, state::grid::Grid, DrawBundle}
    },
    utils::math::AroundEqual
};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The distance between a set origin and the cursor position.
#[derive(Clone, Copy)]
pub(in crate::map) struct CursorDelta
{
    /// The point by which the distance to the cursor is measured.
    origin: Vec2,
    /// The distance of the cursor from the origin.
    delta:  Vec2
}

impl CursorDelta
{
    /// The label of the horizontal delta tooltip.
    pub(in crate::map) const X_DELTA: &'static str = "x_delta";
    /// The label of the vertical delta tooltip.
    pub(in crate::map) const Y_DELTA: &'static str = "y_delta";

    /// Returns a new [`Drag`] from `origin`.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state::core) const fn new(origin: Vec2) -> Self
    {
        Self {
            origin,
            delta: Vec2::ZERO
        }
    }

    /// Tries to create a new [`Drag`] from the parameters.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state::core) fn try_new(
        cursor: &Cursor,
        grid: &Grid,
        origin: Vec2
    ) -> Option<Self>
    {
        let drag = Self::new(origin);
        drag.overall_and_frame_drag_delta_from_origin(cursor, grid)
            .is_some()
            .then_some(drag)
    }

    /// Returns the delta.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state::core) const fn delta(&self) -> Vec2 { self.delta }

    /// Updates `self` and executes `dragger` if the delta changed. `dragger` is fed the delta of
    /// the current frame. The new overall delta is stored if `dragger` returns true.
    #[inline]
    pub(in crate::map::editor::state::core) fn conditional_update<F: FnOnce(Vec2) -> bool>(
        &mut self,
        cursor: &Cursor,
        grid: &Grid,
        dragger: F
    )
    {
        let (overall_delta, frame_delta) =
            return_if_none!(self.overall_and_frame_drag_delta_from_origin(cursor, grid));

        if dragger(frame_delta)
        {
            self.delta = overall_delta;
        }
    }

    /// Updates `self` and executes `dragger` if the delta changed. `dragger` is fed the delta of
    /// the current frame.
    #[inline]
    pub(in crate::map::editor::state::core) fn update<F: FnOnce(Vec2)>(
        &mut self,
        cursor: &Cursor,
        grid: &Grid,
        dragger: F
    )
    {
        let (overall_delta, frame_delta) =
            return_if_none!(self.overall_and_frame_drag_delta_from_origin(cursor, grid));

        dragger(frame_delta);
        self.delta = overall_delta;
    }

    /// Returns the overall delta and the delta of the current frame.
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    fn overall_and_frame_drag_delta_from_origin(
        &self,
        cursor: &Cursor,
        grid: &Grid
    ) -> Option<(Vec2, Vec2)>
    {
        let snap = cursor.snap();
        let cursor_pos = cursor.world_snapped();
        let prev_step = self.origin + self.delta;

        let delta = if snap
        {
            let delta = cursor_pos - prev_step;
            let target = prev_step + grid.square(delta).nearest_corner_to_point(delta);

            if prev_step.around_equal(&target)
            {
                return None;
            }

            (target - self.origin).round()
        }
        else
        {
            if prev_step.around_equal(&cursor_pos)
            {
                return None;
            }

            cursor_pos - self.origin
        };

        Some((delta, delta - self.delta))
    }

    /// Draws two lines, a horizontal one and a vertical one, that represent the distance of the
    /// cursor from the origin.
    #[inline]
    pub(in crate::map::editor::state::core) fn draw(&self, bundle: &mut DrawBundle)
    {
        /// The color of the tooltip.
        const TOOLTIP_TEXT_COLOR: egui::Color32 = egui::Color32::from_rgb(127, 255, 212);

        if self.delta.around_equal_narrow(&Vec2::ZERO)
        {
            return;
        }

        let DrawBundle {
            window,
            drawer,
            camera,
            ..
        } = bundle;

        let p = self.origin + self.delta;
        drawer.line(self.origin, Vec2::new(p.x, self.origin.y), Color::Hull);
        drawer.line(Vec2::new(p.x, self.origin.y), p, Color::Hull);

        if self.delta.x != 0f32
        {
            drawer.draw_tooltip_x_centered_above_pos(
                window,
                camera,
                Self::X_DELTA,
                #[allow(clippy::cast_possible_truncation)]
                &format!("{}", self.delta.x as i16),
                Vec2::new(self.origin.x + self.delta.x / 2f32, self.origin.y),
                Vec2::new(0f32, -4f32),
                TOOLTIP_TEXT_COLOR,
                egui::Color32::TRANSPARENT
            );
        }

        if self.delta.y == 0f32
        {
            return;
        }

        drawer.draw_tooltip_y_centered(
            window,
            camera,
            Self::Y_DELTA,
            #[allow(clippy::cast_possible_truncation)]
            format!("{}", self.delta.y as i8).as_str(),
            Vec2::new(p.x, p.y - self.delta.y / 2f32),
            Vec2::new(4f32, 0f32),
            TOOLTIP_TEXT_COLOR,
            egui::Color32::from_black_alpha(0)
        );
    }
}
