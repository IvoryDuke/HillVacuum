//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::ops::RangeInclusive;

use bevy::{transform::components::Transform, window::Window};
use glam::Vec2;

use super::manager::EntitiesManager;
use crate::{
    map::{drawer::color::Color, BoundToMap, GridSettings},
    utils::{
        hull::Hull,
        math::{angles::FastSinCosTan, points::fast_rotate_point_around_origin},
        misc::Camera
    }
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[derive(Clone, Copy)]
enum Change
{
    False,
    ChangedRequiresUpdate,
    Changed
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The grid of the map.
#[must_use]
#[derive(Clone, Copy)]
pub(crate) struct Grid
{
    /// The size of the grid's squares.
    size:        i16,
    settings:    GridSettings,
    /// Whether the grid should be drawn on screen.
    pub visible: bool,
    /// When true, the position of the grid squares is shifted by half of its size, both
    /// horizontally and vertically.
    pub shifted: bool,
    change:      Change
}

impl Default for Grid
{
    #[inline]
    fn default() -> Self
    {
        Self {
            size:     64,
            settings: GridSettings::default(),
            visible:  true,
            shifted:  false,
            change:   Change::False
        }
    }
}

impl Grid
{
    /// The range of the possible angle values.
    pub const ANGLE_RANGE: RangeInclusive<i16> = RangeInclusive::new(-180, 180);
    /// The range of the possible skew values.
    pub const SKEW_RANGE: RangeInclusive<i8> = RangeInclusive::new(-45, 45);

    //==============================================================
    // New

    /// Returns a new [`Grid`].
    #[inline]
    pub(in crate::map::editor::state) const fn new(settings: GridSettings) -> Self
    {
        Self {
            size: 64,
            settings,
            visible: true,
            shifted: false,
            change: Change::False
        }
    }

    /// Returns a [`Grid`] used for a quick snap.
    #[inline]
    pub(in crate::map::editor::state) fn quick_snap(shifted: bool) -> Self
    {
        Self {
            size: 2,
            settings: GridSettings::default(),
            visible: true,
            shifted,
            change: Change::False
        }
    }

    #[inline]
    pub(in crate::map::editor) fn absolute(&self) -> Self
    {
        let mut grid = *self;
        grid.set_angle(0);
        grid.set_skew(0);
        grid
    }

    #[inline]
    pub(in crate::map::editor::state) const fn with_size(&self, size: i16) -> Self
    {
        let mut grid = *self;
        grid.size = size;
        grid
    }

    //==============================================================
    // Info

    /// Returns the length of the sides of the squares.
    #[inline]
    #[must_use]
    pub(in crate::map::editor) const fn size(&self) -> i16 { self.size }

    /// Returns the length of the sides of the squares as an `f32`.
    #[inline]
    #[must_use]
    pub(in crate::map::editor) fn size_f32(&self) -> f32 { f32::from(self.size) }

    #[inline]
    #[must_use]
    pub const fn skew(&self) -> i8 { self.settings.skew() }

    #[inline]
    #[must_use]
    pub const fn angle(&self) -> i16 { self.settings.angle() }

    #[inline]
    pub(in crate::map) const fn settings(&self) -> GridSettings { self.settings }

    #[inline]
    #[must_use]
    pub const fn isometric(&self) -> bool
    {
        matches!(self.settings, GridSettings::Isometric { .. })
    }

    #[inline]
    #[must_use]
    pub const fn changed(&self) -> bool
    {
        matches!(self.change, Change::Changed | Change::ChangedRequiresUpdate)
    }

    #[inline]
    #[must_use]
    pub const fn changed_requires_update(&self) -> bool
    {
        matches!(self.change, Change::ChangedRequiresUpdate)
    }

    /// Returns the square that contains `pos`.
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    #[must_use]
    pub fn square(&self, pos: Vec2) -> Hull
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

        Hull::new(top, bottom, left, right).unwrap()
    }

    //==============================================================
    // Size

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

    /// Decreases the grid size to the previous power of two.
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

    #[inline]
    pub(in crate::map::editor::state) fn set_skew(&mut self, value: i8)
    {
        let prev = self.skew();
        self.settings.set_skew(value);

        if prev != value
        {
            self.change = Change::ChangedRequiresUpdate;
        }
    }

    #[inline]
    pub(in crate::map::editor::state) fn set_angle(&mut self, value: i16)
    {
        let prev = self.angle();
        self.settings.set_angle(value);

        if prev != value
        {
            self.change = Change::ChangedRequiresUpdate;
        }
    }

    /// Toggles whether the grid is shifted or not.
    #[inline]
    pub(in crate::map::editor::state) fn toggle_shift(&mut self, manager: &mut EntitiesManager)
    {
        if self.visible
        {
            self.shifted = !self.shifted;
            manager.schedule_outline_update();
        }
    }

    #[inline]
    pub(in crate::map::editor) fn set_updated(&mut self) { self.change = Change::Changed; }

    #[inline]
    pub(in crate::map::editor) fn reset_changed(&mut self) { self.change = Change::False; }

    //==============================================================
    // Transform

    #[inline]
    #[must_use]
    pub fn transform_point(&self, mut point: Vec2) -> Vec2
    {
        #[inline]
        fn skew(point: &mut Vec2, skew: i8) { point.x += skew.fast_tan() * point.y; }

        #[inline]
        fn rotate(point: &mut Vec2, angle: i16)
        {
            *point = fast_rotate_point_around_origin(*point, angle);
        }

        match self.settings
        {
            GridSettings::None => (),
            GridSettings::Skew(s) => skew(&mut point, s),
            GridSettings::Rotate(a) => rotate(&mut point, a),
            GridSettings::Isometric { skew: s, angle: a } =>
            {
                skew(&mut point, s);
                rotate(&mut point, a);
            }
        };

        point
    }

    #[inline]
    #[must_use]
    pub fn point_projection(&self, mut point: Vec2) -> Vec2
    {
        #[inline]
        fn skew(point: &mut Vec2, skew: i8) { point.x -= skew.fast_tan() * point.y; }

        #[inline]
        fn rotate(point: &mut Vec2, angle: i16)
        {
            *point = fast_rotate_point_around_origin(*point, -angle);
        }

        match self.settings
        {
            GridSettings::None => (),
            GridSettings::Skew(s) => skew(&mut point, s),
            GridSettings::Rotate(a) => rotate(&mut point, a),
            GridSettings::Isometric { skew: s, angle: a } =>
            {
                rotate(&mut point, a);
                skew(&mut point, s);
            }
        };

        point
    }

    /// Snaps `point` to the closest grid vertex.
    #[inline]
    #[must_use]
    pub fn snap_point(&self, point: Vec2) -> Option<Vec2>
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
    fn snap_value_from_center(&self, value: f32, center: f32) -> f32
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
    pub fn snap_point_from_center(&self, point: Vec2, center: Vec2) -> Option<Vec2>
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
    pub(in crate::map::editor::state) fn snap_hull(&self, hull: &Hull) -> Hull
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

        Hull::new(top, bottom, left, right).unwrap()
    }

    //==============================================================
    // Draw

    /// Returns an iterator that returns the grid lines, and the lines representing the x and y axis
    /// if they are visible.
    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    pub(in crate::map) fn lines(&self, window: &Window, camera: &Transform) -> GridLines
    {
        let viewport = camera.viewport(window, self);
        let (top, bottom, left, right) = viewport.decompose();
        let top = top.bound();
        let bottom = bottom.bound();
        let left = left.bound();
        let right = right.bound();
        let (x_range, y_range) = viewport.range();

        GridLines {
            axis:           Axis {
                x: y_range
                    .contains(&0f32)
                    .then(|| (Vec2::new(left, 0f32), Vec2::new(right, 0f32))),
                y: x_range
                    .contains(&0f32)
                    .then(|| (Vec2::new(0f32, top), Vec2::new(0f32, bottom)))
            },
            parallel_lines: ParallelLines::new(self, top, bottom, left, right)
        }
    }
}

//=======================================================================//

pub(in crate::map) struct GridLines
{
    pub axis:           Axis,
    pub parallel_lines: ParallelLines
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
pub(in crate::map) struct ParallelLines
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

impl ExactSizeIterator for ParallelLines
{
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    #[inline]
    #[must_use]
    fn len(&self) -> usize { (self.y_right - self.y_left + self.x_right - self.x_left) as usize }
}

impl Iterator for ParallelLines
{
    type Item = (Vec2, Vec2, Color);

    #[inline]
    #[must_use]
    fn next(&mut self) -> Option<Self::Item>
    {
        if self.x_left <= self.x_right
        {
            let line_x = self.x_left;
            self.x_left += self.grid_size;
            Some((
                Vec2::new(line_x, self.bottom),
                Vec2::new(line_x, self.top),
                (self.color)(self.half_grid_size, line_x)
            ))
        }
        else if self.y_left <= self.y_right
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

impl ParallelLines
{
    /// Returns a new [`ParallelLines`] based on the parameters.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    #[inline]
    fn new(grid: &Grid, top: f32, bottom: f32, left: f32, right: f32) -> Self
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

        let y_right = div_ceil(top as i16, grid.size);
        let y_left = div_ceil(bottom as i16, grid.size);
        let x_left = div_ceil(left as i16, grid.size);
        let x_right = div_ceil(right as i16, grid.size);

        let grid_size = grid.size_f32();
        let mut y_left = f32::from(y_left) * grid_size;
        let mut y_right = f32::from(y_right) * grid_size;
        let mut x_left = f32::from(x_left) * grid_size;
        let mut x_right = f32::from(x_right) * grid_size;

        let half_grid_size = grid_size / 2f32;

        if grid.shifted
        {
            for (ns, delta) in [
                ([&mut x_right, &mut y_right], half_grid_size),
                ([&mut x_left, &mut y_left], -half_grid_size)
            ]
            {
                for n in ns
                {
                    *n = delta;
                }
            }
        }

        Self {
            x_left,
            x_right,
            y_left,
            y_right,
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
            else if grid.shifted
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

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

#[allow(clippy::cast_possible_truncation)]
#[inline]
#[must_use]
const fn floor_multiple(value: f32, grid_size: i16) -> i16
{
    (value as i16 / grid_size) * grid_size
}
