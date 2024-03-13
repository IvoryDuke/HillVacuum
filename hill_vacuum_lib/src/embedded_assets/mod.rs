//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::Plugin;
use proc_macros::embedded_assets;

//=======================================================================//
// TYPES
//
//=======================================================================//

/// Plugin that loads the embedded assets.
pub struct EmbeddedPlugin;

impl Plugin for EmbeddedPlugin
{
    #[inline]
    fn build(&self, app: &mut bevy::prelude::App)
    {
        embedded_assets!();
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Returns the path of the embedded asset with name `file_name`.
#[inline]
#[must_use]
pub fn embedded_asset_path(file_name: &str) -> String
{
    const ROOT: &str = "embedded://hill_vacuum_lib/embedded_assets/";
    format!("{ROOT}{file_name}")
}
