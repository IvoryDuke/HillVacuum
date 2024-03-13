//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::ops::RangeInclusive;

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

pub const TEXTURE_HEIGHT_RANGE: RangeInclusive<i8> = 0..=20;
pub const FILE_EXTENSION: &str = "hv";

//=======================================================================//
// TRAITS
//
//=======================================================================//

/// A trait for iterators to get the next value and immediately unwrap it.
pub trait NextValue<T>
where
    Self: Iterator<Item = T>
{
    /// Returns the next unwrapped value.
    /// # Panics
    /// Panic occurs if the next value is None.
    #[inline]
    #[must_use]
    fn next_value(&mut self) -> T { self.next().unwrap() }
}

impl<T, U: Iterator<Item = T>> NextValue<T> for U {}

//=======================================================================//
// MACROS
//
//=======================================================================//

/// Iterates a slice in triplets.
#[macro_export]
macro_rules! iterate_slice_in_triplets {
    ($i:ident, $j:ident, $k:ident, $max: expr, $f:block) => (
		let (mut $i, mut $j, mut $k) = ($max - 2, $max - 1, 0);

		while $k < $max
		{
			$f

			$i = $j;
            $j = $k;
            $k += 1;
		}
	);
}

//=======================================================================//

/// Ends the function call if `$value` is `None`. Otherwise it returns the contained value.
#[macro_export]
macro_rules! return_if_none {
    ($value:expr) => {
        match $value
        {
            Some(value) => value,
            None => return
        }
    };

    ($value:expr, $return_value:expr) => {
        match $value
        {
            Some(value) => value,
            None => return $return_value
        }
    };
}

//=======================================================================//

/// Ends the function call if `$value` does not match `$pattern`. Otherwise it returns `$f`
#[macro_export]
macro_rules! return_if_no_match {
    ($value:expr, $pattern:pat, $f:expr) => {
        match $value
        {
            $pattern => $f,
            _ => return
        }
    };

    ($value:expr, $pattern:pat, $f:expr, $return_value:expr) => {
        match $value
        {
            $pattern => $f,
            _ => return $return_value
        }
    };
}

//=======================================================================//

/// Ends the function call if `$value` is `Err`. Otherwise it returns the contained value.
#[macro_export]
macro_rules! return_if_err {
    ($value:expr) => {
        match $value
        {
            Ok(value) => value,
            Err(_) => return
        }
    };

    ($value:expr, $return_value:expr) => {
        match $value
        {
            Ok(value) => value,
            Err(_) => return $return_value
        }
    };
}

//=======================================================================//

/// Continues the loop if `$value` is `None`. Otherwise it returns the contained value.
#[macro_export]
macro_rules! continue_if_none {
    ($value:expr) => (
		match $value
        {
            Some(value) => value,
            None => continue
        }
	);

    ($value:expr, $label:tt) => (
		match $value
        {
            Some(value) => value,
            None => continue $label
        }
	);
}

//=======================================================================//

/// Continues the loop if `$value` is `None`. Otherwise it returns the contained value.
#[macro_export]
macro_rules! continue_if_err {
    ($value:expr) => {
        match $value
        {
            Ok(value) => value,
            Err(_) => continue
        }
    };
}

//=======================================================================//

/// Continues the loop if `$value` does not match `$pattern`. Otherwise it returns `$f`.
#[macro_export]
macro_rules! continue_if_no_match {
    ($value:expr, $pattern:pat, $f:expr) => {
        match $value
        {
            $pattern => $f,
            _ => continue
        }
    };
}

//=======================================================================//

/// Panics if `$value` does not match `$pattern`. Otherwise it returns `$f`.
#[macro_export]
macro_rules! match_or_panic {
    ($value:expr, $pattern:pat, $f:expr) => {
        match $value
        {
            $pattern => $f,
            _ => panic!("Pattern does not match.")
        }
    };

    ($value:expr, $pattern:pat) => {
        match $value
        {
            $pattern => (),
            _ => panic!("Pattern does not match.")
        }
    };
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

#[inline]
#[must_use]
pub fn draw_height_to_world(height: i8) -> f32 { height as f32 / 8f32 }
