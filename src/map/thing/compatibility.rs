//=======================================================================//
// IMPORTS
//
//=======================================================================//

use glam::Vec2;
use serde::{Deserialize, Serialize};

use super::ThingId;
use crate::{HvHashMap, HvVec, Id, Node, Value};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

#[must_use]
#[derive(Serialize, Deserialize)]
pub struct ThingViewer
{
    /// The unique id.
    pub id:          Id,
    /// The id of the [`Thing`].
    pub thing_id:    ThingId,
    /// The position of the center.
    pub pos:         Vec2,
    /// The angle.
    pub angle:       f32,
    /// The draw height.
    pub draw_height: f32,
    /// The optional associated path.
    pub path:        Option<HvVec<Node>>,
    /// The associated properties.
    pub properties:  HvHashMap<String, Value>
}
