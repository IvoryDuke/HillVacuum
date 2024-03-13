//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::cmp::Ordering;

use bevy::prelude::Vec2;

use super::{
    angles::vector_angle_cosine,
    points::{vertexes_orientation, VertexesOrientation}
};
use crate::{
    map::HvVec,
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

/// # Panics
/// Panics if there are issues comparing calculated cosines.
#[inline]
pub fn convex_hull(vertexes: impl ExactSizeIterator<Item = Vec2>) -> impl Iterator<Item = Vec2>
{
    let mut convex_hull = Vec::with_capacity(vertexes.len());
    let mut pivot = (Vec2::new(f32::MAX, f32::MAX), 0);

    for vx in vertexes
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

            #[allow(clippy::float_cmp)]
            if convex_hull[i].1 == convex_hull[j].1
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
    }

    convex_hull.into_iter()
}

//=======================================================================//

/// # Panics
/// May panic in some extreme conditions due to poor floating numbers rounding.
#[inline]
#[must_use]
pub fn clip_polygon(
    input: impl Iterator<Item = [Vec2; 2]> + Clone,
    clip_segment: &[Vec2; 2]
) -> Option<HvVec<Vec2>>
{
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

// #[inline]
// pub fn ear_clipping(input: Vec<Vec2>) -> impl Iterator<Item = Vec<Vec2>>
// {
// #[derive(Debug)]
// enum Angle
// {
//     Convex(f32),
//     Concave
// }

// let input_len = input.len();
// let mut triangles = Vec::with_capacity(input_len - 2);

// if input_len == 3
// {
//     triangles.push(input);
//     return triangles.into_iter();
// }

// // Calculate the cosines of the internal vertexes.
// let mut vxs_cosines = Vec::with_capacity(input_len);

// for [vx_i, vx_j, vx_k] in input.triplet_iter().unwrap()
// {
//     if are_vxs_ccw(&[*vx_i, *vx_j, *vx_k])
//     {
//         vxs_cosines
//             .push((*vx_j, Angle::Convex(vectors_angle_cosine(*vx_j - *vx_i, *vx_k -
// *vx_j))));     }
//     else
//     {
//         vxs_cosines.push((*vx_j, Angle::Concave));
//     }
// }

// std::mem::drop(input);

// // Extract the triangles.
// loop
// {
//     let mut max_cos = (None, f32::MIN);
//     let len = vxs_cosines.len();

//     'outer: for ([i, j, k], [vx_i, vx_j, vx_k]) in
//         vxs_cosines.triplet_iter().unwrap().enumerate()
//     {
//         let cos = continue_if_no_match!(vx_j.1, Angle::Convex(cos), cos);

//         if cos <= max_cos.1
//         {
//             continue;
//         }

//         // This is the triangle's side which is not one of the polygon's sides and must
//         // be checked for intersections.
//         let check_side = [vx_k.0, vx_i.0];
//         let mut l = k;
//         let mut m = next(l, len);

//         while l != i
//         {
//             if let Some((_, t)) =
//                 segments_intersection(&check_side, &[vxs_cosines[l].0, vxs_cosines[m].0])
//             {
//                 if t.around_equal(&0f32) && t.around_equal(&1f32)
//                 {
//                     continue 'outer;
//                 }
//             }

//             l = m;
//             m = next(m, len);
//         }

//         max_cos.1 = cos;
//         max_cos.0 = (i, j, k).into();
//     }

//     let (i, mut j, k) = max_cos.0.unwrap();

//     triangles.push(vec![vxs_cosines[i].0, vxs_cosines[j].0, vxs_cosines[k].0]);

//     // Remove the vertex and update the cosines of the next and prev vertexes.
//     vxs_cosines.remove(j);
//     let len = vxs_cosines.len();

//     if len < 4
//     {
//         break;
//     }

//     j %= len;

//     for idx in [j, prev(j, len)]
//     {
//         let next = next(idx, len);
//         let prev = prev(idx, len);

//         if are_vxs_ccw(&[vxs_cosines[prev].0, vxs_cosines[idx].0, vxs_cosines[next].0])
//         {
//             vxs_cosines[idx].1 = Angle::Convex(vectors_angle_cosine(
//                 vxs_cosines[idx].0 - vxs_cosines[prev].0,
//                 vxs_cosines[next].0 - vxs_cosines[idx].0
//             ));
//             continue;
//         }

//         vxs_cosines[idx].1 = Angle::Concave;
//     }
// }

// triangles.push(vec![vxs_cosines[0].0, vxs_cosines[1].0, vxs_cosines[2].0]);
// triangles.into_iter()
// }
