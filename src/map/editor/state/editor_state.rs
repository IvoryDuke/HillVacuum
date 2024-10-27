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
    prelude::NextState,
    render::texture::Image,
    window::Window
};
use bevy_egui::{egui, EguiUserTextures};
use glam::{UVec2, Vec2};
use hill_vacuum_proc_macros::{EnumFromUsize, EnumIter, EnumSize};
use hill_vacuum_shared::{return_if_no_match, return_if_none, NextValue, FILE_EXTENSION};
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
    config::{
        controls::{bind::Bind, BindsKeyCodes},
        Config
    },
    error_message,
    map::{
        brush::Brush,
        drawer::{
            color::Color,
            drawing_resources::DrawingResources,
            file_animations,
            texture_loader::TextureLoadingProgress,
            TextureSize
        },
        editor::{
            state::{
                clipboard::prop::Prop,
                core::{tool::ToolInterface, Core},
                dialog_if_error,
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
        properties::{
            DefaultBrushProperties,
            DefaultThingProperties,
            EngineDefaultBrushProperties,
            EngineDefaultThingProperties
        },
        thing::{catalog::ThingsCatalog, Thing, ThingInstance},
        version_number,
        FileStructure,
        GridSettings,
        MapHeader,
        Viewer,
        FILE_VERSION,
        PREVIOUS_FILE_VERSION,
        UPGRADE_WARNING
    },
    utils::{
        collections::hv_hash_map,
        hull::Hull,
        misc::{next, prev, Camera, TakeValue, Toggle}
    },
    Animation,
    EditorState,
    HardcodedActions,
    HvHashMap,
    HvVec,
    TextureInterface,
    TextureSettings
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

    #[inline]
    pub(in crate::map::editor::state) fn edit_target<E, T, U, V>(
        self,
        bundle: &mut ToolUpdateBundle,
        extra: V,
        e: E,
        t: T
    ) -> U
    where
        E: FnOnce(&mut ToolUpdateBundle, bool, V) -> U,
        T: FnOnce(&mut ToolUpdateBundle, V) -> U
    {
        match self
        {
            Self::Entity => e(bundle, false, extra),
            Self::Both => e(bundle, true, extra),
            Self::Texture => t(bundle, extra)
        }
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

#[must_use]
struct FileRead
{
    animations: HvHashMap<String, Animation>,
    manager: EntitiesManager,
    map_default_brush_properties: DefaultBrushProperties,
    map_default_thing_properties: DefaultThingProperties,
    clipboard: Clipboard,
    grid: Grid,
    path: PathBuf
}

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
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        default_properties: &mut AllDefaultProperties,
        file: Option<PathBuf>
    ) -> (
        Self,
        HvHashMap<String, Animation>,
        EntitiesManager,
        Clipboard,
        EditsHistory,
        Grid,
        Option<PathBuf>
    )
    {
        /// The [`State`] to default to in case of errors in the file load or if there is no file to
        /// load.
        #[inline]
        fn default(
            asset_server: &AssetServer,
            user_textures: &mut EguiUserTextures,
            default_brush_properties: &DefaultBrushProperties,
            default_thing_properties: &DefaultThingProperties
        ) -> State
        {
            State {
                core:               Core::default(),
                ui:                 Ui::new(
                    asset_server,
                    user_textures,
                    default_brush_properties,
                    default_thing_properties
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
                    default_properties.map_brushes,
                    default_properties.map_things
                ),
                hv_hash_map![],
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
            default_properties.engine_brushes,
            default_properties.engine_things
        )
        {
            Ok(file_read) =>
            {
                *default_properties.map_brushes = file_read.map_default_brush_properties;
                *default_properties.map_things = file_read.map_default_thing_properties;

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

                (
                    state,
                    file_read.animations,
                    file_read.manager,
                    file_read.clipboard,
                    EditsHistory::default(),
                    file_read.grid,
                    file_read.path.into()
                )
            },
            Err(err) =>
            {
                error_message(err);

                (
                    default(
                        asset_server,
                        user_textures,
                        default_properties.map_brushes,
                        default_properties.map_things
                    ),
                    hv_hash_map![],
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

            dialog_if_error!(self.new_file(bundle));
            return true;
        }

        if HardcodedActions::Save.pressed(bundle.key_inputs)
        {
            if !self.core.save_available()
            {
                return false;
            }

            dialog_if_error!(Self::save(
                bundle.window,
                bundle.config,
                bundle.default_properties,
                bundle.drawing_resources,
                bundle.manager,
                bundle.clipboard,
                bundle.edits_history,
                bundle.grid,
                bundle.inputs.shift_pressed().then_some("Save as")
            ));
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
    /// Returns whether the procedure was not canceled.
    #[inline]
    fn save_unsaved_changes(
        window: &mut Window,
        config: &mut Config,
        default_properties: &AllDefaultProperties,
        drawing_resources: &mut DrawingResources,
        manager: &mut EntitiesManager,
        clipboard: &mut Clipboard,
        edits_history: &mut EditsHistory,
        grid: &mut Grid
    ) -> Result<bool, &'static str>
    {
        if Self::no_edits(drawing_resources, manager, clipboard, edits_history, grid)
        {
            return Ok(true);
        }

        match rfd::MessageDialog::new()
            .set_title("WARNING")
            .set_description("There are unsaved changes, do you wish to save?")
            .set_level(rfd::MessageLevel::Warning)
            .set_buttons(rfd::MessageButtons::YesNoCancel)
            .show()
        {
            rfd::MessageDialogResult::Yes =>
            {
                match Self::save(
                    window,
                    config,
                    default_properties,
                    drawing_resources,
                    manager,
                    clipboard,
                    edits_history,
                    grid,
                    None
                )
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
        if !Self::save_unsaved_changes(
            bundle.window,
            bundle.config,
            bundle.default_properties,
            bundle.drawing_resources,
            bundle.manager,
            bundle.clipboard,
            bundle.edits_history,
            bundle.grid
        )?
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
    fn no_edits(
        drawing_resources: &DrawingResources,
        manager: &EntitiesManager,
        clipboard: &Clipboard,
        edits_history: &EditsHistory,
        grid: &Grid
    ) -> bool
    {
        edits_history.no_unsaved_edits() &&
            !clipboard.props_changed() &&
            !drawing_resources.default_animations_changed() &&
            !manager.loaded_file_modified() &&
            !grid.changed()
    }

    #[inline]
    #[must_use]
    fn save_file(title: &str, filter_description: &str, filter_extension: &str) -> Option<PathBuf>
    {
        rfd::FileDialog::new()
            .set_directory(std::env::current_dir().unwrap())
            .set_title(title)
            .add_filter(filter_description, &[filter_extension])
            .save_file()
    }

    /// Saves the map being edited. If the file has not being created yet user is asked to specify
    /// where it should be stored. If the file exists, if `save as` contains a value user is
    /// asked to specify in which new file the map should be saved. Otherwise the map is stored
    /// in the previously opened file.
    #[inline]
    fn save(
        window: &mut Window,
        config: &mut Config,
        default_properties: &AllDefaultProperties,
        drawing_resources: &mut DrawingResources,
        manager: &mut EntitiesManager,
        clipboard: &mut Clipboard,
        edits_history: &mut EditsHistory,
        grid: &mut Grid,
        save_as: Option<&'static str>
    ) -> Result<(), &'static str>
    {
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
                State::save_file(title, HV_FILTER_NAME, FILE_EXTENSION),
                SaveTarget::None
            );

            SaveTarget::New(check_path_extension(path, FILE_EXTENSION))
        }

        let target = match save_as
        {
            Some(msg) => save_as_dialog(msg),
            None =>
            {
                match config.open_file.path()
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
                    test_writer!(FILE_VERSION, &mut writer, "Error saving version number.");
                },
                FileStructure::Header =>
                {
                    test_writer!(
                        &MapHeader {
                            brushes:    manager.brushes_amount(),
                            things:     manager.things_amount(),
                            animations: drawing_resources.animations_amount(),
                            props:      clipboard.props_amount()
                        },
                        &mut writer,
                        "Error saving file header"
                    );
                },
                FileStructure::Grid =>
                {
                    test_writer!(&grid.settings(), &mut writer, "Error saving grid settings.");
                },
                FileStructure::Animations =>
                {
                    drawing_resources.export_animations(&mut writer)?;
                },
                FileStructure::Properties =>
                {
                    test_writer!(
                        &default_properties.map_brushes.clone().to_viewer(),
                        &mut writer,
                        "Error saving Brush default properties."
                    );
                    test_writer!(
                        &default_properties.map_things.clone().to_viewer(),
                        &mut writer,
                        "Error saving Thing default properties."
                    );
                },
                FileStructure::Brushes =>
                {
                    for brush in manager.brushes().iter()
                    {
                        test_writer!(
                            &brush.clone().to_viewer(),
                            &mut writer,
                            "Error saving brushes."
                        );
                    }
                },
                FileStructure::Things =>
                {
                    for thing in manager.things()
                    {
                        test_writer!(
                            &thing.clone().to_viewer(),
                            &mut writer,
                            "Error saving things."
                        );
                    }
                },
                FileStructure::Props => clipboard.export_props(&mut writer)?
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
            SaveTarget::Opened => config.open_file.path().unwrap()
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
            config.open_file.update(path.clone(), window);
        }

        edits_history.reset_last_save_edit();
        clipboard.reset_props_changed();
        manager.reset_loaded_file_modified();
        drawing_resources.reset_default_animation_changed();
        grid.reset_changed();

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
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        engine_default_brush_properties: &EngineDefaultBrushProperties,
        engine_default_thing_properties: &EngineDefaultThingProperties
    ) -> Result<FileRead, &'static str>
    {
        #[must_use]
        struct OldFileRead
        {
            header:                   MapHeader,
            grid:                     GridSettings,
            animations:               HvHashMap<String, Animation>,
            default_brush_properties: DefaultBrushProperties,
            default_thing_properties: DefaultThingProperties,
            brushes:                  HvVec<Brush>,
            things:                   HvVec<ThingInstance>,
            props:                    HvVec<Prop>
        }

        #[must_use]
        struct DrawingResourcesTemp<'a>
        {
            resources:  &'a DrawingResources,
            animations: HvHashMap<String, Animation>
        }

        impl TextureSize for DrawingResourcesTemp<'_>
        {
            #[inline]
            fn texture_size(&self, texture: &str, settings: &TextureSettings) -> UVec2
            {
                let size = self.resources.texture_or_error(texture).size();

                if !settings.sprite()
                {
                    return size;
                }

                let animation = match settings.animation()
                {
                    Animation::None => return_if_none!(self.animations.get(texture), size),
                    anim => anim
                };

                return_if_no_match!(animation, Animation::Atlas(anim), anim, size).size(size)
            }
        }

        #[inline]
        fn convert_09(mut reader: BufReader<File>) -> Result<OldFileRead, &'static str>
        {
            unreachable!();

            // // Header.
            // let header = ciborium::from_reader::<MapHeader, _>(&mut reader)
            //     .map_err(|_| "Error reading file header for conversion.")?;

            // // Grid.
            // let grid = ciborium::from_reader::<GridSettings, _>(&mut reader)
            //     .map_err(|_| "Error reading grid settings for conversion.")?;

            // // Animations.
            // let animations = file_animations(header.animations, &mut reader)
            //     .map_err(|_| "Error reading animations for conversion.")?;

            // // Properties.
            // let default_brush_properties =
            //     DefaultBrushProperties::from(
            //         ciborium::from_reader::<
            //             crate::map::properties::compatibility::DefaultProperties,
            //             _
            //         >(&mut reader)
            //         .map_err(|_| "Error reading default brush properties for conversion.")?
            //     );

            // let default_thing_properties =
            //     DefaultThingProperties::from(
            //         ciborium::from_reader::<
            //             crate::map::properties::compatibility::DefaultProperties,
            //             _
            //         >(&mut reader)
            //         .map_err(|_| "Error reading default thing properties for conversion.")?
            //     );

            // // Brushes.
            // let mut brushes = hv_vec![];

            // for _ in 0..header.brushes
            // {
            //     brushes.push(Brush::from_viewer(
            //         ciborium::from_reader::<crate::map::brush::BrushViewer, _>(&mut reader)
            //             .map_err(|_| "Error reading brushes for conversion.")?
            //     ));
            // }

            // // Things.
            // let mut things = hv_vec![];

            // for _ in 0..header.things
            // {
            //     things.push(ThingInstance::from(
            //         ciborium::from_reader::<crate::map::thing::compatibility::ThingViewer, _>(
            //             &mut reader
            //         )
            //         .map_err(|_| "Error reading things for conversion.")?
            //     ));
            // }

            // Ok(OldFileRead {
            //     header,
            //     grid,
            //     animations,
            //     default_brush_properties,
            //     default_thing_properties,
            //     brushes,
            //     things,
            //     props: convert_08_props(&mut reader, header.props)?
            // })
        }

        #[inline]
        fn convert(
            version: &str,
            path: &mut PathBuf,
            reader: BufReader<File>,
            f: fn(BufReader<File>) -> Result<OldFileRead, &'static str>
        ) -> Result<BufReader<File>, &'static str>
        {
            unreachable!();

            // let mut file_name = path.file_stem().unwrap().to_str().unwrap().to_string();
            // file_name.push_str(CONVERTED_FILE_APPENDIX);

            // warning_message(&format!(
            //     "This file appears to use the old file structure {version}, if it is valid it \
            //      will now be converted to {file_name}."
            // ));

            // let OldFileRead {
            //     header,
            //     grid,
            //     mut animations,
            //     mut default_brush_properties,
            //     mut default_thing_properties,
            //     mut brushes,
            //     mut things,
            //     mut props
            // } = f(reader)?;

            // // Write to file.
            // let mut data = Vec::new();
            // let mut writer = BufWriter::new(&mut data);

            // for step in FileStructure::iter()
            // {
            //     match step
            //     {
            //         FileStructure::Version =>
            //         {
            //             test_writer!(FILE_VERSION, &mut writer, "Error converting version
            // number.");         },
            //         FileStructure::Header =>
            //         {
            //             test_writer!(&header, &mut writer, "Error converting header.");
            //         },
            //         FileStructure::Grid =>
            //         {
            //             test_writer!(&grid, &mut writer, "Error converting grid settings.");
            //         },
            //         FileStructure::Animations =>
            //         {
            //             for (texture, animation) in animations.take_value()
            //             {
            //                 test_writer!(
            //                     &DefaultAnimation { texture, animation },
            //                     &mut writer,
            //                     "Error converting animations."
            //                 );
            //             }
            //         },
            //         FileStructure::Properties =>
            //         {
            //             test_writer!(
            //                 &default_brush_properties.take_value().to_viewer(),
            //                 &mut writer,
            //                 "Error converting Brush default properties."
            //             );
            //             test_writer!(
            //                 &default_thing_properties.take_value().to_viewer(),
            //                 &mut writer,
            //                 "Error converting Thing default properties."
            //             );
            //         },
            //         FileStructure::Brushes =>
            //         {
            //             for brush in brushes
            //                 .take_value()
            //                 .into_iter()
            //                 .map(crate::map::brush::Brush::to_viewer)
            //             {
            //                 test_writer!(&brush, &mut writer, "Error converting brushes.");
            //             }
            //         },
            //         FileStructure::Things =>
            //         {
            //             for thing in things
            //                 .take_value()
            //                 .into_iter()
            //                 .map(crate::map::thing::ThingInstance::to_viewer)
            //             {
            //                 test_writer!(&thing, &mut writer, "Error converting things.");
            //             }
            //         },
            //         FileStructure::Props => save_imported_08_props(&mut writer,
            // props.take_value())?     };
            // }

            // drop(writer);

            // path.pop();
            // path.push(file_name);

            // test_writer!(
            //     BufWriter::new(
            //         OpenOptions::new()
            //             .create(true)
            //             .write(true)
            //             .truncate(true)
            //             .open(&*path)
            //             .unwrap()
            //     )
            //     .write_all(&data),
            //     "Error saving converted file."
            // );

            // // Return exported file.
            // let mut file = BufReader::new(File::open(path).unwrap());
            // _ = version_number(&mut file)?;
            // Ok(file)
        }

        let mut reader = BufReader::new(File::open(&path).unwrap());
        let mut steps = FileStructure::iter();

        steps.next_value().assert(FileStructure::Version);
        let version_number = version_number(&mut reader)?;
        let version_number = version_number.as_str();

        let mut file = match version_number
        {
            PREVIOUS_FILE_VERSION | FILE_VERSION => reader,
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

        let animations = file_animations(header.animations, &mut file)?;

        let drawing_resources = DrawingResourcesTemp {
            resources: drawing_resources,
            animations
        };

        let (manager, map_default_brush_properties, map_default_thing_properties) =
            EntitiesManager::from_file(
                &header,
                &mut file,
                &drawing_resources,
                things_catalog,
                &grid,
                engine_default_brush_properties,
                engine_default_thing_properties,
                &mut steps
            )?;

        steps.next_value().assert(FileStructure::Props);
        let mut clipboard = Clipboard::from_file(
            images,
            prop_cameras,
            user_textures,
            &drawing_resources,
            things_catalog,
            &grid,
            &header,
            &mut file
        )?;
        clipboard.reset_props_changed();

        Ok(FileRead {
            animations: drawing_resources.animations,
            manager,
            map_default_brush_properties,
            map_default_thing_properties,
            clipboard,
            grid,
            path
        })
    }

    #[inline]
    #[must_use]
    fn open_file(title: &str, filter_description: &str, filter_extension: &str) -> Option<PathBuf>
    {
        rfd::FileDialog::new()
            .set_directory(std::env::current_dir().unwrap())
            .set_title(title)
            .add_filter(filter_description, &[filter_extension])
            .pick_file()
    }

    /// Opens a map file, unless the file cannot be properly read. If there are unsaved changes in
    /// the currently open map the save procedure is initiated.
    #[inline]
    fn open(&mut self, bundle: &mut StateUpdateBundle)
    {
        if !dialog_if_error!(
            ret;
            Self::save_unsaved_changes(
                bundle.window,
                bundle.config,
                bundle.default_properties,
                bundle.drawing_resources,
                bundle.manager,
                bundle.clipboard,
                bundle.edits_history,
                bundle.grid
            )
        )
        {
            return;
        }

        let file_to_open = return_if_none!(Self::open_file("Open", HV_FILTER_NAME, FILE_EXTENSION));

        match Self::process_map_file(
            bundle.images,
            bundle.prop_cameras,
            bundle.user_textures,
            file_to_open,
            bundle.drawing_resources,
            bundle.things_catalog,
            bundle.default_properties.engine_brushes,
            bundle.default_properties.engine_things
        )
        {
            Ok(FileRead {
                animations,
                manager,
                map_default_brush_properties,
                map_default_thing_properties,
                clipboard,
                grid,
                path
            }) =>
            {
                bundle.drawing_resources.replace_animations(animations);
                *bundle.manager = manager;
                *bundle.clipboard = clipboard;
                *bundle.grid = grid;
                *bundle.inputs = InputsPresses::default();
                *bundle.edits_history = EditsHistory::default();
                bundle.config.open_file.update(path, bundle.window);
                *bundle.default_properties.map_brushes = map_default_brush_properties;
                *bundle.default_properties.map_things = map_default_thing_properties;

                self.ui.regenerate_properties_window(
                    bundle.default_properties.map_brushes,
                    bundle.default_properties.map_things
                );
                self.core = Core::default();
            },
            Err(err) => error_message(err)
        };
    }

    //==============================================================
    // Export

    /// Initiates the map export procedure if an exporter executable is specified.
    /// If there are unsaved changes in the currently open map the save procedure is initiated.
    #[inline]
    fn export(bundle: &mut StateUpdateBundle)
    {
        if !dialog_if_error!(ret; Self::save_unsaved_changes(
            bundle.window,
            bundle.config,
            bundle.default_properties,
            bundle.drawing_resources,
            bundle.manager,
            bundle.clipboard,
            bundle.edits_history,
            bundle.grid
        ))
        {
            return;
        }

        let file = return_if_none!(bundle.config.open_file.path());
        let exporter = return_if_none!(bundle.config.exporter.as_ref());

        if !exporter.exists() || !exporter.is_executable()
        {
            error_message("Exporter executable does not exist.");
            bundle.config.exporter = None;
            return;
        }

        dialog_if_error!(
            map;
            std::process::Command::new(exporter).arg(file).output(),
            "Error exporting map"
        );
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
        if HardcodedActions::Quit.pressed(bundle.key_inputs) &&
            Self::quit(
                bundle.window,
                bundle.config,
                bundle.default_properties,
                bundle.drawing_resources,
                bundle.manager,
                bundle.clipboard,
                bundle.edits_history,
                bundle.grid,
                bundle.next_editor_state
            )
        {
            return false;
        }

        // Reactive update to previous frame's changes.
        bundle.manager.update_tool_and_overall_values(
            bundle.drawing_resources,
            bundle.things_catalog,
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
        #[inline]
        fn export<E>(
            items: &str,
            filter_description: &str,
            filter_extension: &'static str,
            len: usize,
            e: E
        ) where
            E: FnOnce(&mut BufWriter<&mut Vec<u8>>) -> Result<(), &'static str>
        {
            let path = return_if_none!(State::save_file(
                &format!("Export {items}"),
                filter_description,
                filter_extension
            ));
            let path = check_path_extension(path, filter_extension);

            let mut data = Vec::<u8>::new();
            let mut writer = BufWriter::new(&mut data);

            dialog_if_error!(
                map;
                ciborium::ser::into_writer(FILE_VERSION, &mut writer),
                "Error writing version number."
            );
            dialog_if_error!(
                map;
                ciborium::ser::into_writer(&len, &mut writer),
                &format!("Error writing {items} amount.")
            );
            dialog_if_error!(e(&mut writer));

            drop(writer);

            let new_file = !path.exists();

            if new_file && File::create(&path).is_err()
            {
                error_message(&format!("Error creating {items} file."));
                return;
            }

            let mut file = match OpenOptions::new().write(true).open(&path)
            {
                Ok(file) => BufWriter::new(file),
                Err(_) =>
                {
                    error_message(&format!("Error opening {items} file."));

                    if new_file
                    {
                        _ = std::fs::remove_file(path);
                    }

                    return;
                }
            };

            if file.write_all(&data).is_err()
            {
                if new_file
                {
                    _ = std::fs::remove_file(&path);
                }

                error_message(&format!("Error writing {items} file."));
            }
        }

        #[inline]
        fn import<C, I>(items: &str, filter_description: &str, filter_extension: &str, c: C, i: I)
        where
            C: FnOnce(
                &str,
                BufReader<File>,
                PathBuf,
                usize
            ) -> Result<BufReader<File>, &'static str>,
            I: FnOnce(&mut BufReader<File>, usize) -> Result<(), &'static str>
        {
            let path = return_if_none!(State::open_file(
                &format!("Import {items}"),
                filter_description,
                filter_extension
            ));

            let mut reader = BufReader::new(File::open(&path).unwrap());
            let version = dialog_if_error!(ret; version_number(&mut reader));
            let len = dialog_if_error!(
                map;
                ciborium::de::from_reader(&mut reader),
                &format!("Error reading {items} length")
            );
            let mut reader = dialog_if_error!(ret; c(version.as_str(), reader, path, len));
            dialog_if_error!(i(&mut reader, len));
        }

        bundle.clipboard.update(
            bundle.images,
            bundle.prop_cameras,
            bundle.user_textures,
            bundle.drawing_resources,
            bundle.things_catalog,
            bundle.grid
        );

        match ui_interaction.command
        {
            Command::None => (),
            Command::ChangeTool(tool) =>
            {
                self.change_tool(tool, bundle, tool_change_conditions);
            },
            Command::New => dialog_if_error!(self.new_file(bundle)),
            Command::Save =>
            {
                dialog_if_error!(Self::save(
                    bundle.window,
                    bundle.config,
                    bundle.default_properties,
                    bundle.drawing_resources,
                    bundle.manager,
                    bundle.clipboard,
                    bundle.edits_history,
                    bundle.grid,
                    None
                ));
            },
            Command::SaveAs =>
            {
                dialog_if_error!(Self::save(
                    bundle.window,
                    bundle.config,
                    bundle.default_properties,
                    bundle.drawing_resources,
                    bundle.manager,
                    bundle.clipboard,
                    bundle.edits_history,
                    bundle.grid,
                    "Save as".into()
                ));
            },
            Command::Open => self.open(bundle),
            Command::Export => Self::export(bundle),
            Command::ImportAnimations =>
            {
                import(
                    "Import animations",
                    ANIMATIONS_FILTER_NAME,
                    ANIMATIONS_EXTENSION,
                    |_, reader, _, _| Ok(reader),
                    |file, len| bundle.drawing_resources.import_animations(len, file)
                );
            },
            Command::ExportAnimations =>
            {
                export(
                    "animations",
                    ANIMATIONS_FILTER_NAME,
                    ANIMATIONS_EXTENSION,
                    bundle.drawing_resources.animations_amount(),
                    |writer| bundle.drawing_resources.export_animations(writer)
                );
            },
            Command::ImportProps =>
            {
                import(
                    "Import props",
                    PROPS_FILTER_NAME,
                    PROPS_EXTENSION,
                    |_, reader, _, _| Ok(reader),
                    |file, len| {
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
                    }
                );
            },
            Command::ExportProps =>
            {
                export(
                    "props",
                    PROPS_FILTER_NAME,
                    PROPS_EXTENSION,
                    bundle.clipboard.props_amount(),
                    |writer| bundle.clipboard.export_props(writer)
                );
            },
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
                if let Some(hull) = bundle.manager.selected_brushes_polygon_hull()
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
                _ = Self::quit(
                    bundle.window,
                    bundle.config,
                    bundle.default_properties,
                    bundle.drawing_resources,
                    bundle.manager,
                    bundle.clipboard,
                    bundle.edits_history,
                    bundle.grid,
                    bundle.next_editor_state
                );
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

        if Self::no_edits(
            bundle.drawing_resources,
            bundle.manager,
            bundle.clipboard,
            bundle.edits_history,
            bundle.grid
        )
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
                _ = Self::quit(
                    bundle.window,
                    bundle.config,
                    bundle.default_properties,
                    bundle.drawing_resources,
                    bundle.manager,
                    bundle.clipboard,
                    bundle.edits_history,
                    bundle.grid,
                    bundle.next_editor_state
                );
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

    #[inline]
    #[must_use]
    fn reload_warning(message: &str) -> bool
    {
        match rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title("WARNING")
            .set_description(message)
            .set_buttons(rfd::MessageButtons::YesNoCancel)
            .show()
        {
            rfd::MessageDialogResult::Yes => true,
            rfd::MessageDialogResult::No => false,
            _ => unreachable!()
        }
    }

    /// Reloads the things.
    #[inline]
    fn reload_things(bundle: &mut StateUpdateBundle)
    {
        if !Self::reload_warning(
            "Reloading the things will erase the history of all things edits and will set all \
             things that will result out of bound to errors. Are you sure you wish to proceed?"
        )
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
    /// Returns whether the application should actually be closed.
    #[inline]
    #[must_use]
    pub fn quit(
        window: &mut Window,
        config: &mut Config,
        default_properties: &AllDefaultProperties,
        drawing_resources: &mut DrawingResources,
        manager: &mut EntitiesManager,
        clipboard: &mut Clipboard,
        edits_history: &mut EditsHistory,
        grid: &mut Grid,
        next_editor_state: &mut NextState<EditorState>
    ) -> bool
    {
        if !dialog_if_error!(
            default;
            Self::save_unsaved_changes(
                window,
                config,
                default_properties,
                drawing_resources,
                manager,
                clipboard,
                edits_history,
                grid
            ),
            true
        )
        {
            return false;
        }

        next_editor_state.set(EditorState::ShutDown);
        true
    }

    /// Returns the [`Hull`] representing the rectangle encompassing all the selected entities if
    /// the quick zoom key combo was pressed.
    #[inline]
    #[must_use]
    pub fn quick_zoom_hull(
        key_inputs: &ButtonInput<KeyCode>,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        manager: &EntitiesManager,
        grid: &Grid,
        binds: &BindsKeyCodes
    ) -> Option<Hull>
    {
        if Bind::Zoom.alt_just_pressed(key_inputs, binds)
        {
            return Hull::from_hulls_iter(
                manager
                    .selected_brushes()
                    .map(|brush| brush.hull(drawing_resources, grid))
                    .chain(manager.selected_things().map(|thing| thing.hull(things_catalog)))
            );
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

        if !Self::reload_warning(
            "Reloading the textures will erase the history of all texture edits and will set all \
             the textures of brushes with associated sprites that will result out of bound to \
             errors. Are you sure you wish to proceed?"
        )
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
        things_catalog: &ThingsCatalog,
        manager: &mut EntitiesManager,
        clipboard: &mut Clipboard,
        edits_history: &mut EditsHistory,
        grid: &Grid
    )
    {
        assert!(self.reloading_textures.take_value(), "No ongoing texture reload.");

        edits_history.purge_texture_edits();
        clipboard.finish_textures_reload(
            images,
            user_textures,
            prop_cameras,
            drawing_resources,
            things_catalog,
            grid
        );
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
        bundle.manager.draw_error_highlight(
            bundle.things_catalog,
            bundle.drawer,
            bundle.delta_time
        );

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
