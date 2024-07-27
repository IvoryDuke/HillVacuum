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

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Returns the vector of [`DefaultAnimation`]s contained in `file`.
#[inline]
pub(in crate::map) fn file_animations(
    amount: usize,
    file: &mut BufReader<File>
) -> Result<Vec<DefaultAnimation>, &'static str>
{
    let mut animations = vec![];

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
mod ui_mod
{
    //=======================================================================//
    // IMPORTS
    //
    //=======================================================================//

    use glam::Vec2;

    use super::drawing_resources::DrawingResources;
    use crate::{
        map::{
            editor::state::grid::Grid,
            thing::{catalog::ThingsCatalog, ThingInterface}
        },
        utils::hull::EntityHull,
        Hull
    };

    //=======================================================================//
    // FUNCTIONS
    //
    //=======================================================================//

    #[inline]
    #[must_use]
    pub(in crate::map::drawer) fn thing_texture_hull<T: ThingInterface + EntityHull>(
        resources: &DrawingResources,
        catalog: &ThingsCatalog,
        grid: Grid,
        thing: &T
    ) -> Hull
    {
        let mut vxs = resources
            .texture_materials(resources.texture_or_error(catalog.texture(thing.thing())).name())
            .texture()
            .hull() +
            grid.transform_point(thing.pos());

        if grid.isometric()
        {
            vxs += Vec2::new(0f32, thing.hull().half_height());
        }

        vxs
    }
}

#[cfg(feature = "ui")]
use ui_mod::*;
