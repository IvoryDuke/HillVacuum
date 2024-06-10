//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::{Transform, Vec2, Window};

use super::manager::EntitiesManager;
use crate::{
    map::drawer::{color::Color, EditDrawer},
    utils::{hull::Hull, math::AroundEqual, misc::Camera}
};

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The grid of the map.
#[derive(Clone, Copy, Debug)]
pub(in crate::map) struct Grid
{
    /// The size of the grid's squares.
    size:        i16,
    /// Whever the grid should be drawn on screen..
    pub visible: bool,
    /// When true, the position of the grid squares is shifted by half of its size, both
    /// horizontally and vertically.
    pub shifted: bool
}

impl Default for Grid
{
    #[inline]
    #[must_use]
    fn default() -> Self
    {
        Self {
            size:    64,
            visible: true,
            shifted: false
        }
    }
}

impl Grid
{
    //==============================================================
    // New

    /// Returns a new [`Grid`].
    #[inline]
    pub(in crate::map::editor::state) const fn new(size: i16, visible: bool, shifted: bool)
        -> Self
    {
        Self {
            size,
            visible,
            shifted
        }
    }

    //==============================================================
    // Info

    /// Returns the length of the sides of the squares.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) const fn size(self) -> i16 { self.size }

    /// Returns the length of the sides of the squares as an `f32`.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn size_f32(self) -> f32 { f32::from(self.size) }

    //==============================================================
    // Square

    /// Returns the square that contains `pos`.
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    #[must_use]
    pub fn square(self, pos: Vec2) -> Hull
    {
        let size_f = self.size_f32();
        let (mut top, mut bottom, mut left, mut right);

        // Y coordinates.
        let mut y = (pos.y as i16 / self.size) * self.size;

        if pos.y.is_sign_positive()
        {
            y += self.size;
        }

        top = f32::from(y);
        bottom = top - size_f;

        // X coordinates.
        let mut x = (pos.x as i16 / self.size) * self.size;

        if pos.x.is_sign_negative()
        {
            x -= self.size;
        }

        left = f32::from(x);
        right = left + size_f;

        // Shift.
        if self.shifted
        {
            let half_grid_size = self.size_f32() / 2f32;

            if pos.y > (bottom + top) / 2f32
            {
                top += half_grid_size;
                bottom += half_grid_size;
            }
            else
            {
                top -= half_grid_size;
                bottom -= half_grid_size;
            }

            if pos.x > (left + right) / 2f32
            {
                left += half_grid_size;
                right += half_grid_size;
            }
            else
            {
                left -= half_grid_size;
                right -= half_grid_size;
            }
        }

        Hull::new(top, bottom, left, right)
    }

    //==============================================================
    // Snap

    /// Increases the grid size to the next power of two.
    /// Capped at 256 units.
    #[inline]
    pub(in crate::map::editor::state) fn increase_size(&mut self, manager: &mut EntitiesManager)
    {
        if self.visible
        {
            self.size = (self.size * 2).min(256);
            manager.schedule_outline_update();
        }
    }

    /// Increases the grid size to the previous power of two.
    /// 2 units is the minimum length.
    #[inline]
    pub(in crate::map::editor::state) fn decrease_size(&mut self, manager: &mut EntitiesManager)
    {
        if self.visible
        {
            self.size = (self.size / 2).max(2);
            manager.schedule_outline_update();
        }
    }

    /// Toggles whever the grid is shifted or not.
    #[inline]
    pub(in crate::map::editor::state) fn toggle_shift(&mut self, manager: &mut EntitiesManager)
    {
        if self.visible
        {
            self.shifted = !self.shifted;
            manager.schedule_outline_update();
        }
    }

    //==============================================================
    // Snap

    /// Snaps `point` to the closest grid vertex.
    #[inline]
    #[must_use]
    pub fn snap_point(self, point: Vec2) -> Option<Vec2>
    {
        let center = self.square(point).center();
        let snapped = Vec2::new(
            self.snap_value_from_center(point.x, center.x),
            self.snap_value_from_center(point.y, center.y)
        );

        (snapped != point).then_some(snapped)
    }

    /// Snaps `value` to the grid, in a way that moves it further away from `center`.
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    #[must_use]
    fn snap_value_from_center(self, value: f32, center: f32) -> f32
    {
        let rounded = if value < center { value.floor() } else { value.ceil() };
        let rounded_i = rounded as i16;

        if self.shifted
        {
            // Round away from the center.
            let mut result;

            let half_grid_size = f32::from(self.size) / 2f32;
            let div = rounded_i + half_grid_size as i16;

            if div % self.size == 0
            {
                return rounded;
            }

            result = f32::from(div / self.size * self.size);

            if value < 0f32
            {
                result -= f32::from(self.size);
            }

            if value < center
            {
                result -= half_grid_size;
            }
            else
            {
                result += half_grid_size;
            }

            return result;
        }

        // Round away from the center.
        if rounded_i % self.size == 0
        {
            return rounded;
        }

        let mut result = f32::from(value as i16 / self.size * self.size);

        if value < center
        {
            if value < 0f32
            {
                result -= f32::from(self.size);
            }
        }
        else if value > 0f32
        {
            result += f32::from(self.size);
        }

        result
    }

    /// Snaps `point` to the grid in a way that moves it further away from `center`.
    #[inline]
    #[must_use]
    pub fn snap_point_from_center(self, point: Vec2, center: Vec2) -> Option<Vec2>
    {
        let snapped = Vec2::new(
            self.snap_value_from_center(point.x, center.x),
            self.snap_value_from_center(point.y, center.y)
        );

        (snapped != point).then_some(snapped)
    }

    /// Snaps `hull` to the grid.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn snap_hull(self, hull: &Hull) -> Hull
    {
        // Transform the hull to match the grid for better pivot setting.
        let center = hull.center();
        let (mut top, mut bottom, mut left, mut right) =
            (hull.top(), hull.bottom(), hull.left(), hull.right());

        for (value, center) in [
            (&mut top, center.y),
            (&mut bottom, center.y),
            (&mut left, center.x),
            (&mut right, center.x)
        ]
        {
            *value = self.snap_value_from_center(*value, center);
        }

        Hull::new(top, bottom, left, right)
    }

    //==============================================================
    // Draw

    /// Draws the grid.
    pub(in crate::map::editor::state) fn draw(
        self,
        window: &Window,
        drawer: &mut EditDrawer,
        camera: &Transform
    )
    {
        if !self.visible
        {
            return;
        }

        drawer.grid(self, window, camera);
    }

    /// Returns an iterator that returns the grid lines, and the lines representing the x and y axis
    /// if they are visible.
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    pub(in crate::map) fn lines(
        self,
        window: &Window,
        camera: &Transform
    ) -> (impl ExactSizeIterator<Item = (Vec2, Vec2, Color)>, Axis)
    {
        let (top, bottom, left, right) = camera.viewport_ui_constricted(window).decompose();

        (GridLines::new(top, bottom, left, right, self), Axis {
            x: (bottom..top)
                .contains(&0f32)
                .then(|| (Vec2::new(left, 0f32), Vec2::new(right, 0f32))),
            y: (left..right)
                .contains(&0f32)
                .then(|| (Vec2::new(0f32, top), Vec2::new(0f32, bottom)))
        })
    }
}

//=======================================================================//

/// The lines representing the x and y axis, if visible.
pub(in crate::map) struct Axis
{
    /// The x axis.
    pub x: Option<(Vec2, Vec2)>,
    /// The y axis.
    pub y: Option<(Vec2, Vec2)>
}

//=======================================================================//

/// An iterator that returns the visible grid lines to be drawn.
struct GridLines
{
    /// The y coordinate of the next horizontal line.
    y_left:         f32,
    /// The y coordinate of the last horizontal line.
    y_right:        f32,
    /// The x coordinate of the next vertical line.
    x_left:         f32,
    /// The x cordinate of the last vertical line.
    x_right:        f32,
    /// The length of the side of the squares of the grid.
    grid_size:      f32,
    /// Half of the length of the sides of the squares of the grid.
    half_grid_size: f32,
    /// The y coordinate of the highest point of the vertical lines.
    top:            f32,
    /// The y coordinate of the lowest point of the vertical lines.
    bottom:         f32,
    /// The x coordinate of the left point of the horizontal lines.
    left:           f32,
    /// The x coordinate of the right point of the horizontal lines.
    right:          f32,
    /// The function returning the color the next line should be drawn.
    color:          fn(f32, f32) -> Color
}

impl ExactSizeIterator for GridLines
{
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    #[inline]
    #[must_use]
    fn len(&self) -> usize { (self.y_right - self.y_left + self.x_right - self.x_left) as usize }
}

impl Iterator for GridLines
{
    type Item = (Vec2, Vec2, Color);

    #[inline]
    #[must_use]
    fn next(&mut self) -> Option<Self::Item>
    {
        if !self.x_left.around_equal(&self.x_right)
        {
            let line_x = self.x_left;
            self.x_left += self.grid_size;
            Some((
                Vec2::new(line_x, self.bottom),
                Vec2::new(line_x, self.top),
                (self.color)(self.half_grid_size, line_x)
            ))
        }
        else if !self.y_left.around_equal(&self.y_right)
        {
            let line_y = self.y_left;
            self.y_left += self.grid_size;
            Some((
                Vec2::new(self.left, line_y),
                Vec2::new(self.right, line_y),
                (self.color)(self.half_grid_size, line_y)
            ))
        }
        else
        {
            None
        }
    }
}

impl GridLines
{
    /// Returns a new [`GridLines`] based on the parameters.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    #[inline]
    fn new(top: f32, bottom: f32, left: f32, right: f32, grid: Grid) -> Self
    {
        /// Returns the result of the division of `value`/`rhs` rounded to the higher integer.
        #[inline]
        #[must_use]
        const fn div_ceil(value: i16, rhs: i16) -> i16
        {
            let d = value / rhs;
            let r = value % rhs;

            if (r > 0 && rhs > 0) || (r < 0 && rhs < 0)
            {
                d + 1
            }
            else
            {
                d
            }
        }

        assert!(top > bottom, "Top {top} is equal or lower than bottom {bottom}.");
        assert!(left < right, "Left {left} is equal or higher than right {right}.");

        let grid_size = grid.size_f32();
        let grid_shifted = grid.shifted;

        let i_grid_size = grid_size as i16;
        let y_right = div_ceil(top as i16, i_grid_size);
        let y_left = div_ceil(bottom as i16, i_grid_size);
        let x_left = div_ceil(left as i16, i_grid_size);
        let x_right = div_ceil(right as i16, i_grid_size);

        let mut y_right = f32::from(y_right) * grid_size;
        let mut y_left = f32::from(y_left) * grid_size;
        let mut x_left = f32::from(x_left) * grid_size;
        let mut x_right = f32::from(x_right) * grid_size;

        let half_grid_size = grid_size / 2f32;

        if grid.shifted
        {
            y_right += half_grid_size;
            y_left -= half_grid_size;
            x_left -= half_grid_size;
            x_right += half_grid_size;
        }

        Self {
            y_right,
            y_left,
            x_left,
            x_right,
            grid_size,
            half_grid_size,
            top,
            bottom,
            left,
            right,
            color: if grid_size >= 64f32
            {
                Self::grid_64_line_color
            }
            else if grid_shifted
            {
                Self::grid_less_than_64_shifted_line_color
            }
            else
            {
                Self::grid_less_than_64_line_color
            }
        }
    }

    /// Returns the color the next line of a grid with size 64 or higher should be drawn.
    #[inline]
    #[must_use]
    const fn grid_64_line_color(_: f32, _: f32) -> Color { Color::GridLines }

    /// Returns the color the next line of a grid with size less than 64 should be drawn.
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    #[must_use]
    const fn grid_less_than_64_line_color(_: f32, line: f32) -> Color
    {
        if line as i16 % 64 == 0
        {
            Color::GridLines
        }
        else
        {
            Color::SoftGridLines
        }
    }

    /// Returns the color the next line of a shifted grid with size less than 64 should be drawn.
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    #[must_use]
    fn grid_less_than_64_shifted_line_color(half_grid_size: f32, line: f32) -> Color
    {
        if (line - half_grid_size) as i16 % 64 == 0
        {
            Color::GridLines
        }
        else
        {
            Color::SoftGridLines
        }
    }
}
