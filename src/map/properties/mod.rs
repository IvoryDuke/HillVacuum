#[cfg(feature = "ui")]
pub mod compatibility;
pub mod value;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{fs::File, io::BufReader};

use serde::{Deserialize, Serialize};
use value::Value;

use super::indexed_map::IndexedMap;
use crate::HvHashMap;
#[allow(unused_imports)]
use crate::{Brush, ThingInstance};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The default properties to be associated with the [`Brush`]es.
#[must_use]
#[derive(Clone, Serialize, Deserialize)]
pub(in crate::map) struct DefaultBrushProperties
{
    user:     IndexedMap<String, Value>,
    instance: BrushProperties
}

//=======================================================================//

/// The default properties to be associated with the [`ThingInstance`]s.
#[must_use]
#[derive(Clone, Serialize, Deserialize)]
pub(in crate::map) struct DefaultThingProperties
{
    user:     IndexedMap<String, Value>,
    instance: ThingProperties
}

//=======================================================================//

/// Key-value pairs associated to a [`Brush`].
#[must_use]
#[derive(Clone, Serialize, Deserialize)]
pub(in crate::map) struct BrushProperties
{
    collision: Value,
    user:      HvHashMap<String, Value>
}

//=======================================================================//

/// Key-value pairs associated to a [`ThingInstance`].
#[must_use]
#[derive(Clone, Serialize, Deserialize)]
pub(in crate::map) struct ThingProperties
{
    angle:  Value,
    height: Value,
    user:   HvHashMap<String, Value>
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

#[inline]
pub(in crate::map) fn read_default_properties(
    file: &mut BufReader<File>
) -> Result<(DefaultBrushProperties, DefaultThingProperties), &'static str>
{
    Ok((
        ciborium::from_reader::<DefaultBrushProperties, _>(&mut *file)
            .map_err(|_| "Error reading Brush default properties")?,
        ciborium::from_reader::<DefaultThingProperties, _>(&mut *file)
            .map_err(|_| "Error reading Thing default properties")?
    ))
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

    use bevy::prelude::Resource;
    use hill_vacuum_shared::{return_if_none, NextValue};

    use crate::{
        map::{
            drawer::drawing_resources::DrawingResources,
            editor::state::grid::Grid,
            indexed_map::IndexedMap,
            properties::{
                value::{ToValue, Value},
                BrushProperties,
                DefaultBrushProperties,
                DefaultThingProperties,
                ThingProperties
            }
        },
        utils::{
            collections::{hv_hash_map, hv_vec},
            misc::AssertedInsertRemove
        },
        HvHashMap,
        HvVec
    };

    //=======================================================================//
    // MACROS
    //
    //=======================================================================//

    macro_rules! entity_properties {
        ($($entity:ident, $entity_str:literal, $entities_str:literal, $len:literal, $(($property:ident, $property_name:ident, $default:expr)),+),+) => { paste::paste! { $(
            #[doc = concat!("The default properties associated with all ", $entities_str)]
            #[must_use]
            #[derive(Resource)]
            pub struct [< $entity UserProperties >](pub Vec<(&'static str, Value)>);

            impl [< $entity UserProperties >]
            {
                #[doc = concat!("Returns a new [`", $entity_str, "UserProperties`].")]
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

            impl Default for [< Default $entity Properties >]
            {
                #[inline]
                fn default() -> Self
                {
                    Self {
                        user:      IndexedMap::default(),
                        instance:  [< $entity Properties >]::from_parts(hv_hash_map![])
                    }
                }
            }

            impl std::fmt::Display for [< Default $entity Properties >]
            {
                #[inline]
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
                {
                    #[inline]
                    #[must_use]
                    fn format(k: &str, v: &Value) -> String { format!("{k}: {v:?}") }

                    let mut properties = String::new();
                    $(
                        properties.push_str(&format($property, &$default));
                        properties.push_str(",\n");
                    )+

                    let len = self.user_len();

                    if len == 0
                    {
                        return write!(f, "{properties}");
                    }

                    let mut iter = self.user.iter();

                    for _ in 0..len - 1
                    {
                        let (k, v) = iter.next_value();
                        properties.push_str(&format(k, v));
                        properties.push_str(",\n");
                    }

                    let (k, v) = iter.next_value();
                    properties.push_str(&format(k, v));

                    write!(f, "{properties}")
                }
            }

            impl PartialEq for [< Default $entity Properties >]
            {
                #[inline]
                fn eq(&self, other: &Self) -> bool
                {
                    if self.user_len() != other.user_len()
                    {
                        return false;
                    }

                    self.user.iter().all(|(k, v0)| {
                        let v1 = return_if_none!(other.user.get(k), false);
                        v0.eq_discriminant(v1) && v0 == v1
                    })
                }
            }

            impl DefaultProperties for [< Default $entity Properties >]
            {
                #[inline]
                fn refactor<'a>(&self, new: &'a Self)
                    -> PropertiesRefactor<'a, Self>
                {
                    let mut remove = hv_vec![];

                    for (k, v) in self.user.iter()
                    {
                        if !new.contains(k) || !v.eq_discriminant(new.get(k))
                        {
                            remove.push(k.clone());
                        }
                    }

                    let mut insert = hv_vec![];

                    for (k, v) in new.user.iter()
                    {
                        if !self.contains(k) || !v.eq_discriminant(self.get(k))
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

                #[inline]
                fn len(&self) -> usize { self.instance.len() }

                #[inline]
                fn get(&self, k: &str) -> &Value
                {
                    $(
                        if k == $property
                        {
                            return &$default;
                        }
                    )+

                    self.user.get(k).unwrap()
                }

                #[inline]
                fn iter(&self) -> impl Iterator<Item = (&str, &Value)> { self.instance.iter() }
            }

            impl [< Default $entity Properties >]
            {
                #[inline]
                pub fn new<I, T>(values: I) -> Self
                where
                    I: IntoIterator<Item = (T, Value)>,
                    T: ToString
                {
                    let mut properties = hv_hash_map![];

                    for (name, value) in values
                    {
                        properties.insert(name.to_string(), value);
                    }

                    $(_ = properties.remove($property);)+

                    let mut properties = hv_vec![collect; properties];
                    properties.sort_by(|a, b| a.0.cmp(&b.0));

                    let mut values = hv_vec![];
                    let mut keys = hv_vec![];

                    for (k, v) in &properties
                    {
                        values.push(v.clone());
                        keys.push(k.clone());
                    }

                    let mut keys = keys.into_iter();
                    let map = IndexedMap::new(values, |_| keys.next_value());

                    Self {
                        user:      map,
                        instance:  [< $entity Properties >]::from_parts(hv_hash_map![collect; properties])
                    }
                }

                /// Returns the amount of contained values.
                #[inline]
                #[must_use]
                pub fn user_len(&self) -> usize { self.user.len() }

                #[inline]
                pub fn contains(&self, k: &str) -> bool
                {
                    if $(k == $property) ||+
                    {
                        return true;
                    }

                    self.user.contains(k)
                }

                /// Returns an instance of [`BrushProperties`] with default values.
                #[inline]
                pub fn instance(&self) -> [< $entity Properties >] { self.instance.clone() }
            }

            //=======================================================================//

            impl Default for [< $entity Properties >]
            {
                #[inline]
                fn default() -> Self
                {
                    Self {
                        $($property_name: $default,)+
                        user:  HvHashMap::default()
                    }
                }
            }

            impl Properties for [< $entity Properties >]
            {
                #[inline]
                fn len(&self) -> usize { self.user.len() + $len }

                #[inline]
                fn get(&self, k: &str) -> &Value
                {
                    $(
                        if k == $property
                        {
                            return &self.$property_name;
                        }
                    )+

                    self.user.get(k).unwrap()
                }
            }

            impl [< $entity Properties >]
            {
                #[inline]
                pub fn from_parts(mut map: HvHashMap<String, Value>) -> Self
                {
                    Self {
                        $($property_name: map.remove($property).unwrap_or($default),)+
                        user: map
                    }
                }

                #[inline]
                pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)>
                {
                    [$(($property, self.get($property))),+]
                        .into_iter()
                        .chain(self.user.iter().map(|(name, value)| (name.as_str(), value)))
                }

                /// Sets the [`Value`] associated with `k` to `value`.
                /// Returns the previous value if different.
                #[inline]
                pub fn set(&mut self, k: &str, value: &Value) -> Option<Value>
                {
                    $(
                        if k == $property
                        {
                            return self.$property_name.set(value);
                        }
                    )+

                    self.user.get_mut(k).unwrap().set(value)
                }

                /// Consumes `self` and returns the underlying hashmap of values.
                #[inline]
                pub fn take(self) -> HvHashMap<String, Value>
                {
                    let mut map = self.user;
                    $(map.asserted_insert(($property.to_string(), self.$property_name.clone()));)+
                    map
                }

                /// Refactors `self` based on `refactor`.
                #[inline]
                pub fn refactor(&mut self, refactor: &PropertiesRefactor<[< Default $entity Properties >]>)
                {
                    for k in &refactor.remove
                    {
                        _ = self.user.asserted_remove(k);
                    }

                    for k in &refactor.insert
                    {
                        self.user.asserted_insert((
                            (*k).to_string(),
                            refactor.default_properties.get(k).clone()
                        ));
                    }
                }
            }
        )+ }};
    }

    //=======================================================================//
    // CONSTANTS
    //
    //=======================================================================//

    pub(in crate::map) const COLLISION_LABEL: &str = "collision";
    pub(in crate::map) const COLLISION_DEFAULT: Value = Value::Bool(true);
    pub(in crate::map) const ANGLE_LABEL: &str = "angle";
    pub(in crate::map) const ANGLE_DEFAULT: Value = Value::I16(0);
    pub(in crate::map) const HEIGHT_LABEL: &str = "height";
    pub(in crate::map) const HEIGHT_DEFAULT: Value = Value::I8(0);

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
            value: &mut Value
        );
    }

    //=======================================================================//

    pub(in crate::map) trait DefaultProperties
    where
        Self: Sized + std::fmt::Display + Clone + PartialEq
    {
        /// Generates a [`PropertiesRefactor`] describing how the [`Properties`] created from `self`
        /// should be refactored to be compatible with `new`.
        fn refactor<'a>(&self, new: &'a Self) -> PropertiesRefactor<'a, Self>;

        /// Returns the amount of contained values.
        #[must_use]
        fn len(&self) -> usize;

        /// Returns a reference to the [`Value`] associated with `k`.
        fn get(&self, k: &str) -> &Value;

        /// Returns an iterator the the key-value pairs.
        fn iter(&self) -> impl Iterator<Item = (&str, &Value)>;
    }

    //=======================================================================//

    pub(in crate::map) trait Properties
    {
        /// Returns the amount of contained values.
        #[must_use]
        fn len(&self) -> usize;

        /// Returns a reference to the [`Value`] associated with `k`.
        fn get(&self, k: &str) -> &Value;
    }

    //=======================================================================//
    // STRUCTS
    //
    //=======================================================================//

    entity_properties!(
        Brush,
        "Brush",
        "[`Brush`]es",
        1,
        (COLLISION_LABEL, collision, COLLISION_DEFAULT),
        Thing,
        "Thing",
        "[`ThingInstance`]s",
        2,
        (ANGLE_LABEL, angle, ANGLE_DEFAULT),
        (HEIGHT_LABEL, height, HEIGHT_DEFAULT)
    );

    //=======================================================================//

    /// Information concerning how [`Properties`] instances should be refactored upon map file load.
    #[must_use]
    pub(in crate::map) struct PropertiesRefactor<'a, T>
    {
        /// The keys of the values to be removed.
        remove:             HvVec<String>,
        /// The keys of the values inside `default_properties` to be inserted.
        insert:             HvVec<&'a str>,
        /// A reference to the [`DefaultProperties`] upon which [`PropertiesRefactor`] is based.
        default_properties: &'a T
    }
}

#[cfg(feature = "ui")]
pub use ui_mod::*;
