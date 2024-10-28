#![doc = include_str!("../docs/crate_description.md")]
#![allow(clippy::single_match_else)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_transmute_annotations)]
#![allow(clippy::needless_doctest_main)]
#![allow(clippy::large_enum_variant)]
#![allow(clippy::ignored_unit_patterns)]
#![warn(clippy::missing_assert_message)]
#![warn(clippy::missing_const_for_fn)]
#![warn(clippy::missing_errors_doc)]
#![warn(clippy::missing_panics_doc)]
#![forbid(clippy::enum_glob_use)]
// #![forbid(clippy::wildcard_imports)]
// #![warn(clippy::missing_docs_in_private_items)]
#![cfg_attr(feature = "arena_alloc", feature(allocator_api))]

#[cfg(feature = "ui")]
mod config;
#[cfg(feature = "ui")]
mod embedded_assets;
mod map;
mod utils;

//=======================================================================//
// IMPORTS-EXPORTS
//
//=======================================================================//

pub use crate::{
    map::{
        brush::{group::GroupViewer as Group, BrushViewer as Brush},
        drawer::{
            animation::{Animation, Atlas, List, Timing},
            texture::{TextureInterface, TextureSettings}
        },
        path::nodes::{Movement, NodeViewer as Node},
        properties::value::Value,
        thing::{Thing, ThingId, ThingViewer as ThingInstance},
        Exporter
    },
    utils::{
        collections::{HvHashMap, HvHashSet, HvVec},
        identifiers::Id
    }
};

//=======================================================================//
// UI
//
//=======================================================================//

#[cfg(feature = "ui")]
hill_vacuum_proc_macros::str_array!(INDEXES, 128);

#[cfg(feature = "ui")]
pub(crate) mod ui_mod
{
    //=======================================================================//
    // IMPORTS
    //
    //=======================================================================//

    use std::{collections::HashMap, io::Write};

    pub use bevy;
    use bevy::{
        app::PluginGroup,
        asset::{AssetMode, AssetPlugin},
        diagnostic::DiagnosticsPlugin,
        hierarchy::HierarchyPlugin,
        input::keyboard::KeyCode,
        log::LogPlugin,
        render::texture::{ImageAddressMode, ImagePlugin, ImageSamplerDescriptor},
        state::{app::AppExtStates, state::States},
        window::{
            Cursor,
            CursorIcon,
            PresentMode,
            Window,
            WindowPlugin,
            WindowPosition,
            WindowResizeConstraints
        },
        DefaultPlugins
    };

    use crate::{
        config::ConfigPlugin,
        embedded_assets::EmbeddedPlugin,
        map::{
            properties::{BrushUserProperties, ThingUserProperties},
            thing::HardcodedThings,
            MapEditorPlugin
        },
        Value
    };
    #[allow(unused_imports)]
    use crate::{Brush, Thing, ThingInstance};

    //=======================================================================//
    // CONSTANTS
    //
    //=======================================================================//

    /// The name of the application.
    pub(crate) const NAME: &str = "HillVacuum";

    //=======================================================================//
    // ENUMS
    //
    //=======================================================================//

    /// The overall state of the application.
    #[derive(States, Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
    pub(crate) enum EditorState
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
    pub(crate) enum HardcodedActions
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
                Self::ToggleManual => "Ctrl+`",
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
        pub fn pressed(self, key_inputs: &bevy::input::ButtonInput<KeyCode>) -> bool
        {
            if matches!(self, Self::Fullscreen)
            {
                return (key_inputs.pressed(KeyCode::AltLeft) ||
                    key_inputs.pressed(KeyCode::AltRight)) &&
                    key_inputs.just_pressed(self.key());
            }

            (key_inputs.pressed(KeyCode::ControlLeft) || key_inputs.pressed(KeyCode::ControlRight)) &&
                key_inputs.just_pressed(self.key())
        }
    }

    //=======================================================================//
    // STRUCTS
    //
    //=======================================================================//

    #[must_use]
    /// The main plugin.
    pub struct HillVacuumPlugin
    {
        pub brush_properties: HashMap<&'static str, Value>,
        pub thing_properties: HashMap<&'static str, Value>,
        pub things:           Vec<Thing>
    }

    impl Default for HillVacuumPlugin
    {
        #[inline]
        fn default() -> Self
        {
            Self {
                brush_properties: HashMap::default(),
                thing_properties: HashMap::default(),
                things:           Vec::default()
            }
        }
    }

    impl bevy::app::Plugin for HillVacuumPlugin
    {
        #[inline]
        fn build(&self, app: &mut bevy::app::App)
        {
            std::panic::set_hook(Box::new(|panic_info| {
                std::fs::File::create("backtrace.log")
                    .unwrap()
                    .write_all(std::backtrace::Backtrace::force_capture().to_string().as_bytes())
                    .ok();

                let message = panic_info.payload();
                let message = message.downcast_ref::<String>().map_or_else(
                    || message.downcast_ref::<&str>().copied().unwrap_or_default(),
                    String::as_str
                );

                error_message(&format!(
                    "A fatal error has occurred: {message}\nThe backtrace has been logged to \
                     backtrace.log."
                ));
            }));

            let mut window = Window {
                cursor: Cursor {
                    icon: CursorIcon::Pointer,
                    ..Default::default()
                },
                title: NAME.into(),
                position: WindowPosition::At((0, 0).into()),
                resize_constraints: WindowResizeConstraints {
                    min_width: 640f32,
                    min_height: 480f32,
                    ..Default::default()
                },
                present_mode: PresentMode::AutoNoVsync,
                ..Default::default()
            };
            window.set_maximized(true);

            app.add_plugins(
                DefaultPlugins
                    .set(AssetPlugin {
                        file_path: "assets/".to_owned(),
                        processed_file_path: "processed_assets/".to_owned(),
                        watch_for_changes_override: false.into(),
                        mode: AssetMode::Unprocessed,
                        ..Default::default()
                    })
                    .set(ImagePlugin {
                        default_sampler: ImageSamplerDescriptor {
                            address_mode_u: ImageAddressMode::Repeat,
                            address_mode_v: ImageAddressMode::Repeat,
                            address_mode_w: ImageAddressMode::Repeat,
                            ..Default::default()
                        }
                    })
                    .set(WindowPlugin {
                        primary_window: Some(window),
                        ..Default::default()
                    })
                    .disable::<LogPlugin>()
                    .disable::<HierarchyPlugin>()
                    .disable::<DiagnosticsPlugin>()
            )
            .add_plugins((EmbeddedPlugin, ConfigPlugin, MapEditorPlugin))
            .insert_resource(BrushUserProperties(self.brush_properties.clone()))
            .insert_resource(ThingUserProperties(self.thing_properties.clone()))
            .insert_resource(HardcodedThings(self.things.clone()))
            .init_state::<EditorState>();
        }
    }

    //=======================================================================//
    // FUNCTIONS
    //
    //=======================================================================//

    /// The error message showed on screen when issues arise.
    #[inline]
    pub(crate) fn error_message(error: &str)
    {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("ERROR")
            .set_description(error)
            .show();
    }

    //=======================================================================//

    #[inline]
    pub(crate) fn warning_message(message: &str)
    {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title("WARNING")
            .set_description(message)
            .show();
    }
}

#[cfg(feature = "ui")]
pub use ui_mod::*;
