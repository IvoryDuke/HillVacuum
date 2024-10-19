//=======================================================================//
// IMPORTS
//
//=======================================================================//

use glam::Vec2;
use serde::Deserialize;

use super::ThingId;
use crate::{
    map::{
        path::nodes::NodeViewer,
        properties::{ANGLE_DEFAULT, ANGLE_LABEL, HEIGHT_DEFAULT, HEIGHT_LABEL},
        selectable_vector::SelectableVector,
        Viewer
    },
    utils::{collections::hv_vec, misc::AssertedInsertRemove},
    HvHashMap,
    HvVec,
    Id,
    Movement,
    ToValue,
    Value
};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

#[must_use]
#[derive(Deserialize)]
pub(in crate::map) struct Node
{
    pub selectable_vector: SelectableVector,
    pub movement:          Movement
}

//=======================================================================//

#[must_use]
#[derive(Deserialize)]
pub struct ThingViewer
{
    /// The unique id.
    pub id:         Id,
    /// The id of the [`Thing`].
    pub thing_id:   ThingId,
    /// The position of the center.
    pub pos:        Vec2,
    /// The optional associated path.
    pub path:       Option<HvVec<Node>>,
    /// The associated properties.
    pub properties: HvHashMap<String, Value>
}

impl From<ThingViewer> for crate::map::thing::ThingInstance
{
    #[inline]
    fn from(value: ThingViewer) -> Self
    {
        let ThingViewer {
            id,
            thing_id,
            pos,
            path,
            mut properties
        } = value;

        if !properties.contains_key(ANGLE_LABEL)
        {
            properties.asserted_insert((ANGLE_LABEL.to_string(), ANGLE_DEFAULT.to_value()));
        }

        if !properties.contains_key(HEIGHT_LABEL)
        {
            properties.asserted_insert((HEIGHT_LABEL.to_string(), HEIGHT_DEFAULT.to_value()));
        }

        Self::from_viewer(crate::map::thing::ThingViewer {
            id,
            thing_id,
            pos,
            path: path.map(|path| {
                hv_vec![collect; path.into_iter().map(|node| NodeViewer {
                    pos: node.selectable_vector.vec,
                    movement: node.movement
                })]
            }),
            properties
        })
    }
}
