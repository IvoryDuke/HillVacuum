//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::cmp::Ordering;

use bevy::prelude::Vec2;

use super::{
    angles::vector_angle_cosine,
    points::{vertexes_orientation, VertexesOrientation},
    HashVec2
};
use crate::{
    map::containers::{HvHashSet, HvVec},
    utils::{
        math::{
            lines_and_segments::{is_point_inside_clip_edge, lines_intersection},
            AroundEqual
        },
        misc::prev
    }
};

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Returns a list of points describing the convex hull of `vertexes`.
/// # Panics
/// Panics if there are issues comparing calculated cosines.
#[inline]
pub fn convex_hull(vertexes: HvHashSet<HashVec2>) -> Option<impl Iterator<Item = Vec2>>
{
    let mut convex_hull = Vec::with_capacity(vertexes.len());
    let mut pivot = (Vec2::new(f32::MAX, f32::MAX), 0);

    for vx in vertexes.into_iter().map(|vx| vx.0)
    {
        convex_hull.push((vx, 0f32));

        if vx.y < pivot.0.y || (vx.y.around_equal_narrow(&pivot.0.y) && vx.x < pivot.0.x)
        {
            pivot = (vx, convex_hull.len() - 1);
        }
    }

    // Make the pivot the first element.
    convex_hull.swap(pivot.1, 0);

    // If there are  multiple vertexes that are at the same angle from `pivot`
    // remove the ones that are closer to it. Also cache the value of the
    // angle's cosine for the sorting.
    let mut i = 1;

    'i_loop: while i < convex_hull.len()
    {
        convex_hull[i].1 = vector_angle_cosine(convex_hull[i].0 - pivot.0);
        let mut j = i + 1;

        while j < convex_hull.len()
        {
            convex_hull[j].1 = vector_angle_cosine(convex_hull[j].0 - pivot.0);

            if convex_hull[i].1.around_equal_narrow(&convex_hull[j].1)
            {
                if (convex_hull[i].0 - pivot.0).length_squared() <
                    (convex_hull[j].0 - pivot.0).length_squared()
                {
                    convex_hull.remove(i);
                    continue 'i_loop;
                }

                convex_hull.remove(j);
                continue;
            }

            j += 1;
        }

        i += 1;
    }

    // Sort the vertexes from right to left (except the pivot).
    convex_hull[1..].sort_by(|a, b| {
        match a.1.partial_cmp(&b.1).unwrap()
        {
            Ordering::Less => Ordering::Greater,
            Ordering::Greater => Ordering::Less,
            Ordering::Equal => panic!("Set contains vertexes with the same coordinates.")
        }
    });
    let mut convex_hull = convex_hull.into_iter().map(|x| x.0).collect::<Vec<_>>();

    // Only keep vertexes that make the to-be-spawned polygon convex.
    let (mut j, mut i, mut k) = (0, 1, 2);

    while k < convex_hull.len()
    {
        match vertexes_orientation(&[convex_hull[j], convex_hull[i], convex_hull[k]])
        {
            VertexesOrientation::CounterClockwise =>
            {
                j = i;
                i = k;
                k += 1;
            },
            VertexesOrientation::Collinear | VertexesOrientation::Clockwise =>
            {
                convex_hull.remove(i);

                if convex_hull.is_empty()
                {
                    return None;
                }

                k = i;
                i = prev(k, convex_hull.len());
                j = prev(i, convex_hull.len());
            }
        };
    }

    if let VertexesOrientation::Collinear | VertexesOrientation::Clockwise =
        vertexes_orientation(&[convex_hull[j], convex_hull[i], convex_hull[0]])
    {
        convex_hull.remove(i);

        if convex_hull.is_empty()
        {
            return None;
        }
    }

    convex_hull.into_iter().into()
}

//=======================================================================//

/// Returns the left half, if any, of the polygon represented by `input` cut by `clip_segment`.
/// # Panics
/// May panic in some extreme conditions due to poor floating numbers rounding.
#[inline]
#[must_use]
pub fn clip_polygon(
    input: impl Iterator<Item = [Vec2; 2]> + Clone,
    clip_segment: &[Vec2; 2]
) -> Option<HvVec<Vec2>>
{
    /// Inserts `point` into `collected_points` if it is not around equal to any other point inside
    /// it.
    #[inline]
    fn check_clip_point(point: Vec2, collected_points: &mut HvVec<Vec2>)
    {
        if collected_points.iter().any(|vx| vx.around_equal(&point))
        {
            return;
        }

        collected_points.push(point);
    }

    let mut output = HvVec::new();

    for side in input
    {
        let starting_inside = is_point_inside_clip_edge(clip_segment, side[0]);

        if is_point_inside_clip_edge(clip_segment, side[1])
        {
            if !starting_inside
            {
                if let Some(inter) = lines_intersection(clip_segment, &side)
                {
                    check_clip_point(inter.0, &mut output);
                }
            }

            check_clip_point(side[1], &mut output);
        }
        else if starting_inside
        {
            match lines_intersection(clip_segment, &side)
            {
                None => (),
                Some(inter) => check_clip_point(inter.0, &mut output)
            };
        }
    }

    (output.len() >= 3).then_some(output)
}
