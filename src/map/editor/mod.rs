mod cursor;
pub mod state;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::{
    asset::{AssetServer, Assets},
    ecs::{
        entity::Entity,
        event::EventReader,
        query::With,
        system::{Commands, Query, Res, ResMut}
    },
    input::{
        keyboard::KeyCode,
        mouse::{MouseButton, MouseScrollUnit, MouseWheel},
        ButtonInput
    },
    render::{mesh::Mesh, texture::Image},
    sprite::{ColorMaterial, Mesh2dHandle},
    state::state::NextState,
    time::Time,
    transform::components::Transform,
    window::Window
};
use bevy_egui::{egui, EguiUserTextures};
use glam::Vec2;
use state::{
    clipboard::Clipboard,
    edits_history::EditsHistory,
    grid::Grid,
    inputs_presses::InputsPresses,
    manager::EntitiesManager
};

use self::state::{
    clipboard::{PropCameras, PropCamerasMut},
    ui::{ui_left_space, ui_right_space, ui_top_space}
};
use super::{
    drawer::{
        color::ColorResources,
        drawers::{EditDrawer, MapPreviewDrawer},
        drawing_resources::DrawingResources,
        texture::Texture,
        texture_loader::{TextureLoader, TextureLoadingProgress}
    },
    properties::{BrushProperties, DefaultProperties, ThingProperties},
    thing::{catalog::ThingsCatalog, HardcodedThings}
};
use crate::{
    config::{controls::BindsKeyCodes, Config},
    map::{
        editor::{cursor::Cursor, state::editor_state::State},
        MAP_HALF_SIZE
    },
    utils::{
        math::AroundEqual,
        misc::{Camera, TakeValue}
    },
    EditorState,
    HardcodedActions
};

//=======================================================================//
// TRAITS
//
//=======================================================================//

/// A trait for structs to create placeholder instances to be replaced after startup.
pub(in crate::map) trait Placeholder
{
    /// Returns a placeholder instance of [`Self`] to be replaced after startup.
    #[must_use]
    unsafe fn placeholder() -> Self;
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// A collection of references to the loaded [`DefaultProperties`].
#[allow(clippy::missing_docs_in_private_items)]
#[must_use]
struct AllDefaultProperties<'a>
{
    brushes:     &'a DefaultProperties,
    things:      &'a DefaultProperties,
    map_brushes: &'a mut DefaultProperties,
    map_things:  &'a mut DefaultProperties
}

//=======================================================================//

/// A bundle of variables required to update the state of the [`Editor`].
#[allow(clippy::missing_docs_in_private_items)]
#[must_use]
struct StateUpdateBundle<'world, 'state, 'a, 'b, 'c>
{
    window:             &'a mut Window,
    images:             &'a mut Assets<Image>,
    materials:          &'a mut Assets<ColorMaterial>,
    camera:             &'b mut Transform,
    prop_cameras:       &'a mut PropCamerasMut<'world, 'state, 'c>,
    elapsed_time:       f32,
    delta_time:         f32,
    mouse_buttons:      &'a ButtonInput<MouseButton>,
    key_inputs:         &'a mut ButtonInput<KeyCode>,
    egui_context:       &'a egui::Context,
    user_textures:      &'a mut EguiUserTextures,
    config:             &'a mut Config,
    cursor:             &'b Cursor,
    things_catalog:     &'b mut ThingsCatalog,
    drawing_resources:  &'b mut DrawingResources,
    default_properties: &'b mut AllDefaultProperties<'b>,
    manager:            &'b mut EntitiesManager,
    clipboard:          &'b mut Clipboard,
    edits_history:      &'b mut EditsHistory,
    inputs:             &'b mut InputsPresses,
    grid:               &'b mut Grid,
    next_editor_state:  &'a mut NextState<EditorState>,
    next_tex_load:      &'a mut NextState<TextureLoadingProgress>
}

//=======================================================================//

/// A bundle of variables required to update the currently active tool of the [`Editor`].
#[allow(clippy::missing_docs_in_private_items)]
#[must_use]
struct ToolUpdateBundle<'world, 'state, 'a, 'b, 'c>
{
    window: &'a Window,
    images: &'a mut Assets<Image>,
    delta_time: f32,
    camera: &'a mut Transform,
    prop_cameras: &'a mut PropCamerasMut<'world, 'state, 'c>,
    paint_tool_camera: (&'a mut bevy::render::camera::Camera, &'a mut Transform),
    user_textures: &'a mut EguiUserTextures,
    things_catalog: &'b ThingsCatalog,
    drawing_resources: &'b DrawingResources,
    cursor: &'b Cursor,
    brushes_default_properties: &'b DefaultProperties,
    things_default_properties: &'b DefaultProperties,
    manager: &'b mut EntitiesManager,
    clipboard: &'b mut Clipboard,
    edits_history: &'b mut EditsHistory,
    inputs: &'b mut InputsPresses,
    grid: &'b Grid
}

//=======================================================================//

/// A bundle of variables required to draw the visible portion of the map.
#[allow(clippy::missing_docs_in_private_items)]
#[must_use]
struct DrawBundle<'world, 'state, 'w, 's, 'a, 'b, 'c>
{
    window:            &'a Window,
    delta_time:        f32,
    drawer:            &'b mut EditDrawer<'w, 's, 'a>,
    camera:            &'a Transform,
    prop_cameras:      &'a PropCameras<'world, 'state, 'c>,
    paint_tool_camera: &'a Transform,
    things_catalog:    &'b ThingsCatalog,
    cursor:            &'b Cursor,
    manager:           &'b mut EntitiesManager,
    clipboard:         &'b Clipboard
}

//=======================================================================//

/// A bundle of variables required to draw the visible portion of the map in map preview mode.
#[allow(clippy::missing_docs_in_private_items)]
#[must_use]
struct DrawBundleMapPreview<'w, 's, 'a, 'b>
{
    window:         &'a Window,
    egui_context:   &'a egui::Context,
    camera:         &'a Transform,
    drawer:         MapPreviewDrawer<'w, 's, 'a>,
    things_catalog: &'b ThingsCatalog,
    manager:        &'b EntitiesManager
}

//=======================================================================//

/// The map editor .
#[must_use]
pub(in crate::map) struct Editor
{
    /// The current state.
    state: State,
    /// The position of the cursor on the map.
    cursor: Cursor,
    /// The catalog of the loaded [`Thing`]s.
    things_catalog: ThingsCatalog,
    /// The resources to draw the map on screen.
    drawing_resources: DrawingResources,
    /// The engine defined default brush properties.
    brushes_default_properties: DefaultProperties,
    /// The engine defined default [`ThingInstance`] properties.
    things_default_properties: DefaultProperties,
    /// The defined default brush properties to be used for the currently opened map.
    map_brushes_default_properties: DefaultProperties,
    /// The defined default [`ThingInstance`] properties to be used for the currently opened map.
    map_things_default_properties: DefaultProperties,
    /// The manager of all entities.
    manager: EntitiesManager,
    /// The clipboard used for copy paste and prop spawning.
    clipboard: Clipboard,
    /// The history of the edits made to the map.
    edits_history: EditsHistory,
    /// The state of all necessary input presses.
    inputs: InputsPresses,
    /// The grid of the map.
    grid: Grid
}

impl Placeholder for Editor
{
    #[inline]
    unsafe fn placeholder() -> Self
    {
        unsafe {
            Self {
                state: State::placeholder(),
                cursor: Cursor::default(),
                things_catalog: ThingsCatalog::default(),
                drawing_resources: DrawingResources::placeholder(),
                brushes_default_properties: DefaultProperties::default(),
                things_default_properties: DefaultProperties::default(),
                map_brushes_default_properties: DefaultProperties::default(),
                map_things_default_properties: DefaultProperties::default(),
                manager: EntitiesManager::new(),
                clipboard: Clipboard::new(),
                edits_history: EditsHistory::default(),
                inputs: InputsPresses::default(),
                grid: Grid::default()
            }
        }
    }
}

impl Editor
{
    /// Creates a new [`Editor`].
    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub fn new(
        window: &mut Window,
        prop_cameras: &mut PropCamerasMut,
        asset_server: &AssetServer,
        images: &mut Assets<Image>,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<ColorMaterial>,
        user_textures: &mut EguiUserTextures,
        config: &mut Config,
        texture_loader: &mut TextureLoader,
        hardcoded_things: Option<Res<HardcodedThings>>,
        brush_properties: Option<ResMut<BrushProperties>>,
        thing_properties: Option<ResMut<ThingProperties>>
    ) -> Self
    {
        let mut drawing_resources = DrawingResources::new(
            prop_cameras,
            asset_server,
            meshes,
            materials,
            user_textures,
            texture_loader
        );
        let things_catalog = ThingsCatalog::new(hardcoded_things);
        let path = match config.open_file.path().cloned()
        {
            Some(path) => path.exists().then_some(path),
            None => None
        };

        let brushes_default_properties = brush_properties
            .map_or(DefaultProperties::default(), |mut d_p| {
                DefaultProperties::new(d_p.0.take_value())
            });
        let things_default_properties = thing_properties
            .map_or(DefaultProperties::default(), |mut d_p| {
                DefaultProperties::new(d_p.0.take_value())
            });
        let mut map_brushes_default_properties = brushes_default_properties.clone();
        let mut map_things_default_properties = things_default_properties.clone();

        let mut default_properties = AllDefaultProperties {
            brushes:     &brushes_default_properties,
            things:      &things_default_properties,
            map_brushes: &mut map_brushes_default_properties,
            map_things:  &mut map_things_default_properties
        };

        let (state, manager, clipboard, edits_history, grid, path) = State::new(
            asset_server,
            images,
            prop_cameras,
            user_textures,
            &mut drawing_resources,
            &things_catalog,
            &mut default_properties,
            path
        );

        match path
        {
            Some(path) => config.open_file.update(path, window),
            None => config.open_file.clear(window)
        };

        Self {
            state,
            cursor: Cursor::default(),
            things_catalog,
            drawing_resources,
            brushes_default_properties,
            things_default_properties,
            map_brushes_default_properties,
            map_things_default_properties,
            manager,
            clipboard,
            edits_history,
            inputs: InputsPresses::default(),
            grid
        }
    }

    //==============================================================
    // Info

    #[inline]
    pub const fn is_window_focused(&self) -> bool { self.state.is_window_focused() }

    //==============================================================
    // Update

    /// Update the state of the [`Editor`].
    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub fn update(
        &mut self,
        window: &mut Window,
        images: &mut Assets<Image>,
        materials: &mut Assets<ColorMaterial>,
        camera: &mut Transform,
        prop_cameras: &mut PropCamerasMut,
        time: &Time,
        egui_context: &egui::Context,
        user_textures: &mut EguiUserTextures,
        mouse_buttons: &ButtonInput<MouseButton>,
        mouse_wheel: &mut EventReader<MouseWheel>,
        key_inputs: &mut ButtonInput<KeyCode>,
        config: &mut Config,
        next_editor_state: &mut NextState<EditorState>,
        next_tex_load: &mut NextState<TextureLoadingProgress>
    )
    {
        if !window.focused
        {
            key_inputs.reset_all();
            egui_context.input_mut(|i| {
                i.events.clear();
            });
        }

        // Set up the frame update.
        let ui_hovered = self.state.update(&mut StateUpdateBundle {
            window,
            images,
            materials,
            camera,
            prop_cameras,
            elapsed_time: time.elapsed_seconds(),
            delta_time: time.delta_seconds(),
            mouse_buttons,
            key_inputs,
            egui_context,
            user_textures,
            config,
            cursor: &self.cursor,
            things_catalog: &mut self.things_catalog,
            drawing_resources: &mut self.drawing_resources,
            default_properties: &mut AllDefaultProperties {
                brushes:     &self.brushes_default_properties,
                things:      &self.things_default_properties,
                map_brushes: &mut self.map_brushes_default_properties,
                map_things:  &mut self.map_things_default_properties
            },
            manager: &mut self.manager,
            clipboard: &mut self.clipboard,
            edits_history: &mut self.edits_history,
            inputs: &mut self.inputs,
            grid: &mut self.grid,
            next_editor_state,
            next_tex_load
        });

        // Move view around, if the UI is not being hovered.
        self.update_view(
            window,
            camera,
            egui_context,
            key_inputs,
            &config.binds,
            mouse_wheel,
            ui_hovered
        );
    }

    /// Update the currently active tool.
    #[inline]
    pub fn update_active_tool(
        &mut self,
        window: &Window,
        images: &mut Assets<Image>,
        camera: &mut Transform,
        prop_cameras: &mut PropCamerasMut,
        paint_tool_camera: (&mut bevy::render::camera::Camera, &mut Transform),
        time: &Time,
        user_textures: &mut EguiUserTextures
    )
    {
        self.state.update_active_tool(&mut ToolUpdateBundle {
            window,
            images,
            delta_time: time.delta_seconds(),
            camera,
            prop_cameras,
            paint_tool_camera,
            user_textures,
            cursor: &self.cursor,
            things_catalog: &self.things_catalog,
            drawing_resources: &self.drawing_resources,
            brushes_default_properties: &self.map_brushes_default_properties,
            things_default_properties: &self.map_things_default_properties,
            manager: &mut self.manager,
            clipboard: &mut self.clipboard,
            edits_history: &mut self.edits_history,
            inputs: &mut self.inputs,
            grid: &self.grid
        });
    }

    /// Update the position and scale of the camera based on the user inputs.
    #[inline]
    fn update_view(
        &mut self,
        window: &Window,
        camera: &mut Transform,
        egui_context: &egui::Context,
        key_inputs: &ButtonInput<KeyCode>,
        binds: &BindsKeyCodes,
        mouse_wheel: &mut EventReader<MouseWheel>,
        ui_hovered: bool
    )
    {
        let moved_with_keyboard = self.update_view_keyboard(window, camera, key_inputs, binds);

        if let Some(cursor_pos) = window.cursor_position()
        {
            if ui_hovered
            {
                egui_context.set_cursor_icon(egui::CursorIcon::Default);
            }
            else if !self.update_view_mouse(window, camera, mouse_wheel) && !moved_with_keyboard
            {
                self.drag_view(camera, egui_context);
            }

            self.cursor.update(
                cursor_pos,
                window,
                camera,
                &self.state,
                &self.grid,
                self.inputs.space_pressed()
            );
        }

        Self::cap_map_size(window, camera);
    }

    /// Update the position and scale of the camera based on the keyboard inputs.
    #[inline]
    #[must_use]
    fn update_view_keyboard(
        &mut self,
        window: &Window,
        camera: &mut Transform,
        key_inputs: &ButtonInput<KeyCode>,
        binds: &BindsKeyCodes
    ) -> bool
    {
        if self.inputs.space_pressed()
        {
            return false;
        }

        if self.inputs.ctrl_pressed()
        {
            if let Some(delta) = self.inputs.directional_keys_vector(self.grid.size())
            {
                camera.translate(delta);
                return true;
            }

            if HardcodedActions::ZoomIn.pressed(key_inputs)
            {
                camera.zoom_in();
                return true;
            }

            if HardcodedActions::ZoomOut.pressed(key_inputs)
            {
                camera.zoom_out();
                return true;
            }

            return false;
        }

        if let Some(hull) = State::quick_zoom_hull(key_inputs, &mut self.manager, binds)
        {
            // Zoom on the selected entities.
            camera.scale_viewport_to_hull(window, &self.grid, &hull, self.grid.size_f32());
            return true;
        }

        false
    }

    /// Update the position and scale of the camera based on the mouse inputs.
    #[inline]
    #[must_use]
    fn update_view_mouse(
        &mut self,
        window: &Window,
        camera: &mut Transform,
        mouse_wheel: &mut EventReader<MouseWheel>
    ) -> bool
    {
        if self.inputs.space_pressed()
        {
            return false;
        }

        let mouse_wheel_scroll = {
            let mut scroll = 0f32;

            for ev in mouse_wheel.read()
            {
                match ev.unit
                {
                    MouseScrollUnit::Line =>
                    {
                        scroll = ev.y.signum();
                        break;
                    },
                    MouseScrollUnit::Pixel => scroll += ev.y
                };
            }

            scroll
        };

        if mouse_wheel_scroll.around_equal_narrow(&0f32)
        {
            return false;
        }

        if self.inputs.ctrl_pressed()
        {
            camera.zoom_on_ui_pos(
                window,
                &self.grid,
                self.cursor.world_snapped(),
                self.cursor.ui_snapped(),
                mouse_wheel_scroll
            );
        }
        else
        {
            let mouse_wheel_scroll = mouse_wheel_scroll * self.grid.size_f32();

            camera.translate(
                if self.inputs.shift_pressed()
                {
                    Vec2::new(mouse_wheel_scroll, 0f32)
                }
                else
                {
                    Vec2::new(0f32, mouse_wheel_scroll)
                }
            );
        }

        true
    }

    /// Drags the camera around, CAD software-like.
    #[inline]
    fn drag_view(&mut self, camera: &mut Transform, egui_context: &egui::Context)
    {
        // Drag the view around.
        if !self.inputs.space_pressed()
        {
            egui_context.set_cursor_icon(egui::CursorIcon::Default);
            return;
        }

        let delta = self.cursor.delta_ui() * camera.scale();
        camera.translate(Vec2::new(-delta.x, delta.y));
        egui_context.set_cursor_icon(egui::CursorIcon::Grabbing);
    }

    /// Caps the camera position so that its viewport does not go out of map bounds.
    #[inline]
    fn cap_map_size(window: &Window, camera: &mut Transform)
    {
        let (half_width, half_height) = camera.scaled_window_half_sizes(window);
        let mut camera_pos = camera.pos();

        // Y Cap.
        let top_dif = camera_pos.y + half_height - ui_top_space() * camera.scale() - MAP_HALF_SIZE;

        if top_dif > 0f32
        {
            camera_pos.y -= top_dif;
        }
        else
        {
            let bottom_dif = camera_pos.y - half_height + MAP_HALF_SIZE;

            if bottom_dif < 0f32
            {
                camera_pos.y -= bottom_dif;
            }
        }

        // X Cap.
        let right_dif =
            camera_pos.x + half_width - ui_right_space() * camera.scale() - MAP_HALF_SIZE;

        if right_dif > 0f32
        {
            camera_pos.x -= right_dif;
        }
        else
        {
            let left_dif =
                camera_pos.x - half_width + ui_left_space() * camera.scale() + MAP_HALF_SIZE;

            if left_dif < 0f32
            {
                camera_pos.x -= left_dif;
            }
        }

        camera.set_pos(camera_pos);
    }

    /// Quits the application.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub fn quit(
        &mut self,
        window: &mut Window,
        images: &mut Assets<Image>,
        materials: &mut Assets<ColorMaterial>,
        camera: &mut Transform,
        prop_cameras: &mut PropCamerasMut,
        time: &Time,
        egui_context: &egui::Context,
        user_textures: &mut EguiUserTextures,
        mouse_buttons: &ButtonInput<MouseButton>,
        key_inputs: &mut ButtonInput<KeyCode>,
        config: &mut Config,
        next_editor_state: &mut NextState<EditorState>,
        next_tex_load: &mut NextState<TextureLoadingProgress>
    ) -> bool
    {
        State::quit(
            &mut StateUpdateBundle {
                window,
                images,
                materials,
                camera,
                prop_cameras,
                elapsed_time: time.elapsed_seconds(),
                delta_time: time.delta_seconds(),
                mouse_buttons,
                key_inputs,
                egui_context,
                user_textures,
                config,
                cursor: &self.cursor,
                things_catalog: &mut self.things_catalog,
                drawing_resources: &mut self.drawing_resources,
                default_properties: &mut AllDefaultProperties {
                    brushes:     &self.brushes_default_properties,
                    things:      &self.things_default_properties,
                    map_brushes: &mut self.map_brushes_default_properties,
                    map_things:  &mut self.map_things_default_properties
                },
                manager: &mut self.manager,
                clipboard: &mut self.clipboard,
                edits_history: &mut self.edits_history,
                inputs: &mut self.inputs,
                grid: &mut self.grid,
                next_editor_state,
                next_tex_load
            },
            rfd::MessageButtons::YesNo
        )
    }

    //==============================================================
    // Drawing

    /// Draws the visible portion of the map.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub fn draw(
        &mut self,
        commands: &mut Commands,
        window: &Window,
        camera: &Transform,
        prop_cameras: &PropCameras,
        paint_tool_camera: &Transform,
        time: &Time,
        meshes: &mut Assets<Mesh>,
        egui_context: &egui::Context,
        meshes_query: &Query<Entity, With<Mesh2dHandle>>,
        color_resources: &ColorResources
    )
    {
        let elapsed_time = time.elapsed_seconds();

        if self.state.map_preview()
        {
            self.state.draw_map_preview(&mut DrawBundleMapPreview {
                window,
                egui_context,
                drawer: MapPreviewDrawer::new(
                    commands,
                    prop_cameras,
                    meshes,
                    meshes_query,
                    &mut self.drawing_resources,
                    &self.grid,
                    elapsed_time
                ),
                camera,
                things_catalog: &self.things_catalog,
                manager: &self.manager
            });

            return;
        }

        self.state.draw(&mut DrawBundle {
            window,
            delta_time: time.delta_seconds(),
            drawer: &mut EditDrawer::new(
                commands,
                prop_cameras,
                meshes,
                meshes_query,
                egui_context,
                &mut self.drawing_resources,
                color_resources,
                self.state.tools_settings(),
                &self.grid,
                elapsed_time,
                camera.scale(),
                paint_tool_camera.scale(),
                self.state.show_collision_overlay(),
                self.state.show_tooltips()
            ),
            camera,
            prop_cameras,
            paint_tool_camera,
            things_catalog: &self.things_catalog,
            cursor: &self.cursor,
            manager: &mut self.manager,
            clipboard: &self.clipboard
        });
    }

    //==============================================================
    // Misc

    /// Reloads the stored textures.
    #[inline]
    pub fn reload_textures(
        &mut self,
        prop_cameras: &mut PropCamerasMut,
        images: &mut Assets<Image>,
        materials: &mut Assets<ColorMaterial>,
        user_textures: &mut EguiUserTextures,
        textures: Vec<(Texture, egui::TextureId)>
    )
    {
        self.drawing_resources.reload_textures(materials, textures);
        self.state.finish_textures_reload(
            prop_cameras,
            images,
            user_textures,
            &self.drawing_resources,
            &mut self.manager,
            &mut self.clipboard,
            &mut self.edits_history,
            &self.grid
        );
    }

    /// Shutdown cleanup.
    #[inline]
    pub fn cleanup(&self, meshes: &mut Assets<Mesh>) { self.drawing_resources.cleanup(meshes); }
}
