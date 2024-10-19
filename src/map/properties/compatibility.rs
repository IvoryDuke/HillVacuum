//=======================================================================//
// IMPORTS
//
//=======================================================================//

use serde::Deserialize;

use super::{DefaultBrushProperties, DefaultThingProperties};
use crate::{map::indexed_map::IndexedMap, HvHashMap, Value};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// Key-value pairs associated to an entity.
#[must_use]
#[derive(Clone, Deserialize)]
pub(in crate::map) struct Properties(pub HvHashMap<String, Value>);

//=======================================================================//

/// The default properties to be associated with certain entities.
#[allow(dead_code)]
#[must_use]
#[derive(Clone, Deserialize)]
pub(in crate::map) struct DefaultProperties(IndexedMap<String, Value>, Properties);

impl From<DefaultProperties> for DefaultBrushProperties
{
    #[inline]
    fn from(value: DefaultProperties) -> Self { Self::new(value.1 .0) }
}

impl From<DefaultProperties> for DefaultThingProperties
{
    #[inline]
    fn from(value: DefaultProperties) -> Self { Self::new(value.1 .0) }
}
