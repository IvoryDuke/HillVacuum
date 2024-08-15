//=======================================================================//
// IMPORTS
//
//=======================================================================//

use glam::Vec2;
use serde::{Deserialize, Serialize};

use super::{ThingId, ThingInterface};
use crate::{map::properties::Properties, Hull, Id, Path};

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The data of [`ThingInstance`].
#[must_use]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::map::thing) struct ThingInstanceData
{
    /// The [`ThingId`] of the [`Thing`] it represents.
    pub thing:       ThingId,
    /// The position on the map.
    pub pos:         Vec2,
    /// The spawn angle of the [`Thing`] in the map.
    pub angle:       f32,
    /// The height its preview should be drawn.
    pub draw_height: i8,
    /// The bounding box.
    pub hull:        Hull,
    /// The path describing the [`ThingInstance`] movement, if any.
    pub path:        Option<Path>,
    /// The associated properties.
    pub properties:  Properties
}

//=======================================================================//

/// An instance of a [`Thing`] which can be placed in a map.
#[must_use]
#[derive(Clone, Serialize, Deserialize)]
pub(in crate::map) struct ThingInstance
{
    /// The id.
    pub(in crate::map::thing) id:   Id,
    /// All entity data.
    pub(in crate::map::thing) data: ThingInstanceData
}

impl ThingInterface for ThingInstance
{
    #[inline]
    fn thing(&self) -> ThingId { self.data.thing }

    #[inline]
    fn pos(&self) -> Vec2 { self.data.pos }

    #[inline]
    fn draw_height_f32(&self) -> f32 { f32::from(self.data.draw_height) }

    #[inline]
    fn angle_f32(&self) -> f32 { f32::from(self.data.angle) }
}
