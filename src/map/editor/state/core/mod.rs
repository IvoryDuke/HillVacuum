mod clip_tool;
pub(in crate::map) mod cursor_delta;
pub(in crate::map::editor::state) mod draw_tool;
mod entity_tool;
mod flip_tool;
mod item_selector;
mod map_preview;
mod paint_tool;
mod path_tool;
mod rect;
pub(in crate::map::editor::state) mod rotate_tool;
mod scale_tool;
mod shatter_tool;
mod shear_tool;
mod side_tool;
mod subtract_tool;
mod thing_tool;
pub(in crate::map::editor::state) mod tool;
mod vertex_tool;
mod zoom_tool;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use glam::Vec2;
use hill_vacuum_shared::{match_or_panic, return_if_no_match};

use self::{
    cursor_delta::CursorDelta,
    tool::{
        ActiveTool,
        ChangeConditions,
        DisableSubtool,
        EditingTarget,
        EnabledTool,
        OngoingMultiframeChange,
        Tool
    }
};
use super::{
    clipboard::Clipboard,
    editor_state::{InputsPresses, ToolsSettings},
    edits_history::{edit_type::BrushType, EditsHistory},
    manager::{BrushMut, EntitiesManager, MovingMut, ThingMut},
    ui::{ToolsButtons, Ui}
};
use crate::{
    map::{
        brush::{convex_polygon::TextureSetResult, BrushData},
        drawer::{
            drawing_resources::DrawingResources,
            texture::{Sprite, TextureSettings}
        },
        editor::{
            state::{core::zoom_tool::ZoomTool, grid::Grid},
            DrawBundle,
            DrawBundleMapPreview,
            StateUpdateBundle,
            ToolUpdateBundle
        },
        path::Path,
        properties::Value,
        thing::{catalog::ThingsCatalog, ThingId, ThingInstanceData}
    },
    utils::{
        collections::HvBox,
        identifiers::{EntityId, Id}
    }
};

//=======================================================================//
// MACROS
//
//=======================================================================//

/// Draws the selected and non selected brushes.
macro_rules! draw_selected_and_non_selected_brushes {
    ($bundle:ident, $manager:ident $(, $filters:expr)?) => {
        crate::map::editor::state::core::draw_selected_and_non_selected!(
            brushes,
            $bundle,
            $manager,
            |brush, camera, drawer, color, _| {
                crate::map::brush::Brush::draw_with_color(brush, camera, drawer, color);
            }
            $(, $filters)?
        );
    };
}

use draw_selected_and_non_selected_brushes;

//=======================================================================//

/// Draws the selected and non selected [`ThingInstance`]s.
macro_rules! draw_selected_and_non_selected_things {
    ($bundle:ident, $manager:ident $(, $filters:expr)?) => {{
        crate::map::editor::state::core::draw_selected_and_non_selected!(
            things,
            $bundle,
            $manager,
            |thing,
            _: &bevy::transform::components::Transform, drawer: &mut crate::map::drawer::drawers::EditDrawer, color, _| {
                drawer.thing($bundle.things_catalog, thing, color);
            }
            $(, $filters)?
        );
    }};
}

use draw_selected_and_non_selected_things;

//=======================================================================//

/// Draws the selected and non selected [`ThingInstance`]s.
macro_rules! draw_selected_and_non_selected_sprites {
    ($bundle:ident, $manager:ident, $show_outline:expr $(, $filters:expr)?) => {{
        crate::map::editor::state::core::draw_selected_and_non_selected!(
            sprites,
            $bundle,
            $manager,
            |brush: &Brush, _: &bevy::transform::components::Transform, drawer: &mut crate::map::drawer::drawers::EditDrawer, color, show_outline: bool| {
                brush.draw_sprite(drawer, color, show_outline);
            }
            $(, $filters)?
            ; $show_outline
        );
    }};
}

use draw_selected_and_non_selected_sprites;

//=======================================================================//

/// Draws the selected and non selected `entities`.
macro_rules! draw_selected_and_non_selected {
    ($entities:ident, $bundle:ident, $manager:ident, $draw:expr $(, $filters:expr)? $(; $outline:expr)?) => { paste::paste! {
        use crate::map::drawer::color::Color;

        let DrawBundle {
            window,
            drawer,
            camera,
            ..
        } = $bundle;

        let draw_outline = $($outline ||)? false;
        let mut selected_entities_iterated = 0;
        let selected_entities_len = $manager.[< selected_ $entities _amount >]();

        let entities = $manager.[< visible_ $entities >](window, camera, drawer.grid());
        let mut entities = entities.iter()$(.filter_set_with_predicate($filters, |brush| brush.id()))?;

        for entity in &mut entities
        {
            let id = crate::utils::identifiers::EntityId::id(entity);

            if $manager.is_selected(id)
            {
                #[allow(clippy::redundant_closure_call)]
                $draw(entity, camera, drawer, Color::SelectedEntity, draw_outline);
                selected_entities_iterated += 1;

                if selected_entities_iterated == selected_entities_len
                {
                    break;
                }
            }
            else
            {
                #[allow(clippy::redundant_closure_call)]
                $draw(entity, camera, drawer, Color::NonSelectedEntity, draw_outline);
            }
        }

        for entity in entities
        {
            #[allow(clippy::redundant_closure_call)]
            $draw(entity, camera, drawer, Color::NonSelectedEntity, draw_outline);
        }
    }};
}

use draw_selected_and_non_selected;

//=======================================================================//

/// Draws the bottom UI panel.
macro_rules! bottom_area {
    (
        $self:ident,
        $egui_context:ident,
        $source:ident,
        $label:literal,
        $object:ident,
        $min_height:expr,
        $preview_frame:expr,
        $preview:expr
        $(, $drawing_resources:ident)?
    ) => {{
        const EXTRA_PADDING: f32 = 4f32;

        egui::TopBottomPanel::bottom($label)
            .resizable(true)
            .min_height($min_height + EXTRA_PADDING)
            .max_height($self.max_bottom_panel_height)
            .show($egui_context, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    const PREVIEW_FRAME: egui::Vec2 = $preview_frame;

                    #[inline]
                    fn draw_preview(
                        ui: &mut egui::Ui,
                        texture: ChunkItem,
                        clicked_prop: &mut Option<usize>
                    ) -> egui::Response
                    {
                        #[allow(clippy::redundant_closure_call)]
                        let response = $preview(ui, texture, PREVIEW_FRAME);

                        if response.clicked()
                        {
                            *clicked_prop = texture.index.into();
                        }

                        response
                    }

                    #[inline]
                    fn row_without_highlight<'a>(
                        ui: &mut egui::Ui,
                        chunk: impl Iterator<Item = ChunkItem<'a>>,
                        clicked_prop: &mut Option<usize>
                    )
                    {
                        ui.horizontal(|ui| {
                            for texture in chunk
                            {
                                draw_preview(ui, texture, clicked_prop);
                            }

                            ui.add_space(ui.available_width());
                        });
                    }

                    let mut clicked = None;
                    let textures_per_row = crate::map::editor::state::ui::texture_per_row(ui, PREVIEW_FRAME.x);

                    paste::paste! {
                        crate::map::editor::state::ui::textures_gallery!(
                            ui,
                            textures_per_row,
                            $source.[< chunked_ $object s >](textures_per_row $(, $drawing_resources)?),
                            $source.[< selected_ $object _index>](),
                            |ui, texture| { draw_preview(ui, texture, &mut clicked) },
                            |ui, chunk| row_without_highlight(ui, chunk, &mut clicked)
                        );
                    }

                    $self.max_bottom_panel_height = ui.min_rect().height() + EXTRA_PADDING;
                    clicked
                }).inner
            }).inner
    }};
}

use bottom_area;

//=======================================================================//

/// Generates the definition of a [`SelectedVertexes`] struct.
macro_rules! selected_vertexes {
    ($count:ident) => {
        /// A record of the selected brushes selected vertexes.
        #[must_use]
        #[derive(Debug)]
        struct SelectedVertexes(crate::utils::collections::HvHashMap<Id, u8>, usize);

        impl Default for SelectedVertexes
        {
            #[inline]
            fn default() -> Self { Self(crate::utils::collections::hv_hash_map![], 0) }
        }

        impl SelectedVertexes
        {
            /// Whether there are any selected vertexes.
            #[inline]
            #[must_use]
            pub const fn any_selected_vx(&self) -> bool { self.1 != 0 }

            /// Whether the vertexes merge is available.
            #[inline]
            #[must_use]
            pub const fn vx_merge_available(&self) -> bool
            {
                self.1 > 2 && self.1 < u8::MAX as usize
            }

            /// Inserts the selected vertexes of `brush`.
            #[inline]
            pub fn insert(&mut self, brush: &Brush)
            {
                assert!(brush.has_selected_vertexes(), "Brush has no selected vertexes.");

                self.0.insert(brush.id(), brush.$count());
                self.1 = self.0.iter().fold(0, |acc, (_, n)| acc + *n as usize);
            }

            /// Removes the selected vertexes of `brush`.
            #[inline]
            pub fn remove(&mut self, brush: &Brush)
            {
                use crate::map::AssertedInsertRemove;

                assert!(!brush.has_selected_vertexes(), "Brush has selected vertexes.");
                self.1 -= self.0.asserted_remove(brush.id_as_ref()) as usize;
            }

            /// Removes the selected vertexes associated with the brush with [`Id`]
            /// `identifier`.
            #[inline]
            pub fn remove_id(&mut self, manager: &EntitiesManager, identifier: Id)
            {
                use crate::map::AssertedInsertRemove;

                assert!(!manager.entity_exists(identifier), "Brush exists.");
                self.1 -= self.0.asserted_remove(&identifier) as usize;
            }

            /// Clears the selected vertexes.
            #[allow(dead_code)]
            #[inline]
            pub fn clear(&mut self)
            {
                self.0.clear();
                self.1 = 0;
            }
        }
    };
}

use selected_vertexes;

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The result of a vertex selection toggle.
enum VertexesToggle
{
    /// None.
    None,
    /// Vertex is now selected.
    Selected,
    /// Vertex is now not selected.
    NonSelected
}

impl From<bool> for VertexesToggle
{
    #[inline]
    #[must_use]
    fn from(value: bool) -> Self
    {
        if value
        {
            return Self::Selected;
        }

        Self::NonSelected
    }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

#[allow(clippy::missing_docs_in_private_items)]
type PreviousActiveTool = HvBox<ActiveTool>;

//=======================================================================//

/// An interface to the editor [`Core`] for the undo/redo routines.
#[allow(clippy::missing_docs_in_private_items)]
pub(in crate::map::editor::state) struct UndoRedoInterface<'a>
{
    things_catalog: &'a ThingsCatalog,
    manager:        &'a mut EntitiesManager,
    active_tool:    &'a mut ActiveTool
}

impl<'a> UndoRedoInterface<'a>
{
    /// Returns a new [`UndoRedoInterface`].
    #[inline]
    #[must_use]
    fn new(
        core: &'a mut Core,
        things_catalog: &'a ThingsCatalog,
        manager: &'a mut EntitiesManager
    ) -> Self
    {
        Self {
            things_catalog,
            manager,
            active_tool: if let ActiveTool::Zoom(..) = core.active_tool
            {
                &mut **match_or_panic!(
                    &mut core.active_tool,
                    ActiveTool::Zoom(ZoomTool {
                        previous_active_tool,
                        ..
                    }),
                    previous_active_tool
                )
            }
            else
            {
                &mut core.active_tool
            }
        }
    }

    /// Selects the entity with [`Id`] `identifier`.
    #[inline]
    pub fn select_entity(&mut self, identifier: Id)
    {
        self.manager.insert_entity_selection(identifier);
    }

    /// Deselects the entity with [`Id`] `identifier`.
    #[inline]
    pub fn deselect_entity(&mut self, identifier: Id)
    {
        self.manager.remove_entity_selection(identifier);
    }

    /// Spawns a new brush.
    #[inline]
    pub fn spawn_brush(
        &mut self,
        drawing_resources: &DrawingResources,
        identifier: Id,
        data: BrushData,
        b_type: BrushType
    )
    {
        self.manager
            .spawn_brush_from_parts(drawing_resources, identifier, data, b_type.selected());

        if b_type.drawn()
        {
            return_if_no_match!(self.active_tool, ActiveTool::Draw(t), t)
                .undo_redo_spawn(self.manager, identifier);
        }
    }

    /// Despawns the brush with [`Id`] `identifier`.
    #[inline]
    pub fn despawn_brush(
        &mut self,
        drawing_resources: &DrawingResources,
        identifier: Id,
        b_type: BrushType
    ) -> BrushData
    {
        let data = self.manager.despawn_brush_into_parts(drawing_resources, identifier);

        match self.active_tool
        {
            ActiveTool::Draw(t) =>
            {
                if b_type.drawn()
                {
                    t.undo_redo_despawn(self.manager, identifier);
                }
            },
            ActiveTool::Subtract(t) => t.undo_redo_despawn(self.manager, identifier),
            ActiveTool::Path(t) =>
            {
                if b_type.selected() && data.has_path()
                {
                    t.undo_redo_despawn(self.manager, identifier);
                }
            },
            _ => ()
        };

        data
    }

    /// Returns a [`BrushMut`] wrapping the brush with [`Id`] `identifier`.
    #[inline]
    pub fn brush_mut<'b>(
        &'b mut self,
        drawing_resources: &'b DrawingResources,
        identifier: Id
    ) -> BrushMut
    {
        self.manager.brush_mut(drawing_resources, identifier)
    }

    /// Returns a [`MovingMut`] wrapping the entity with id `identifier`.
    #[inline]
    pub fn moving_mut<'b>(
        &'b mut self,
        drawing_resources: &'b DrawingResources,
        identifier: Id
    ) -> MovingMut<'_>
    {
        self.manager.moving_mut(drawing_resources, identifier)
    }

    /// Gives the brush with [`Id`] `identifier` a [`Path`].
    #[inline]
    pub fn set_path(&mut self, drawing_resources: &DrawingResources, identifier: Id, path: Path)
    {
        self.manager.set_path(drawing_resources, identifier, path);
    }

    /// Removes the [`Path`] from the entity with [`Id`] `identifier`.
    #[inline]
    pub fn remove_path(&mut self, drawing_resources: &DrawingResources, identifier: Id) -> Path
    {
        let path = self.manager.remove_path(drawing_resources, identifier);

        if self.manager.is_selected(identifier)
        {
            if let ActiveTool::Path(t) = self.active_tool
            {
                t.undo_redo_despawn(self.manager, identifier);
            }
        }

        path
    }

    /// Inserts the brush with [`Id`] `identifier` in the subtractees.
    /// # Panics
    /// Panics if the subtract tool is not currently active.
    #[inline]
    pub fn insert_subtractee(&mut self, identifier: Id)
    {
        match_or_panic!(self.active_tool, ActiveTool::Subtract(t), t)
            .insert_subtractee(self.manager, identifier);
    }

    /// Removes the brush with [`Id`] `identifier` from the subtractees.
    /// # Panics
    /// Panics if the subtract tool is not currently active.
    #[inline]
    pub fn remove_subtractee(&mut self, identifier: Id)
    {
        match_or_panic!(self.active_tool, ActiveTool::Subtract(t), t)
            .remove_subtractee(self.manager, identifier);
    }

    /// Attaches the brush with [`Id`] `attachment` to the one with [`Id`] `owner`.
    #[inline]
    pub fn attach(&mut self, owner: Id, attachment: Id) { self.manager.attach(owner, attachment); }

    /// Detaches the brush with [`Id`] `attachment` from the one with [`Id`] `owner`.
    #[inline]
    pub fn detach(&mut self, owner: Id, attachment: Id) { self.manager.detach(owner, attachment); }

    /// Sets the texture of the brush with [`Id`] identifier.
    /// Returns the name of the replaced texture, if any.
    #[inline]
    pub fn set_texture(
        &mut self,
        drawing_resources: &DrawingResources,
        identifier: Id,
        texture: &str
    ) -> TextureSetResult
    {
        self.manager.set_texture(drawing_resources, identifier, texture)
    }

    /// Set the [`TextureSettings`] of the brush with [`Id`] `identifier`.
    #[inline]
    pub fn set_texture_settings(
        &mut self,
        drawing_resources: &DrawingResources,
        identifier: Id,
        texture: TextureSettings
    )
    {
        self.manager
            .set_texture_settings(drawing_resources, identifier, texture);
    }

    /// Removes the texture from the brush with [`Id`] identifier, and returns its
    /// [`TextureSettings`].
    #[inline]
    pub fn remove_texture(
        &mut self,
        drawing_resources: &DrawingResources,
        identifier: Id
    ) -> TextureSettings
    {
        self.manager.remove_texture(drawing_resources, identifier)
    }

    /// Sets whether the texture of the selected brush with [`Id`] `identifier` should be
    /// rendered as a sprite or not. Returns the previous sprite rendering parameters.
    #[inline]
    pub fn set_single_sprite(
        &mut self,
        drawing_resources: &DrawingResources,
        identifier: Id,
        value: bool
    ) -> (Sprite, f32, f32)
    {
        self.manager.set_single_sprite(drawing_resources, identifier, value)
    }

    /// Deletes the free draw point at position `p` or `index` depending on the active tool.
    #[inline]
    pub fn delete_free_draw_point(&mut self, p: Vec2, index: usize)
    {
        match self.active_tool
        {
            ActiveTool::Draw(t) => t.delete_free_draw_vertex(p),
            ActiveTool::Vertex(t) => t.delete_free_draw_path_node(index),
            ActiveTool::Path(t) => t.delete_free_draw_path_node(self.manager, index),
            _ => panic!("Tool does not have free draw capabilities.")
        };
    }

    /// Inserts a free draw point at `index` and position `p`.
    #[inline]
    pub fn insert_free_draw_point(&mut self, p: Vec2, index: usize)
    {
        match self.active_tool
        {
            ActiveTool::Draw(t) => t.insert_free_draw_vertex(p),
            ActiveTool::Vertex(t) => t.insert_free_draw_path_node(p, index),
            ActiveTool::Path(t) => t.insert_free_draw_path_node(self.manager, p, index),
            _ => panic!("Tool does not have free draw capabilities.")
        };
    }

    /// Sets the [`ThingId`] of the [`ThingInstance`] with [`Id`] `identifier`.
    #[inline]
    pub fn set_thing(&mut self, identifier: Id, thing: ThingId) -> ThingId
    {
        let catalog = unsafe { std::ptr::from_ref(self.things_catalog).as_ref() }.unwrap();
        self.thing_mut(identifier)
            .set_thing(catalog.thing(thing).unwrap())
            .unwrap()
    }

    /// Returns the [`ThingMut`] with [`Id`] `identifier`.
    #[inline]
    pub fn thing_mut(&mut self, identifier: Id) -> ThingMut { self.manager.thing_mut(identifier) }

    /// Spawns a new [`ThingInstance`].
    #[inline]
    pub fn spawn_thing(&mut self, identifier: Id, data: ThingInstanceData, drawn: bool)
    {
        self.manager.spawn_thing_from_parts(identifier, data);

        if drawn
        {
            return_if_no_match!(self.active_tool, ActiveTool::Thing(t), t)
                .undo_redo_spawn(self.manager, identifier);
        }
    }

    /// Despawns the [`ThingInstance`] with [`Id`] `identifier`.
    #[inline]
    pub fn despawn_thing(&mut self, identifier: Id, drawn: bool) -> ThingInstanceData
    {
        let thing = self.manager.remove_thing(identifier);

        if drawn
        {
            if let ActiveTool::Thing(t) = self.active_tool
            {
                t.undo_redo_despawn(self.manager, identifier);
            }
        }

        thing.take_data()
    }

    /// Schedules the overall brushes collision update.
    #[inline]
    pub fn schedule_overall_collision_update(&mut self)
    {
        self.manager.schedule_overall_collision_update();
    }

    /// Schedules the overall [`ThingInstance`]s info update.
    #[inline]
    pub fn schedule_overall_things_info_update(&mut self)
    {
        self.manager.schedule_overall_things_info_update();
    }

    /// Schedule the overall node update.
    #[inline]
    pub fn schedule_overall_node_update(&mut self) { self.manager.schedule_overall_node_update(); }

    /// Sets the property with key `k` of the entity with [`Id`] `identifier` to `value`.
    #[inline]
    pub fn set_property(
        &mut self,
        drawing_resources: &DrawingResources,
        identifier: Id,
        k: &str,
        value: &Value
    ) -> Value
    {
        if self.manager.is_thing(identifier)
        {
            self.manager.schedule_overall_things_property_update(k);
            self.manager.thing_mut(identifier).set_property(k, value).unwrap()
        }
        else
        {
            self.manager.schedule_overall_brushes_property_update(k);
            self.manager
                .brush_mut(drawing_resources, identifier)
                .set_property(k, value)
                .unwrap()
        }
    }
}

//=======================================================================//

/// The core of the [`Editor`].
#[derive(Default)]
pub(in crate::map::editor::state) struct Core
{
    /// The active tool.
    active_tool:         ActiveTool,
    /// The [`EditingTarget`] of the previous frame.
    prev_editing_target: EditingTarget
}

impl EnabledTool for Core
{
    type Item = Tool;

    #[inline]
    fn is_tool_enabled(&self, tool: Self::Item) -> bool { self.active_tool.is_tool_enabled(tool) }
}

impl Core
{
    //==============================================================
    // Info

    /// Whether an ongoing multiframe change is happening.
    #[inline]
    #[must_use]
    pub fn ongoing_multi_frame_change(&self) -> bool
    {
        self.active_tool.ongoing_multi_frame_change()
    }

    /// Whether the entity tool is active.
    #[inline]
    #[must_use]
    pub const fn entity_tool(&self) -> bool { self.active_tool.entity_tool() }

    /// Whether the active tool has texture editing capabilities.
    #[inline]
    #[must_use]
    pub const fn texture_tool(&self) -> bool { self.active_tool.texture_tool() }

    /// Whether the map preview is active.
    #[inline]
    #[must_use]
    pub const fn map_preview(&self) -> bool { self.active_tool.map_preview() }

    //==============================================================
    // Save

    /// Whether it is possible to save the file.
    #[inline]
    #[must_use]
    pub fn save_available(&self) -> bool { !self.active_tool.ongoing_multi_frame_change() }

    //==============================================================
    // Select all

    /// Whether select all is available.
    #[inline]
    #[must_use]
    pub fn select_all_available(&self) -> bool { !self.active_tool.ongoing_multi_frame_change() }

    /// Selects all.
    #[inline]
    pub fn select_all(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: Grid,
        settings: &ToolsSettings
    )
    {
        self.active_tool
            .select_all(drawing_resources, manager, edits_history, grid, settings);
    }

    //==============================================================
    // Undo/Redo

    /// Whether undo/redo is available.
    #[inline]
    #[must_use]
    pub fn undo_redo_available(&self) -> bool { self.active_tool.undo_redo_available() }

    /// Undoes an edit.
    #[inline]
    pub fn undo(
        &mut self,
        bundle: &mut StateUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        ui: &mut Ui
    )
    {
        assert!(self.undo_redo_available(), "Undo redo is not available.");
        edits_history.undo(
            &mut UndoRedoInterface::new(self, bundle.things_catalog, manager),
            bundle.drawing_resources,
            ui
        );
    }

    /// Redoes an edit.
    #[inline]
    pub fn redo(
        &mut self,
        bundle: &mut StateUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        ui: &mut Ui
    )
    {
        assert!(self.undo_redo_available(), "Undo redo is not available.");
        edits_history.redo(
            &mut UndoRedoInterface::new(self, bundle.things_catalog, manager),
            bundle.drawing_resources,
            ui
        );
    }

    //==============================================================
    // Copy/Paste

    /// Whether it is possible to copy/paste.
    #[inline]
    #[must_use]
    pub fn copy_paste_available(&self) -> bool { self.active_tool.copy_paste_available() }

    /// Copies the selected entities.
    #[inline]
    pub fn copy(
        &mut self,
        bundle: &StateUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        clipboard: &mut Clipboard
    )
    {
        self.active_tool.copy(bundle, manager, inputs, clipboard);
    }

    /// Cuts the selected entities
    #[inline]
    pub fn cut(
        &mut self,
        bundle: &StateUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        clipboard: &mut Clipboard,
        edits_history: &mut EditsHistory
    )
    {
        self.active_tool
            .cut(bundle, manager, inputs, clipboard, edits_history);
    }

    /// Pastes the copied entities.
    #[inline]
    pub fn paste(
        &mut self,
        bundle: &StateUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        clipboard: &mut Clipboard,
        edits_history: &mut EditsHistory
    )
    {
        self.active_tool
            .paste(bundle, manager, inputs, clipboard, edits_history);
    }

    #[inline]
    pub fn duplicate(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        clipboard: &mut Clipboard,
        edits_history: &mut EditsHistory,
        delta: Vec2
    )
    {
        self.active_tool
            .duplicate(drawing_resources, manager, clipboard, edits_history, delta);
    }

    //==============================================================
    // Update

    /// Disables the currently active subtool.
    #[inline]
    pub fn disable_subtool(&mut self) { self.active_tool.disable_subtool(); }

    /// Toggles the map preview.
    #[inline]
    pub fn toggle_map_preview(&mut self, bundle: &StateUpdateBundle, manager: &EntitiesManager)
    {
        self.active_tool.toggle_map_preview(bundle, manager);
    }

    /// Updates the outline of certain tools.
    #[inline]
    pub fn update_outline(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &EntitiesManager,
        grid: Grid,
        settings: &mut ToolsSettings
    )
    {
        self.active_tool
            .update_outline(drawing_resources, manager, grid, settings);
    }

    /// Updates the data stored concerning the selected vertexes.
    #[inline]
    pub fn update_selected_vertexes<'a>(
        &mut self,
        manager: &EntitiesManager,
        ids: impl Iterator<Item = &'a Id>
    )
    {
        self.active_tool.update_selected_vertexes(manager, ids);
    }

    /// Updates the overall node UI elements.
    #[inline]
    pub fn update_overall_node(&mut self, manager: &EntitiesManager)
    {
        self.active_tool.update_overall_node(manager);
    }

    /// Updates the tool.
    #[inline]
    pub fn update(
        &mut self,
        bundle: &mut ToolUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        clipboard: &mut Clipboard,
        grid: Grid,
        settings: &mut ToolsSettings
    )
    {
        self.active_tool
            .update(bundle, manager, inputs, edits_history, clipboard, grid, settings);

        // Close the edit history.
        edits_history.push_frame_edit();
    }

    /// Changes the active tool.
    #[inline]
    pub fn change_tool(
        &mut self,
        tool: Tool,
        bundle: &StateUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        inputs: &InputsPresses,
        grid: Grid,
        settings: &ToolsSettings,
        tool_change_conditions: &ChangeConditions
    )
    {
        self.active_tool.change(
            tool,
            bundle,
            manager,
            edits_history,
            inputs,
            grid,
            settings,
            tool_change_conditions
        );
    }

    /// Executes the update of the frame start.
    #[inline]
    pub fn frame_start_update(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        clipboard: &Clipboard
    )
    {
        self.active_tool.fallback(manager, clipboard);

        let editing_target = self.active_tool.editing_target(self.prev_editing_target);

        if editing_target.requires_tool_edits_purge(self.prev_editing_target)
        {
            match self.prev_editing_target
            {
                EditingTarget::Sides | EditingTarget::Vertexes =>
                {
                    for mut brush in manager.selected_brushes_mut(drawing_resources)
                    {
                        brush.deselect_vertexes_no_indexes();
                    }
                },
                EditingTarget::Path =>
                {
                    for mut brush in manager.selected_movings_mut(drawing_resources)
                    {
                        brush.deselect_path_nodes_no_indexes();
                    }
                },
                _ => ()
            };

            edits_history.purge_tools_edits(self.prev_editing_target, editing_target);
        }

        self.prev_editing_target = editing_target;
    }

    /// Executes a snap to a [`Grid`] with size 2.
    #[inline]
    pub fn quick_snap(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        settings: &ToolsSettings,
        grid_shifted: bool
    )
    {
        self.active_tool.snap_tool(
            drawing_resources,
            manager,
            edits_history,
            Grid::quick_snap(grid_shifted),
            settings
        );
    }

    //==============================================================
    // Draw

    /// Draws the active tool.
    #[inline]
    pub fn draw_active_tool(
        &self,
        bundle: &mut DrawBundle,
        manager: &EntitiesManager,
        grid: Grid,
        settings: &ToolsSettings
    )
    {
        self.active_tool.draw(bundle, manager, grid, settings);
    }

    /// Draws the map preview.
    #[inline]
    pub fn draw_map_preview(&self, bundle: &mut DrawBundleMapPreview, manager: &EntitiesManager)
    {
        self.active_tool.draw_map_preview(bundle, manager);
    }

    /// Draws the bottom panel.
    #[inline]
    pub fn bottom_panel(
        &mut self,
        bundle: &mut StateUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        clipboard: &mut Clipboard
    )
    {
        self.active_tool
            .bottom_panel(bundle, manager, inputs, edits_history, clipboard);
    }

    /// Draws the UI of the tool.
    #[inline]
    pub fn tool_ui(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        clipboard: &mut Clipboard,
        ui: &mut egui::Ui,
        settings: &mut ToolsSettings
    )
    {
        self.active_tool.ui(
            drawing_resources,
            manager,
            inputs,
            edits_history,
            clipboard,
            ui,
            settings
        );
    }

    /// Draws the subtools.
    #[inline]
    pub fn draw_subtools(
        &mut self,
        ui: &mut egui::Ui,
        bundle: &StateUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: Grid,
        buttons: &mut ToolsButtons,
        tool_change_conditions: &ChangeConditions
    )
    {
        self.active_tool.draw_subtools(
            ui,
            bundle,
            manager,
            edits_history,
            grid,
            buttons,
            tool_change_conditions
        );
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Deselects all the selected vertexes.
#[inline]
fn deselect_vertexes(
    drawing_resources: &DrawingResources,
    manager: &mut EntitiesManager,
    edits_history: &mut EditsHistory
)
{
    edits_history.vertexes_selection_cluster(
        manager
            .selected_brushes_mut(drawing_resources)
            .filter_map(|mut brush| brush.deselect_vertexes().map(|idxs| (brush.id(), idxs)))
    );
}

//=======================================================================//

/// Draws the non selected brushes.
#[inline]
fn draw_non_selected_brushes(bundle: &mut DrawBundle, manager: &EntitiesManager)
{
    let DrawBundle {
        window,
        drawer,
        camera,
        ..
    } = bundle;

    let mut selected_entities_iterated = 0;
    let selected_entities_len = manager.selected_brushes_amount();

    let brushes = manager.visible_brushes(window, camera, drawer.grid());
    let mut brushes = brushes.iter();

    for brush in &mut brushes
    {
        let id = brush.id();

        if manager.is_selected(id)
        {
            selected_entities_iterated += 1;

            if selected_entities_iterated == selected_entities_len
            {
                break;
            }

            continue;
        }

        brush.draw_non_selected(camera, drawer);
    }

    for brush in brushes
    {
        brush.draw_non_selected(camera, drawer);
    }
}
