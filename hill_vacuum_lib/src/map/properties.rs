//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{mem::Discriminant, str::FromStr};

use bevy::ecs::system::Resource;
use serde::{Deserialize, Serialize};
use shared::{match_or_panic, return_if_none, NextValue};

use super::{containers::HvVec, AssertedInsertRemove};
use crate::map::{
    containers::{hv_hash_map, hv_vec, HvHashMap},
    indexed_map::IndexedMap
};

//=======================================================================//
// MACROS
//
//=======================================================================//

macro_rules! to_value {
    ($(($value:ident, $t:ty)),+) => {$(
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
// TRAITS
//
//=======================================================================//

pub trait ToValue
{
    /// Converts `self` to a [`Value`].
    fn to_value(&self) -> Value;
}

//=======================================================================//

pub(in crate::map) trait SetProperty
{
    fn set_property(&mut self, key: &str, value: &Value);
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[must_use]
#[derive(Clone, Serialize, Deserialize)]
/// A primitive value (+ [`String`]).
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

impl PartialEq for Value
{
    #[inline]
    fn eq(&self, other: &Self) -> bool
    {
        match (self, other)
        {
            (Self::Bool(l0), Self::Bool(r0)) => l0 == r0,
            (Self::U8(l0), Self::U8(r0)) => l0 == r0,
            (Self::U16(l0), Self::U16(r0)) => l0 == r0,
            (Self::U32(l0), Self::U32(r0)) => l0 == r0,
            (Self::U64(l0), Self::U64(r0)) => l0 == r0,
            (Self::U128(l0), Self::U128(r0)) => l0 == r0,
            (Self::I8(l0), Self::I8(r0)) => l0 == r0,
            (Self::I16(l0), Self::I16(r0)) => l0 == r0,
            (Self::I32(l0), Self::I32(r0)) => l0 == r0,
            (Self::I64(l0), Self::I64(r0)) => l0 == r0,
            (Self::I128(l0), Self::I128(r0)) => l0 == r0,
            (Self::F32(l0), Self::F32(r0)) => l0 == r0,
            (Self::F64(l0), Self::F64(r0)) => l0 == r0,
            (Self::String(l0), Self::String(r0)) => l0 == r0,
            _ => panic!("Tried comparing values that differ in type.")
        }
    }
}

to_value!(
    (Bool, bool),
    (U8, u8),
    (U16, u16),
    (U32, u32),
    (U64, u64),
    (U128, u128),
    (I8, i8),
    (I16, i16),
    (I32, i32),
    (I64, i64),
    (I128, i128),
    (F32, f32),
    (F64, f64),
    (String, &str)
);

impl std::fmt::Debug for Value
{
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        macro_rules! display {
            ($(($value:ident, $t:literal)),+) => {
                match self
                {
                    $(Self::$value(value) => write!(f, "{}: {value}", $t)),+
                }
            }
        }

        display!(
            (Bool, "bool"),
            (U8, "u8"),
            (U16, "u16"),
            (U32, "u32"),
            (U64, "u64"),
            (U128, "u128"),
            (I8, "i8"),
            (I16, "i16"),
            (I32, "i32"),
            (I64, "i64"),
            (I128, "i128"),
            (F32, "f32"),
            (F64, "f64"),
            (String, "String")
        )
    }
}

impl FromStr for Value
{
    type Err = ();

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> { Ok(Self::String(s.to_string())) }
}

impl std::fmt::Display for Value
{
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
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

impl Value
{
    pub(in crate::map) const BOOL_DISCRIMINANT: Discriminant<Self> =
        std::mem::discriminant(&Self::Bool(true));

    #[inline]
    #[must_use]
    pub(in crate::map) fn eq_discriminant(&self, other: &Self) -> bool
    {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }

    #[inline]
    pub(in crate::map) fn set(&mut self, value: &Self) -> Option<Self>
    {
        assert!(self.eq_discriminant(value));

        if *self == *value
        {
            return None;
        }

        std::mem::replace(self, value.clone()).into()
    }

    #[inline]
    #[must_use]
    pub(in crate::map) fn parse(&self, value: &Self) -> Option<Self>
    {
        if matches!(self, Self::String(_))
        {
            return value.clone().into();
        }

        let string = match_or_panic!(value, Self::String(s), s);

        macro_rules! convert {
            ($(($value:ident, $t:ty)),+) => {
                match value
                {
                    $(Self::$value(_) =>
                    {
                        <$t>::from_str(string)
                            .ok()
                            .map(|value| Self::$value(value))
                    },)+
                    _ => unreachable!()
                }
            };
        }

        convert!(
            (Bool, bool),
            (U8, u8),
            (U16, u16),
            (U32, u32),
            (U64, u64),
            (U128, u128),
            (I8, i8),
            (I16, i16),
            (I32, i32),
            (I64, i64),
            (I128, i128),
            (F32, f32),
            (F64, f64)
        )
    }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

#[must_use]
#[derive(Resource)]
/// The properties associated with all [`Brush`]es.
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

#[must_use]
#[derive(Resource)]
/// The properties associated with all [`Thing`]s.
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

#[must_use]
#[derive(Clone, Serialize, Deserialize)]
pub(in crate::map) struct DefaultProperties(IndexedMap<String, Value>, Properties);

impl Default for DefaultProperties
{
    #[inline]
    fn default() -> Self { Self(IndexedMap::default(), Properties::default()) }
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
            properties.push_str("]");
            return write!(f, "{properties}");
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
    #[inline]
    pub fn new(values: Vec<(&'static str, Value)>) -> Self
    {
        let mut properties = hv_hash_map![];

        for (name, value) in values
        {
            properties.insert(name.to_string(), value);
        }

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
    #[must_use]
    pub fn len(&self) -> usize { self.0.len() }

    #[inline]
    pub fn get(&self, k: &str) -> &Value { self.0.get(k).unwrap() }

    #[inline]
    pub fn instance(&self) -> Properties { self.1.clone() }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> { self.1 .0.iter() }

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

        assert!(!remove.is_empty() || !insert.is_empty());

        PropertiesRefactor {
            remove,
            insert,
            default_properties: new
        }
    }
}

//=======================================================================//

#[must_use]
pub(in crate::map) struct PropertiesRefactor<'a>
{
    remove:             HvVec<String>,
    insert:             HvVec<&'a str>,
    default_properties: &'a DefaultProperties
}

//=======================================================================//

#[must_use]
#[derive(Debug, Clone, Serialize, Deserialize)]

pub(in crate::map) struct Properties(HvHashMap<String, Value>);

impl Default for Properties
{
    #[inline]
    fn default() -> Self { Self(HvHashMap::default()) }
}

impl Properties
{
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize { self.0.len() }

    #[inline]
    pub fn get(&self, k: &str) -> &Value { self.0.get(k).unwrap() }

    #[inline]
    pub fn take(self) -> HvHashMap<String, Value> { self.0 }

    #[inline]
    pub fn set(&mut self, k: &str, value: &Value) -> Option<Value>
    {
        self.0.get_mut(k).unwrap().set(value)
    }

    #[inline]
    pub fn refactor(&mut self, refactor: &PropertiesRefactor)
    {
        for k in &refactor.remove
        {
            _ = self.0.asserted_remove(k);
        }

        for k in &refactor.insert
        {
            self.0
                .asserted_insert((k.to_string(), refactor.default_properties.get(k).clone()));
        }
    }
}
