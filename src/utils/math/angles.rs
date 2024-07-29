//=======================================================================//
// IMPORTS
//
//=======================================================================//

use glam::Vec2;

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

hill_vacuum_proc_macros::sin_cos_tan_array!();

//=======================================================================//
// TRAITS
//
//=======================================================================//

pub(crate) trait FastSinCosTan
{
    #[must_use]
    fn fast_sin_cos(&self) -> (f32, f32);

    #[must_use]
    fn fast_tan(&self) -> f32;
}

impl FastSinCosTan for i8
{
    #[inline]
    fn fast_sin_cos(&self) -> (f32, f32)
    {
        let slot = i8_sin_cos_slot(*self);
        (slot.0, slot.1)
    }

    #[must_use]
    fn fast_tan(&self) -> f32 { i8_sin_cos_slot(*self).2 }
}

impl FastSinCosTan for i16
{
    #[inline]
    fn fast_sin_cos(&self) -> (f32, f32)
    {
        let (sin, cos, _) = i16_sin_cos_slot(*self);
        (*sin, *cos)
    }

    #[must_use]
    fn fast_tan(&self) -> f32 { i16_sin_cos_slot(*self).2 }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

#[inline]
#[must_use]
fn i8_sin_cos_slot(angle: i8) -> &'static (f32, f32, f32) { i16_sin_cos_slot(i16::from(angle)) }

//=======================================================================//

#[inline]
#[must_use]
fn i16_sin_cos_slot(angle: i16) -> &'static (f32, f32, f32)
{
    let idx = if angle < 0 { 360 + angle } else { angle };
    &SIN_COS_TAN_LOOKUP[usize::try_from(idx).unwrap()]
}

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
