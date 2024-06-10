mod cursor_pos;
pub mod state;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::fs::File;

use bevy::{
    input::mouse::{MouseScrollUnit, MouseWheel},
    prelude::*,
    sprite::Mesh2dHandle
};
use bevy_egui::{egui, EguiUserTextures};

use self::state::{
    clipboard::{PropCameras, PropCamerasMut},
    ui::{ui_left_space, ui_right_space, ui_top_space}
};
use super::{
    drawer::{
        color::ColorResources,
        drawing_resources::DrawingResources,
        texture::Texture,
        texture_loader::{TextureLoader, TextureLoadingProgress},
        EditDrawer,
        MapPreviewDrawer
    },
    properties::{BrushProperties, DefaultProperties, ThingProperties},
    thing::catalog::ThingsCatalog
};
use crate::{
    config::{controls::BindsKeyCodes, Config},
    map::{
        editor::{cursor_pos::Cursor, state::editor_state::State},
        hv_vec,
        MAP_HALF_SIZE
    },
    utils::{math::AroundEqual, misc::Camera},
    EditorState,
    HardcodedActions,
    HardcodedThings,
    NAME
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
// TYPES
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
    egui_context:       &'a mut egui::Context,
    user_textures:      &'a mut EguiUserTextures,
    config:             &'a mut Config,
    cursor:             &'b Cursor,
    things_catalog:     &'b mut ThingsCatalog,
    drawing_resources:  &'b mut DrawingResources,
    default_properties: &'b mut AllDefaultProperties<'b>,
    next_editor_state:  &'a mut NextState<EditorState>,
    next_tex_load:      &'a mut NextState<TextureLoadingProgress>
}

impl<'world, 'state, 'a, 'b, 'c> StateUpdateBundle<'world, 'state, 'a, 'b, 'c>
{
    /// Updates the title of the window based on the file being edited, if any.
    #[inline]
    pub fn update_window_title(&mut self)
    {
        self.window.title = window_title(self.config.open_file.file_stem());
    }
}

//=======================================================================//

/// A bundle of variables required to update the currently active tool of the [`Editor`].
#[allow(clippy::missing_docs_in_private_items)]
#[must_use]
struct ToolUpdateBundle<'world, 'state, 'a, 'b, 'c>
{
    window:                     &'a Window,
    images:                     &'a mut Assets<Image>,
    delta_time:                 f32,
    camera:                     &'a mut Transform,
    prop_cameras:               &'a mut PropCamerasMut<'world, 'state, 'c>,
    paint_tool_camera:          (&'a mut bevy::prelude::Camera, &'a mut Transform),
    user_textures:              &'a mut EguiUserTextures,
    things_catalog:             &'b ThingsCatalog,
    drawing_resources:          &'b DrawingResources,
    cursor:                     &'b Cursor,
    brushes_default_properties: &'b DefaultProperties,
    things_default_properties:  &'b DefaultProperties
}

//=======================================================================//

/// A bundle of variables required to draw the visible portion of the map.
#[allow(clippy::missing_docs_in_private_items)]
#[must_use]
struct DrawBundle<
    'world,
    'state,
    'w,
    's,
    'a,
    'b,
    'c,
    #[cfg(feature = "debug")] 't,
    #[cfg(feature = "debug")] 'u
> {
    window:            &'a Window,
    delta_time:        f32,
    egui_context:      &'a mut egui::Context,
    drawer:            EditDrawer<'w, 's, 'a>,
    camera:            &'a Transform,
    prop_cameras:      &'a PropCameras<'world, 'state, 'c>,
    paint_tool_camera: &'a Transform,
    things_catalog:    &'b ThingsCatalog,
    cursor:            &'b Cursor,
    #[cfg(feature = "debug")]
    gizmos:            &'a mut Gizmos<'t, 'u>
}

//=======================================================================//

/// A bundle of variables required to draw the visible portion of the map in map preview mode.
#[allow(clippy::missing_docs_in_private_items)]
#[must_use]
struct DrawBundleMapPreview<'w, 's, 'a, 'b>
{
    window:         &'a Window,
    egui_context:   &'a mut egui::Context,
    drawer:         MapPreviewDrawer<'w, 's, 'a>,
    things_catalog: &'b ThingsCatalog,
    camera:         &'a Transform
}

//=======================================================================//

/// The map editor .
#[must_use]
pub(in crate::map) struct Editor
{
    /// The current state.
    state: State,
    /// The position of the cursor on the map.
    cursor_pos: Cursor,
    /// The catalog of the loaded [`Thing`]s.
    things_catalog: ThingsCatalog,
    /// The resources to draw the map on screen.
    drawing_resources: DrawingResources,
    /// The engine defined default [`Brush`] properties.
    brushes_default_properties: DefaultProperties,
    /// The engine defined default [`ThingInstance`] properties.
    things_default_properties: DefaultProperties,
    /// The defined default [`Brush`] properties to be used for the currently opened map.
    map_brushes_default_properties: DefaultProperties,
    /// The defined default [`ThingInstance`] properties to be used for the currently opened map.
    map_things_default_properties: DefaultProperties
}

impl Placeholder for Editor
{
    #[inline]
    unsafe fn placeholder() -> Self
    {
        unsafe {
            Self {
                state: State::placeholder(),
                cursor_pos: Cursor::default(),
                things_catalog: ThingsCatalog::default(),
                drawing_resources: DrawingResources::placeholder(),
                brushes_default_properties: DefaultProperties::default(),
                things_default_properties: DefaultProperties::default(),
                map_brushes_default_properties: DefaultProperties::default(),
                map_things_default_properties: DefaultProperties::default()
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
        config: &Config,
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
        let file = match config.open_file.path().cloned()
        {
            Some(path) =>
            {
                match File::open(&path)
                {
                    Ok(file) =>
                    {
                        window.title = window_title(path.file_stem().unwrap().to_str());
                        file.into()
                    },
                    Err(_) => None
                }
            },
            None => None
        };

        let brushes_default_properties = brush_properties
            .map_or(DefaultProperties::default(), |mut d_p| {
                DefaultProperties::new(std::mem::take(&mut d_p.0))
            });
        let things_default_properties = thing_properties
            .map_or(DefaultProperties::default(), |mut d_p| {
                DefaultProperties::new(std::mem::take(&mut d_p.0))
            });
        let mut map_brushes_default_properties = brushes_default_properties.clone();
        let mut map_things_default_properties = things_default_properties.clone();

        let mut default_properties = AllDefaultProperties {
            brushes:     &brushes_default_properties,
            things:      &things_default_properties,
            map_brushes: &mut map_brushes_default_properties,
            map_things:  &mut map_things_default_properties
        };

        let state = State::new(
            asset_server,
            images,
            prop_cameras,
            user_textures,
            &mut drawing_resources,
            &things_catalog,
            &mut default_properties,
            file
        );

        Self {
            state,
            cursor_pos: Cursor::default(),
            things_catalog,
            drawing_resources,
            brushes_default_properties,
            things_default_properties,
            map_brushes_default_properties,
            map_things_default_properties
        }
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
        egui_context: &mut egui::Context,
        user_textures: &mut EguiUserTextures,
        mouse_buttons: &ButtonInput<MouseButton>,
        key_inputs: &mut ButtonInput<KeyCode>,
        config: &mut Config,
        next_editor_state: &mut NextState<EditorState>,
        next_tex_load: &mut NextState<TextureLoadingProgress>
    ) -> bool
    {
        self.state.quit(
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
                cursor: &self.cursor_pos,
                things_catalog: &mut self.things_catalog,
                drawing_resources: &mut self.drawing_resources,
                default_properties: &mut AllDefaultProperties {
                    brushes:     &self.brushes_default_properties,
                    things:      &self.things_default_properties,
                    map_brushes: &mut self.map_brushes_default_properties,
                    map_things:  &mut self.map_things_default_properties
                },
                next_editor_state,
                next_tex_load
            },
            rfd::MessageButtons::YesNo
        )
    }

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
        egui_context: &mut egui::Context,
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
        if !self.state.update(&mut StateUpdateBundle {
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
            cursor: &self.cursor_pos,
            things_catalog: &mut self.things_catalog,
            drawing_resources: &mut self.drawing_resources,
            default_properties: &mut AllDefaultProperties {
                brushes:     &self.brushes_default_properties,
                things:      &self.things_default_properties,
                map_brushes: &mut self.map_brushes_default_properties,
                map_things:  &mut self.map_things_default_properties
            },
            next_editor_state,
            next_tex_load
        })
        {
            // Move view around, if the UI is not being hovered.
            self.update_view(window, camera, egui_context, key_inputs, &config.binds, mouse_wheel);
        }
    }

    /// Update the currently active tool.
    #[inline]
    pub fn update_active_tool(
        &mut self,
        window: &Window,
        images: &mut Assets<Image>,
        camera: &mut Transform,
        prop_cameras: &mut PropCamerasMut,
        paint_tool_camera: (&mut bevy::prelude::Camera, &mut Transform),
        time: &Time,
        user_textures: &mut EguiUserTextures
    )
    {
        // Manipulate entities.
        self.state.update_active_tool(&mut ToolUpdateBundle {
            window,
            images,
            delta_time: time.delta_seconds(),
            camera,
            prop_cameras,
            paint_tool_camera,
            user_textures,
            cursor: &self.cursor_pos,
            things_catalog: &self.things_catalog,
            drawing_resources: &self.drawing_resources,
            brushes_default_properties: &self.map_brushes_default_properties,
            things_default_properties: &self.map_things_default_properties
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
        mouse_wheel: &mut EventReader<MouseWheel>
    )
    {
        let mut view_moved = self.update_view_keyboard(window, camera, key_inputs, binds);

        if let Some(cursor_pos) = window.cursor_position()
        {
            view_moved |= self.update_view_mouse(window, camera, mouse_wheel);

            self.cursor_pos.update(
                cursor_pos,
                window,
                camera,
                &self.state,
                self.state.space_pressed()
            );

            if !view_moved
            {
                self.drag_view(camera);
            }

            if self.state.space_pressed()
            {
                egui_context.set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            else
            {
                egui_context.set_cursor_icon(egui::CursorIcon::Default);
            }
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
        if self.state.space_pressed()
        {
            return false;
        }

        if self.state.ctrl_pressed()
        {
            if let Some(delta) = self.state.directional_keys_vector()
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

        if let Some(hull) = self.state.quick_zoom_hull(key_inputs, binds)
        {
            // Zoom on the selected entities.
            camera.scale_viewport_ui_constricted_to_hull(window, &hull, self.state.grid_size_f32());
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
        if self.state.space_pressed()
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

        if self.state.ctrl_pressed()
        {
            camera.zoom_on_ui_pos(window, self.cursor_pos.ui_snapped(), mouse_wheel_scroll);
        }
        else
        {
            let mouse_wheel_scroll = mouse_wheel_scroll * self.state.grid_size_f32();

            camera.translate(
                if self.state.shift_pressed()
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
    fn drag_view(&mut self, camera: &mut Transform)
    {
        // Drag the view around.
        if !self.state.space_pressed()
        {
            return;
        }

        let delta = self.cursor_pos.delta_ui() * camera.scale();
        camera.translate(Vec2::new(-delta.x, delta.y));
    }

    /// Caps the camera position so that its viewport does not go out of map bounds.
    #[inline]
    fn cap_map_size(window: &Window, camera: &mut Transform)
    {
        /// A more constrained map size cap to avoid [`QuadTree`] crashes caused by an out of bounds
        /// cursor position.
        const CAP: f32 = MAP_HALF_SIZE - 64f32;

        let (half_width, half_height) = camera.scaled_window_half_sizes(window);
        let mut camera_pos = camera.pos();

        // Y Cap.
        let top_dif = camera_pos.y + half_height - ui_top_space() * camera.scale() - CAP;

        if top_dif > 0f32
        {
            camera_pos.y -= top_dif;
        }
        else
        {
            let bottom_dif = camera_pos.y - half_height + CAP;

            if bottom_dif < 0f32
            {
                camera_pos.y -= bottom_dif;
            }
        }

        // X Cap.
        let right_dif = camera_pos.x + half_width - ui_right_space() * camera.scale() - CAP;

        if right_dif > 0f32
        {
            camera_pos.x -= right_dif;
        }
        else
        {
            let left_dif = camera_pos.x - half_width + ui_left_space() * camera.scale() + CAP;

            if left_dif < 0f32
            {
                camera_pos.x -= left_dif;
            }
        }

        camera.set_pos(camera_pos);
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
        egui_context: &mut egui::Context,
        meshes_query: &Query<Entity, With<Mesh2dHandle>>,
        color_resources: &ColorResources,
        #[cfg(feature = "debug")] gizmos: &mut Gizmos
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
                    elapsed_time
                ),
                camera,
                things_catalog: &self.things_catalog
            });

            return;
        }

        self.state.draw(&mut DrawBundle {
            window,
            delta_time: time.delta_seconds(),
            egui_context,
            drawer: EditDrawer::new(
                commands,
                prop_cameras,
                meshes,
                meshes_query,
                &mut self.drawing_resources,
                color_resources,
                self.state.tools_settings(),
                elapsed_time,
                camera.scale(),
                paint_tool_camera.scale(),
                self.state.show_collision_overlay()
            ),
            camera,
            prop_cameras,
            paint_tool_camera,
            things_catalog: &self.things_catalog,
            cursor: &self.cursor_pos,
            #[cfg(feature = "debug")]
            gizmos
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
            &self.drawing_resources
        );
    }

    /// Shutdown cleanup.
    #[inline]
    pub fn cleanup(&self, meshes: &mut Assets<Mesh>) { self.drawing_resources.cleanup(meshes); }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Returns the title of the window based on the opened file.
#[inline]
#[must_use]
fn window_title(open_file: Option<&str>) -> String
{
    match open_file
    {
        Some(file) => format!("{NAME} - {file}"),
        None => NAME.to_owned()
    }
}
