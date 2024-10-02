//=======================================================================//
// IMPORTS
//
//=======================================================================//

use serde::{Deserialize, Serialize};

use crate::{map::indexed_map::IndexedMap, HvHashMap};

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

/// Generates [`ToValue`] implementations for `t`.
macro_rules! to_value {
    ($($value:ident, $t:ty, $str:literal, $default:expr),+) => {$(
        impl ToValue for $t
        {
            #[inline]
            fn to_value(&self) -> Value
            {
                Value::$value((*self).to_owned())
            }
        }
    )+};
}

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

pub const COLLISION_LABEL: &str = "collision";

//=======================================================================//
// TRAITS
//
//=======================================================================//

/// A trait to generate a [`Value`] from `self`.
pub trait ToValue
{
    /// Converts `self` to a [`Value`].
    fn to_value(&self) -> Value;
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
                    $(Self::$value(value) => write!(f, "{}: {value}", $str)),+
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
            ($($value:ident),+) => {
                match self
                {
                    $(Self::$value(value) => value.fmt(f)),+
                }
            }
        }

        display!(Bool, U8, U16, U32, U64, U128, I8, I16, I32, I64, I128, F32, F64, String)
    }
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// Key-value pairs associated to an entity.
#[must_use]
#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::map) struct Properties(HvHashMap<String, Value>);

//=======================================================================//

/// The default properties to be associated with certain entities.
#[must_use]
#[derive(Clone, Serialize, Deserialize)]
pub(in crate::map) struct DefaultProperties(IndexedMap<String, Value>, Properties);

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

    use std::{mem::Discriminant, str::FromStr};

    use bevy::prelude::Resource;
    use hill_vacuum_shared::{match_or_panic, return_if_none, NextValue};

    use super::{DefaultProperties, Properties, COLLISION_LABEL};
    use crate::{
        map::{
            drawer::drawing_resources::DrawingResources,
            editor::state::grid::Grid,
            indexed_map::IndexedMap
        },
        utils::{
            collections::{hv_hash_map, hv_vec},
            misc::{AssertedInsertRemove, ReplaceValue}
        },
        HvHashMap,
        HvVec,
        ToValue,
        Value
    };
    #[allow(unused_imports)]
    use crate::{Brush, ThingInstance};

    //=======================================================================//
    // TRAITS
    //
    //=======================================================================//

    /// A trait for entities with associated [`Properties`] to set the value of a certain [`Value`].
    pub(in crate::map) trait SetProperty
    {
        /// Sets the property `key` to `value`.
        fn set_property(
            &mut self,
            drawing_resources: &DrawingResources,
            grid: &Grid,
            key: &str,
            value: &Value
        );
    }

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
        /// The [`Discriminant`] of the boolean value.
        pub(in crate::map) const BOOL_DISCRIMINANT: Discriminant<Self> =
            std::mem::discriminant(&Self::Bool(true));

        /// Whether `self` and `other` have the same [`Discriminant`].
        #[inline]
        #[must_use]
        pub(in crate::map) fn eq_discriminant(&self, other: &Self) -> bool
        {
            std::mem::discriminant(self) == std::mem::discriminant(other)
        }

        #[inline]
        #[must_use]
        pub(in crate::map) fn discriminant_type(discriminant: Discriminant<Self>) -> &'static str
        {
            macro_rules! match_discriminant {
                ($($value:ident, $t:ty, $str:literal, $default:expr),+) => {{
                    $(
                        if discriminant == std::mem::discriminant(&Value::$value($default))
                        {
                            return $str;
                        }
                    )+

                    unreachable!()
                }};
            }

            for_each_value!(ret, match_discriminant)
        }

        /// Sets `self` to `value`. Returns the previous value if different.
        #[inline]
        pub(in crate::map) fn set(&mut self, value: &Self) -> Option<Self>
        {
            assert!(self.eq_discriminant(value), "Mismatching discriminants.");

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

    //=======================================================================//
    // STRUCTS
    //
    //=======================================================================//

    /// The default properties associated with all [`Brush`]es.
    #[must_use]
    #[derive(Resource)]
    pub struct BrushProperties(pub Vec<(&'static str, Value)>);

    impl BrushProperties
    {
        /// Returns a new [`BrushProperties`].
        #[inline]
        pub fn new(values: impl IntoIterator<Item = (&'static str, &'static dyn ToValue)>) -> Self
        {
            Self(
                values
                    .into_iter()
                    .map(|(key, value)| (key, value.to_value()))
                    .collect()
            )
        }
    }

    //=======================================================================//

    /// The default properties associated with all [`ThingInstance`]s.
    #[must_use]
    #[derive(Resource)]
    pub struct ThingProperties(pub Vec<(&'static str, Value)>);

    impl ThingProperties
    {
        /// Returns a new [`ThingProperties`].
        #[inline]
        pub fn new(values: impl IntoIterator<Item = (&'static str, &'static dyn ToValue)>) -> Self
        {
            Self(
                values
                    .into_iter()
                    .map(|(key, value)| (key, value.to_value()))
                    .collect()
            )
        }
    }

    //=======================================================================//

    impl Default for DefaultProperties
    {
        #[inline]
        fn default() -> Self { Self::new(Vec::<(&'static str, _)>::new()) }
    }

    impl std::fmt::Display for DefaultProperties
    {
        #[inline]
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
        {
            let mut properties = "[".to_string();
            let len = self.len();

            if len == 0
            {
                return write!(f, "{properties}]");
            }

            let mut iter = self.0.iter();

            for _ in 0..len - 1
            {
                let (k, v) = iter.next_value();
                properties.push_str(&format!("({k}, {v:?}) "));
            }

            let (k, v) = iter.next_value();
            properties.push_str(&format!("({k}, {v:?})]"));

            write!(f, "{properties}")
        }
    }

    impl PartialEq for DefaultProperties
    {
        #[inline]
        fn eq(&self, other: &Self) -> bool
        {
            if self.len() != other.len()
            {
                return false;
            }

            self.0.iter().all(|(k, v0)| {
                let v1 = return_if_none!(other.0.get(k), false);
                v0.eq_discriminant(v1) && v0 == v1
            })
        }
    }

    impl DefaultProperties
    {
        /// Returns a new [`DefaultProperties`] generated for the values contained in `values`.
        #[inline]
        pub fn new<T: ToString>(values: Vec<(T, Value)>) -> Self
        {
            let mut properties = hv_hash_map![];

            for (name, value) in values
            {
                properties.insert(name.to_string(), value);
            }
            properties.insert(COLLISION_LABEL.to_string(), true.to_value());

            let mut values = hv_vec![];
            let mut keys = hv_vec![];

            for (k, v) in &properties
            {
                values.push(v.clone());
                keys.push(k.clone());
            }

            let mut keys = keys.into_iter();
            let map = IndexedMap::new(values, |_| keys.next_value());
            Self(map, Properties(properties))
        }

        #[inline]
        pub fn insert_collision_property(&mut self)
        {
            *self =
                Self::new(self.0.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>());
        }

        /// Returns the amount of contained values.
        #[inline]
        #[must_use]
        pub fn len(&self) -> usize { self.0.len() }

        /// Returns a reference to the [`Value`] associated with `k`.
        #[inline]
        pub fn get(&self, k: &str) -> &Value { self.0.get(k).unwrap() }

        /// Returns an instance of [`Properties`] with default values.
        #[inline]
        pub fn instance(&self) -> Properties { self.1.clone() }

        /// Returns an iterator the the key-value pairs.
        #[inline]
        pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> { self.1 .0.iter() }

        /// Generates a [`PropertiesRefactor`] describing how the [`Properties`] created from `self`
        /// should be refactored to be compatible with `new`.
        #[inline]
        pub fn refactor<'a>(&self, new: &'a Self) -> PropertiesRefactor<'a>
        {
            let mut remove = hv_vec![];

            for (k, v) in &self.1 .0
            {
                if !new.1 .0.contains_key(k) || v.eq_discriminant(new.get(k))
                {
                    remove.push(k.clone());
                }
            }

            let mut insert = hv_vec![];

            for k in new.1 .0.keys()
            {
                if !self.1 .0.contains_key(k)
                {
                    insert.push(k.as_str());
                }
            }

            assert!(!remove.is_empty() || !insert.is_empty(), "Empty refactor.");

            PropertiesRefactor {
                remove,
                insert,
                default_properties: new
            }
        }
    }

    //=======================================================================//

    /// Information concerning how [`Properties`] instances should be refactored upon map file load.
    #[must_use]
    pub(in crate::map) struct PropertiesRefactor<'a>
    {
        /// The keys of the values to be removed.
        remove:             HvVec<String>,
        /// The keys of the values inside `default_properties` to be inserted.
        insert:             HvVec<&'a str>,
        /// A reference to the [`DefaultProperties`] upon which [`PropertiesRefactor`] is based.
        default_properties: &'a DefaultProperties
    }

    //=======================================================================//

    impl Default for Properties
    {
        #[inline]
        fn default() -> Self { Self(HvHashMap::default()) }
    }

    impl Properties
    {
        /// Consumes `self` and returns the underlying hashmap of values.
        #[inline]
        pub fn take(self) -> HvHashMap<String, Value> { self.0 }
    }

    impl Properties
    {
        #[inline]
        pub const fn from_parts(map: HvHashMap<String, Value>) -> Self { Self(map) }

        /// Returns the amount of contained values.
        #[inline]
        #[must_use]
        pub fn len(&self) -> usize { self.0.len() }

        /// Returns a reference to the [`Value`] associated with `k`.
        #[inline]
        pub fn get(&self, k: &str) -> &Value { self.0.get(k).unwrap() }

        /// Sets the [`Value`] associated with `k` to `value`.
        /// Returns the previous value if different.
        #[inline]
        pub fn set(&mut self, k: &str, value: &Value) -> Option<Value>
        {
            self.0.get_mut(k).unwrap().set(value)
        }

        /// Refactors `self` based on `refactor`.
        #[inline]
        pub fn refactor(&mut self, refactor: &PropertiesRefactor)
        {
            for k in &refactor.remove
            {
                _ = self.0.asserted_remove(k);
            }

            for k in &refactor.insert
            {
                self.0.asserted_insert((
                    (*k).to_string(),
                    refactor.default_properties.get(k).clone()
                ));
            }
        }
    }
}

#[cfg(feature = "ui")]
pub use ui_mod::*;
