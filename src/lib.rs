#![forbid(clippy::enum_glob_use)]
#![allow(clippy::single_match_else)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::inline_always)]
#![allow(clippy::module_name_repetitions)]
#![warn(clippy::missing_assert_message)]
#![warn(clippy::missing_const_for_fn)]
// #![warn(clippy::missing_errors_doc)]
// #![warn(clippy::missing_panics_doc_)]
// #![warn(clippy::missing_docs_in_private_items)]
#![cfg_attr(feature = "arena_alloc", feature(allocator_api))]

mod config;
mod embedded_assets;
mod map;
mod utils;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::{
    a11y::AccessibilityPlugin,
    core_pipeline::CorePipelinePlugin,
    input::InputPlugin,
    prelude::*,
    render::{
        pipelined_rendering::PipelinedRenderingPlugin,
        texture::{ImageAddressMode, ImageSamplerDescriptor},
        RenderPlugin
    },
    sprite::SpritePlugin,
    time::TimePlugin,
    window::Cursor,
    winit::WinitPlugin
};
use config::ConfigPlugin;
use embedded_assets::EmbeddedPlugin;
use hill_vacuum_proc_macros::str_array;
use map::MapEditorPlugin;

//=======================================================================//
// EXPORTS
//
//=======================================================================//
pub use crate::map::{
    brush::{
        mover::{Motor, Mover},
        BrushViewer as Brush
    },
    containers::{HvHashMap, HvHashSet, HvVec},
    drawer::{
        animation::{Animation, Atlas, List},
        texture::{Sprite, TextureInterface, TextureSettings}
    },
    path::{
        nodes::{Movement, Node},
        Path
    },
    properties::{BrushProperties, ThingProperties, ToValue, Value},
    thing::{catalog::HardcodedThings, MapThing, Thing, ThingId, ThingViewer as ThingInstance},
    Exporter
};
pub use crate::utils::{hull::Hull, identifiers::Id};

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

/// The name of the application.
const NAME: &str = "HillVacuum";
/// The folder where the assets are stored.
const ASSETS_PATH: &str = "assets/";
str_array!(INDEXES, 128);
/// The rows of cameras used to take screenshots of the props placed around the map area.
const PROP_CAMERAS_ROWS: usize = 2;
/// The amount of prop screenshot taking cameras placed around the map.
const PROP_CAMERAS_AMOUNT: usize = 8 * (PROP_CAMERAS_ROWS * (PROP_CAMERAS_ROWS + 1)) / 2;

//=======================================================================//
// MACROS
//
//====================================================================

/// Loads the desired [`Thing`]s as an available resource coded into the executable.
/// # Example
/// ```
/// use hill_vacuum::{hardcoded_things, MapThing, Thing};
///
/// struct Test;
///
/// impl MapThing for Test
/// {
///     fn thing() -> Thing { Thing::new("test", 0, 32f32, 32f32, "test").unwrap() }
/// }
///
/// let mut app = bevy::prelude::App::new();
/// hardcoded_things!(app, Test);
/// ```
#[macro_export]
macro_rules! hardcoded_things {
    ($app:expr, $($thing:ident),+) => {{
        use hill_vacuum::MapThing;

        let mut hardcoded_things = hill_vacuum::HardcodedThings::new();
        $(hardcoded_things.push::<$thing>();)+
        $app.insert_resource(hardcoded_things);
    }}
}

//====================================================================

/// Inserts the default properties that will be associated to all [`Brush`]es.
/// # Example
/// ```
/// use hill_vacuum::{brush_properties, BrushProperties, Value};
///
/// let mut app = bevy::prelude::App::new();
/// brush_properties!(app, [("Tag", 0u8), ("Destructible", false)]);
/// ```
#[macro_export]
macro_rules! brush_properties {
    ($app:expr, [$(($key:literal, $value:literal)),+]) => {
        $app.insert_resource(hill_vacuum::BrushProperties::new([
            $(($key, &$value as &dyn hill_vacuum::ToValue)),+
        ]));
    }
}

//====================================================================

/// Inserts the default properties that will be associated to all [`ThingInstance`]s.
/// # Example
/// ```
/// use hill_vacuum::{thing_properties, BrushProperties, Value};
///
/// let mut app = bevy::prelude::App::new();
/// thing_properties!(app, [("Fire resistance", 1f32), ("Invisible", false)]);
/// ```
#[macro_export]
macro_rules! thing_properties {
    ($app:expr, [$(($key:literal, $value:literal)),+]) => {
        $app.insert_resource(hill_vacuum::ThingProperties::new([
            $(($key, &$value as &dyn hill_vacuum::ToValue)),+
        ].into_iter()));
    }
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The overall state of the application.
#[derive(States, Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
enum EditorState
{
    /// Boot.
    #[default]
    SplashScreen,
    /// Program running.
    Run,
    /// Shutdown procedure.
    ShutDown
}

//=======================================================================//

/// Actions with hardcoded key binds.
enum HardcodedActions
{
    /// New file.
    New,
    /// Save file.
    Save,
    /// Open file.
    Open,
    /// Export file.
    Export,
    /// Select all.
    SelectAll,
    /// Copy.
    Copy,
    /// Paste.
    Paste,
    /// Cut.
    Cut,
    /// Duplicate.
    Duplicate,
    /// Undo.
    Undo,
    /// Redo.
    Redo,
    /// Camera zoom in.
    ZoomIn,
    /// Camera zoom out.
    ZoomOut,
    /// Toggle fullscreen view.
    Fullscreen,
    /// Toggle the manual.
    ToggleManual,
    /// Quit.
    Quit
}

impl HardcodedActions
{
    /// A string representation of the key presses required to initiate the action.
    #[inline]
    #[must_use]
    pub const fn key_combo(self) -> &'static str
    {
        match self
        {
            Self::New => "Ctrl+N",
            Self::Save => "Ctrl+S",
            Self::Open => "Ctrl+O",
            Self::Export => "Ctrl+E",
            Self::SelectAll => "Ctrl+A",
            Self::Copy => "Ctrl+C",
            Self::Paste => "Ctrl+V",
            Self::Cut => "Ctrl+X",
            Self::Duplicate => "Ctrl+D",
            Self::Undo => "Ctrl+Z",
            Self::Redo => "Ctrl+Y",
            Self::ZoomIn => "Ctrl+Plus",
            Self::ZoomOut => "Ctrl+Minus",
            Self::Fullscreen => "Alt+Enter",
            Self::ToggleManual => "`",
            Self::Quit => "Ctrl+Q"
        }
    }

    /// Returns the [`Keycode`] associated to the action.
    #[inline]
    #[must_use]
    pub const fn key(self) -> KeyCode
    {
        match self
        {
            Self::New => KeyCode::KeyN,
            Self::Save => KeyCode::KeyS,
            Self::Open => KeyCode::KeyO,
            Self::Export => KeyCode::KeyE,
            Self::Fullscreen => KeyCode::Enter,
            Self::ToggleManual => KeyCode::Backquote,
            Self::SelectAll => KeyCode::KeyA,
            Self::Copy => KeyCode::KeyC,
            Self::Paste => KeyCode::KeyV,
            Self::Cut => KeyCode::KeyX,
            Self::Duplicate => KeyCode::KeyD,
            Self::Undo => KeyCode::KeyZ,
            Self::Redo => KeyCode::KeyY,
            Self::ZoomIn => KeyCode::NumpadAdd,
            Self::ZoomOut => KeyCode::Minus,
            Self::Quit => KeyCode::KeyQ
        }
    }

    /// Whether the action's keys were pressed.
    #[inline]
    #[must_use]
    pub fn pressed(self, key_inputs: &ButtonInput<KeyCode>) -> bool
    {
        match self
        {
            Self::Fullscreen =>
            {
                return (key_inputs.pressed(KeyCode::AltLeft) ||
                    key_inputs.pressed(KeyCode::AltRight)) &&
                    key_inputs.just_pressed(self.key())
            },
            Self::ToggleManual => return key_inputs.just_pressed(self.key()),
            _ => ()
        };

        if !(key_inputs.pressed(KeyCode::ControlLeft) || key_inputs.pressed(KeyCode::ControlRight))
        {
            return false;
        }

        key_inputs.just_pressed(self.key())
    }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The main plugin.
pub struct HillVacuumPlugin;

impl Plugin for HillVacuumPlugin
{
    #[inline]
    fn build(&self, app: &mut App)
    {
        app.add_plugins((
            AssetPlugin {
                file_path: ASSETS_PATH.to_owned(),
                processed_file_path: "processed_assets/".to_owned(),
                watch_for_changes_override: false.into(),
                mode: bevy::prelude::AssetMode::Unprocessed
            },
            AccessibilityPlugin,
            TaskPoolPlugin::default(),
            TypeRegistrationPlugin,
            FrameCountPlugin,
            TimePlugin,
            TransformPlugin,
            InputPlugin,
            WindowPlugin {
                primary_window: Some(Window {
                    cursor: Cursor {
                        icon: CursorIcon::Pointer,
                        ..Default::default()
                    },
                    title: NAME.into(),
                    position: WindowPosition::At((0, 0).into()),
                    resolution: (1920f32, 1080f32).into(),
                    resize_constraints: WindowResizeConstraints {
                        min_width: 640f32,
                        min_height: 480f32,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                ..Default::default()
            },
            WinitPlugin {
                run_on_any_thread: true
            },
            RenderPlugin::default(),
            ImagePlugin {
                default_sampler: ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::Repeat,
                    address_mode_v: ImageAddressMode::Repeat,
                    address_mode_w: ImageAddressMode::Repeat,
                    ..Default::default()
                }
            }
        ));

        #[cfg(not(target_arch = "wasm32"))]
        {
            app.add_plugins(PipelinedRenderingPlugin);
        }

        app.add_plugins((CorePipelinePlugin, SpritePlugin));

        #[cfg(feature = "debug")]
        {
            app.add_plugins(bevy::gizmos::GizmoPlugin);
        }

        app.add_plugins((EmbeddedPlugin, ConfigPlugin, MapEditorPlugin))
            .insert_state(EditorState::default())
            .insert_resource(Msaa::Sample4);
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// The error message showed on screen when issues arise.
#[inline]
fn error_message(error: &str)
{
    rfd::MessageDialog::new()
        .set_title("ERROR")
        .set_description(error)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}
