//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::app::{App, Plugin};
use hill_vacuum_proc_macros::embedded_assets;

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// Plugin that loads the embedded assets.
pub(crate) struct EmbeddedPlugin;

impl Plugin for EmbeddedPlugin
{
    #[inline]
    fn build(&self, app: &mut App)
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
    const ROOT: &str = "embedded://hill_vacuum/embedded_assets/";
    format!("{ROOT}{file_name}")
}
