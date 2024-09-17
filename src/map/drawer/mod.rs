pub mod animation;
#[cfg(feature = "ui")]
pub(crate) mod color;
#[cfg(feature = "ui")]
pub(in crate::map) mod drawers;
#[cfg(feature = "ui")]
pub(in crate::map) mod drawing_resources;
#[cfg(feature = "ui")]
pub(in crate::map) mod overall_values;
pub mod texture;
#[cfg(feature = "ui")]
pub(in crate::map) mod texture_loader;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{fs::File, io::BufReader};

use texture::DefaultAnimation;

use crate::{utils::collections::hv_vec, HvVec};

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Returns the vector of [`DefaultAnimation`]s contained in `file`.
#[inline]
pub(in crate::map) fn file_animations(
    amount: usize,
    file: &mut BufReader<File>
) -> Result<HvVec<DefaultAnimation>, &'static str>
{
    let mut animations = hv_vec![];

    for _ in 0..amount
    {
        animations.push(ciborium::from_reader(&mut *file).map_err(|_| "Error loading animations")?);
    }

    Ok(animations)
}

//=======================================================================//
// UI
//
//=======================================================================//

#[cfg(feature = "ui")]
pub(in crate::map::drawer) type BevyColor = bevy::color::Color;
