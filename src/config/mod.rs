pub mod controls;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf}
};

use bevy::{
    app::{App, AppExit, Plugin},
    asset::Assets,
    ecs::{
        event::EventWriter,
        system::{Res, ResMut, Resource},
        world::{FromWorld, Mut, World}
    },
    sprite::ColorMaterial,
    state::state::OnEnter
};
use configparser::ini::Ini;
use hill_vacuum_shared::FILE_EXTENSION;
use is_executable::IsExecutable;

use self::controls::{bind::Bind, BindsKeyCodes};
use crate::{
    error_message,
    map::drawer::color::{Color, ColorResources},
    EditorState
};

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

/// The name of the config file.
const CONFIG_FILE_NAME: &str = "hill_vacuum.ini";
/// The ini section of the first boot warning.
const WARNING_SECTION: &str = "WARNING";
/// The ini field of the first boot warning.
const WARNING_FIELD: &str = "displayed";
/// The ini section of the open file key.
const OPEN_FILE_SECTION: &str = "OPEN_FILE";
/// The open file ini key.
const OPEN_FILE_FIELD: &str = "file";
/// The ini section of the exporter key.
const EXPORTER_SECTION: &str = "EXPORTER";
/// The exporter executable ini key.
const EXPORTER_FIELD: &str = "exporter";

//=======================================================================//
// TYPES
//
//=======================================================================//

/// Plugin in charge of loading and saving the config file.
pub(crate) struct ConfigPlugin;

impl Plugin for ConfigPlugin
{
    #[inline]
    fn build(&self, app: &mut App)
    {
        app.init_resource::<Config>()
            .init_resource::<IniConfig>()
            .add_systems(OnEnter(EditorState::ShutDown), save_config);
    }
}

//=======================================================================//

/// The opened file being edited, if any.
#[must_use]
#[derive(Clone, Default)]
pub(crate) struct OpenFile(Option<PathBuf>);

impl OpenFile
{
    /// Returns a new [`OpenFile`] from the `path`.
    #[inline]
    pub fn new(path: impl Into<String>) -> Self
    {
        let path = PathBuf::from(Into::<String>::into(path));
        assert!(
            path.extension().unwrap().to_str().unwrap() == FILE_EXTENSION,
            "Improper file load."
        );
        Self(path.into())
    }

    /// Clears the file path.
    #[inline]
    pub fn clear(&mut self) { self.0 = None; }

    /// Returns the file stem of the opened file, if any.
    #[inline]
    #[must_use]
    pub fn file_stem(&self) -> Option<&str>
    {
        self.0
            .as_ref()
            .map(|path| path.file_stem().unwrap().to_str().unwrap())
    }

    /// Returns the file path, if any.
    #[inline]
    #[must_use]
    pub const fn path(&self) -> Option<&PathBuf> { self.0.as_ref() }
}

//=======================================================================//

#[derive(Default, Resource)]
pub(crate) struct Config
{
    /// The keyboard binds.
    pub binds:             BindsKeyCodes,
    /// The file being edited.
    pub open_file:         OpenFile,
    /// The executable to export the map.
    pub exporter:          Option<PathBuf>,
    /// The user defined colors.
    pub colors:            ColorResources,
    /// Whever the first boot warning was displayed.
    pub warning_displayed: bool
}

//=======================================================================//

/// Wrapper of the ini config parser.
#[derive(Resource)]
pub(crate) struct IniConfig(Ini);

impl FromWorld for IniConfig
{
    /// Loads the config file, or created a new one if it does not exist.
    #[inline]
    #[must_use]
    fn from_world(world: &mut World) -> Self
    {
        if !Path::new(CONFIG_FILE_NAME).exists() && create_default_config_file().is_err()
        {
            error_message("Error saving the default config file");
        }

        let mut ini_config = Ini::new_cs();
        ini_config.load(CONFIG_FILE_NAME).unwrap();

        world.resource_scope(|world, mut materials: Mut<Assets<ColorMaterial>>| {
            let mut config = world.get_resource_mut::<Config>().unwrap();

            config.warning_displayed = ini_config
                .get(WARNING_SECTION, WARNING_FIELD)
                .unwrap_or("false".to_string())
                .parse()
                .unwrap_or_default();

            config.binds.load(&ini_config);

            if let Some(file) = ini_config.get(OPEN_FILE_SECTION, OPEN_FILE_FIELD)
            {
                if Path::new(&file).exists()
                {
                    config.open_file = OpenFile::new(file);
                }
            }

            if let Some(file) = ini_config.get(EXPORTER_SECTION, EXPORTER_FIELD)
            {
                let file = PathBuf::from(file);

                if file.exists() && file.is_executable()
                {
                    config.exporter = file.into();
                }
            }

            config.colors.load(&ini_config, &mut materials);
        });

        Self(ini_config)
    }
}

impl IniConfig
{
    #[inline]
    pub fn set(&mut self, section: &str, key: &str, value: Option<String>)
    {
        self.0.set(section, key, value);
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Creates a default config if there isn't one.
#[inline]
fn create_default_config_file() -> std::io::Result<()>
{
    // Write it to a newly created file.
    let mut file = File::create(CONFIG_FILE_NAME)?;

    let mut config = format!(
        "[{WARNING_SECTION}]\n{WARNING_FIELD}\n[{OPEN_FILE_SECTION}]\n{OPEN_FILE_FIELD}\\
         n[{EXPORTER_SECTION}]\n{EXPORTER_FIELD}\n"
    );
    config.push_str(&Bind::default_binds());
    config.push_str(&Color::default_colors());

    file.write_all(config.as_bytes())?;
    Ok(())
}

//=======================================================================//

/// Saves `config` to file.
#[allow(clippy::needless_pass_by_value)]
#[inline]
fn save_config(
    mut ini_config: ResMut<IniConfig>,
    config: Res<Config>,
    mut app_exit_events: EventWriter<AppExit>
)
{
    ini_config
        .0
        .set(WARNING_SECTION, WARNING_FIELD, config.warning_displayed.to_string().into());

    ini_config.0.set(
        OPEN_FILE_SECTION,
        OPEN_FILE_FIELD,
        config
            .open_file
            .path()
            .map(|path| path.as_os_str().to_str().unwrap().to_string())
    );

    ini_config.0.set(
        EXPORTER_SECTION,
        EXPORTER_FIELD,
        config
            .exporter
            .as_ref()
            .map(|path| path.as_os_str().to_str().unwrap().to_owned())
    );

    config.binds.save(&mut ini_config);
    config.colors.save(&mut ini_config);

    if ini_config.0.write(CONFIG_FILE_NAME).is_err()
    {
        error_message("Error while saving config file");
    }

    app_exit_events.send(AppExit::Success);
}
