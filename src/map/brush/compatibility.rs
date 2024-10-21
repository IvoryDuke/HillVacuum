//=======================================================================//
// IMPORTS
//
//=======================================================================//

use glam::Vec2;
use serde::Deserialize;

use crate::{
    map::{
        path::{compatibility::Node, nodes::NodeViewer},
        Viewer
    },
    utils::collections::{hv_vec, Ids},
    HvHashMap,
    HvVec,
    Id,
    TextureSettings,
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
    None,
    Attachments(Ids),
    Path
    {
        path:             HvVec<Node>,
        attached_brushes: Ids
    },
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
            properties
        } = value;

        Self::from_viewer(crate::map::brush::BrushViewer {
            id,
            vertexes,
            texture,
            group: group.into(),
            properties
        })
    }
}
