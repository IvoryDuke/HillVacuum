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

use bevy::utils::HashMap;
use texture::DefaultAnimation;

use crate::{utils::misc::AssertedInsertRemove, Animation};

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Returns the vector of [`DefaultAnimation`]s contained in `file`.
#[inline]
pub(in crate::map) fn file_animations(
    amount: usize,
    file: &mut BufReader<File>
) -> Result<HashMap<String, Animation>, &'static str>
{
    let mut animations = HashMap::new();

    for _ in 0..amount
    {
        let DefaultAnimation { texture, animation } =
            ciborium::from_reader(&mut *file).map_err(|_| "Error loading animations")?;
        animations.asserted_insert((texture, animation));
    }

    Ok(animations)
}

//=======================================================================//
// UI
//
//=======================================================================//

#[cfg(feature = "ui")]
mod ui_mod
{
    //=======================================================================//
    // TRAITS
    //
    //=======================================================================//

    use glam::UVec2;

    use crate::TextureSettings;

    pub(in crate::map) trait TextureSize
    {
        #[must_use]
        fn texture_size(&self, texture: &str, settings: &TextureSettings) -> UVec2;
    }

    //=======================================================================//
    // TYPES
    //
    //=======================================================================//

    pub(in crate::map::drawer) type BevyColor = bevy::color::Color;
}

#[cfg(feature = "ui")]
pub(in crate::map) use ui_mod::*;
