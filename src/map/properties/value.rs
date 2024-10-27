//=======================================================================//
// IMPORTS
//
//=======================================================================//

use serde::{Deserialize, Serialize};

//=======================================================================//
// MACROS
//
//=======================================================================//

#[rustfmt::skip]
macro_rules! for_each_value {
    ($macro:ident) => {
        $macro!(
            Bool, bool, "bool", true,
            U8, u8, "u8", 0,
            U16, u16, "u16", 0,
            U32, u32, "u32", 0,
            U64, u64, "u64", 0,
            U128, u128, "u128", 0,
            I8, i8, "i8", 0,
            I16, i16, "i16", 0,
            I32, i32, "i32", 0,
            I64, i64, "i64", 0,
            I128, i128, "i128", 0,
            F32, f32, "f32", 0f32,
            F64, f64, "f64", 0f64,
            String, String, "String", String::new()
        );
    };

    (ret, $macro:ident) => {
        $macro!(
            Bool, bool, "bool", true,
            U8, u8, "u8", 0,
            U16, u16, "u16", 0,
            U32, u32, "u32", 0,
            U64, u64, "u64", 0,
            U128, u128, "u128", 0,
            I8, i8, "i8", 0,
            I16, i16, "i16", 0,
            I32, i32, "i32", 0,
            I64, i64, "i64", 0,
            I128, i128, "i128", 0,
            F32, f32, "f32", 0f32,
            F64, f64, "f64", 0f64,
            String, String, "String", String::new()
        )
    }
}

use for_each_value;

//=======================================================================//

/// Generates [`From`] implementations for `$t`.
macro_rules! to_value {
    ($($value:ident, $t:ty, $str:literal, $default:expr),+) => {$(
        impl From<$t> for Value
        {
            #[inline]
            fn from(value: $t) -> Self
            {
                Value::$value(value.to_owned())
            }
        }
    )+};
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[must_use]
#[derive(Clone, Serialize, Deserialize)]
/// A primitive value or a [`String`].
pub enum Value
{
    /// Bool.
    Bool(bool),
    /// u8.
    U8(u8),
    /// u16.
    U16(u16),
    /// u32.
    U32(u32),
    /// u64.
    U64(u64),
    /// u128.
    U128(u128),
    /// i8.
    I8(i8),
    /// i16.
    I16(i16),
    /// i32.
    I32(i32),
    /// i64.
    I64(i64),
    /// i128.
    I128(i128),
    /// f32.
    F32(f32),
    /// f64.
    F64(f64),
    /// String.
    String(String)
}

for_each_value!(to_value);

impl std::fmt::Debug for Value
{
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        /// Implements debug for all enum arms.
        macro_rules! debug {
            ($($value:ident, $t:ty, $str:literal, $default:expr),+) => {
                match self
                {
                    $(Self::$value(value) => write!(f, "{value} ({})", $str)),+
                }
            }
        }

        for_each_value!(ret, debug)
    }
}

impl std::fmt::Display for Value
{
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        /// Implements display for all enum arms.
        macro_rules! display {
            ($($value:ident, $t:ty, $str:literal, $default:expr),+) => {
                match self
                {
                    $(Self::$value(value) => value.fmt(f)),+
                }
            }
        }

        for_each_value!(ret, display)
    }
}

//=======================================================================//
// UI
//
//=======================================================================//

#[cfg(feature = "ui")]
pub(in crate::map) mod ui_mod
{
    //=======================================================================//
    // IMPORTS
    //
    //=======================================================================//

    use std::str::FromStr;

    use hill_vacuum_shared::match_or_panic;

    use super::Value;
    use crate::utils::misc::ReplaceValue;

    //=======================================================================//
    // ENUMS
    //
    //=======================================================================//

    impl PartialEq for Value
    {
        #[inline]
        fn eq(&self, other: &Self) -> bool
        {
            macro_rules! cmp {
                ($($value:ident, $t:ty, $str:literal, $default:expr),+) => {
                    match (self, other)
                    {
                        $((Self::$value(l0), Self::$value(r0)) => l0 == r0,)+
                        _ => panic!("Tried comparing values that differ in type.")
                    }
                }
            }

            for_each_value!(ret, cmp)
        }
    }

    impl FromStr for Value
    {
        type Err = ();

        #[inline]
        fn from_str(s: &str) -> Result<Self, Self::Err> { Ok(Self::String(s.to_string())) }
    }

    impl Value
    {
        pub(in crate::map) const BOOL_TAG: u8 = 0;

        #[inline]
        #[must_use]
        pub(in crate::map) fn tag(&self) -> u8
        {
            match self
            {
                Value::Bool(_) => Self::BOOL_TAG,
                Value::U8(_) => 1,
                Value::U16(_) => 2,
                Value::U32(_) => 3,
                Value::U64(_) => 4,
                Value::U128(_) => 5,
                Value::I8(_) => 6,
                Value::I16(_) => 7,
                Value::I32(_) => 8,
                Value::I64(_) => 9,
                Value::I128(_) => 10,
                Value::F32(_) => 11,
                Value::F64(_) => 12,
                Value::String(_) => 13
            }
        }

        /// Whether `self` and `other` have the same [`Discriminant`].
        #[inline]
        #[must_use]
        pub(in crate::map) fn eq_tag(&self, other: &Self) -> bool { self.tag() == other.tag() }

        #[inline]
        #[must_use]
        pub(in crate::map) fn type_str(&self) -> &'static str
        {
            macro_rules! value {
                ($($value:ident, $t:ty, $str:literal, $default:expr),+) => {
                    match self
                    {
                        $(Self::$value(_) => $str,)+
                    }
                }
            }

            for_each_value!(ret, value)
        }

        /// Sets `self` to `value`. Returns the previous value if different.
        #[inline]
        pub(in crate::map) fn set(&mut self, value: &Self) -> Option<Self>
        {
            assert!(self.eq_tag(value), "Mismatching discriminants.");

            if *self == *value
            {
                return None;
            }

            self.replace_value(value.clone()).into()
        }

        /// Tries to convert `value` to the same type of `self`.
        #[inline]
        #[must_use]
        pub(in crate::map) fn parse(&self, value: &Self) -> Option<Self>
        {
            let string = match_or_panic!(value, Self::String(s), s);

            /// Implements the conversion of `string` to the [`Value`] variant of `self`, if
            /// possible.
            macro_rules! convert {
                ($($value:ident, $t:ty, $str:literal, $default:expr),+) => {
                    match self
                    {
                        $(Self::$value(_) =>
                        {
                            <$t>::from_str(string)
                                .ok()
                                .map(|value| Self::$value(value))
                        }),+
                    }
                };
            }

            for_each_value!(ret, convert)
        }
    }
}
