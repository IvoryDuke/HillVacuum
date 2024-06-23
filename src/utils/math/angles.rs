//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::Vec2;

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Computes the cosine of the angle of `v`.
#[inline]
#[must_use]
pub fn vector_angle_cosine(v: Vec2) -> f32 { v.normalize().dot(Vec2::X) }

//=======================================================================//

/// Computes the cosine of the angle between `vec_1` and `vec_2`.
#[inline]
#[must_use]
pub fn vectors_angle_cosine(vec_1: Vec2, vec_2: Vec2) -> f32
{
    vec_1.dot(vec_2) / (vec_1.length() * vec_2.length())
}
