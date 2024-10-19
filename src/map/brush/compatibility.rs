//=======================================================================//
// IMPORTS
//
//=======================================================================//

use glam::Vec2;
use serde::Deserialize;

use crate::{
    map::{
        path::nodes::NodeViewer,
        properties::{COLLISION_DEFAULT, COLLISION_LABEL},
        selectable_vector::SelectableVector,
        Viewer
    },
    utils::{
        collections::{hv_vec, Ids},
        misc::AssertedInsertRemove
    },
    HvHashMap,
    HvVec,
    Id,
    Movement,
    TextureSettings,
    ToValue,
    Value
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[must_use]
#[derive(Deserialize)]
pub(in crate::map) enum GroupViewer
{
    /// No group.
    None,
    /// Has some attached [`Brush`]es.
    Attachments(Ids),
    /// Has a path and maybe some attached [`Brush`]es.
    Path
    {
        /// The travel path.
        path:             HvVec<Node>,
        /// The attached [`Brush`]es.
        attached_brushes: Ids
    },
    /// Is attached to a [`Brush`].
    Attached(Id)
}

impl From<GroupViewer> for crate::map::brush::group::GroupViewer
{
    #[inline]
    fn from(value: GroupViewer) -> Self
    {
        match value
        {
            GroupViewer::None => Self::None,
            GroupViewer::Attachments(ids) => Self::Attachments(ids),
            GroupViewer::Path {
                path,
                attached_brushes
            } =>
            {
                Self::Path {
                    path: hv_vec![collect; path.into_iter().map(|node| NodeViewer {
                        pos: node.selectable_vector.vec,
                        movement: node.movement
                    })],
                    attached_brushes
                }
            },
            GroupViewer::Attached(id) => Self::Attached(id)
        }
    }
}

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
pub(in crate::map) struct BrushViewer
{
    pub id:         Id,
    pub vertexes:   HvVec<Vec2>,
    pub texture:    Option<TextureSettings>,
    pub group:      GroupViewer,
    pub properties: HvHashMap<String, Value>
}

impl From<BrushViewer> for crate::map::brush::Brush
{
    #[inline]
    fn from(value: BrushViewer) -> Self
    {
        let BrushViewer {
            id,
            vertexes,
            texture,
            group,
            mut properties
        } = value;

        if !properties.contains_key(COLLISION_LABEL)
        {
            properties.asserted_insert((COLLISION_LABEL.to_string(), COLLISION_DEFAULT.to_value()));
        }

        Self::from_viewer(crate::map::brush::BrushViewer {
            id,
            vertexes,
            texture,
            group: group.into(),
            properties
        })
    }
}
