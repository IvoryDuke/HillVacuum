//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::{Transform, Vec2, Window};

use super::manager::EntitiesManager;
use crate::{
    map::drawer::{color::Color, EditDrawer},
    utils::{hull::Hull, misc::Camera}
};

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

const SKEW_PIVOT: f32 = 64f32;

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The grid of the map.
#[must_use]
#[derive(Clone, Copy, Debug)]
pub(in crate::map) struct Grid
{
    /// The size of the grid's squares.
    size:        i16,
    skew:        i8,
    /// Whever the grid should be drawn on screen..
    pub visible: bool,
    /// When true, the position of the grid squares is shifted by half of its size, both
    /// horizontally and vertically.
    pub shifted: bool
}

impl Default for Grid
{
    #[inline]
    fn default() -> Self
    {
        Self {
            size:    64,
            skew:    64,
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
    pub(in crate::map::editor::state) const fn new(
        size: i16,
        skew: i8,
        visible: bool,
        shifted: bool
    ) -> Self
    {
        Self {
            size,
            skew,
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
        let mut y = floor_multiple(pos.y, self.size);

        if pos.y.is_sign_positive()
        {
            y += self.size;
        }

        top = f32::from(y);
        bottom = top - size_f;

        // X coordinates.
        let mut x = floor_multiple(pos.x, self.size);

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

        let mut result = f32::from(floor_multiple(value, self.size));

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
    pub(in crate::map) fn lines(self, window: &Window, camera: &Transform) -> GridLines
    {
        #[inline(always)]
        #[must_use]
        fn skewed_y(y: f32, skew: f32) -> f32 { (y / SKEW_PIVOT) * skew }

        let viewport = camera.viewport_ui_constricted(window);
        let (top, bottom, left, right) = viewport.decompose();
        let (x_range, y_range) = viewport.range();

        let axis = if self.skew == 0
        {
            Axis {
                x: y_range
                    .contains(&0f32)
                    .then(|| (Vec2::new(left, 0f32), Vec2::new(right, 0f32))),
                y: x_range
                    .contains(&0f32)
                    .then(|| (Vec2::new(0f32, top), Vec2::new(0f32, bottom)))
            }
        }
        else
        {
            let skew_f32 = f32::from(self.skew);
            let y_left = skewed_y(bottom, skew_f32);
            let y_right = skewed_y(top, skew_f32);
            let draw_y = x_range.contains(&y_left) || x_range.contains(&y_right);

            Axis {
                x: y_range
                    .contains(&0f32)
                    .then(|| (Vec2::new(left, 0f32), Vec2::new(right, 0f32))),
                y: draw_y.then(|| (Vec2::new(y_right, top), Vec2::new(y_left, bottom)))
            }
        };

        GridLines {
            axis,
            horizontal_lines: HorizontalLines::new(&viewport, self),
            vertical_lines: VerticalLines::new(&viewport, self)
        }
    }
}

//=======================================================================//

pub(in crate::map) struct GridLines
{
    pub axis:             Axis,
    pub horizontal_lines: HorizontalLines,
    pub vertical_lines:   VerticalLines
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

pub(in crate::map) struct HorizontalLines
{
    /// The y coordinate of the next horizontal line.
    y:              f32,
    /// The x coordinate of the left point of the horizontal lines.
    left:           f32,
    /// The x coordinate of the right point of the horizontal lines.
    right:          f32,
    /// The length of the side of the squares of the grid.
    grid_size:      f32,
    /// Half of the length of the sides of the squares of the grid.
    half_grid_size: f32,
    len:            usize,
    /// The function returning the color the next line should be drawn.
    color:          fn(f32, f32) -> Color
}

impl Iterator for HorizontalLines
{
    type Item = (Vec2, Vec2, Color);

    #[inline]
    #[must_use]
    fn next(&mut self) -> Option<Self::Item>
    {
        if self.len == 0
        {
            return None;
        }

        let line_y = self.y;
        self.y += self.grid_size;
        self.len -= 1;

        Some((
            Vec2::new(self.left, line_y),
            Vec2::new(self.right, line_y),
            (self.color)(self.half_grid_size, line_y)
        ))
    }
}

impl HorizontalLines
{
    /// Returns a new [`HorizontalLines`] based on the parameters.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    #[inline]
    fn new(viewport: &Hull, grid: Grid) -> Self
    {
        let bottom = floor_multiple(viewport.bottom(), grid.size);
        let mut y = f32::from(bottom);
        let grid_size_f32 = grid.size_f32();
        let half_grid_size = grid_size_f32 / 2f32;

        if grid.shifted
        {
            y -= half_grid_size;
        }

        Self {
            y,
            left: viewport.left(),
            right: viewport.right(),
            grid_size: grid_size_f32,
            half_grid_size,
            len: usize::try_from((floor_multiple(viewport.top(), grid.size) - bottom) / grid.size)
                .unwrap() +
                1,
            color: color_function(grid)
        }
    }
}

//=======================================================================//

pub(in crate::map) struct VerticalLines
{
    x:              f32,
    bottom:         f32,
    top:            f32,
    width:          f32,
    grid_size:      f32,
    half_grid_size: f32,
    len:            usize,
    color:          fn(f32, f32) -> Color,
    color_offset:   f32
}

impl Iterator for VerticalLines
{
    type Item = (Vec2, Vec2, Color);

    #[inline]
    #[must_use]
    fn next(&mut self) -> Option<Self::Item>
    {
        if self.len == 0
        {
            return None;
        }

        let line_x = self.x;
        self.x += self.grid_size;
        self.len -= 1;

        Some((
            Vec2::new(line_x, self.bottom),
            Vec2::new(line_x + self.width, self.top),
            (self.color)(self.half_grid_size, line_x + self.color_offset)
        ))
    }
}

impl VerticalLines
{
    /// Returns a new [`GridLines`] based on the parameters.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    #[inline]
    fn new(viewport: &Hull, grid: Grid) -> Self
    {
        let grid_size_f32 = grid.size_f32();
        let half_grid_size = grid_size_f32 / 2f32;
        let (top, mut bottom, left, right) = viewport.decompose();
        bottom -= grid_size_f32;

        let width = right - left;
        let mut height = top - bottom;
        let mut len = ((width + height) / grid_size_f32) as i16;
        let mut bottom = f32::from(floor_multiple(bottom, grid.size));

        let x = if grid.skew <= 0
        {
            f32::from(floor_multiple(left, grid.size))
        }
        else
        {
            f32::from((right as i16 / grid.size - len) * grid.size)
        };

        if grid.shifted
        {
            len += 1;
            bottom -= half_grid_size;
            height += half_grid_size;
        }

        let grid_skew_f32 = f32::from(grid.skew);

        Self {
            x,
            bottom,
            top: bottom + height,
            grid_size: grid_size_f32,
            half_grid_size,
            width: height * grid_skew_f32 / SKEW_PIVOT,
            len: usize::try_from(len).unwrap(),
            color: color_function(grid),
            color_offset: bottom * grid_skew_f32 / SKEW_PIVOT
        }
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

#[allow(clippy::cast_possible_truncation)]
#[inline(always)]
#[must_use]
const fn floor_multiple(value: f32, grid_size: i16) -> i16
{
    (value as i16 / grid_size) * grid_size
}

//=======================================================================//

#[inline]
#[must_use]
fn color_function(grid: Grid) -> fn(f32, f32) -> Color
{
    if grid.size >= 64
    {
        grid_64_line_color
    }
    else if grid.shifted
    {
        grid_less_than_64_shifted_line_color
    }
    else
    {
        grid_less_than_64_line_color
    }
}

//=======================================================================//

/// Returns the color the next line of a grid with size 64 or higher should be drawn.
#[inline]
#[must_use]
const fn grid_64_line_color(_: f32, _: f32) -> Color { Color::GridLines }

//=======================================================================//

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

//=======================================================================//

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
