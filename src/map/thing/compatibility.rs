use bevy::utils::HashMap;
use glam::Vec2;
use serde::{Deserialize, Deserializer};

use super::ThingId;
use crate::{Id, Node, Value};

#[must_use]
#[derive(Deserialize)]
pub(in crate::map) struct ThingViewer
{
    id:         Id,
    thing_id:   ThingId,
    pos:        Vec2,
    path:       Option<Vec<Node>>,
    properties: CompatHashMap
}

impl From<ThingViewer> for super::ThingViewer
{
    #[inline]
    fn from(value: ThingViewer) -> Self
    {
        let ThingViewer {
            id,
            thing_id,
            pos,
            path,
            properties
        } = value;

        Self {
            id,
            thing_id,
            pos,
            path,
            properties: properties.0
        }
    }
}

#[must_use]
#[derive(Deserialize)]
pub(in crate::map) struct ThingInstanceDataViewer
{
    pub thing_id:   ThingId,
    pub pos:        Vec2,
    pub path:       Option<Vec<Node>>,
    pub properties: CompatHashMap
}

pub(in crate::map) struct CompatHashMap(pub HashMap<String, Value>);

impl<'de> Deserialize<'de> for CompatHashMap
{
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<CompatHashMap, D::Error>
    where
        D: Deserializer<'de>
    {
        Vec::<(String, Value)>::deserialize(deserializer).map(|vec| Self(vec.into_iter().collect()))
    }
}
