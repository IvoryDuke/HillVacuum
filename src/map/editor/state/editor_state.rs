//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf}
};

use bevy::{
    asset::{AssetServer, Assets},
    input::{keyboard::KeyCode, ButtonInput},
    render::texture::Image
};
use bevy_egui::{egui, EguiUserTextures};
use glam::Vec2;
use hill_vacuum_proc_macros::{EnumFromUsize, EnumIter, EnumSize};
use hill_vacuum_shared::{return_if_none, NextValue, FILE_EXTENSION};
use is_executable::IsExecutable;

use super::{
    clipboard::{Clipboard, PropCamerasMut},
    core::{
        rotate_tool::RotateAngle,
        tool::{ChangeConditions, Tool}
    },
    edits_history::EditsHistory,
    grid::Grid,
    inputs_presses::InputsPresses,
    manager::EntitiesManager,
    ui::{Interaction, UiFocus}
};
use crate::{
    config::controls::{bind::Bind, BindsKeyCodes},
    error_message,
    map::{
        brush::Brush,
        drawer::{
            color::Color,
            drawing_resources::DrawingResources,
            file_animations,
            texture::DefaultAnimation,
            texture_loader::TextureLoadingProgress
        },
        editor::{
            state::{
                clipboard::prop::{Prop, PropViewer},
                core::{tool::ToolInterface, Core},
                read_default_properties,
                test_writer,
                ui::{Command, Ui}
            },
            AllDefaultProperties,
            DrawBundle,
            DrawBundleMapPreview,
            Placeholder,
            StateUpdateBundle,
            ToolUpdateBundle
        },
        properties::DefaultProperties,
        thing::{catalog::ThingsCatalog, Thing, ThingInstance},
        version_number,
        FileStructure,
        GridSettings,
        MapHeader,
        CONVERTED_FILE_APPENDIX,
        FILE_VERSION_NUMBER,
        UPGRADE_WARNING
    },
    utils::{
        collections::hv_vec,
        hull::Hull,
        misc::{next, prev, Camera, TakeValue, Toggle}
    },
    warning_message,
    EditorState,
    HardcodedActions,
    HvVec,
    NAME
};

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

/// The filter of the map file types.
const HV_FILTER_NAME: &str = "HV files (.hv)";
/// The filter of the animations files.
const ANIMATIONS_FILTER_NAME: &str = "Animations files (.anms)";
/// The filter of the props files.
const PROPS_FILTER_NAME: &str = "Props files (.prps)";
/// The animations file extension.
const ANIMATIONS_EXTENSION: &str = "anms";
/// The props file extension.
const PROPS_EXTENSION: &str = "prps";

//=======================================================================//
// MACROS
//
//=======================================================================//

/// A macro to choose what entity should be updated based on the value of [`TargetSwitch`] and
/// execute the relative piece of code.
macro_rules! edit_target {
    ($target_switch:expr, $entity_func:expr, $texture_func:expr) => {{
        use crate::map::editor::state::editor_state::TargetSwitch;

        #[allow(clippy::redundant_closure_call)]
        match $target_switch
        {
            TargetSwitch::Entity => $entity_func(false),
            TargetSwitch::Both => $entity_func(true),
            TargetSwitch::Texture => $texture_func
        }
    }};
}

pub(in crate::map::editor::state) use edit_target;

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// Determines whether the editor should edit entities, or the associaed textures, or both.
#[derive(Clone, Copy, Default, EnumIter, EnumSize, EnumFromUsize, PartialEq, Eq)]
pub(in crate::map::editor::state) enum TargetSwitch
{
    /// Edit entities.
    #[default]
    Entity,
    /// Edit both entities and textures.
    Both,
    /// Edit textures.
    Texture
}

impl std::fmt::Display for TargetSwitch
{
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        write!(f, "{}", self.tag())
    }
}

impl TargetSwitch
{
    /// Returns a string representation of `self`.
    #[inline]
    #[must_use]
    const fn tag(self) -> &'static str
    {
        match self
        {
            Self::Entity => "Entity",
            Self::Both => "Entity+Tex",
            Self::Texture => "Texture"
        }
    }

    /// Whether the value of `self` can be changed based on the number of entities and
    /// textures in the map.
    #[inline]
    #[must_use]
    fn can_cycle(core: &Core, manager: &EntitiesManager) -> bool
    {
        if core.entity_tool()
        {
            return manager.textured_amount() != 0;
        }

        manager.selected_textured_amount() != 0
    }

    /// Change the value of `self` to the next one as defined in the enum order.
    #[inline]
    fn cycle(&mut self, core: &Core, manager: &EntitiesManager, inputs: &InputsPresses)
    {
        if !Self::can_cycle(core, manager)
        {
            return;
        }

        if inputs.shift_pressed()
        {
            *self = Self::from(prev(*self as usize, Self::SIZE));
        }
        else
        {
            *self = Self::from(next(*self as usize, Self::SIZE));
        }
    }

    /// Resets the value of `self` to `TargetSwitch::Entity` if the map state does not allow texture
    /// editing.
    #[inline]
    fn update(&mut self, core: &Core, manager: &EntitiesManager) -> bool
    {
        if !Self::can_cycle(core, manager)
        {
            *self = Self::Entity;
            return false;
        }

        true
    }

    /// Whether `self` allows editing of entities.
    #[inline]
    pub const fn entity_editing(self) -> bool { matches!(self, Self::Entity | Self::Both) }

    /// Whether `self` allows editing of textures.
    #[inline]
    pub const fn texture_editing(self) -> bool { matches!(self, Self::Texture | Self::Both) }

    /// Draws an UI combobox that does not allow the value of `self` to be changed.
    #[inline]
    pub(in crate::map::editor::state) fn entity_ui(&mut self, ui: &mut egui::Ui)
    {
        ui.horizontal(|ui| {
            egui::ComboBox::from_label("")
                .selected_text(format!("{self}"))
                .show_ui(ui, |ui| {
                    ui.selectable_value(self, Self::Entity, "Polygon");
                });
        });
    }

    /// Draws an UI combobox that allows to change the value of `self`.
    #[inline]
    pub(in crate::map::editor::state) fn ui(&mut self, ui: &mut egui::Ui)
    {
        ui.horizontal(|ui| {
            egui::ComboBox::from_label("")
                .selected_text(format!("{self}"))
                .show_ui(ui, |ui| {
                    for t in TargetSwitch::iter()
                    {
                        ui.selectable_value(self, t, t.tag());
                    }
                });
        });
    }
}

//=======================================================================//

/// The point of the bounding box of a [`ThingInstance`] used as a reference for its spawning.
#[derive(Default, Clone, Copy, PartialEq, EnumIter, EnumFromUsize, EnumSize)]
pub(in crate::map::editor::state) enum ThingPivot
{
    /// Top left.
    TopLeft,
    /// Top center.
    TopCenter,
    /// Top right.
    TopRight,
    /// Center left.
    CenterLeft,
    /// Center.
    #[default]
    Center,
    /// Center right.
    CenterRight,
    /// Bottom left.
    BottomLeft,
    /// Bottom center.
    BottomCenter,
    /// Bottom right.
    BottomRight
}

impl std::fmt::Display for ThingPivot
{
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        write!(f, "{}", self.tag())
    }
}

impl ThingPivot
{
    /// A string representation of `self`.
    #[inline]
    #[must_use]
    const fn tag(self) -> &'static str
    {
        match self
        {
            Self::TopLeft => "Top Left",
            Self::TopCenter => "Top Center",
            Self::TopRight => "Top Right",
            Self::CenterLeft => "Center Left",
            Self::Center => "Center",
            Self::CenterRight => "Center Right",
            Self::BottomLeft => "Bottom Left",
            Self::BottomCenter => "Bottom Center",
            Self::BottomRight => "Bottom Right"
        }
    }

    /// The position where a [`ThingInstance`] should be spawned based on the value of `self`.
    #[inline]
    pub fn spawn_pos(&mut self, thing: &Thing, cursor_pos: Vec2) -> Vec2
    {
        let half_width = thing.width() / 2f32;
        let half_height = thing.height() / 2f32;

        let delta = match self
        {
            Self::TopLeft => Vec2::new(half_width, -half_height),
            Self::TopCenter => Vec2::new(0f32, -half_height),
            Self::TopRight => Vec2::new(-half_width, -half_height),
            Self::CenterLeft => Vec2::new(half_width, 0f32),
            Self::Center => Vec2::ZERO,
            Self::CenterRight => Vec2::new(-half_width, 0f32),
            Self::BottomLeft => Vec2::new(half_width, half_height),
            Self::BottomCenter => Vec2::new(0f32, half_height),
            Self::BottomRight => Vec2::new(-half_width, half_height)
        };

        cursor_pos + delta
    }

    /// Changes the value of `self` to the next one in the enum order.
    #[inline]
    pub fn next(&mut self) { *self = Self::from(next(*self as usize, Self::SIZE)); }

    /// Changes the value of `self` to the previous one in the enum order.
    #[inline]
    pub fn prev(&mut self) { *self = Self::from(prev(*self as usize, Self::SIZE)); }

    /// Draws an UI elements that allows to change the value of `self`.
    #[inline]
    pub fn ui(&mut self, strip: &mut egui_extras::Strip)
    {
        strip.cell(|ui| {
            ui.add(egui::Label::new("Pivot"));
        });

        strip.cell(|ui| {
            egui::ComboBox::from_label("")
                .selected_text(format!("{self}"))
                .show_ui(ui, |ui| {
                    for p in Self::iter()
                    {
                        ui.selectable_value(self, p, p.tag());
                    }
                });
        });
    }
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// A collection of settings used by various tools that need to remained store throughout the
/// application's execution.
#[derive(Clone, Copy)]
pub(in crate::map) struct ToolsSettings
{
    /// The current editing target (entities, textures, or both).
    target_switch: TargetSwitch,
    /// Whether the [`TargetSwitch`] can be changed in value.
    can_switch: bool,
    /// The resolution of the circle drawing tool (how many sides the circle has).
    pub(in crate::map::editor::state) circle_draw_resolution: u8,
    /// The minimum angle the entities can be rotated when using the rotate tool.
    pub(in crate::map::editor::state) rotate_angle: RotateAngle,
    /// Whether texture scrolling is enabled while editing the map.
    pub scroll_enabled: bool,
    /// Whether texture parallax is enabled while editing the map.
    pub parallax_enabled: bool,
    /// The spawn pivot of the [`ThingInstance`] used by the thing tool.
    pub(in crate::map::editor::state) thing_pivot: ThingPivot
}

impl Default for ToolsSettings
{
    #[inline]
    #[must_use]
    fn default() -> Self
    {
        Self {
            target_switch:          TargetSwitch::default(),
            can_switch:             false,
            circle_draw_resolution: 2,
            rotate_angle:           RotateAngle::default(),
            scroll_enabled:         true,
            parallax_enabled:       true,
            thing_pivot:            ThingPivot::default()
        }
    }
}

impl ToolsSettings
{
    /// Cycles the value of the [`TargetSwitch`], but only if the current tool has texture editing
    /// capabilities and there are no ongoing changes.
    #[inline]
    fn cycle_texture_editing(
        &mut self,
        core: &Core,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses
    )
    {
        if !core.texture_tool() || core.ongoing_multi_frame_change()
        {
            return;
        }

        self.target_switch.cycle(core, manager, inputs);
        manager.schedule_outline_update();
    }

    /// Updates the [`TargetSwitch`].
    #[inline]
    fn update(&mut self, core: &Core, manager: &mut EntitiesManager)
    {
        let prev = self.target_switch;
        self.can_switch = self.target_switch.update(core, manager);

        if prev != self.target_switch
        {
            manager.schedule_outline_update();
        }
    }

    /// Returns a copy of the [`TargetSwitch`].
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) const fn target_switch(&self) -> TargetSwitch
    {
        self.target_switch
    }

    /// Whether entities editing is enabled.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) const fn entity_editing(&self) -> bool
    {
        self.target_switch.entity_editing()
    }

    /// Whether texture editing is enabled.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) const fn texture_editing(&self) -> bool
    {
        self.target_switch.texture_editing()
    }

    /// Draws the UI [`TargetSwitch`] elements.
    #[inline]
    pub(in crate::map::editor::state) fn ui(&mut self, ui: &mut egui::Ui, enabled: bool)
    {
        ui.horizontal(|ui| {
            ui.add_enabled(enabled, egui::Label::new("Target"));

            if !self.can_switch
            {
                self.target_switch.entity_ui(ui);
                return;
            }

            self.target_switch.ui(ui);
        });
    }
}

//=======================================================================//

/// The state of the [`Editor`].
pub(in crate::map::editor) struct State
{
    /// The core of the editor.
    core:               Core,
    /// The retained settings of the tools.
    tools_settings:     ToolsSettings,
    /// The UI of the editor.
    ui:                 Ui,
    /// Whether tooltips should be shown (ex. coordinates of the vertexes).
    show_tooltips:      bool,
    /// Whether the cursor should be snapped to the grid.
    cursor_snap:        bool,
    /// Whether a grey semitransparent rectangle should be drawn on the map beneath the cursor.
    show_cursor:        bool,
    /// Whether the "clip" texture should be drawn on top of the brushes with collision enabled.
    show_collision:     bool,
    /// Whether textures are currently being reloaded.
    reloading_textures: bool
}

impl Placeholder for State
{
    #[inline]
    unsafe fn placeholder() -> Self
    {
        Self {
            core:               Core::default(),
            tools_settings:     ToolsSettings::default(),
            ui:                 Ui::placeholder(),
            show_tooltips:      true,
            cursor_snap:        true,
            show_cursor:        true,
            show_collision:     true,
            reloading_textures: false
        }
    }
}

impl State
{
    //==============================================================
    // New

    /// Creates a new [`State`].
    #[inline]
    pub fn new(
        asset_server: &AssetServer,
        images: &mut Assets<Image>,
        prop_cameras: &mut PropCamerasMut,
        user_textures: &mut EguiUserTextures,
        drawing_resources: &mut DrawingResources,
        things_catalog: &ThingsCatalog,
        default_properties: &mut AllDefaultProperties,
        file: Option<PathBuf>
    ) -> (Self, EntitiesManager, Clipboard, EditsHistory, Grid, Option<PathBuf>)
    {
        /// The [`State`] to default to in case of errors in the file load or if there is no file to
        /// load.
        #[inline]
        fn default(
            asset_server: &AssetServer,
            user_textures: &mut EguiUserTextures,
            brushes_default_properties: &DefaultProperties,
            things_default_properties: &DefaultProperties
        ) -> State
        {
            State {
                core:               Core::default(),
                ui:                 Ui::new(
                    asset_server,
                    user_textures,
                    brushes_default_properties,
                    things_default_properties
                ),
                tools_settings:     ToolsSettings::default(),
                show_tooltips:      true,
                cursor_snap:        true,
                show_cursor:        true,
                show_collision:     true,
                reloading_textures: false
            }
        }

        let file = return_if_none!(
            file,
            (
                default(
                    asset_server,
                    user_textures,
                    default_properties.brushes,
                    default_properties.things
                ),
                EntitiesManager::new(),
                Clipboard::new(),
                EditsHistory::default(),
                Grid::default(),
                None
            )
        );

        match Self::process_map_file(
            images,
            prop_cameras,
            user_textures,
            file,
            drawing_resources,
            things_catalog,
            default_properties
        )
        {
            Ok((mut manager, clipboard, grid, path)) =>
            {
                let state = Self {
                    core:               Core::default(),
                    ui:                 Ui::new(
                        asset_server,
                        user_textures,
                        default_properties.map_brushes,
                        default_properties.map_things
                    ),
                    tools_settings:     ToolsSettings::default(),
                    show_tooltips:      true,
                    cursor_snap:        true,
                    show_cursor:        true,
                    show_collision:     true,
                    reloading_textures: false
                };

                manager.finish_things_reload(things_catalog);
                manager.finish_textures_reload(drawing_resources, &grid);

                (state, manager, clipboard, EditsHistory::default(), grid, path.into())
            },
            Err(err) =>
            {
                error_message(err);

                (
                    default(
                        asset_server,
                        user_textures,
                        default_properties.brushes,
                        default_properties.things
                    ),
                    EntitiesManager::new(),
                    Clipboard::new(),
                    EditsHistory::default(),
                    Grid::default(),
                    None
                )
            }
        }
    }

    //==============================================================
    // Info

    /// Whether the cursor should be snapped to the grid.
    #[inline]
    #[must_use]
    pub const fn cursor_snap(&self) -> bool { self.cursor_snap }

    /// Returns a reference to the tools' stored settings.
    #[inline]
    #[must_use]
    pub const fn tools_settings(&self) -> &ToolsSettings { &self.tools_settings }

    /// Whether map preview mode is enabled.
    #[inline]
    #[must_use]
    pub const fn map_preview(&self) -> bool { self.core.map_preview() }

    /// Whether the brushes collision overlay should be drawn.
    #[inline]
    #[must_use]
    pub const fn show_collision_overlay(&self) -> bool { self.show_collision }

    #[inline]
    #[must_use]
    pub const fn show_tooltips(&self) -> bool { self.show_tooltips }

    /// Checks whether any hardcoded keyboard input was pressed and executes the necessary piece of
    /// code. Returns true if that was the case.
    #[inline]
    #[must_use]
    fn hardcoded_key_inputs(&mut self, bundle: &mut StateUpdateBundle) -> bool
    {
        if HardcodedActions::New.pressed(bundle.key_inputs)
        {
            if !self.core.save_available()
            {
                return false;
            }

            if let Err(err) = self.new_file(bundle)
            {
                error_message(err);
            }

            return true;
        }

        if HardcodedActions::Save.pressed(bundle.key_inputs)
        {
            if !self.core.save_available()
            {
                return false;
            }

            if let Err(err) = Self::save(bundle, bundle.inputs.shift_pressed().then_some("Save as"))
            {
                error_message(err);
            }

            return true;
        }

        if HardcodedActions::Open.pressed(bundle.key_inputs)
        {
            if !self.core.save_available()
            {
                return false;
            }

            self.open(bundle);
            return true;
        }

        if HardcodedActions::Export.pressed(bundle.key_inputs)
        {
            if !self.core.save_available()
            {
                return false;
            }

            Self::export(bundle);
            return true;
        }

        if HardcodedActions::SelectAll.pressed(bundle.key_inputs) &&
            self.core.select_all_available()
        {
            self.select_all(bundle);
            return true;
        }

        if HardcodedActions::Undo.pressed(bundle.key_inputs)
        {
            if !self.core.undo_redo_available()
            {
                return false;
            }

            self.undo(bundle);
            return true;
        }

        if HardcodedActions::Redo.pressed(bundle.key_inputs)
        {
            if !self.core.undo_redo_available()
            {
                return false;
            }

            self.redo(bundle);
            return true;
        }

        if !self.copy_paste_available()
        {
            return false;
        }

        if bundle.inputs.copy_just_pressed()
        {
            self.core.copy(bundle);
        }
        else if bundle.inputs.paste_just_pressed()
        {
            self.core.paste(bundle);
        }
        else if bundle.inputs.cut_just_pressed()
        {
            self.core.cut(bundle);
        }
        else if HardcodedActions::Duplicate.pressed(bundle.key_inputs)
        {
            self.duplicate(bundle);
        }
        else
        {
            return false;
        }

        true
    }

    #[inline]
    pub const fn is_ui_focused(&self) -> UiFocus { self.ui.is_focused() }

    //==============================================================
    // File

    /// Executes the file save file routine if there are unsaved changes and the user decides to
    /// save.
    ///
    /// `Ok(true)` -> properly saved or decided not to.
    ///
    /// `Ok(false)` -> user decided to cancel operation.
    ///
    /// `Err` -> error during save procedure.
    #[inline]
    fn unsaved_changes(
        bundle: &mut StateUpdateBundle,
        buttons: rfd::MessageButtons
    ) -> Result<bool, &'static str>
    {
        if Self::no_edits(bundle)
        {
            return Ok(true);
        }

        match rfd::MessageDialog::new()
            .set_buttons(buttons)
            .set_title(NAME)
            .set_description("There are unsaved changes, do you wish to save?")
            .show()
        {
            rfd::MessageDialogResult::Yes =>
            {
                match Self::save(bundle, None)
                {
                    Err(err) => Err(err),
                    Ok(()) => Ok(true)
                }
            },
            rfd::MessageDialogResult::No => Ok(true),
            rfd::MessageDialogResult::Cancel => Ok(false),
            _ => unreachable!()
        }
    }

    /// Creates a new file, initiates save procedure if the map currently being edited has unsaved
    /// edits.
    #[inline]
    fn new_file(&mut self, bundle: &mut StateUpdateBundle) -> Result<(), &'static str>
    {
        if !Self::unsaved_changes(bundle, rfd::MessageButtons::YesNoCancel)?
        {
            return Ok(());
        }

        self.core = Core::default();
        *bundle.manager = EntitiesManager::new();
        *bundle.clipboard = Clipboard::new();
        *bundle.edits_history = EditsHistory::default();
        *bundle.inputs = InputsPresses::default();
        *bundle.grid = Grid::default();
        bundle.config.open_file.clear(bundle.window);

        Ok(())
    }

    //==============================================================
    // Undo/Redo

    /// Executes the undo procedure.
    /// # Panics
    /// Panics if the operation is unavailable.
    #[inline]
    fn undo(&mut self, bundle: &mut StateUpdateBundle)
    {
        assert!(self.core.undo_redo_available(), "Undo is not available.");

        self.core.undo(
            bundle.drawing_resources,
            bundle.things_catalog,
            bundle.manager,
            bundle.edits_history,
            bundle.grid,
            &mut self.ui
        );
    }

    /// Executes the redo procedure.
    /// # Panics
    /// Panics if the operation is unavailable.
    #[inline]
    fn redo(&mut self, bundle: &mut StateUpdateBundle)
    {
        assert!(self.core.undo_redo_available(), "Redo is not available.");

        self.core.redo(
            bundle.drawing_resources,
            bundle.things_catalog,
            bundle.manager,
            bundle.edits_history,
            bundle.grid,
            &mut self.ui
        );
    }

    //==============================================================
    // Save

    /// Whether there are no unsaved changes.
    #[inline]
    #[must_use]
    fn no_edits(bundle: &StateUpdateBundle) -> bool
    {
        bundle.edits_history.no_unsaved_edits() &&
            !bundle.clipboard.props_changed() &&
            !bundle.drawing_resources.default_animations_changed() &&
            !bundle.manager.refactored_properties() &&
            !bundle.grid.changed()
    }

    /// Saves the map being edited. If the file has not being created yet user is asked to specify
    /// where it should be stored. If the file exists, if `save as` contains a value user is
    /// asked to specify in which new file the map should be saved. Otherwise the map is stored
    /// in the previously opened file.
    #[inline]
    fn save(
        bundle: &mut StateUpdateBundle,
        save_as: Option<&'static str>
    ) -> Result<(), &'static str>
    {
        use crate::map::{brush::BrushViewer, thing::ThingViewer};

        /// The target of the file save process.
        enum SaveTarget
        {
            /// No save.
            None,
            /// Save to new file.
            New(PathBuf),
            /// Save to open file.
            Opened
        }

        impl SaveTarget
        {
            /// Whether `self` represents a new file to create.
            #[inline]
            #[must_use]
            const fn is_new(&self) -> bool { matches!(self, Self::New(_)) }
        }

        /// The dialog window to save the file. Returns a [`SaveTarget`] describing the outcome.
        #[inline]
        #[must_use]
        fn save_as_dialog(title: &'static str) -> SaveTarget
        {
            let path = return_if_none!(
                rfd::FileDialog::new()
                    .set_title(title)
                    .add_filter(HV_FILTER_NAME, &[FILE_EXTENSION])
                    .set_directory(std::env::current_dir().unwrap())
                    .save_file(),
                SaveTarget::None
            );

            SaveTarget::New(check_path_extension(path, FILE_EXTENSION))
        }

        let target = match save_as
        {
            Some(msg) => save_as_dialog(msg),
            None =>
            {
                match bundle.config.open_file.path()
                {
                    Some(path) =>
                    {
                        if Path::new(&path).exists()
                        {
                            SaveTarget::Opened
                        }
                        else
                        {
                            SaveTarget::New(path.clone())
                        }
                    },
                    None => save_as_dialog("Save as")
                }
            },
        };

        if let SaveTarget::None = target
        {
            return Ok(());
        }

        let mut data = Vec::new();
        let mut writer = BufWriter::new(&mut data);

        for step in FileStructure::iter()
        {
            match step
            {
                FileStructure::Version =>
                {
                    test_writer!(FILE_VERSION_NUMBER, &mut writer, "Error saving version number.");
                },
                FileStructure::Header =>
                {
                    test_writer!(
                        &MapHeader {
                            brushes:    bundle.manager.brushes_amount(),
                            things:     bundle.manager.things_amount(),
                            animations: bundle.drawing_resources.animations_amount(),
                            props:      bundle.clipboard.props_amount()
                        },
                        &mut writer,
                        "Error saving file header"
                    );
                },
                FileStructure::Grid =>
                {
                    test_writer!(
                        &bundle.grid.settings(),
                        &mut writer,
                        "Error saving grid settings."
                    );
                },
                FileStructure::Animations =>
                {
                    bundle.drawing_resources.export_animations(&mut writer)?;
                },
                FileStructure::Properties =>
                {
                    test_writer!(
                        bundle.default_properties.map_brushes,
                        &mut writer,
                        "Error saving Brush default properties."
                    );
                    test_writer!(
                        bundle.default_properties.map_things,
                        &mut writer,
                        "Error saving Thing default properties."
                    );
                },
                FileStructure::Brushes =>
                {
                    for brush in bundle.manager.brushes().iter()
                    {
                        test_writer!(
                            &BrushViewer::from(brush.clone()),
                            &mut writer,
                            "Error saving brushes."
                        );
                    }
                },
                FileStructure::Things =>
                {
                    for thing in bundle.manager.things()
                    {
                        test_writer!(
                            &ThingViewer::from(thing.clone()),
                            &mut writer,
                            "Error saving things."
                        );
                    }
                },
                FileStructure::Props => bundle.clipboard.export_props(&mut writer)?
            }
        }

        drop(writer);

        let mut file = OpenOptions::new();
        let mut file = file.write(true);

        let path = match &target
        {
            SaveTarget::None => unreachable!(),
            SaveTarget::New(path) =>
            {
                file = file.create(true);
                path
            },
            SaveTarget::Opened => bundle.config.open_file.path().unwrap()
        };

        let file = match file.open(path)
        {
            Ok(file) => file,
            Err(_) =>
            {
                if target.is_new()
                {
                    _ = std::fs::remove_file(path);
                }

                return Err("Error opening file.");
            }
        };

        test_writer!(BufWriter::new(file).write_all(&data), "Error writing file.");

        if target.is_new()
        {
            bundle.config.open_file.update(path.clone(), bundle.window);
        }

        bundle.edits_history.reset_last_save_edit();
        bundle.clipboard.reset_props_changed();
        bundle.manager.reset_refactored_properties();
        bundle.drawing_resources.reset_default_animation_changed();
        bundle.grid.reset_changed();

        Ok(())
    }

    //==============================================================
    // Open

    /// Returns new [`EntitiesManager`] and [`Clipboard`] loading the content of `file`.
    /// Returns `Err` if the file could not be properly read.
    #[inline]
    fn process_map_file(
        images: &mut Assets<Image>,
        prop_cameras: &mut PropCamerasMut,
        user_textures: &mut EguiUserTextures,
        mut path: PathBuf,
        drawing_resources: &mut DrawingResources,
        things_catalog: &ThingsCatalog,
        default_properties: &mut AllDefaultProperties
    ) -> Result<(EntitiesManager, Clipboard, Grid, PathBuf), &'static str>
    {
        #[must_use]
        struct OldFileRead
        {
            header:             MapHeader,
            grid:               GridSettings,
            animations:         HvVec<DefaultAnimation>,
            default_properties: [DefaultProperties; 2],
            brushes:            HvVec<Brush>,
            things:             HvVec<ThingInstance>,
            props:              HvVec<Prop>
        }

        #[inline]
        fn convert_08(
            mut reader: BufReader<File>,
            things_catalog: &ThingsCatalog
        ) -> Result<OldFileRead, &'static str>
        {
            // Header.
            let header = ciborium::from_reader::<MapHeader, _>(&mut reader)
                .map_err(|_| "Error reading file header for conversion.")?;

            // Grid.
            let grid = ciborium::from_reader::<GridSettings, _>(&mut reader)
                .map_err(|_| "Error reading grid settings for conversion.")?;

            // Animations.
            let animations = file_animations(header.animations, &mut reader)
                .map_err(|_| "Error reading animations for conversion.")?;

            // Properties.
            let default_properties = read_default_properties(&mut reader)
                .map_err(|_| "Error reading default properties for conversion.")?;

            // Brushes.
            let mut brushes = hv_vec![];

            for _ in 0..header.brushes
            {
                brushes.push(Brush::from(
                    ciborium::from_reader::<crate::map::brush::BrushViewer, _>(&mut reader)
                        .map_err(|_| "Error reading brushes for conversion.")?
                ));
            }

            // Things.
            let mut things = hv_vec![];

            for _ in 0..header.things
            {
                things.push(ThingInstance::from((
                    ciborium::from_reader::<crate::map::thing::ThingViewer, _>(&mut reader)
                        .map_err(|_| "Error reading things for conversion.")?,
                    things_catalog
                )));
            }

            // Props.
            let mut props = hv_vec![];

            for _ in 0..header.props
            {
                props.push(Prop::from(
                    ciborium::from_reader::<
                        crate::map::editor::state::clipboard::compatibility::Prop,
                        _
                    >(&mut reader)
                    .map_err(|_| "Error reading props for conversion.")?
                ));
            }

            Ok(OldFileRead {
                header,
                grid,
                animations,
                default_properties,
                brushes,
                things,
                props
            })
        }

        #[inline]
        fn convert(
            version: &str,
            path: &mut PathBuf,
            reader: BufReader<File>,
            things_catalog: &ThingsCatalog,
            f: fn(BufReader<File>, &ThingsCatalog) -> Result<OldFileRead, &'static str>
        ) -> Result<BufReader<File>, &'static str>
        {
            let mut file_name = path.file_stem().unwrap().to_str().unwrap().to_string();
            file_name.push_str(CONVERTED_FILE_APPENDIX);

            warning_message(&format!(
                "This file appears to use the old file structure {version}, if it is valid it \
                 will now be converted to {file_name}."
            ));

            let OldFileRead {
                header,
                grid,
                animations,
                default_properties: [default_brush_properties, default_thing_properties],
                mut brushes,
                mut things,
                props
            } = f(reader, things_catalog)?;

            let default_brush_properties = default_brush_properties.with_brush_properties();
            let default_thing_properties = default_thing_properties.with_thing_properties();

            // Write to file.
            let mut data = Vec::new();
            let mut writer = BufWriter::new(&mut data);

            for step in FileStructure::iter()
            {
                match step
                {
                    FileStructure::Version =>
                    {
                        test_writer!(
                            FILE_VERSION_NUMBER,
                            &mut writer,
                            "Error converting version number."
                        );
                    },
                    FileStructure::Header =>
                    {
                        test_writer!(&header, &mut writer, "Error converting header.");
                    },
                    FileStructure::Grid =>
                    {
                        test_writer!(&grid, &mut writer, "Error converting grid settings.");
                    },
                    FileStructure::Animations =>
                    {
                        for anim in &animations
                        {
                            test_writer!(anim, &mut writer, "Error converting animations.");
                        }
                    },
                    FileStructure::Properties =>
                    {
                        test_writer!(
                            &default_brush_properties,
                            &mut writer,
                            "Error converting Brush default properties."
                        );
                        test_writer!(
                            &default_thing_properties,
                            &mut writer,
                            "Error converting Thing default properties."
                        );
                    },
                    FileStructure::Brushes =>
                    {
                        for brush in brushes
                            .take_value()
                            .into_iter()
                            .map(crate::map::brush::BrushViewer::from)
                        {
                            test_writer!(&brush, &mut writer, "Error converting brushes.");
                        }
                    },
                    FileStructure::Things =>
                    {
                        for thing in things
                            .take_value()
                            .into_iter()
                            .map(crate::map::thing::ThingViewer::from)
                        {
                            test_writer!(&thing, &mut writer, "Error converting things.");
                        }
                    },
                    FileStructure::Props =>
                    {
                        for prop in props.iter().cloned().map(PropViewer::from)
                        {
                            test_writer!(&prop, &mut writer, "Error converting props.");
                        }
                    }
                };
            }

            drop(writer);

            path.pop();
            path.push(file_name);

            test_writer!(
                BufWriter::new(
                    OpenOptions::new()
                        .create(true)
                        .write(true)
                        .truncate(true)
                        .open(&*path)
                        .unwrap()
                )
                .write_all(&data),
                "Error saving converted file."
            );

            // Return exported file.
            let mut file = BufReader::new(File::open(path).unwrap());
            _ = version_number(&mut file).as_str();
            Ok(file)
        }

        let mut reader = BufReader::new(File::open(&path).unwrap());
        let mut steps = FileStructure::iter();

        steps.next_value().assert(FileStructure::Version);
        let version_number = version_number(&mut reader);
        let version_number = version_number.as_str();

        let mut file = match version_number
        {
            "0.8" => convert(version_number, &mut path, reader, things_catalog, convert_08)?,
            FILE_VERSION_NUMBER => reader,
            _ => return Err(UPGRADE_WARNING)
        };

        steps.next_value().assert(FileStructure::Header);
        let header = ciborium::from_reader::<MapHeader, _>(&mut file)
            .map_err(|_| "Error reading file header.")?;

        steps.next_value().assert(FileStructure::Grid);
        let grid = Grid::new(
            ciborium::from_reader(&mut file).map_err(|_| "Error reading grid settings.")?
        );

        steps.next_value().assert(FileStructure::Animations);
        drawing_resources.reset_animations(header.animations, &mut file)?;
        drawing_resources.reset_default_animation_changed();

        let manager = EntitiesManager::from_file(
            &header,
            &mut file,
            drawing_resources,
            things_catalog,
            &grid,
            default_properties,
            &mut steps
        )?;

        steps.next_value().assert(FileStructure::Props);
        let mut clipboard = Clipboard::from_file(
            images,
            prop_cameras,
            user_textures,
            drawing_resources,
            things_catalog,
            &grid,
            &header,
            &mut file
        )?;
        clipboard.reset_props_changed();

        Ok((manager, clipboard, grid, path))
    }

    /// Opens a map file, unless the file cannot be properly read. If there are unsaved changes in
    /// the currently open map the save procedure is initiated.
    #[inline]
    fn open(&mut self, bundle: &mut StateUpdateBundle)
    {
        match Self::unsaved_changes(bundle, rfd::MessageButtons::YesNoCancel)
        {
            Ok(false) => return,
            Err(err) =>
            {
                error_message(err);
                return;
            },
            _ => ()
        };

        let file_to_open = return_if_none!(rfd::FileDialog::new()
            .set_title("Open")
            .add_filter(HV_FILTER_NAME, &[FILE_EXTENSION])
            .set_directory(std::env::current_dir().unwrap())
            .pick_file());

        match Self::process_map_file(
            bundle.images,
            bundle.prop_cameras,
            bundle.user_textures,
            file_to_open,
            bundle.drawing_resources,
            bundle.things_catalog,
            bundle.default_properties
        )
        {
            Ok((manager, clipboard, grid, path)) =>
            {
                *bundle.manager = manager;
                *bundle.clipboard = clipboard;
                *bundle.grid = grid;
                bundle.config.open_file.update(path, bundle.window);
            },
            Err(err) =>
            {
                error_message(err);
                return;
            }
        };

        self.core = Core::default();
        *bundle.inputs = InputsPresses::default();
        *bundle.edits_history = EditsHistory::default();
    }

    //==============================================================
    // Export

    /// Initiates the map export procedure if an exporter executable is specified.
    /// If there are unsaved changes in the currently open map the save procedure is initiated.
    #[inline]
    fn export(bundle: &mut StateUpdateBundle)
    {
        let file = match Self::unsaved_changes(bundle, rfd::MessageButtons::YesNoCancel)
        {
            Ok(false) => return,
            Err(err) =>
            {
                error_message(err);
                return;
            },
            Ok(true) => return_if_none!(bundle.config.open_file.path())
        };

        let exporter = return_if_none!(bundle.config.exporter.as_ref());

        if !exporter.exists() || !exporter.is_executable()
        {
            error_message("Exporter executable does not exist.");
            bundle.config.exporter = None;
            return;
        }

        if std::process::Command::new(exporter).arg(file).output().is_err()
        {
            error_message("Error exporting map.");
        }
    }

    //==============================================================
    // Select all

    /// Initiated the select all procedure.
    #[inline]
    fn select_all(&mut self, bundle: &mut StateUpdateBundle)
    {
        self.core.select_all(bundle, &self.tools_settings);
    }

    //==============================================================
    // Copy/Paste

    /// Whether copy paste is available.
    #[inline]
    #[must_use]
    fn copy_paste_available(&self) -> bool { self.core.copy_paste_available() }

    /// Initiates the duplicate procedure.
    #[inline]
    fn duplicate(&mut self, bundle: &mut StateUpdateBundle)
    {
        self.core.duplicate(bundle, Vec2::new(bundle.grid.size_f32(), 0f32));
    }

    //==============================================================
    // Update

    /// Updates `self`.
    #[inline]
    #[must_use]
    pub fn update(&mut self, bundle: &mut StateUpdateBundle) -> bool
    {
        if HardcodedActions::Quit.pressed(bundle.key_inputs)
        {
            Self::quit(bundle, rfd::MessageButtons::YesNoCancel);
        }

        // Reactive update to previous frame's changes.
        bundle.manager.update_tool_and_overall_values(
            bundle.drawing_resources,
            &mut self.core,
            &mut self.ui,
            bundle.grid,
            &mut self.tools_settings
        );

        // Update inputs.
        bundle.inputs.update(
            bundle.key_inputs,
            bundle.mouse_buttons,
            bundle.config,
            bundle.grid.size()
        );

        // Create UI.
        let tool_change_conditions = ChangeConditions::new(
            bundle.inputs,
            bundle.clipboard,
            &self.core,
            bundle.things_catalog,
            bundle.manager
        );

        let ui_interaction = self.ui.frame_start_update(
            bundle,
            &mut self.core,
            &mut self.tools_settings,
            &tool_change_conditions
        );

        if self.reloading_textures
        {
            return false;
        }

        if ui_interaction.hovered
        {
            bundle.inputs.left_mouse.clear();
        }

        if self.map_preview()
        {
            self.map_preview_update(bundle, &ui_interaction)
        }
        else
        {
            self.edit_update(bundle, &tool_change_conditions, &ui_interaction)
        }
    }

    /// Update cycle when the map is being edited.
    #[inline]
    #[must_use]
    fn edit_update(
        &mut self,
        bundle: &mut StateUpdateBundle,
        tool_change_conditions: &ChangeConditions,
        ui_interaction: &Interaction
    ) -> bool
    {
        /// Generates the procedure to read an export type file.
        macro_rules! import {
            ($extension:ident, $label:literal, $importer:expr) => {{ paste::paste! {
                let mut file = BufReader::new(
                    File::open(return_if_none!(
                        rfd::FileDialog::new()
                        .set_title(concat!("Export ", $label))
                        .add_filter([< $extension _FILTER_NAME >], &[[< $extension _EXTENSION >]])
                        .set_directory(std::env::current_dir().unwrap())
                        .pick_file(),
                        false
                    ))
                    .unwrap()
                );

                // Right now has no actual purpose.
                _ = version_number(&mut file);

                let len = match ciborium::from_reader(&mut file)
                {
                    Ok(len) => len,
                    Err(_) =>
                    {
                        error_message(concat!("Error reading ", $label, " file."));
                        return false;
                    }
                };

                if let Err(err) = ($importer)(&mut file, len)
                {
                    error_message(err);
                }
            }}};
        }

        /// Generates the procedure to save an export type file.
        macro_rules! export {
            ($extension:ident, $label:literal, $argument:ident, $source:expr) => {{ paste::paste! {
                let path = return_if_none!(
                    rfd::FileDialog::new()
                        .set_title(concat!("Export ", $label))
                        .add_filter([< $extension _FILTER_NAME >], &[[< $extension _EXTENSION >]])
                        .set_directory(std::env::current_dir().unwrap())
                        .save_file(),
                    false
                );

                let path = check_path_extension(path, [< $extension _EXTENSION >]);

                let mut data = Vec::<u8>::new();
                let mut writer = BufWriter::new(&mut data);

                if ciborium::ser::into_writer(
                    FILE_VERSION_NUMBER,
                    &mut writer
                ).is_err()
                {
                    error_message("Error writing version number.");
                    return false;
                }

                if ciborium::ser::into_writer(
                    &$source.[< $argument _amount >](),
                    &mut writer
                ).is_err()
                {
                    error_message(concat!("Error writing ", $label, " amount."));
                    return false;
                }

                if let Err(err) = $source.[< export_ $argument >](&mut writer)
                {
                    error_message(err);
                    return false;
                }

                drop(writer);
                let new_file = !path.exists();

                if new_file && File::create(&path).is_err()
                {
                    error_message(concat!("Error creating ", $label, " file."));
                    return false;
                }

                let mut file = match OpenOptions::new().write(true).open(&path)
                {
                    Ok(file) => BufWriter::new(file),
                    Err(_) =>
                    {
                        error_message(concat!("Error opening ", $label, " file."));

                        if new_file
                        {
                            _ = std::fs::remove_file(path);
                        }

                        return false;
                    }
                };

                if file.write_all(&data).is_err()
                {
                    if new_file
                    {
                        _ = std::fs::remove_file(&path);
                    }

                    error_message(concat!("Error writing ", $label, " file."));
                }
            }}};
        }

        bundle.clipboard.update(
            bundle.images,
            bundle.prop_cameras,
            bundle.user_textures,
            bundle.drawing_resources,
            bundle.grid
        );

        match ui_interaction.command
        {
            Command::None => (),
            Command::ChangeTool(tool) =>
            {
                self.change_tool(tool, bundle, tool_change_conditions);
            },
            Command::New =>
            {
                if let Err(err) = self.new_file(bundle)
                {
                    error_message(err);
                }
            },
            Command::Save =>
            {
                if let Err(err) = Self::save(bundle, None)
                {
                    error_message(err);
                }
            },
            Command::SaveAs =>
            {
                if let Err(err) = Self::save(bundle, "Save as".into())
                {
                    error_message(err);
                }
            },
            Command::Open => self.open(bundle),
            Command::Export => Self::export(bundle),
            Command::ImportAnimations =>
            {
                import!(ANIMATIONS, "animations", |file, len| {
                    bundle.drawing_resources.import_animations(len, file)
                });
            },
            Command::ExportAnimations =>
            {
                export!(ANIMATIONS, "animations", animations, bundle.drawing_resources);
            },
            Command::ImportProps =>
            {
                import!(PROPS, "props", |file, len| {
                    bundle.clipboard.import_props(
                        bundle.images,
                        bundle.prop_cameras,
                        bundle.user_textures,
                        bundle.drawing_resources,
                        bundle.things_catalog,
                        bundle.grid,
                        len,
                        file
                    )
                });
            },
            Command::ExportProps => export!(PROPS, "props", props, bundle.clipboard),
            Command::SelectAll => self.select_all(bundle),
            Command::Copy => self.core.copy(bundle),
            Command::Paste => self.core.paste(bundle),
            Command::Cut => self.core.cut(bundle),
            Command::Duplicate => self.duplicate(bundle),
            Command::Undo => self.undo(bundle),
            Command::Redo => self.redo(bundle),
            Command::ToggleGrid => Self::toggle_grid(bundle.grid),
            Command::IncreaseGridSize => Self::increase_grid_size(bundle),
            Command::DecreaseGridSize => Self::decrease_grid_size(bundle),
            Command::ShiftGrid => Self::shift_grid(bundle),
            Command::ToggleTooltips => self.toggle_tooltips(),
            Command::ToggleCursorSnap => self.toggle_cursor_snap(),
            Command::ToggleMapPreview => self.toggle_map_preview(bundle),
            Command::ToggleCollision => self.toggle_collision(),
            Command::ReloadTextures => self.start_texture_reload(bundle),
            Command::ReloadThings => Self::reload_things(bundle),
            Command::QuickZoom =>
            {
                if let Some(hull) = bundle.manager.selected_brushes_hull()
                {
                    bundle.camera.scale_viewport_to_hull(
                        bundle.window,
                        bundle.grid,
                        &hull,
                        bundle.grid.size_f32()
                    );
                }
            },
            Command::QuickSnap => self.quick_snap(bundle),
            Command::Quit =>
            {
                Self::quit(bundle, rfd::MessageButtons::YesNoCancel);
                return true;
            }
        };

        if !(ui_interaction.command.world_edit() || self.hardcoded_key_inputs(bundle))
        {
            if Bind::ToggleGrid.just_pressed(bundle.key_inputs, &bundle.config.binds)
            {
                Self::toggle_grid(bundle.grid);
            }
            else if Bind::IncreaseGridSize.just_pressed(bundle.key_inputs, &bundle.config.binds)
            {
                Self::increase_grid_size(bundle);
            }
            else if Bind::DecreaseGridSize.just_pressed(bundle.key_inputs, &bundle.config.binds)
            {
                Self::decrease_grid_size(bundle);
            }
            else if Bind::ShiftGrid.just_pressed(bundle.key_inputs, &bundle.config.binds)
            {
                Self::shift_grid(bundle);
            }
            else if Bind::ToggleCursorSnap.just_pressed(bundle.key_inputs, &bundle.config.binds)
            {
                self.toggle_cursor_snap();
            }
            else if Bind::ToggleTooltips.just_pressed(bundle.key_inputs, &bundle.config.binds)
            {
                self.toggle_tooltips();
            }
            else if Bind::ToggleCollision.just_pressed(bundle.key_inputs, &bundle.config.binds)
            {
                self.toggle_collision();
            }
            else if HardcodedActions::Fullscreen.pressed(bundle.key_inputs)
            {
                bundle.window.mode.toggle();
            }
            else if Bind::Snap.alt_just_pressed(bundle.key_inputs, &bundle.config.binds) &&
                Tool::Snap.change_conditions_met(tool_change_conditions)
            {
                self.quick_snap(bundle);
            }
            else if bundle.inputs.esc.just_pressed()
            {
                self.core.disable_subtool();
            }
            else if Bind::TextureEditor.alt_just_pressed(bundle.key_inputs, &bundle.config.binds)
            {
                self.tools_settings.cycle_texture_editing(
                    &self.core,
                    bundle.manager,
                    bundle.inputs
                );
            }
            else
            {
                // Update tool based on key presses.
                for tool in Tool::iter()
                {
                    if !tool.just_pressed(bundle.key_inputs, &bundle.config.binds)
                    {
                        continue;
                    }

                    if tool.change_conditions_met(tool_change_conditions)
                    {
                        self.change_tool(tool, bundle, tool_change_conditions);
                    }

                    break;
                }
            }
        }

        self.core.frame_start_update(bundle);
        self.tools_settings.update(&self.core, bundle.manager);
        let starts_with_star = bundle.window.title.starts_with('*');

        if Self::no_edits(bundle)
        {
            if starts_with_star
            {
                bundle.window.title.remove(0);
            }
        }
        else if !starts_with_star
        {
            bundle.window.title.insert(0, '*');
        }

        ui_interaction.hovered
    }

    /// Update cycle when the map is being previewed.
    #[inline]
    #[must_use]
    fn map_preview_update(
        &mut self,
        bundle: &mut StateUpdateBundle,
        ui_interaction: &Interaction
    ) -> bool
    {
        match ui_interaction.command
        {
            Command::ToggleMapPreview => self.toggle_map_preview(bundle),
            Command::ReloadTextures => self.start_texture_reload(bundle),
            Command::Quit =>
            {
                Self::quit(bundle, rfd::MessageButtons::YesNoCancel);
                return true;
            },
            _ => ()
        };

        if bundle.inputs.esc.just_pressed()
        {
            self.toggle_map_preview(bundle);
            bundle.inputs.esc.clear();
        }

        ui_interaction.hovered
    }

    /// Updates the active tool.
    #[inline]
    pub fn update_active_tool(&mut self, bundle: &mut ToolUpdateBundle)
    {
        if self.reloading_textures
        {
            return;
        }

        self.core.update(bundle, &mut self.tools_settings);
    }

    /// Changes the active tool.
    #[inline]
    fn change_tool(
        &mut self,
        tool: Tool,
        bundle: &mut StateUpdateBundle,
        tool_change_conditions: &ChangeConditions
    )
    {
        if self.map_preview()
        {
            return;
        }

        self.core
            .change_tool(tool, bundle, &self.tools_settings, tool_change_conditions);
    }

    /// Toggles the grid visibiity.
    #[inline]
    fn toggle_grid(grid: &mut Grid) { grid.visible.toggle(); }

    /// Increased the grid size.
    #[inline]
    fn increase_grid_size(bundle: &mut StateUpdateBundle)
    {
        bundle.grid.increase_size(bundle.manager);
    }

    /// Decreases the grid size.
    #[inline]
    fn decrease_grid_size(bundle: &mut StateUpdateBundle)
    {
        bundle.grid.decrease_size(bundle.manager);
    }

    /// Shifts the grid by half of its size, both vertically and horizontally.
    #[inline]
    fn shift_grid(bundle: &mut StateUpdateBundle) { bundle.grid.toggle_shift(bundle.manager); }

    /// Toggles the cursor grid snap.
    #[inline]
    fn toggle_cursor_snap(&mut self) { self.cursor_snap.toggle(); }

    /// Toggles the tooltips visibility (ex. vertexes coordinates).
    #[inline]
    fn toggle_tooltips(&mut self) { self.show_tooltips.toggle(); }

    /// Toggles the map preview mode.
    #[inline]
    fn toggle_map_preview(&mut self, bundle: &StateUpdateBundle)
    {
        self.core.toggle_map_preview(bundle);
    }

    /// Toggles the collision overlay.
    #[inline]
    fn toggle_collision(&mut self) { self.show_collision.toggle(); }

    /// Reloads the things.
    #[inline]
    fn reload_things(bundle: &mut StateUpdateBundle)
    {
        if let rfd::MessageDialogResult::No = rfd::MessageDialog::new()
            .set_buttons(rfd::MessageButtons::YesNo)
            .set_title("WARNING")
            .set_description(
                "Reloading the things will erase the history of all things edits and will set all \
                 things that will result out of bound to errors. Are you sure you wish to proceed?"
            )
            .show()
        {
            return;
        }

        bundle.things_catalog.reload_things();
        bundle.edits_history.purge_thing_edits();
        bundle.clipboard.reload_things(
            bundle.images,
            bundle.user_textures,
            bundle.prop_cameras,
            bundle.drawing_resources,
            bundle.things_catalog,
            bundle.grid
        );
        bundle.manager.finish_things_reload(bundle.things_catalog);
    }

    /// Starts the application shutdown procedure.
    #[inline]
    pub fn quit(bundle: &mut StateUpdateBundle, buttons: rfd::MessageButtons) -> bool
    {
        if let Ok(false) = Self::unsaved_changes(bundle, buttons)
        {
            return false;
        }

        bundle.next_editor_state.set(EditorState::ShutDown);
        true
    }

    /// Returns the [`Hull`] representing the rectangle encompassing all the selected entities if
    /// the quick zoom key combo was pressed.
    #[inline]
    #[must_use]
    pub fn quick_zoom_hull(
        key_inputs: &ButtonInput<KeyCode>,
        manager: &mut EntitiesManager,
        binds: &BindsKeyCodes
    ) -> Option<Hull>
    {
        if Bind::Zoom.alt_just_pressed(key_inputs, binds)
        {
            return manager.selected_entities_hull().unwrap().into();
        }

        None
    }

    /// Snaps the editable entities to the grid.
    #[inline]
    fn quick_snap(&mut self, bundle: &mut StateUpdateBundle)
    {
        self.core.quick_snap(bundle, &self.tools_settings);
    }

    //==============================================================
    // Texture reload

    /// Starts the texture reload procedure.
    #[inline]
    fn start_texture_reload(&mut self, bundle: &mut StateUpdateBundle)
    {
        if self.reloading_textures
        {
            return;
        }

        if let rfd::MessageDialogResult::No = rfd::MessageDialog::new()
            .set_buttons(rfd::MessageButtons::YesNo)
            .set_title("WARNING")
            .set_description(
                "Reloading the textures will erase the history of all texture edits and will set \
                 all the textures of brushes with associated sprites that will result out of \
                 bound to errors. Are you sure you wish to proceed?"
            )
            .show()
        {
            return;
        }

        self.reloading_textures = true;
        bundle.inputs.clear();
        bundle.next_tex_load.set(TextureLoadingProgress::Initiated);
    }

    /// Concludes the texture reload.
    #[inline]
    pub fn finish_textures_reload(
        &mut self,
        prop_cameras: &mut PropCamerasMut,
        images: &mut Assets<Image>,
        user_textures: &mut EguiUserTextures,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        clipboard: &mut Clipboard,
        edits_history: &mut EditsHistory,
        grid: &Grid
    )
    {
        assert!(self.reloading_textures.take_value(), "No ongoing texture reload.");

        edits_history.purge_texture_edits();
        clipboard.reload_textures(images, user_textures, prop_cameras, drawing_resources, grid);
        manager.finish_textures_reload(drawing_resources, grid);
        self.ui.update_overall_texture(drawing_resources, manager);
    }

    //==============================================================
    // Draw

    /// Draws the visible portion of the map.
    #[inline]
    pub fn draw(&mut self, bundle: &mut DrawBundle)
    {
        bundle.clipboard.draw_props_to_photograph(bundle);
        bundle.drawer.grid_lines(bundle.window, bundle.camera);
        self.core.draw_active_tool(bundle, &self.tools_settings);
        bundle.manager.draw_error_highlight(bundle.drawer, bundle.delta_time);

        if self.show_cursor
        {
            bundle
                .drawer
                .square_highlight(bundle.cursor.world_snapped(), Color::DefaultCursor);
        }

        self.ui.frame_end_update(bundle.drawer.egui_context());
    }

    /// Draws the map preview.
    #[inline]
    pub fn draw_map_preview(&mut self, bundle: &mut DrawBundleMapPreview)
    {
        self.core.draw_map_preview(bundle);
        self.ui.frame_end_update(bundle.egui_context);
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Adds `extension` to `path` if it doesn't already end with it.
#[inline]
#[must_use]
fn check_path_extension(path: PathBuf, extension: &'static str) -> PathBuf
{
    if let Some(ext) = path.extension()
    {
        if ext.eq_ignore_ascii_case(extension)
        {
            return path;
        }
    }

    let mut path = path.to_str().unwrap().to_string();
    path.push('.');
    path.push_str(extension);
    PathBuf::from(path)
}
