//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::ops::Range;

use glam::Vec2;
use serde::{Deserialize, Serialize};

use super::ClipboardData;
use crate::HvVec;

//=======================================================================//
// STRUCTS
//
//=======================================================================//

#[must_use]
#[derive(Serialize, Deserialize)]
pub(in crate::map::editor::state) struct Prop
{
    pub data:               HvVec<ClipboardData>,
    pub data_center:        Vec2,
    pub pivot:              Vec2,
    pub attachments_owners: usize,
    pub attached_range:     Range<usize>
}
