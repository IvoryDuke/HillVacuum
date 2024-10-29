use glam::Vec2;
use serde::{Deserialize, Deserializer};

use crate::{utils::collections::HashMap, Group, Id, TextureSettings, Value};

#[must_use]
#[derive(Deserialize)]
pub(in crate::map) struct BrushViewer
{
    id:         Id,
    vertexes:   Vec<Vec2>,
    texture:    Option<TextureSettings>,
    group:      Group,
    properties: CompatHashMap
}

impl From<BrushViewer> for super::BrushViewer
{
    #[inline]
    fn from(value: BrushViewer) -> Self
    {
        let BrushViewer {
            id,
            vertexes,
            texture,
            group,
            properties
        } = value;

        Self {
            id,
            vertexes,
            texture,
            group,
            properties: properties.0
        }
    }
}

#[derive(Deserialize)]
pub(in crate::map) struct BrushDataViewer
{
    pub vertexes:   Vec<Vec2>,
    pub texture:    Option<TextureSettings>,
    pub group:      Group,
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
