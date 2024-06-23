pub(in crate::map::editor) mod checkbox;
mod manual;
mod minus_plus_buttons;
pub(in crate::map::editor::state) mod overall_value_field;
mod properties_window;
mod settings_window;
mod texture_editor;
mod tooltip;
mod window;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use arrayvec::ArrayVec;
use bevy::prelude::{AssetServer, Transform, Vec2};
use bevy_egui::{egui, EguiUserTextures};
use hill_vacuum_shared::{return_if_none, NextValue};

use self::{
    manual::Manual,
    properties_window::PropertiesWindow,
    settings_window::SettingsWindow,
    texture_editor::TextureEditor,
    tooltip::Tooltip
};
use super::{
    clipboard::Clipboard,
    core::{
        tool::{ChangeConditions, EnabledTool, SubTool, Tool, ToolInterface},
        Core
    },
    editor_state::{InputsPresses, ToolsSettings},
    edits_history::EditsHistory,
    grid::Grid,
    manager::EntitiesManager
};
use crate::{
    config::{controls::bind::Bind, Config},
    embedded_assets::embedded_asset_path,
    map::{
        drawer::drawing_resources::DrawingResources,
        editor::{cursor_pos::Cursor, Placeholder, StateUpdateBundle},
        properties::DefaultProperties
    },
    utils::misc::{Camera, FromToStr, Toggle},
    HardcodedActions
};

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

/// The width of the left panel.
const LEFT_SIDE_PANEL_WIDTH: f32 = 184f32;
/// The width of the right panel.
const RIGHT_SIDE_PANEL_WIDTH: f32 = 54f32;
/// The height of the menu bar.
const MENU_BAR_HEIGHT: f32 = 28f32;
/// The actual with of the left panel.
const LEFT_SIDE_PANEL_REAL_WIDTH: f32 = 1.08 * LEFT_SIDE_PANEL_WIDTH;
/// The actual with of the right panel.
const RIGHT_SIDE_PANEL_REAL_WIDTH: f32 = 0.99 * RIGHT_SIDE_PANEL_WIDTH;
/// The actual height of the menu bar.
const MENU_BAR_REAL_HEIGHT: f32 = 1.08 * MENU_BAR_HEIGHT;
/// The size of the tool icons.
const ICON_DRAW_SIZE: egui::Vec2 = egui::Vec2::splat(32f32);
/// The padding between two icons.
const ICONS_PADDING: egui::Vec2 = egui::Vec2::new(8f32, 4f32);

//=======================================================================//
// MACROS
//
//=======================================================================//

/// Draws a gallery of textures.
macro_rules! textures_gallery {
    (
        $ui:ident,
        $frame_size:expr,
        $chunker:expr,
        $highlight_index:expr,
        $draw_texture:expr,
        $row_without_highlight:expr
    ) => {{
        #[allow(clippy::cast_possible_truncation)]
        #[allow(clippy::cast_sign_loss)]
        let textures_per_row = ($ui.available_width() /
            ($frame_size + 2f32 * $ui.spacing().item_spacing.x))
            .floor() as usize;

        #[allow(clippy::redundant_closure_call)]
        let mut chunks = $chunker(textures_per_row);
        let len = chunks.len();

        if let Some(highlight_index) = $highlight_index
        {
            let row_with_highlight = highlight_index / textures_per_row;

            for _ in 0..row_with_highlight
            {
                #[allow(clippy::redundant_closure_call)]
                $row_without_highlight($ui, chunks.next().unwrap());
            }

            $ui.horizontal(|ui| {
                let highlight_index_in_row = highlight_index % textures_per_row;
                let mut textures = chunks.next().unwrap().into_iter();

                for _ in 0..highlight_index_in_row
                {
                    #[allow(clippy::redundant_closure_call)]
                    $draw_texture(ui, textures.next().unwrap());
                }

                #[allow(clippy::redundant_closure_call)]
                $draw_texture(ui, textures.next().unwrap()).highlight();

                for texture in textures
                {
                    #[allow(clippy::redundant_closure_call)]
                    $draw_texture(ui, texture);
                }

                ui.add_space(ui.available_width());
            });
        }

        for chunk in chunks
        {
            #[allow(clippy::redundant_closure_call)]
            $row_without_highlight($ui, chunk);
        }

        len
    }};
}

pub(in crate::map::editor::state) use textures_gallery;

//=======================================================================//
// TRAITS
//
//=======================================================================//

/// A trait to know whever an UI element has lost focus by the standards required by the editor.
pub(in crate::map::editor::state) trait ActuallyLostFocus
{
    /// Whever an UI element has lost focus by the standards required by the editor.
    #[must_use]
    fn actually_lost_focus(&self) -> bool;
}

impl ActuallyLostFocus for egui::Response
{
    #[inline]
    fn actually_lost_focus(&self) -> bool
    {
        if self.lost_focus()
        {
            return true;
        }

        self.has_focus() &&
            self.ctx
                .input(|i| i.pointer.primary_clicked() || i.pointer.secondary_clicked()) &&
            !self.contains_pointer()
    }
}

//=======================================================================//

/// A trait to know if an UI element is being interacted with.
pub(in crate::map::editor::state) trait Interacting
{
    /// Whever the UI element is being interacted with.
    #[must_use]
    fn interacting(&self) -> bool;
}

impl Interacting for egui::Response
{
    #[inline]
    fn interacting(&self) -> bool { self.has_focus() || self.lost_focus() }
}

//=======================================================================//

/// A trait to return the info to close a window.
trait WindowCloserInfo
{
    /// Returns the info to close the window, if open.
    fn window_closer(&self) -> Option<WindowCloser>;
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// A command to be executed following a UI element press.
#[derive(Clone, Copy, Default)]
pub(in crate::map::editor::state) enum Command
{
    /// Nothing to do.
    #[default]
    None,
    /// Change the active tool.
    ChangeTool(Tool),
    /// Open new map.
    New,
    /// Save current map.
    Save,
    /// Save current map to new path.
    SaveAs,
    /// Open map.
    Open,
    /// Export map.
    Export,
    /// Export the map's animations to a .anms file.
    ExportAnimations,
    /// Import an .anms file.
    ImportAnimations,
    /// Export the map's props  to a .prps file.
    ExportProps,
    /// Import a .prps file.
    ImportProps,
    /// Select all entities.
    SelectAll,
    /// Copy the selected entities.
    Copy,
    /// Paste the copied entities.
    Paste,
    /// Cut the selected entities.
    Cut,
    /// Duplicate the selected entities.
    Duplicate,
    /// Undo.
    Undo,
    /// Redo.
    Redo,
    /// Toggle the grid.
    ToggleGrid,
    /// Increase the grid size.
    IncreaseGridSize,
    /// Decrease the grid size.
    DecreaseGridSize,
    /// Shift the grid.
    ShifGrid,
    /// Toggle the tooltips.
    ToggleTooltips,
    /// Toggle the cursor grid snap.
    ToggleCursorSnap,
    /// Toggles the map preview.
    ToggleMapPreview,
    /// Toggles the collision of the selected brushes.
    ToggleCollision,
    /// Reload the textures.
    ReloadTextures,
    /// Reload the things.
    ReloadThings,
    /// Zoom on the selected entities.
    QuickZoom,
    /// Snap the vertexes of the selected brushes.
    QuickSnap,
    /// Quits the application
    Quit,
    #[cfg(feature = "debug")]
    /// Toggles the debug lines.
    ToggleDebugLines
}

impl Command
{
    /// Returns whever `self` represents a command that changes the entities in the map.
    #[inline]
    #[must_use]
    pub const fn world_edit(self) -> bool
    {
        matches!(
            self,
            Self::ChangeTool(_) |
                Self::Paste |
                Self::Cut |
                Self::Duplicate |
                Self::Undo |
                Self::Redo |
                Self::QuickSnap
        )
    }
}

//=======================================================================//

/// The information required to close a UI window.
#[allow(clippy::type_complexity)]
#[must_use]
#[derive(Clone, Copy)]
enum WindowCloser
{
    /// Texture editor.
    TextureEditor(egui::LayerId, fn(&mut TextureEditor)),
    /// Settings window.
    Settings(egui::LayerId, fn(&mut SettingsWindow)),
    /// Properties window.
    Properties(egui::LayerId, fn(&mut PropertiesWindow)),
    /// Manual window.
    Manual(egui::LayerId, fn(&mut Manual))
}

impl WindowCloser
{
    /// Returns the contained [`LayerId`].
    #[inline]
    #[must_use]
    const fn layer_id(self) -> egui::LayerId
    {
        let (Self::TextureEditor(id, _) |
        Self::Settings(id, _) |
        Self::Properties(id, _) |
        Self::Manual(id, _)) = self;
        id
    }

    /// Checks whever a UI window should be closed.
    #[inline]
    fn check_window_close(
        layer_ids: impl ExactSizeIterator<Item = egui::LayerId>,
        inputs: &InputsPresses,
        ui: &mut Ui
    )
    {
        if !inputs.f4.just_pressed()
        {
            return;
        }

        let mut windows = [
            ui.texture_editor.window_closer(),
            ui.settings_window.window_closer(),
            ui.properties_window.window_closer(),
            ui.manual.window_closer()
        ]
        .into_iter()
        .flatten()
        .collect::<ArrayVec<_, 5>>();

        if windows.is_empty()
        {
            return;
        }

        let mut topmost_window = None;

        'outer: for id in layer_ids
            .skip_while(|id| id.order != egui::Order::Middle)
            .filter(|id| id.order == egui::Order::Middle)
        {
            for i in 0..windows.len()
            {
                if windows[i].layer_id() != id
                {
                    continue;
                }

                topmost_window = Some(windows[i]);
                _ = windows.swap_remove(i);

                if windows.is_empty()
                {
                    break 'outer;
                }

                break;
            }
        }

        match return_if_none!(topmost_window)
        {
            Self::Settings(_, closer) => closer(&mut ui.settings_window),
            Self::TextureEditor(_, closer) => closer(&mut ui.texture_editor),
            Self::Properties(_, closer) => closer(&mut ui.properties_window),
            Self::Manual(_, closer) => closer(&mut ui.manual)
        };
    }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The buttons used to change the currently used tool.
pub(in crate::map::editor::state) struct ToolsButtons
{
    /// The icons of the tools.
    icons:   [egui::TextureId; Tool::SIZE + SubTool::SIZE],
    /// The tooltip showed when a tool button is being hovered.
    tooltip: Tooltip
}

impl ToolsButtons
{
    /// Returns a new [`ToolsButtons`].
    #[inline]
    #[must_use]
    fn new(asset_server: &AssetServer, user_textures: &mut EguiUserTextures) -> Self
    {
        let mut iter = Tool::iter()
            .map(ToolInterface::icon_file_name)
            .chain(SubTool::iter().map(ToolInterface::icon_file_name))
            .map(|file| user_textures.add_image(asset_server.load(embedded_asset_path(file))));

        Self {
            icons:   std::array::from_fn(|_| iter.next_value()),
            tooltip: Tooltip::new()
        }
    }

    /// The index of `tool`.
    #[inline]
    #[must_use]
    fn index(tool: impl ToolInterface) -> usize
    {
        if tool.subtool()
        {
            tool.index() + Tool::SIZE
        }
        else
        {
            tool.index()
        }
    }

    /// Draws the tool's UI element.
    #[inline]
    #[must_use]
    pub fn draw<T, E>(
        &mut self,
        ui: &mut egui::Ui,
        bundle: &StateUpdateBundle,
        tool: T,
        change_conditions: &ChangeConditions,
        enabled: &E
    ) -> bool
    where
        T: ToolInterface,
        E: EnabledTool<Item = T>
    {
        let response = ui.add_enabled(
            tool.change_conditions_met(change_conditions),
            egui::ImageButton::new(egui::Image::new((
                self.icons[Self::index(tool)],
                ICON_DRAW_SIZE
            )))
        );

        self.tooltip.show(bundle, tool, &response);
        let clicked = response.clicked();

        if clicked || enabled.is_tool_enabled(tool)
        {
            response.highlight();
        }

        clicked
    }

    /// Draws the image representing `tool`.
    #[inline]
    fn image(&self, ui: &mut egui::Ui, tool: impl ToolInterface)
    {
        ui.image((self.icons[Self::index(tool)], ICON_DRAW_SIZE));
    }
}

//=======================================================================//

/// The result of the interaction with the UI elements.
#[must_use]
pub(in crate::map::editor::state) struct Interaction
{
    /// Whever the UI is currently being hovered
    pub hovered: bool,
    /// A command to be executed.
    pub command: Command
}

//=======================================================================//

/// The UI of the editor.
pub(in crate::map::editor::state) struct Ui
{
    /// The buttons to enable the tools.
    tools_buttons:        ToolsButtons,
    /// The id of the left panel
    left_panel_layer_id:  egui::LayerId,
    /// The id of the right panel.
    right_panel_layer_id: egui::LayerId,
    /// The settings window.
    settings_window:      SettingsWindow,
    /// The parameters window.
    properties_window:    PropertiesWindow,
    /// The texture editor.
    texture_editor:       TextureEditor,
    /// The manual.
    manual:               Manual
}

impl Placeholder for Ui
{
    #[inline]
    unsafe fn placeholder() -> Self
    {
        Self {
            tools_buttons:        ToolsButtons {
                icons:   [egui::TextureId::default(); Tool::SIZE + SubTool::SIZE],
                tooltip: Tooltip::new()
            },
            left_panel_layer_id:  egui::LayerId::background(),
            right_panel_layer_id: egui::LayerId::background(),
            settings_window:      SettingsWindow::default(),
            properties_window:    PropertiesWindow::placeholder(),
            texture_editor:       TextureEditor::default(),
            manual:               Manual::default()
        }
    }
}

impl Ui
{
    /// Returns a new [`Ui`].
    #[inline]
    #[must_use]
    pub fn new(
        asset_server: &AssetServer,
        user_textures: &mut EguiUserTextures,
        brushes_default_properties: &DefaultProperties,
        things_default_properties: &DefaultProperties
    ) -> Self
    {
        Self {
            tools_buttons:        ToolsButtons::new(asset_server, user_textures),
            left_panel_layer_id:  egui::LayerId::background(),
            right_panel_layer_id: egui::LayerId::background(),
            settings_window:      SettingsWindow::default(),
            properties_window:    PropertiesWindow::new(
                brushes_default_properties,
                things_default_properties
            ),
            texture_editor:       TextureEditor::default(),
            manual:               Manual::default()
        }
    }

    /// Updates the UI.
    #[inline]
    pub fn frame_start_update(
        &mut self,
        bundle: &mut StateUpdateBundle,
        core: &mut Core,
        manager: &mut EntitiesManager,
        inputs: &mut InputsPresses,
        edits_history: &mut EditsHistory,
        clipboard: &mut Clipboard,
        grid: Grid,
        settings: &mut ToolsSettings,
        tool_change_conditions: &ChangeConditions
    ) -> Interaction
    {
        bundle.egui_context.memory(|mem| {
            WindowCloser::check_window_close(mem.layer_ids(), inputs, self);
        });

        // Top bar.
        let mut command = self.menu_bar(bundle, manager, core);

        // Manual menu.
        self.manual.show(bundle, &self.tools_buttons);

        // Floating windows.
        let mut focused = if core.map_preview()
        {
            false
        }
        else
        {
            self.texture_editor
                .show(bundle, manager, edits_history, clipboard, inputs, settings)
        };

        focused |= self.settings_window.show(bundle, inputs) |
            self.properties_window
                .show(bundle, manager, edits_history, clipboard, inputs);

        // Panels.
        let us_context = unsafe { std::ptr::from_mut(bundle.egui_context).as_mut().unwrap() };

        // Panels.
        self.right_panel_layer_id = egui::SidePanel::right("subtools")
            .resizable(false)
            .exact_width(RIGHT_SIDE_PANEL_WIDTH)
            .show(us_context, |ui| {
                ui.vertical(|ui| {
                    ui.add_space(ICONS_PADDING.y);
                    ui.spacing_mut().item_spacing = ICONS_PADDING;

                    core.draw_subtools(
                        ui,
                        bundle,
                        manager,
                        edits_history,
                        grid,
                        &mut self.tools_buttons,
                        tool_change_conditions
                    );
                });
            })
            .response
            .layer_id;

        // Left Side Panel.
        self.left_panel_layer_id = egui::SidePanel::left("tools")
            .resizable(false)
            .exact_width(LEFT_SIDE_PANEL_WIDTH)
            .show(us_context, |ui| {
                // Tool icons.
                if let Some(tool) = self.tool_icons(core, ui, bundle, tool_change_conditions)
                {
                    command = Command::ChangeTool(tool);
                }

                // Cursor info.
                Self::cursor_info(bundle.cursor, ui);

                // Grid info.
                Self::grid_info(grid, ui);

                // Camera info.
                Self::camera_info(bundle.camera, ui);

                // Extra tool info.
                focused |= core.tool_ui(manager, inputs, edits_history, clipboard, ui, settings);
            })
            .response
            .layer_id;

        // Bottom panel
        focused |= core.bottom_panel(bundle, manager, inputs, edits_history, clipboard);

        if focused
        {
            inputs.clear();
            bundle.key_inputs.clear();
        }

        // Output.
        Interaction {
            hovered: !egui::CentralPanel::default()
                .frame(egui::Frame::none())
                .show(bundle.egui_context, |_| {})
                .response
                .contains_pointer(),
            command
        }
    }

    /// Concludes the UI update.
    #[inline]
    pub(in crate::map) fn frame_end_update(&self, egui_context: &egui::Context)
    {
        egui_context.move_to_top(self.left_panel_layer_id);
        egui_context.move_to_top(self.right_panel_layer_id);
    }

    /// Updates the overall texture.
    #[inline]
    pub fn update_overall_texture(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &EntitiesManager
    )
    {
        self.texture_editor.update_overall_texture(drawing_resources, manager);
    }

    /// Updates the overall [`Brush`] collision.
    #[inline]
    pub fn update_overall_brushes_collision(&mut self, manager: &EntitiesManager)
    {
        self.properties_window.update_overall_brushes_collision(manager);
    }

    /// Updates all overall [`Brush`] properties.
    #[inline]
    pub fn update_overall_total_brush_properties(&mut self, manager: &EntitiesManager)
    {
        self.properties_window.update_overall_total_brush_properties(manager);
    }

    /// Updates the overall [`Brush`] property with key `k`.
    #[inline]
    pub fn update_overall_brushes_property(&mut self, manager: &EntitiesManager, k: &str)
    {
        self.properties_window.update_overall_brushes_property(manager, k);
    }

    /// Updates the hardcoded [`ThingInstance`] properties.
    #[inline]
    pub fn update_overall_things_info(&mut self, manager: &EntitiesManager)
    {
        self.properties_window.update_overall_things_info(manager);
    }

    /// Updates the overall [`ThingInstance`] properties.
    #[inline]
    pub fn update_overall_total_things_properties(&mut self, manager: &EntitiesManager)
    {
        self.properties_window.update_overall_total_things_properties(manager);
    }

    /// Updates the overall [`ThingInstance`] property with key `k`.
    #[inline]
    pub fn update_overall_things_property(&mut self, manager: &EntitiesManager, k: &str)
    {
        self.properties_window.update_overall_things_property(manager, k);
    }

    /// Schedules the texture animation update.
    #[inline]
    pub fn schedule_texture_animation_update(&mut self)
    {
        self.texture_editor.schedule_texture_animation_update();
    }

    /// Draws the menu bar.
    #[inline]
    #[must_use]
    fn menu_bar(
        &mut self,
        bundle: &mut StateUpdateBundle,
        manager: &EntitiesManager,
        core: &mut Core
    ) -> Command
    {
        let mut command = Command::None;

        egui::TopBottomPanel::top("top_panel")
        .exact_height(MENU_BAR_HEIGHT)
        .show(bundle.egui_context, |ui| {
            egui::menu::bar(ui, |ui| {
                let spacing = ui.spacing_mut();
                spacing.button_padding = [6f32; 2].into();
                spacing.item_spacing = [2f32; 2].into();
                ui.visuals_mut().menu_rounding = 0f32.into();

                let StateUpdateBundle { window, camera, config: Config { binds, exporter, .. }, .. } = bundle;

                let select_all = core.select_all_available();
                let copy_paste = core.copy_paste_available();
                let undo_redo = core.undo_redo_available();
                let reload = !core.map_preview();
                let export = exporter.is_some();
                let quick_snap = manager.any_selected_brushes();
                let quick_zoom = manager.any_selected_entities();

                /// Draws a menu button.
                macro_rules! menu_button {
                    (
                        $ui:ident,
                        $label:literal,
                        $action:block
                        $(, $shortcut:expr)?
                    ) => {
                        if $ui.add(egui::Button::new($label)$(.shortcut_text($shortcut))?).clicked()
                        {
                            $action
                            $ui.close_menu();
                        }
                    };

                    (
                        $ui:ident,
                        $enabled:ident,
                        $label:literal,
                        $action:block
                        $(, $shortcut:expr)?
                    ) => {
                        if $ui.add_enabled($enabled, egui::Button::new($label)$(.shortcut_text($shortcut))?).clicked()
                        {
                            $action
                            $ui.close_menu();
                        }
                    };
                }

                /// Draws a submenu.
                macro_rules! submenu {
                    (
                        $ui:ident,
                        $label:literal,
                        $((
                            $($cfg:ident, )?
                            $tag:literal,
                            $($enabled:ident, )?
                            $action:block
                            $(, $shortcut:expr)?
                        )),
                    +) => {
                        egui::menu::menu_button($ui, $label, |ui| {
                            ui.set_min_width(200f32);
                            let spacing = ui.spacing_mut();
                            spacing.button_padding = [6f32; 2].into();
                            spacing.item_spacing = [2f32; 2].into();
                            ui.visuals_mut().menu_rounding = 0f32.into();

                            $(
                                $(#[$cfg(feature = "debug")])?
                                menu_button!(ui, $($enabled, )? $tag, $action $(, $shortcut)?);
                            )+
                        })
                        .response
                        .hovered();
                    };
                }

                submenu!(
                    ui,
                    "File",
                    ("New", {
                        command = Command::New;
                    }, HardcodedActions::New.key_combo()),
                    ("Open", {
                        command = Command::Open;
                    }, HardcodedActions::Open.key_combo()),
                    ("Save", {
                        command = Command::Save;
                    }, HardcodedActions::Save.key_combo()),
                    ("Save as", {
                        command = Command::SaveAs;
                    }, "Ctrl+Shift+S"),
                    ("Export", export, {
                        command = Command::Export;
                    }, HardcodedActions::Export.key_combo()),
                    ("Import animations", {
                        command = Command::ImportAnimations;
                    }),
                    ("Export animations", {
                        command = Command::ExportAnimations;
                    }),
                    ("Import props", {
                        command = Command::ImportProps;
                    }),
                    ("Export props", {
                        command = Command::ExportProps;
                    }),
                    ("Quit", {
                        command = Command::Quit;
                    }, HardcodedActions::Quit.key_combo())
                );

                submenu!(
                    ui,
                    "Edit",
                    ("Select all", select_all, {
                        command = Command::SelectAll;
                    }, HardcodedActions::SelectAll.key_combo()),
                    ("Copy", copy_paste, {
                        command = Command::Copy;
                    }, HardcodedActions::Copy.key_combo()),
                    ("Cut", copy_paste, {
                        command = Command::Cut;
                    }, HardcodedActions::Cut.key_combo()),
                    ("Paste", copy_paste, {
                        command = Command::Paste;
                    }, HardcodedActions::Paste.key_combo()),
                    ("Duplicate", copy_paste, {
                        command = Command::Duplicate;
                    }, HardcodedActions::Duplicate.key_combo()),
                    ("Undo", undo_redo, {
                        command = Command::Undo;
                    }, HardcodedActions::Undo.key_combo()),
                    ("Redo", undo_redo, {
                        command = Command::Redo;
                    }, HardcodedActions::Redo.key_combo()),
                    ("Quick snap", quick_snap, {
                        command = Command::QuickSnap;
                    }, format!("Alt+{}", Tool::Snap.keycode_str(binds))),
                    ("Texture editor", {
                        self.texture_editor.toggle();
                    }, binds.get(Bind::TextureEditor).map_or("", FromToStr::to_str)),
                    ("Properties", {
                        self.properties_window.toggle();
                    }, binds.get(Bind::PropertiesEditor).map_or("", FromToStr::to_str))
                );

                submenu!(
                    ui,
                    "View",
                    ("Zoom in", {
                        camera.zoom_in();
                    }, HardcodedActions::ZoomIn.key_combo()),
                    ("Zoom out", {
                        camera.zoom_out();
                    }, HardcodedActions::ZoomOut.key_combo()),
                    ("Quick zoom", quick_zoom, {
                        command = Command::QuickZoom;
                    }, format!("Alt+{}", Tool::Zoom.keycode_str(binds))),
                    ("Fullscreen", {
                        window.mode.toggle();
                    }, HardcodedActions::Fullscreen.key_combo()),
                    ("Toggle map preview", {
                        command = Command::ToggleMapPreview;
                    })
                );

                submenu!(
                    ui,
                    "Options",
                    ("Toggle grid", {
                        command = Command::ToggleGrid;
                    }, Bind::ToggleGrid.keycode_str(binds)),
                    ("Increase grid size", {
                        command = Command::IncreaseGridSize;
                    }, Bind::IncreaseGridSize.keycode_str(binds)),
                    ("Decrease grid size", {
                        command = Command::DecreaseGridSize;
                    }, Bind::DecreaseGridSize.keycode_str(binds)),
                    ("Shift grid", {
                        command = Command::ShifGrid;
                    }, Bind::ShiftGrid.keycode_str(binds)),
                    ("Toggle tooltips", {
                        command = Command::ToggleTooltips;
                    }, Bind::ToggleTooltips.keycode_str(binds)),
                    ("Toggle cursor snap", {
                        command = Command::ToggleCursorSnap;
                    }, Bind::ToggleCursorSnap.keycode_str(binds)),
                    ("Toggle collision overlay", {
                        command = Command::ToggleCollision;
                    }, Bind::ToggleCollision.keycode_str(binds)),
                    ("Settings", {
                        self.settings_window.toggle();
                    }, Bind::Settings.keycode_str(binds)),
                    ("Reload textures", reload, {
                        command = Command::ReloadTextures;
                    }),
                    ("Reload things", reload, {
                        command = Command::ReloadThings;
                    }),
                    (cfg, "Toggle debug lines", {
                        command = Command::ToggleDebugLines;
                    })
                );

                submenu!(
                    ui,
                    "Help",
                    ("Manual", {
                        self.manual.toggle();
                    }, HardcodedActions::ToggleManual.key_combo())
                );
            });
        });

        command
    }

    /// Draws the tools icons. Returns the clicked tool, if any.
    #[inline]
    #[must_use]
    fn tool_icons(
        &mut self,
        core: &Core,
        ui: &mut egui::Ui,
        bundle: &mut StateUpdateBundle,
        tool_change_conditions: &ChangeConditions
    ) -> Option<Tool>
    {
        /// The icons in each row.
        const ICONS_PER_ROW: usize = 3;

        let row_padding =
            (ui.available_width() - 24f32 - ICON_DRAW_SIZE[1] * 3f32 - ICONS_PADDING.x * 2f32) /
                2f32; // Magic magic magic

        ui.add_space(ICONS_PADDING.y);

        let mut tool_to_enable = None;
        let mut tool_image_buttons_row = |ui: &mut egui::Ui, range| {
            ui.spacing_mut().item_spacing = ICONS_PADDING;
            ui.add_space(row_padding);

            for i in range
            {
                let tool = Into::<Tool>::into(i);

                if self
                    .tools_buttons
                    .draw(ui, bundle, tool, tool_change_conditions, core)
                {
                    tool_to_enable = tool.into();
                }
            }
        };

        for i in 0..Tool::SIZE / ICONS_PER_ROW
        {
            ui.horizontal(|ui| {
                let i = i * 3;
                tool_image_buttons_row(ui, i..i + 3);
            });
        }

        ui.horizontal(|ui| {
            tool_image_buttons_row(ui, (Tool::SIZE / ICONS_PER_ROW) * ICONS_PER_ROW..Tool::SIZE);
        });

        tool_to_enable
    }

    /// The info concerning the cursor.
    #[inline]
    fn cursor_info(cursor: &Cursor, ui: &mut egui::Ui)
    {
        ui.separator();

        let pos = cursor.world_snapped();

        ui.label(egui::RichText::new(format!(
            "CURSOR\nX: {:.2}\nY: {:.2}\nSnapped: {}",
            pos.x,
            pos.y,
            cursor.snap()
        )));
    }

    /// The info concerning the grid.
    #[inline]
    fn grid_info(grid: Grid, ui: &mut egui::Ui)
    {
        ui.separator();

        ui.label(egui::RichText::new(format!(
            "GRID\nSize: {}\nShifted: {}",
            grid.size(),
            grid.shifted
        )));
    }

    /// The info concerning the camera.
    #[inline]
    fn camera_info(camera: &Transform, ui: &mut egui::Ui)
    {
        ui.separator();

        let ui_vector = ui_camera_displacement() * camera.scale();
        let pos = camera.pos() + ui_vector;

        ui.label(egui::RichText::new(format!(
            "CAMERA\nX: {:.2}\nY: {:.2}\nScale: {:.2}",
            pos.x,
            pos.y,
            camera.scale()
        )));
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Returns the width taken by the UI elements on the left of the screen.
#[inline]
#[must_use]
pub(in crate::map) const fn ui_left_space() -> f32 { LEFT_SIDE_PANEL_REAL_WIDTH }

//=======================================================================//

/// Returns the width taken by the UI elements on the right of the screen.
#[inline]
#[must_use]
pub(in crate::map) const fn ui_right_space() -> f32 { RIGHT_SIDE_PANEL_REAL_WIDTH }

//=======================================================================//

/// Returns the width taken by the UI elements on the top of the screen.
#[inline]
#[must_use]
pub(in crate::map) const fn ui_top_space() -> f32 { MENU_BAR_REAL_HEIGHT }

//=======================================================================//

/// The amount the camera needs to be shifted to be centered in the portion of the window where the
/// drawn map can be seen.
#[inline]
#[must_use]
pub(in crate::map) fn ui_camera_displacement() -> Vec2
{
    Vec2::new(ui_left_space() - ui_right_space(), -ui_top_space()) / 2f32
}

//=======================================================================//

/// Returns a vector describing the area of the window taken by permanent UI elements.
#[inline]
#[must_use]
pub(in crate::map) fn ui_size() -> Vec2
{
    Vec2::new(ui_left_space() + ui_right_space(), ui_top_space())
}

//=======================================================================//

/// Returns a window centered in the portion of the window where the map can be seen.
pub(in crate::map) fn centered_window<'open>(
    window: &bevy::prelude::Window,
    title: &'static str
) -> egui::Window<'open>
{
    egui::Window::new(title)
        .pivot(egui::Align2::CENTER_CENTER)
        .fixed_pos(map_view_center(window))
        .movable(false)
        .collapsible(false)
        .resizable(false)
}

//=======================================================================//

/// Returns the UI size of the viewport based on the `window` sizes.
#[inline]
#[must_use]
pub(in crate::map) fn map_view_center(window: &bevy::prelude::Window) -> egui::Pos2
{
    let left_space = ui_left_space();
    let top_space = ui_top_space();

    egui::pos2(
        left_space + (window.width() - left_space - ui_right_space()) / 2f32,
        top_space + (window.height() - top_space) / 2f32
    )
}
