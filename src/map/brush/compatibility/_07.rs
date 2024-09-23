//=======================================================================//
// IMPORTS
//
//=======================================================================//

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{Group, HvHashMap, HvVec, Id, Value};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

super::tex_settings_061_07!();

//=======================================================================//

#[must_use]
#[derive(Serialize, Deserialize)]
pub struct BrushViewer
{
    /// The [`Id`].
    pub id:         Id,
    /// The vertexes.
    pub vertexes:   HvVec<Vec2>,
    /// The texture.
    pub texture:    Option<TextureSettings>,
    /// The [`Mover`].
    pub group:      Group,
    /// Whether collision against the polygonal shape is enabled.
    pub collision:  bool,
    /// The associated properties.
    pub properties: HvHashMap<String, Value>
}
