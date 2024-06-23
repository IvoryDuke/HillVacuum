//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::Vec2;

use super::AroundEqual;

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The equation of a bidimensional line.
#[must_use]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum LineEquation
{
    /// Parallel to the x axis.
    Horizontal(f32),
    /// Parallel to the y axis.
    Vertical(f32),
    /// A generic line.
    Generic(f32, f32)
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Returns the length of the segment connecting a point `p` to the closest point on the line
/// described by `a` and `b`.
/// # Panics
/// Panics if `a` and `b` represent the same point.
#[inline]
#[must_use]
pub fn point_to_segment_distance_squared(a: Vec2, b: Vec2, p: Vec2) -> f32
{
    assert!(
        a != b,
        "Segment extremes have the same value {a}, therefore it is not a segment."
    );
    p.distance_squared(closest_point_on_segment(a, b, p))
}

//=======================================================================//

/// Returns the coordinates of the point of the segment passing through `a` and `b` closest to `p`.
#[inline]
#[must_use]
pub fn closest_point_on_segment(a: Vec2, b: Vec2, p: Vec2) -> Vec2
{
    let ab = b - a;
    let distance = (p - a).dot(ab) / ab.length_squared();

    if distance.is_sign_negative()
    {
        return a;
    }

    if distance > 1f32
    {
        return b;
    }

    a + ab * distance
}

//=======================================================================//

/// Returns the coordinates of the point of the line passing through `a` and `b` closest to `p`.
#[inline]
#[must_use]
pub fn closest_point_on_line(a: Vec2, b: Vec2, p: Vec2) -> Vec2
{
    let ab = b - a;
    a + ab * (p - a).dot(ab) / ab.length_squared()
}

//=======================================================================//

/// Returns the [`LineEquation`] representing the line passing through the two points of `l`.
/// # Panics
/// Panics if the points of `l` are equal.
#[inline]
pub fn line_equation(l: &[Vec2; 2]) -> LineEquation
{
    assert!(
        !l[0].around_equal_narrow(&l[1]),
        "The two points of the line are equal {}.",
        l[0]
    );

    let delta_x = l[1].x - l[0].x;

    if delta_x.around_equal_narrow(&0f32)
    {
        return LineEquation::Vertical(l[0].x);
    }

    let delta_y = l[1].y - l[0].y;

    if delta_y.around_equal_narrow(&0f32)
    {
        return LineEquation::Horizontal(l[0].y);
    }

    let m = delta_y / delta_x;
    let q = l[0].y - m * l[0].x;
    LineEquation::Generic(m, q)
}

//=======================================================================//

/// Computes the linear interpolation between `a` and `b`.
#[inline]
#[must_use]
pub fn lerp(a: Vec2, b: Vec2, t: f32) -> Vec2 { a + (b - a) * t }

//=======================================================================//

/// Computes the intersection of lines `s_1` and `s_2`, if any.
#[inline]
#[must_use]
pub fn lines_intersection(l_1: &[Vec2; 2], l_2: &[Vec2; 2]) -> Option<(Vec2, f32, f32)>
{
    let (a, b, c, d) = (l_1[0], l_1[1], l_2[0], l_2[1]);
    let bottom = (d.y - c.y) * (b.x - a.x) - (d.x - c.x) * (b.y - a.y);

    if bottom.around_equal_narrow(&0f32)
    {
        return None;
    }

    let top = (d.x - c.x) * (a.y - c.y) - (d.y - c.y) * (a.x - c.x);
    let t = top / bottom;

    Some((
        lerp(a, b, t),
        t,
        ((c.y - a.y) * (a.x - b.x) - (c.x - a.x) * (a.y - b.y)) / bottom
    ))
}

//=======================================================================//

/// Computes the intersection of segments `s_1` and `s_2`, if any.
#[inline]
#[must_use]
pub fn segments_intersection(s_1: &[Vec2; 2], s_2: &[Vec2; 2]) -> Option<(Vec2, f32)>
{
    let (inter, t, u) = lines_intersection(s_1, s_2)?;
    ((0f32..=1f32).contains(&t) && (0f32..=1f32).contains(&u)).then_some((inter, t))
}

//=======================================================================//

/// Whever `p` is on the segment `s`.
#[inline]
#[must_use]
pub fn is_point_on_segment(s: &[Vec2; 2], p: Vec2) -> bool
{
    /// The epsilon used to determine whever the point can reasonably considered on the segment.
    const POINT_ON_LINE_EPSILON: f32 = 1f32 / 16f32;

    // This is the only method that has proven itself to be reliable.
    point_to_segment_distance_squared(s[0], s[1], p) < POINT_ON_LINE_EPSILON
}

//=======================================================================//

/// Computes the dot product of the line `l` and the point `p`.
#[inline]
#[must_use]
pub fn line_point_product(l: &[Vec2; 2], p: Vec2) -> f32
{
    (l[1].x - l[0].x) * (p.y - l[0].y) - (l[1].y - l[0].y) * (p.x - l[0].x)
}

//=======================================================================//

/// Whever `p` is inside the clip edge of `l`.
#[inline]
#[must_use]
pub fn is_point_inside_clip_edge(l: &[Vec2; 2], p: Vec2) -> bool { line_point_product(l, p) > 0f32 }
