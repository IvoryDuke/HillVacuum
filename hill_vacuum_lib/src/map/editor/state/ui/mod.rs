mod controls_window;
mod manual;
mod minus_plus_buttons;
pub(in crate::map::editor::state) mod overall_value_field;
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
use is_executable::IsExecutable;
use shared::{continue_if_none, return_if_none, NextValue};

use self::{
    controls_window::ControlsWindow,
    manual::Manual,
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
    config::controls::bind::Bind,
    embedded_assets::embedded_asset_path,
    map::{
        drawer::drawing_resources::DrawingResources,
        editor::{cursor_pos::Cursor, StateUpdateBundle}
    },
    utils::{
        identifiers::EntityId,
        misc::{Camera, FromToStr, Toggle},
        overall_value::{OverallValue, OverallValueInterface}
    },
    HardcodedActions
};

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

const LEFT_SIDE_PANEL_WIDTH: f32 = 184f32;
const RIGHT_SIDE_PANEL_WIDTH: f32 = 54f32;
const MENU_BAR_HEIGHT: f32 = 28f32;
const LEFT_SIDE_PANEL_REAL_WIDTH: f32 = 1.08 * LEFT_SIDE_PANEL_WIDTH;
const RIGHT_SIDE_PANEL_REAL_WIDTH: f32 = 0.99 * RIGHT_SIDE_PANEL_WIDTH;
const MENU_BAR_REAL_HEIGHT: f32 = 1.08 * MENU_BAR_HEIGHT;
const ICON_DRAW_SIZE: egui::Vec2 = egui::Vec2::splat(32f32);
const ICONS_PADDING: egui::Vec2 = egui::Vec2::new(8f32, 4f32);

//=======================================================================//
// MACROS
//
//=======================================================================//

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

pub(in crate::map::editor::state) trait ActuallyLostFocus
{
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

pub(in crate::map::editor::state) trait Interacting
{
    #[must_use]
    fn interacting(&self) -> bool;
}

impl Interacting for egui::Response
{
    #[inline]
    fn interacting(&self) -> bool { self.has_focus() || self.lost_focus() }
}

//=======================================================================//

trait WindowCloserInfo
{
    fn window_closer(&self) -> Option<WindowCloser>;
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[derive(Clone, Copy, Default)]
pub(in crate::map::editor::state) enum Command
{
    #[default]
    None,
    ChangeTool(Tool),
    New,
    Save,
    SaveAs,
    Open,
    Export,
    ExportAnimations,
    ImportAnimations,
    ExportProps,
    ImportProps,
    SelectAll,
    Copy,
    Paste,
    Cut,
    Duplicate,
    Undo,
    Redo,
    ToggleGrid,
    IncreaseGridSize,
    DecreaseGridSize,
    ShifGrid,
    ToggleTooltips,
    ToggleCursorSnap,
    ToggleMapPreview,
    ToggleCollision,
    ReloadTextures,
    ReloadThings,
    QuickZoom,
    QuickSnap,
    Quit,
    #[cfg(feature = "debug")]
    ToggleDebugLines
}

impl Command
{
    #[inline]
    #[must_use]
    pub fn world_edit(self) -> bool
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

#[allow(clippy::type_complexity)]
#[must_use]
#[derive(Clone, Copy)]
enum WindowCloser
{
    TextureEditor((egui::LayerId, fn(&mut TextureEditor))),
    Controls((egui::LayerId, fn(&mut ControlsWindow))),
    Manual((egui::LayerId, fn(&mut Manual)))
}

impl WindowCloser
{
    #[inline]
    #[must_use]
    fn layer_id(self) -> egui::LayerId
    {
        let (Self::TextureEditor((id, _)) | Self::Controls((id, _)) | Self::Manual((id, _))) = self;
        id
    }

    #[inline]
    fn check_window_close(
        layer_ids: impl ExactSizeIterator<Item = egui::LayerId>,
        inputs: &InputsPresses,
        ui: &mut Ui
    )
    {
        if !inputs.esc.just_pressed()
        {
            return;
        }

        let mut windows = [
            ui.texture_editor.window_closer(),
            ui.controls_window.window_closer(),
            ui.manual.window_closer()
        ]
        .into_iter()
        .flatten()
        .collect::<ArrayVec<_, 4>>();

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
            Self::TextureEditor(closer) => closer.1(&mut ui.texture_editor),
            Self::Controls(closer) => closer.1(&mut ui.controls_window),
            Self::Manual(closer) => closer.1(&mut ui.manual)
        };
    }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

pub(in crate::map::editor::state) struct ToolsButtons
{
    icons:   [egui::TextureId; Tool::SIZE + SubTool::SIZE],
    tooltip: Tooltip
}

impl ToolsButtons
{
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

    #[inline]
    fn image(&self, ui: &mut egui::Ui, tool: impl ToolInterface)
    {
        ui.image((self.icons[Self::index(tool)], ICON_DRAW_SIZE));
    }
}

//=======================================================================//

#[must_use]
pub(in crate::map::editor::state) struct Interaction
{
    pub hovered: bool,
    pub focused: bool,
    pub command: Command
}

//=======================================================================//

pub(in crate::map::editor::state) struct Ui
{
    tools_buttons:        ToolsButtons,
    left_panel_layer_id:  egui::LayerId,
    right_panel_layer_id: egui::LayerId,
    controls_window:      ControlsWindow,
    texture_editor:       TextureEditor,
    manual:               Manual,
    collision:            OverallValue<bool>
}

impl Ui
{
    #[inline]
    #[must_use]
    pub fn new(asset_server: &AssetServer, user_textures: &mut EguiUserTextures) -> Self
    {
        Self {
            tools_buttons:        ToolsButtons::new(asset_server, user_textures),
            left_panel_layer_id:  egui::LayerId::background(),
            right_panel_layer_id: egui::LayerId::background(),
            controls_window:      ControlsWindow::default(),
            texture_editor:       TextureEditor::default(),
            manual:               Manual::default(),
            collision:            OverallValue::None
        }
    }

    #[inline]
    pub fn placeholder() -> Self
    {
        Self {
            tools_buttons:        ToolsButtons {
                icons:   [egui::TextureId::default(); Tool::SIZE + SubTool::SIZE],
                tooltip: Tooltip::new()
            },
            left_panel_layer_id:  egui::LayerId::background(),
            right_panel_layer_id: egui::LayerId::background(),
            controls_window:      ControlsWindow::default(),
            texture_editor:       TextureEditor::default(),
            manual:               Manual::default(),
            collision:            OverallValue::None
        }
    }

    #[inline]
    pub fn frame_start_update(
        &mut self,
        bundle: &mut StateUpdateBundle,
        core: &mut Core,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
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
        self.manual.draw(bundle, &self.tools_buttons);

        // Texture selection.
        let mut focused = if core.map_preview()
        {
            false
        }
        else
        {
            self.texture_editor
                .draw(bundle, manager, edits_history, clipboard, inputs, settings)
        };

        // Controls menu.
        focused |= self.controls_window.show(bundle);
        let us_context = unsafe { std::ptr::from_mut(bundle.egui_context).as_mut().unwrap() };

        // Panels.
        self.right_panel_layer_id = egui::SidePanel::right("sub_tools")
            .resizable(false)
            .exact_width(RIGHT_SIDE_PANEL_WIDTH)
            .show(us_context, |ui| {
                ui.vertical(|ui| {
                    ui.add_space(ICONS_PADDING.y);
                    ui.spacing_mut().item_spacing = ICONS_PADDING;

                    core.draw_sub_tools(
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

                self.collision(manager, edits_history, ui);

                // Extra tool info.
                focused |= core.tool_ui(manager, inputs, edits_history, clipboard, ui, settings);
            })
            .response
            .layer_id;

        // Bottom panel
        focused |= core.bottom_panel(bundle, manager, inputs, edits_history, clipboard);

        // Output.
        Interaction {
            hovered: !egui::CentralPanel::default()
                .frame(egui::Frame::none())
                .show(bundle.egui_context, |_| {})
                .response
                .contains_pointer(),
            focused,
            command
        }
    }

    #[inline]
    pub(in crate::map) fn frame_end_update(&self, egui_context: &egui::Context)
    {
        egui_context.move_to_top(self.left_panel_layer_id);
        egui_context.move_to_top(self.right_panel_layer_id);
    }

    #[inline]
    pub fn update_overall_texture(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &EntitiesManager
    )
    {
        self.texture_editor.update_overall_texture(drawing_resources, manager);
    }

    #[inline]
    pub fn update_overall_collision(&mut self, manager: &EntitiesManager)
    {
        self.collision = OverallValue::None;

        for brush in manager.selected_brushes()
        {
            _ = self.collision.stack(&brush.collision());
        }
    }

    #[inline]
    pub fn schedule_texture_animation_update(&mut self)
    {
        self.texture_editor.schedule_texture_animation_update();
    }

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

                let select_all = core.select_all_available();
                let copy_paste = core.copy_paste_available();
                let undo_redo = core.undo_redo_available();
                let reload = !core.map_preview();
                let export = bundle.config.exporter.is_some();
                let quick_snap = manager.selected_brushes_amount() != 0;
                let quick_zoom = manager.selected_entities_amount() != 0;

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
                    }, format!("Alt+{}", Tool::Snap.keycode_str(&bundle.config.binds))),
                    ("Texture editor", {
                        self.texture_editor.toggle();
                    }, bundle.config.binds.get(Bind::TextureEditor).unwrap().to_str())
                );

                submenu!(
                    ui,
                    "View",
                    ("Zoom in", {
                        bundle.camera.zoom_in();
                    }, HardcodedActions::ZoomIn.key_combo()),
                    ("Zoom out", {
                        bundle.camera.zoom_out();
                    }, HardcodedActions::ZoomOut.key_combo()),
                    ("Quick zoom", quick_zoom, {
                        command = Command::QuickZoom;
                    }, format!("Alt+{}", Tool::Zoom.keycode_str(&bundle.config.binds))),
                    ("Fullscreen", {
                        bundle.window.mode.toggle();
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
                    }, Bind::ToggleGrid.keycode_str(&bundle.config.binds)),
                    ("Increase grid size", {
                        command = Command::IncreaseGridSize;
                    }, Bind::IncreaseGridSize.keycode_str(&bundle.config.binds)),
                    ("Decrease grid size", {
                        command = Command::DecreaseGridSize;
                    }, Bind::DecreaseGridSize.keycode_str(&bundle.config.binds)),
                    ("Shift grid", {
                        command = Command::ShifGrid;
                    }, Bind::ShiftGrid.keycode_str(&bundle.config.binds)),
                    ("Toggle tooltips", {
                        command = Command::ToggleTooltips;
                    }, Bind::ToggleTooltips.keycode_str(&bundle.config.binds)),
                    ("Toggle cursor snap", {
                        command = Command::ToggleCursorSnap;
                    }, Bind::ToggleCursorSnap.keycode_str(&bundle.config.binds)),
                    ("Toggle collision overlay", {
                        command = Command::ToggleCollision;
                    }, Bind::ToggleCollision.keycode_str(&bundle.config.binds)),
                    ("Controls", {
                        self.controls_window.toggle();
                    }),
                    ("Exporter", {
                        if let Some(file) = rfd::FileDialog::new()
                            .set_title("Pick exporter")
                            .set_directory(std::env::current_dir().unwrap())
                            .pick_file()
                        {
                            if file.is_executable()
                            {
                                bundle.config.exporter = file.into();
                            }
                        }
                    }, {
                        match &bundle.config.exporter
                        {
                            Some(path) => path.file_stem().unwrap().to_str().unwrap(),
                            None => "",
                        }
                    }),
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

    #[inline]
    fn camera_info(camera: &Transform, ui: &mut egui::Ui)
    {
        ui.separator();

        let ui_vector = half_ui_vector() * camera.scale();
        let pos = camera.pos() + ui_vector;

        ui.label(egui::RichText::new(format!(
            "CAMERA\nX: {:.2}\nY: {:.2}\nScale: {:.2}",
            pos.x,
            pos.y,
            camera.scale()
        )));
    }

    #[inline]
    fn collision(
        &mut self,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        ui: &mut egui::Ui
    )
    {
        ui.separator();
        ui.label("COLLISION");

        ui.horizontal(|ui| {
            ui.label("Enabled:");

            let checked = match self.collision
            {
                OverallValue::None | OverallValue::NonUniform => false,
                OverallValue::Uniform(checked) => checked
            };

            let mut new_checked = checked;

            if !ui
                .add_enabled(
                    self.collision.is_some(),
                    egui::Checkbox::without_text(&mut new_checked)
                )
                .clicked() ||
                checked == new_checked
            {
                return;
            }

            for mut brush in manager.selected_brushes_mut()
            {
                edits_history
                    .collision(brush.id(), continue_if_none!(brush.set_collision(new_checked)));
            }

            self.collision = new_checked.into();
        });
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

#[inline]
#[must_use]
pub(in crate::map) fn ui_left_space() -> f32 { LEFT_SIDE_PANEL_REAL_WIDTH }

//=======================================================================//

#[inline]
#[must_use]
pub(in crate::map) fn ui_right_space() -> f32 { RIGHT_SIDE_PANEL_REAL_WIDTH }

//=======================================================================//

#[inline]
#[must_use]
pub(in crate::map) fn ui_top_space() -> f32 { MENU_BAR_REAL_HEIGHT }

//=======================================================================//

#[inline]
#[must_use]
pub(in crate::map) fn ui_vector() -> Vec2
{
    Vec2::new(ui_left_space() - ui_right_space(), -ui_top_space())
}

//=======================================================================//

#[inline]
#[must_use]
pub(in crate::map) fn half_ui_vector() -> Vec2 { ui_vector() / 2f32 }

//=======================================================================//

#[inline]
#[must_use]
pub(in crate::map) fn ui_size() -> Vec2
{
    Vec2::new(ui_left_space() + ui_right_space(), ui_top_space())
}

//=======================================================================//

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
