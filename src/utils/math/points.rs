//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::cmp::Ordering;

use bevy::prelude::Vec2;

use super::AroundEqual;

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The orientation of three consecutive vertexes.
#[must_use]
#[derive(Clone, Copy, PartialEq)]
pub enum VertexesOrientation
{
    /// Clockwise.
    Clockwise,
    /// Collinear.
    Collinear,
    /// Counter clockwise.
    CounterClockwise
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Rotates a point around origin `o` by `angle`.
#[inline]
#[must_use]
pub fn rotate_point(p: Vec2, o: Vec2, angle: f32) -> Vec2
{
    let (sin, cos) = angle.sin_cos();
    let t = p - o;
    Vec2::new(t.x * cos - t.y * sin + o.x, t.y * cos + t.x * sin + o.y)
}

//=======================================================================//

/// Rotates a point around the origin.
#[inline]
#[must_use]
pub fn rotate_point_around_origin(p: Vec2, angle: f32) -> Vec2
{
    let (sin, cos) = angle.sin_cos();
    Vec2::new(p.x * cos - p.y * sin, p.y * cos + p.x * sin)
}

//=======================================================================//

/// Computes the center of a series of points.
#[allow(clippy::cast_precision_loss)]
#[inline]
#[must_use]
pub fn vxs_center<T>(vxs: T) -> Vec2
where
    T: ExactSizeIterator<Item = Vec2>
{
    let len = vxs.len() as f32;
    vxs.fold(Vec2::ZERO, |sum, x| sum + x) / len
}

//=======================================================================//

/// Sorts `a` and `b` counterclockwise around the `center`.
#[inline]
#[must_use]
pub fn sort_vxs_ccw(a: Vec2, b: Vec2, center: Vec2) -> Ordering
{
    /// <https://stackoverflow.com/questions/6989100/sort-points-in-clockwise-order>
    #[inline]
    #[allow(clippy::similar_names)]
    pub fn sort_vxs_cw(a: Vec2, b: Vec2, center: Vec2) -> Ordering
    {
        let ax_cx = a.x - center.x;
        let bx_cx = b.x - center.x;

        if ax_cx >= 0f32 && bx_cx < 0f32
        {
            return Ordering::Less;
        }
        if ax_cx < 0f32 && bx_cx >= 0f32
        {
            return Ordering::Greater;
        }

        let ay_cy = a.y - center.y;
        let by_cy = b.y - center.y;

        if ax_cx.around_equal_narrow(&0f32) && bx_cx.around_equal_narrow(&0f32)
        {
            if ay_cy >= 0f32 || by_cy >= 0f32
            {
                return b.y.partial_cmp(&a.y).unwrap();
            }

            return a.y.partial_cmp(&b.y).unwrap();
        }

        // Compute the cross product of vectors (center -> a) x (center -> b)
        let det = ax_cx * by_cy - bx_cx * ay_cy;

        if det < 0f32
        {
            return Ordering::Less;
        }
        if det > 0f32
        {
            return Ordering::Greater;
        }

        // Points a and b are on the same side from the center
        // check which point is closer to the center
        let d1 = ax_cx * ax_cx + ay_cy * ay_cy;
        let d2 = bx_cx * bx_cx + by_cy * by_cy;
        d2.partial_cmp(&d1).unwrap()
    }

    match sort_vxs_cw(a, b, center)
    {
        Ordering::Less => Ordering::Greater,
        Ordering::Equal => Ordering::Equal,
        Ordering::Greater => Ordering::Less
    }
}

//=======================================================================//

/// Returns the orientation of the vertexes `vxs`.
#[inline]
pub fn vertexes_orientation(vxs: &[Vec2; 3]) -> VertexesOrientation
{
    let det = (vxs[0].x * vxs[1].y + vxs[1].x * vxs[2].y + vxs[2].x * vxs[0].y) -
        (vxs[0].y * vxs[1].x + vxs[1].y * vxs[2].x + vxs[2].y * vxs[0].x);

    if det.is_sign_negative()
    {
        return VertexesOrientation::Clockwise;
    }

    if det.around_equal_narrow(&0f32)
    {
        return VertexesOrientation::Collinear;
    }

    VertexesOrientation::CounterClockwise
}

//=======================================================================//

/// Returns true if the vertexes contained in vxs are in a counterclockwise order.
#[inline]
pub fn are_vxs_ccw(vxs: &[Vec2; 3]) -> bool
{
    matches!(vertexes_orientation(vxs), VertexesOrientation::CounterClockwise)
}

//=======================================================================//

/// Whever the polygon described by the vertexes `vxs` is convex.
/// Assumes `vxs` are clockwise sorted.
#[inline]
#[must_use]
pub fn is_polygon_convex(vxs: &[Vec2]) -> bool
{
    let len = vxs.len();
    let (mut i, mut j) = (len - 1, 0);

    for k in 1..len
    {
        if !are_vxs_ccw(&[vxs[i], vxs[j], vxs[k]])
        {
            return false;
        }

        i = j;
        j = k;
    }

    are_vxs_ccw(&[vxs[i], vxs[j], vxs[0]])
}
