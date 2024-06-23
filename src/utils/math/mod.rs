pub mod angles;
pub mod lines_and_segments;
pub mod points;
pub mod polygons;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::hash::Hash;

use bevy::prelude::Vec2;

//=======================================================================//
// TRAITS
//
//=======================================================================//

/// A trait to determine whever two objects are equal within a certain error margin.
pub trait AroundEqual
{
    /// Whever `self` and `other` are equal within a somewhat loose margin.
    #[must_use]
    fn around_equal(&self, other: &Self) -> bool;

    /// Whever `self` and `other` are equal within a very tight margin.
    #[must_use]
    fn around_equal_narrow(&self, other: &Self) -> bool;
}

impl AroundEqual for f32
{
    #[inline]
    fn around_equal(&self, other: &Self) -> bool { (*self - *other).abs() < 1f32 / 128f32 }

    #[inline]
    fn around_equal_narrow(&self, other: &Self) -> bool { (*self - *other).abs() < f32::EPSILON }
}

impl AroundEqual for Vec2
{
    #[inline]
    fn around_equal(&self, other: &Self) -> bool
    {
        self.x.around_equal(&other.x) && self.y.around_equal(&other.y)
    }

    #[inline]
    fn around_equal_narrow(&self, other: &Self) -> bool
    {
        self.x.around_equal_narrow(&other.x) && self.y.around_equal_narrow(&other.y)
    }
}

//=======================================================================//

/// A trait to calculate the inverse square root of a number using the famous Quake
/// `fast_inverse_sqrt` algorithm.
pub trait FastSqrt
{
    /// Calculates the inverse square root of a number using the famous Quake `fast_inverse_sqrt`
    /// algorithm.
    #[must_use]
    fn fast_inverse_sqrt(self) -> Self;
}

impl FastSqrt for f32
{
    #[inline]
    fn fast_inverse_sqrt(self) -> Self { inverse_sqrt(self) }
}

//=======================================================================//

/// A trait to normalize a vector using the Quake `fast_inverse_sqrt` algorithm.
pub trait FastNormalize
{
    /// Calculates the normalized vector through the Quake `fast_inverse_sqrt` algorithm.
    #[must_use]
    fn fast_normalize(self) -> Vec2;
}

impl FastNormalize for Vec2
{
    #[inline]
    fn fast_normalize(self) -> Self
    {
        self * ((self.x * self.x + self.y * self.y).fast_inverse_sqrt())
    }
}

//=======================================================================//

/// A trait to create types from types using floating point values, that do not print the fractional
/// part if it's equal to zero.
pub trait NecessaryPrecisionValue<T>
{
    /// Returns a value that only prints the fractional part if it's different from zero.
    #[must_use]
    fn necessary_precision_value(&self) -> T;
}

impl NecessaryPrecisionValue<NecessaryPrecisionVec2> for Vec2
{
    #[inline]
    fn necessary_precision_value(&self) -> NecessaryPrecisionVec2
    {
        NecessaryPrecisionVec2(
            self.x.necessary_precision_value(),
            self.y.necessary_precision_value()
        )
    }
}

impl NecessaryPrecisionValue<NecessaryPrecisionF32> for f32
{
    #[inline]
    fn necessary_precision_value(&self) -> NecessaryPrecisionF32 { NecessaryPrecisionF32(*self) }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// A hashable [`Vec2`]. Only to be used in contexts where the x and y coordinates cannot be NaN.
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct HashVec2(pub Vec2);

impl Eq for HashVec2 {}

impl Hash for HashVec2
{
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H)
    {
        self.0.x.to_bits().hash(state);
        self.0.y.to_bits().hash(state);
    }
}

//=======================================================================//

/// A representation of a vector that only prints the fractional part of the x and y coordinates if
/// they are different from zero.
#[derive(Clone, Copy)]
pub struct NecessaryPrecisionVec2(NecessaryPrecisionF32, NecessaryPrecisionF32);

impl std::fmt::Display for NecessaryPrecisionVec2
{
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        write!(f, "{}, {}", self.0, self.1)
    }
}

//=======================================================================//

/// A representation of a [`f32`] that only prints the fractional part if it's different from zero.
#[derive(Clone, Copy)]
pub struct NecessaryPrecisionF32(f32);

impl std::fmt::Display for NecessaryPrecisionF32
{
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        if self.0.fract() == 0f32
        {
            write!(f, "{}", self.0)
        }
        else
        {
            write!(f, "{:2}", self.0)
        }
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// The Quake fast inverse sqrt algorithm.
/// <https://stackoverflow.com/questions/59081890/is-it-possible-to-write-quakes-fast-invsqrt-function-in-rust>
#[inline]
#[must_use]
pub fn inverse_sqrt(x: f32) -> f32
{
    let y = f32::from_bits(0x5f37_59df - (x.to_bits() >> 1));
    y * (1.5 - 0.5 * x * y * y)
}
