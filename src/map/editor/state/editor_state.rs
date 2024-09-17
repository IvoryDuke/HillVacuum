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
    input::{keyboard::KeyCode, mouse::MouseButton, ButtonInput},
    render::texture::Image,
    state::state::NextState
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
    input_press::InputStateHardCoded,
    manager::EntitiesManager,
    ui::Interaction
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
                clipboard::prop::Prop,
                core::{tool::ToolInterface, Core},
                input_press::InputState,
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
        FILE_VERSION_NUMBER
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

/// A macro to create functions that return whether a key was pressed.
macro_rules! pressed {
    ($($key:ident),+) => ( paste::paste! {$(
        /// Whether the key was pressed.
        #[inline]
        pub const fn [< $key _pressed >](&self) -> bool
        {
            self.inputs.[<$key _pressed>]()
        }
	)+});
}

//=======================================================================//

/// A macro to generate the code of [`InputsPresses`].
macro_rules! input_presses {
    (
        $mouse_buttons:ident,
        $key_inputs:ident,
        $binds_inputs:ident,
        $(($name:ident, $input_type:ty, $key:expr, $source:ident $(, $binds:ident)?)),+
    ) => (
        /// A struct containing the states of all input presses required by the editor.
		pub(in crate::map::editor::state) struct InputsPresses
		{
			$(pub $name: $input_type,)+
		}

        impl Default for InputsPresses
        {
            #[inline]
			fn default() -> Self
			{
				Self {
					$($name: <$input_type>::new($key),)+
				}
			}
        }

		impl InputsPresses
		{
            /// Updates the state of the input presses.
			#[inline]
			fn update(
                &mut self,
                bundle: &mut StateUpdateBundle
            )
			{
				$(self.$name.update(bundle.$source $(, &mut bundle.config.$binds)?);)+
			}

            /// Forcefully resets the input presses to not pressed.
            #[inline]
            pub fn clear(&mut self)
            {
                self.space.clear();
                self.back.clear();
                self.tab.clear();
                self.enter.clear();
                self.plus.clear();
                self.minus.clear();
                self.left_mouse.clear();
                self.right_mouse.clear();
                self.esc.clear();
                self.f4.clear();
                self.copy.clear();
                self.paste.clear();
                self.cut.clear();
                self.left.clear();
                self.right.clear();
                self.up.clear();
                self.down.clear();
            }
		}
	);
}

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
// TYPES
//
//=======================================================================//

input_presses!(
    mouse_buttons,
    key_inputs,
    binds,
    (l_ctrl, InputStateHardCoded<KeyCode>, KeyCode::ControlLeft, key_inputs),
    (r_ctrl, InputStateHardCoded<KeyCode>, KeyCode::ControlRight, key_inputs),
    (l_shift, InputStateHardCoded<KeyCode>, KeyCode::ShiftLeft, key_inputs),
    (r_shift, InputStateHardCoded<KeyCode>, KeyCode::ShiftRight, key_inputs),
    (l_alt, InputStateHardCoded<KeyCode>, KeyCode::AltLeft, key_inputs),
    (r_alt, InputStateHardCoded<KeyCode>, KeyCode::AltRight, key_inputs),
    (space, InputStateHardCoded<KeyCode>, KeyCode::Space, key_inputs),
    (back, InputStateHardCoded<KeyCode>, KeyCode::Backspace, key_inputs),
    (tab, InputStateHardCoded<KeyCode>, KeyCode::Tab, key_inputs),
    (enter, InputStateHardCoded<KeyCode>, KeyCode::Enter, key_inputs),
    (plus, InputStateHardCoded<KeyCode>, KeyCode::NumpadAdd, key_inputs),
    (minus, InputStateHardCoded<KeyCode>, KeyCode::Minus, key_inputs),
    (left_mouse, InputStateHardCoded<MouseButton>, MouseButton::Left, mouse_buttons),
    (right_mouse, InputStateHardCoded<MouseButton>, MouseButton::Right, mouse_buttons),
    (esc, InputStateHardCoded<KeyCode>, KeyCode::Escape, key_inputs),
    (f4, InputStateHardCoded<KeyCode>, KeyCode::F4, key_inputs),
    (copy, InputStateHardCoded<KeyCode>, HardcodedActions::Copy.key(), key_inputs),
    (paste, InputStateHardCoded<KeyCode>, HardcodedActions::Paste.key(), key_inputs),
    (cut, InputStateHardCoded<KeyCode>, HardcodedActions::Cut.key(), key_inputs),
    (left, InputState, Bind::Left, key_inputs, binds),
    (right, InputState, Bind::Right, key_inputs, binds),
    (up, InputState, Bind::Up, key_inputs, binds),
    (down, InputState, Bind::Down, key_inputs, binds)
);

impl InputsPresses
{
    /// Whether shift is pressed.
    #[inline]
    #[must_use]
    pub const fn shift_pressed(&self) -> bool { self.l_shift.pressed() || self.r_shift.pressed() }

    /// Whether alt is pressed.
    #[inline]
    #[must_use]
    pub const fn alt_pressed(&self) -> bool { self.l_alt.pressed() || self.r_alt.pressed() }

    /// Whether ctrl is pressed.
    #[inline]
    #[must_use]
    pub const fn ctrl_pressed(&self) -> bool { self.l_ctrl.pressed() || self.r_ctrl.pressed() }

    /// Whether the copy key combo was just pressed.
    #[inline]
    #[must_use]
    pub const fn copy_just_pressed(&self) -> bool
    {
        self.ctrl_pressed() && self.copy.just_pressed()
    }

    /// Whether the paste key combo was just pressed.
    #[inline]
    #[must_use]
    pub const fn paste_just_pressed(&self) -> bool
    {
        self.ctrl_pressed() && self.paste.just_pressed()
    }

    /// Whether the cut key combo was just pressed.
    #[inline]
    #[must_use]
    pub const fn cut_just_pressed(&self) -> bool { self.ctrl_pressed() && self.cut.just_pressed() }

    /// Returns a vector representing the direction of the pressed arrow keys, if any.
    #[inline]
    #[must_use]
    pub fn directional_keys_vector(&self, grid_size: i16) -> Option<Vec2>
    {
        let mut dir = Vec2::ZERO;

        if self.right.just_pressed()
        {
            dir.x += 1f32;
        }

        if self.left.just_pressed()
        {
            dir.x -= 1f32;
        }

        if self.up.just_pressed()
        {
            dir.y += 1f32;
        }

        if self.down.just_pressed()
        {
            dir.y -= 1f32;
        }

        (dir != Vec2::ZERO).then(|| dir * f32::from(grid_size))
    }
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
    /// How much the selected textures should be scaled when using the scale tool and textures only
    /// are being edited.
    pub(in crate::map::editor::state) texture_scale_interval: f32,
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
            texture_scale_interval: 0.5,
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
    /// The manager of all entities.
    manager:            EntitiesManager,
    /// The clipboard used for copy paste and prop spawning.
    clipboard:          Clipboard,
    /// The history of the edits made to the map.
    edits_history:      EditsHistory,
    /// The state of all necessary input presses.
    inputs:             InputsPresses,
    /// The grid of the map.
    grid:               Grid,
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
            manager:            EntitiesManager::new(),
            clipboard:          Clipboard::new(),
            edits_history:      EditsHistory::default(),
            inputs:             InputsPresses::default(),
            grid:               Grid::default(),
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
    // Keys

    pressed!(ctrl, shift);

    /// Whether space is pressed.
    #[inline]
    pub const fn space_pressed(&self) -> bool { self.inputs.space.pressed() }

    //==============================================================
    // New

    /// Creates a new [`State`].
    #[inline]
    #[must_use]
    pub fn new(
        asset_server: &AssetServer,
        images: &mut Assets<Image>,
        prop_cameras: &mut PropCamerasMut,
        user_textures: &mut EguiUserTextures,
        drawing_resources: &mut DrawingResources,
        things_catalog: &ThingsCatalog,
        default_properties: &mut AllDefaultProperties,
        file: Option<PathBuf>
    ) -> (Self, Option<PathBuf>)
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
                manager:            EntitiesManager::new(),
                clipboard:          Clipboard::new(),
                edits_history:      EditsHistory::default(),
                inputs:             InputsPresses::default(),
                grid:               Grid::default(),
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
            Ok((manager, clipboard, grid, path)) =>
            {
                let mut state = Self {
                    core: Core::default(),
                    manager,
                    clipboard,
                    edits_history: EditsHistory::default(),
                    inputs: InputsPresses::default(),
                    grid,
                    ui: Ui::new(
                        asset_server,
                        user_textures,
                        default_properties.map_brushes,
                        default_properties.map_things
                    ),
                    tools_settings: ToolsSettings::default(),
                    show_tooltips: true,
                    cursor_snap: true,
                    show_cursor: true,
                    show_collision: true,
                    reloading_textures: false
                };

                state.manager.finish_things_reload(things_catalog);
                state.manager.finish_textures_reload(drawing_resources);

                (state, path.into())
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
                    None
                )
            }
        }
    }

    //==============================================================
    // Info

    /// Returns a vector representing the direction of the pressed arrow keys, if any.
    #[inline]
    #[must_use]
    pub fn directional_keys_vector(&self) -> Option<Vec2>
    {
        self.inputs
            .directional_keys_vector(self.grid.size())
            .map(|vec| self.grid.transform_point(vec))
    }

    /// Returns the grid size as a fractional number.
    #[inline]
    #[must_use]
    pub fn grid_size_f32(&self) -> f32 { f32::from(self.grid.size()) }

    #[inline]
    pub const fn grid(&self) -> Grid { self.grid }

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

            if let Err(err) = self.save(bundle, self.inputs.shift_pressed().then_some("Save as"))
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

            self.export(bundle);
            return true;
        }

        if HardcodedActions::SelectAll.pressed(bundle.key_inputs) &&
            self.core.select_all_available()
        {
            self.select_all(bundle.drawing_resources);
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

        if self.inputs.copy_just_pressed()
        {
            self.copy(bundle);
        }
        else if self.inputs.paste_just_pressed()
        {
            self.paste(bundle);
        }
        else if self.inputs.cut_just_pressed()
        {
            self.cut(bundle);
        }
        else if HardcodedActions::Duplicate.pressed(bundle.key_inputs)
        {
            self.duplicate(bundle.drawing_resources);
        }
        else
        {
            return false;
        }

        true
    }

    #[inline]
    #[must_use]
    pub const fn is_window_focused(&self) -> bool { self.ui.is_window_focused() }

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
        &mut self,
        bundle: &mut StateUpdateBundle,
        buttons: rfd::MessageButtons
    ) -> Result<bool, &'static str>
    {
        if self.no_edits(bundle.drawing_resources)
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
                match self.save(bundle, None)
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
        if !self.unsaved_changes(bundle, rfd::MessageButtons::YesNoCancel)?
        {
            return Ok(());
        }

        self.core = Core::default();
        self.manager = EntitiesManager::new();
        self.clipboard = Clipboard::new();
        self.edits_history = EditsHistory::default();
        self.inputs = InputsPresses::default();
        self.grid = Grid::default();
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

        self.core
            .undo(bundle, &mut self.manager, &mut self.edits_history, &mut self.ui);
    }

    /// Executes the redo procedure.
    /// # Panics
    /// Panics if the operation is unavailable.
    #[inline]
    fn redo(&mut self, bundle: &mut StateUpdateBundle)
    {
        assert!(self.core.undo_redo_available(), "Redo is not available.");

        self.core
            .redo(bundle, &mut self.manager, &mut self.edits_history, &mut self.ui);
    }

    //==============================================================
    // Save

    /// Whether there are no unsaved changes.
    #[inline]
    #[must_use]
    fn no_edits(&self, drawing_resources: &DrawingResources) -> bool
    {
        self.edits_history.no_unsaved_edits() &&
            !self.clipboard.props_changed() &&
            !drawing_resources.default_animations_changed() &&
            !self.manager.refactored_properties()
    }

    /// Saves the map being edited. If the file has not being created yet user is asked to specify
    /// where it should be stored. If the file exists, if `save as` contains a value user is
    /// asked to specify in which new file the map should be saved. Otherwise the map is stored
    /// in the previously opened file.
    #[inline]
    fn save(
        &mut self,
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
                            brushes:    self.manager.brushes_amount(),
                            things:     self.manager.things_amount(),
                            animations: bundle.drawing_resources.animations_amount(),
                            props:      self.clipboard.props_amount()
                        },
                        &mut writer,
                        "Error saving file header"
                    );
                },
                FileStructure::Grid =>
                {
                    test_writer!(&self.grid.settings(), &mut writer, "Error saving grid settings.");
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
                    for brush in self.manager.brushes().iter()
                    {
                        test_writer!(
                            &BrushViewer::new(brush.clone()),
                            &mut writer,
                            "Error saving brushes."
                        );
                    }
                },
                FileStructure::Things =>
                {
                    for thing in self.manager.things()
                    {
                        test_writer!(
                            &ThingViewer::new(thing.clone()),
                            &mut writer,
                            "Error saving things."
                        );
                    }
                },
                FileStructure::Props => self.clipboard.export_props(&mut writer)?
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

        self.edits_history.reset_last_save_edit();
        self.clipboard.reset_props_changed();
        self.manager.reset_refactored_properties();
        bundle.drawing_resources.reset_default_animation_changed();

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
        use crate::map::{brush::BrushViewer, thing::ThingViewer};

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
        fn ex_header(reader: &mut BufReader<File>) -> Result<MapHeader, &'static str>
        {
            ciborium::from_reader::<MapHeader, _>(reader)
                .map_err(|_| "Error reading file header for conversion.")
        }

        #[inline]
        fn ex_default_properties(
            reader: &mut BufReader<File>
        ) -> Result<[DefaultProperties; 2], &'static str>
        {
            read_default_properties(reader)
                .map_err(|_| "Error reading default properties for conversion.")
        }

        #[inline]
        fn ex_animations(
            reader: &mut BufReader<File>,
            header: &MapHeader
        ) -> Result<HvVec<DefaultAnimation>, &'static str>
        {
            file_animations(header.animations, reader)
                .map_err(|_| "Error reading animations for conversion.")
        }

        #[inline]
        fn ex_brushes_04(
            reader: &mut BufReader<File>,
            header: &MapHeader
        ) -> Result<HvVec<Brush>, &'static str>
        {
            let mut brushes = hv_vec![];

            for _ in 0..header.brushes
            {
                brushes.push(Brush::from(
                    ciborium::from_reader::<crate::map::brush::compatibility::_04::Brush, _>(
                        &mut *reader
                    )
                    .map_err(|_| "Error reading brushes for conversion.")?
                ));
            }

            Ok(brushes)
        }

        #[inline]
        fn ex_things(
            reader: &mut BufReader<File>,
            header: &MapHeader
        ) -> Result<HvVec<ThingInstance>, &'static str>
        {
            let mut things = hv_vec![];

            for _ in 0..header.things
            {
                things.push(ThingInstance::from(
                    ciborium::from_reader::<crate::map::thing::compatibility::ThingInstance, _>(
                        &mut *reader
                    )
                    .map_err(|_| "Error reading things for conversion.")?
                ));
            }

            Ok(things)
        }

        #[inline]
        fn ex_props(
            reader: &mut BufReader<File>,
            header: &MapHeader
        ) -> Result<HvVec<Prop>, &'static str>
        {
            let mut props = hv_vec![];

            for _ in 0..header.props
            {
                props.push(
                    ciborium::from_reader::<Prop, _>(&mut *reader)
                        .map_err(|_| "Error reading props for conversion.")?
                );
            }

            Ok(props)
        }

        #[inline]
        fn ex_grid(reader: &mut BufReader<File>) -> Result<GridSettings, &'static str>
        {
            ciborium::from_reader::<_, _>(reader)
                .map_err(|_| "Error reading grid settings for conversion.")
        }

        #[inline]
        fn convert_03(
            mut reader: BufReader<File>,
            _: &ThingsCatalog
        ) -> Result<OldFileRead, &'static str>
        {
            // Header
            let header = ex_header(&mut reader)?;

            // Properties.
            let default_properties = ex_default_properties(&mut reader)?;

            // Animations.
            let animations = ex_animations(&mut reader, &header)?;

            // Brushes.
            let mut brushes = hv_vec![];

            for _ in 0..header.brushes
            {
                brushes.push(Brush::from(
                    ciborium::from_reader::<crate::map::brush::compatibility::_03::Brush, _>(
                        &mut reader
                    )
                    .map_err(|_| "Error reading brushes for conversion.")?
                ));
            }

            // Things.
            let things = ex_things(&mut reader, &header)?;

            // Props.
            let props = ex_props(&mut reader, &header)?;

            Ok(OldFileRead {
                header,
                grid: ciborium::from_reader::<GridSettings, _>(&mut reader).unwrap_or_default(),
                animations,
                default_properties,
                brushes,
                things,
                props
            })
        }

        #[inline]
        fn convert_04(
            mut reader: BufReader<File>,
            _: &ThingsCatalog
        ) -> Result<OldFileRead, &'static str>
        {
            // Header
            let header = ex_header(&mut reader)?;

            // Animations.
            let animations = ex_animations(&mut reader, &header)?;

            // Properties.
            let default_properties = ex_default_properties(&mut reader)?;

            // Brushes.
            let brushes = ex_brushes_04(&mut reader, &header)?;

            // Things.
            let things = ex_things(&mut reader, &header)?;

            // Props.
            let props = ex_props(&mut reader, &header)?;

            Ok(OldFileRead {
                header,
                grid: ex_grid(&mut reader)?,
                animations,
                default_properties,
                brushes,
                things,
                props
            })
        }

        #[inline]
        fn convert_05(
            mut reader: BufReader<File>,
            _: &ThingsCatalog
        ) -> Result<OldFileRead, &'static str>
        {
            // Header.
            let header = ex_header(&mut reader)?;

            // Grid.
            let grid = ex_grid(&mut reader)?;

            // Animations.
            let animations = ex_animations(&mut reader, &header)?;

            // Properties.
            let default_properties = ex_default_properties(&mut reader)?;

            // Brushes.
            let brushes = ex_brushes_04(&mut reader, &header)?;

            // Things.
            let things = ex_things(&mut reader, &header)?;

            // Props.
            let props = ex_props(&mut reader, &header)?;

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
        fn convert_06(
            mut reader: BufReader<File>,
            _: &ThingsCatalog
        ) -> Result<OldFileRead, &'static str>
        {
            // Header.
            let header = ex_header(&mut reader)?;

            // Grid.
            let grid = ex_grid(&mut reader)?;

            // Animations.
            let animations = ex_animations(&mut reader, &header)?;

            // Properties.
            let default_properties = ex_default_properties(&mut reader)?;

            // Brushes.
            let mut brushes = hv_vec![];

            for _ in 0..header.brushes
            {
                brushes.push(
                    ciborium::from_reader::<Brush, _>(&mut reader)
                        .map_err(|_| "Error reading brushes for conversion.")?
                );
            }

            // Things.
            let things = ex_things(&mut reader, &header)?;

            // Props.
            let props = ex_props(&mut reader, &header)?;

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
        fn convert_061(
            mut reader: BufReader<File>,
            things_catalog: &ThingsCatalog
        ) -> Result<OldFileRead, &'static str>
        {
            // Header.
            let header = ex_header(&mut reader)?;

            // Grid.
            let grid = ex_grid(&mut reader)?;

            // Animations.
            let animations = ex_animations(&mut reader, &header)?;

            // Properties.
            let default_properties = ex_default_properties(&mut reader)?;

            // Brushes.
            let mut brushes = hv_vec![];

            for _ in 0..header.brushes
            {
                brushes.push(
                    ciborium::from_reader::<
                        crate::map::brush::compatibility::_061::BrushViewer,
                        _
                    >(&mut reader)
                    .map_err(|_| "Error reading brushes for conversion.")?
                    .into()
                );
            }

            // Things.
            let mut things = hv_vec![];

            for _ in 0..header.things
            {
                things.push(ThingInstance::from((
                    ciborium::from_reader::<ThingViewer, _>(&mut reader)
                        .map_err(|_| "Error reading things for conversion.")?,
                    things_catalog
                )));
            }

            // Props.
            let props = ex_props(&mut reader, &header)?;

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
            path: &mut PathBuf,
            reader: BufReader<File>,
            things_catalog: &ThingsCatalog,
            f: fn(BufReader<File>, &ThingsCatalog) -> Result<OldFileRead, &'static str>
        ) -> Result<BufReader<File>, &'static str>
        {
            let mut file_name = path.file_stem().unwrap().to_str().unwrap().to_string();
            file_name.push_str("_07.hv");

            warning_message(&format!(
                "This file appears to have an old file structure, if it is valid it will now be \
                 converted to {file_name}."
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
                        for brush in brushes.take_value().into_iter().map(BrushViewer::new)
                        {
                            test_writer!(&brush, &mut writer, "Error converting brushes.");
                        }
                    },
                    FileStructure::Things =>
                    {
                        for thing in things.take_value().into_iter().map(ThingViewer::new)
                        {
                            test_writer!(&thing, &mut writer, "Error converting things.");
                        }
                    },
                    FileStructure::Props =>
                    {
                        for prop in &props
                        {
                            test_writer!(prop, &mut writer, "Error converting props.");
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
        let mut file = match version_number(&mut reader).as_str()
        {
            "0.3" => convert(&mut path, reader, things_catalog, convert_03)?,
            "0.4" => convert(&mut path, reader, things_catalog, convert_04)?,
            "0.5" => convert(&mut path, reader, things_catalog, convert_05)?,
            "0.6" => convert(&mut path, reader, things_catalog, convert_06)?,
            "0.6.1" => convert(&mut path, reader, things_catalog, convert_061)?,
            FILE_VERSION_NUMBER => reader,
            _ => unreachable!()
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
            grid,
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
        match self.unsaved_changes(bundle, rfd::MessageButtons::YesNoCancel)
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
                self.manager = manager;
                self.clipboard = clipboard;
                self.grid = grid;
                bundle.config.open_file.update(path, bundle.window);
            },
            Err(err) =>
            {
                error_message(err);
                return;
            }
        };

        self.core = Core::default();
        self.inputs = InputsPresses::default();
        self.edits_history = EditsHistory::default();
    }

    //==============================================================
    // Export

    /// Initiates the map export procedure if an exporter executable is specified.
    /// If there are unsaved changes in the currently open map the save procedure is initiated.
    #[inline]
    fn export(&mut self, bundle: &mut StateUpdateBundle)
    {
        let file = match self.unsaved_changes(bundle, rfd::MessageButtons::YesNoCancel)
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
    fn select_all(&mut self, drawing_resources: &DrawingResources)
    {
        self.core.select_all(
            drawing_resources,
            &mut self.manager,
            &mut self.edits_history,
            self.grid,
            &self.tools_settings
        );
    }

    //==============================================================
    // Copy/Paste

    /// Whether copy paste is available.
    #[inline]
    #[must_use]
    fn copy_paste_available(&self) -> bool { self.core.copy_paste_available() }

    /// Initiates the copy procedure.
    #[inline]
    fn copy(&mut self, bundle: &StateUpdateBundle)
    {
        self.core
            .copy(bundle, &mut self.manager, &self.inputs, &mut self.clipboard);
    }

    /// Initiates the cut procedure.
    #[inline]
    fn cut(&mut self, bundle: &StateUpdateBundle)
    {
        self.core.cut(
            bundle,
            &mut self.manager,
            &self.inputs,
            &mut self.clipboard,
            &mut self.edits_history
        );
    }

    /// Initiates the paste procedure.
    #[inline]
    fn paste(&mut self, bundle: &StateUpdateBundle)
    {
        self.core.paste(
            bundle,
            &mut self.manager,
            &self.inputs,
            &mut self.clipboard,
            &mut self.edits_history
        );
    }

    /// Initiates the duplicate procedure.
    #[inline]
    fn duplicate(&mut self, drawing_resources: &DrawingResources)
    {
        let delta = Vec2::new(self.grid_size_f32(), 0f32);

        self.core.duplicate(
            drawing_resources,
            &mut self.manager,
            &mut self.clipboard,
            &mut self.edits_history,
            delta
        );
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
            self.quit(bundle, rfd::MessageButtons::YesNoCancel);
        }

        // Reactive update to previous frame's changes.
        self.manager.update_tool_and_overall_values(
            bundle.drawing_resources,
            &mut self.core,
            &mut self.ui,
            self.grid,
            &mut self.tools_settings
        );

        // Update inputs.
        self.inputs.update(bundle);

        // Create UI.
        let tool_change_conditions = ChangeConditions::new(
            &self.inputs,
            &self.clipboard,
            &self.core,
            bundle.things_catalog,
            &self.manager
        );

        let ui_interaction = self.ui.frame_start_update(
            bundle,
            &mut self.core,
            &mut self.manager,
            &mut self.inputs,
            &mut self.edits_history,
            &mut self.clipboard,
            &mut self.grid,
            &mut self.tools_settings,
            &tool_change_conditions
        );

        if self.reloading_textures
        {
            return false;
        }

        if ui_interaction.hovered
        {
            self.inputs.left_mouse.clear();
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

        self.clipboard.update(
            bundle.images,
            bundle.prop_cameras,
            bundle.user_textures,
            bundle.drawing_resources,
            self.grid
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
                if let Err(err) = self.save(bundle, None)
                {
                    error_message(err);
                }
            },
            Command::SaveAs =>
            {
                if let Err(err) = self.save(bundle, "Save as".into())
                {
                    error_message(err);
                }
            },
            Command::Open => self.open(bundle),
            Command::Export => self.export(bundle),
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
                    self.clipboard.import_props(
                        bundle.images,
                        bundle.prop_cameras,
                        bundle.user_textures,
                        bundle.drawing_resources,
                        bundle.things_catalog,
                        self.grid,
                        len,
                        file
                    )
                });
            },
            Command::ExportProps => export!(PROPS, "props", props, self.clipboard),
            Command::SelectAll => self.select_all(bundle.drawing_resources),
            Command::Copy => self.copy(bundle),
            Command::Paste => self.paste(bundle),
            Command::Cut => self.cut(bundle),
            Command::Duplicate => self.duplicate(bundle.drawing_resources),
            Command::Undo => self.undo(bundle),
            Command::Redo => self.redo(bundle),
            Command::ToggleGrid => self.toggle_grid(),
            Command::IncreaseGridSize => self.increase_grid_size(),
            Command::DecreaseGridSize => self.decrease_grid_size(),
            Command::ShiftGrid => self.shift_grid(),
            Command::ToggleTooltips => self.toggle_tooltips(),
            Command::ToggleCursorSnap => self.toggle_cursor_snap(),
            Command::ToggleMapPreview => self.toggle_map_preview(bundle),
            Command::ToggleCollision => self.toggle_collision(),
            Command::ReloadTextures => self.start_texture_reload(bundle.next_tex_load),
            Command::ReloadThings => self.reload_things(bundle),
            Command::QuickZoom =>
            {
                if let Some(hull) = self.manager.selected_brushes_hull()
                {
                    bundle.camera.scale_viewport_to_hull(
                        bundle.window,
                        self.grid,
                        &hull,
                        self.grid_size_f32()
                    );
                }
            },
            Command::QuickSnap => self.quick_snap(bundle.drawing_resources),
            Command::Quit =>
            {
                self.quit(bundle, rfd::MessageButtons::YesNoCancel);
                return true;
            }
        };

        if !(ui_interaction.command.world_edit() || self.hardcoded_key_inputs(bundle))
        {
            if Bind::ToggleGrid.just_pressed(bundle.key_inputs, &bundle.config.binds)
            {
                self.toggle_grid();
            }
            else if Bind::IncreaseGridSize.just_pressed(bundle.key_inputs, &bundle.config.binds)
            {
                self.increase_grid_size();
            }
            else if Bind::DecreaseGridSize.just_pressed(bundle.key_inputs, &bundle.config.binds)
            {
                self.decrease_grid_size();
            }
            else if Bind::ShiftGrid.just_pressed(bundle.key_inputs, &bundle.config.binds)
            {
                self.shift_grid();
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
                self.quick_snap(bundle.drawing_resources);
            }
            else if self.inputs.esc.just_pressed()
            {
                self.core.disable_subtool();
            }
            else if Bind::TextureEditor.alt_just_pressed(bundle.key_inputs, &bundle.config.binds)
            {
                self.tools_settings.cycle_texture_editing(
                    &self.core,
                    &mut self.manager,
                    &self.inputs
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

        self.core.frame_start_update(
            bundle.drawing_resources,
            &mut self.manager,
            &mut self.edits_history,
            &self.clipboard
        );
        self.tools_settings.update(&self.core, &mut self.manager);
        let starts_with_star = bundle.window.title.starts_with('*');

        if self.no_edits(bundle.drawing_resources)
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
            Command::ReloadTextures => self.start_texture_reload(bundle.next_tex_load),
            Command::Quit =>
            {
                self.quit(bundle, rfd::MessageButtons::YesNoCancel);
                return true;
            },
            _ => ()
        };

        if self.inputs.esc.just_pressed()
        {
            self.toggle_map_preview(bundle);
            self.inputs.esc.clear();
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

        self.core.update(
            bundle,
            &mut self.manager,
            &self.inputs,
            &mut self.edits_history,
            &mut self.clipboard,
            self.grid,
            &mut self.tools_settings
        );
    }

    /// Changes the active tool.
    #[inline]
    fn change_tool(
        &mut self,
        tool: Tool,
        bundle: &StateUpdateBundle,
        tool_change_conditions: &ChangeConditions
    )
    {
        if self.map_preview()
        {
            return;
        }

        self.core.change_tool(
            tool,
            bundle,
            &mut self.manager,
            &mut self.edits_history,
            &self.inputs,
            self.grid,
            &self.tools_settings,
            tool_change_conditions
        );
    }

    /// Toggles the grid visibiity.
    #[inline]
    fn toggle_grid(&mut self) { self.grid.visible.toggle(); }

    /// Increased the grid size.
    #[inline]
    fn increase_grid_size(&mut self) { self.grid.increase_size(&mut self.manager); }

    /// Decreases the grid size.
    #[inline]
    fn decrease_grid_size(&mut self) { self.grid.decrease_size(&mut self.manager); }

    /// Shifts the grid by half of its size, both vertically and horizontally.
    #[inline]
    fn shift_grid(&mut self) { self.grid.toggle_shift(&mut self.manager); }

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
        self.core.toggle_map_preview(bundle, &self.manager);
    }

    /// Toggles the collision overlay.
    #[inline]
    fn toggle_collision(&mut self) { self.show_collision.toggle(); }

    /// Reloads the things.
    #[inline]
    fn reload_things(&mut self, bundle: &mut StateUpdateBundle)
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
        self.edits_history.purge_thing_edits();
        self.clipboard.reload_things(bundle, self.grid);
        self.manager.finish_things_reload(bundle.things_catalog);
    }

    /// Starts the application shutdown procedure.
    #[inline]
    pub fn quit(&mut self, bundle: &mut StateUpdateBundle, buttons: rfd::MessageButtons) -> bool
    {
        if let Ok(false) = self.unsaved_changes(bundle, buttons)
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
        &self,
        drawing_resources: &DrawingResources,
        key_inputs: &ButtonInput<KeyCode>,
        binds: &BindsKeyCodes
    ) -> Option<Hull>
    {
        if Bind::Zoom.alt_just_pressed(key_inputs, binds)
        {
            return self.manager.selected_entities_hull(drawing_resources);
        }

        None
    }

    /// Snaps the editable entities to the grid.
    #[inline]
    fn quick_snap(&mut self, drawing_resources: &DrawingResources)
    {
        self.core.quick_snap(
            drawing_resources,
            &mut self.manager,
            &mut self.edits_history,
            &self.tools_settings,
            self.grid.shifted
        );
    }

    //==============================================================
    // Texture reload

    /// Starts the texture reload procedure.
    #[inline]
    fn start_texture_reload(&mut self, next_tex_load: &mut NextState<TextureLoadingProgress>)
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
        self.inputs.clear();
        next_tex_load.set(TextureLoadingProgress::Initiated);
    }

    /// Concludes the texture reload.
    #[inline]
    pub fn finish_textures_reload(
        &mut self,
        prop_cameras: &mut PropCamerasMut,
        images: &mut Assets<Image>,
        user_textures: &mut EguiUserTextures,
        drawing_resources: &DrawingResources
    )
    {
        assert!(std::mem::take(&mut self.reloading_textures), "No ongoing texture reload.");

        self.edits_history.purge_texture_edits();
        self.clipboard.reload_textures(
            images,
            user_textures,
            prop_cameras,
            drawing_resources,
            self.grid
        );
        self.manager.finish_textures_reload(drawing_resources);
        self.ui.update_overall_texture(drawing_resources, &self.manager);
    }

    //==============================================================
    // Draw

    /// Draws the visible portion of the map.
    #[inline]
    pub fn draw(&mut self, bundle: &mut DrawBundle)
    {
        self.clipboard.draw_props_to_photograph(bundle, self.grid);
        bundle.drawer.grid_lines(bundle.window, bundle.camera);
        self.core
            .draw_active_tool(bundle, &self.manager, self.grid, &self.tools_settings);
        self.manager.draw_error_highlight(bundle);

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
        self.core.draw_map_preview(bundle, &self.manager);
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
