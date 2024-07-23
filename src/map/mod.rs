pub mod brush;
#[cfg(feature = "ui")]
mod camera;
pub mod drawer;
#[cfg(feature = "ui")]
pub mod editor;
mod indexed_map;
pub mod path;
pub mod properties;
mod selectable_vector;
pub mod thing;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{fs::File, io::BufReader, path::PathBuf};

use hill_vacuum_shared::return_if_none;
use properties::DefaultProperties;
use serde::{Deserialize, Serialize};

use crate::{
    map::thing::ThingViewer,
    utils::{
        containers::{hv_hash_map, hv_vec},
        misc::AssertedInsertRemove
    },
    Animation,
    HvHashMap,
    Id,
    TextureInterface
};

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The header of the saved map file.
#[derive(Clone, Copy, Serialize, Deserialize)]
struct MapHeader
{
    /// The amount of brushes.
    pub brushes:    usize,
    /// The amount of things.
    pub things:     usize,
    /// The amount of animations.
    pub animations: usize,
    /// The amount of props.
    pub props:      usize
}

//=======================================================================//

/// The struct used to read a map file and generate the brushes and things to be used to generate
/// another file format.
/// ```
/// let exporter = hill_vacuum::Exporter::new(&std::env::args().collect::<Vec<_>>()[0]);
/// // Your code.
/// ```
#[must_use]
pub struct Exporter(pub HvHashMap<Id, crate::Brush>, pub HvHashMap<Id, crate::ThingInstance>);

impl Exporter
{
    /// Returns a new [`Exporter`] generated from the requested `path`, unless there was an error.
    /// # Errors
    /// Returns an error if there was an issue reading the requested file.
    #[allow(clippy::missing_panics_doc)]
    #[inline]
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, &'static str>
    {
        let file = match File::open(Into::<PathBuf>::into(path))
        {
            Ok(file) => file,
            Err(_) => return Err("Could not open the file")
        };

        let mut file = BufReader::new(file);

        let header = match ciborium::from_reader::<MapHeader, _>(&mut file)
        {
            Ok(header) => header,
            Err(_) => return Err("Error reading file header")
        };

        if ciborium::from_reader::<DefaultProperties, _>(&mut file).is_err()
        {
            return Err("Error reading default Brush properties");
        }

        if ciborium::from_reader::<DefaultProperties, _>(&mut file).is_err()
        {
            return Err("Error reading default Thing properties");
        }

        let animations = match drawer::file_animations(header.animations, &mut file)
        {
            Ok(animations) => animations,
            Err(_) => return Err("Error reading default animations")
        };

        let mut brushes = hv_vec![];

        for _ in 0..header.brushes
        {
            let brush = match ciborium::from_reader::<crate::map::brush::Brush, _>(&mut file)
            {
                Ok(brush) => brush,
                Err(_) => return Err("Error reading Brush")
            };

            brushes.push(crate::Brush::new(brush));
        }

        if !animations.is_empty()
        {
            // Replaces the empty animations of a brush with the texture's default one.
            let mut textured_anim_none = brushes
                .iter()
                .enumerate()
                .filter_map(|(i, brush)| {
                    matches!(return_if_none!(&brush.texture, None).animation(), Animation::None)
                        .then_some(i)
                })
                .collect::<Vec<_>>();

            for animation in animations
            {
                let mut i = 0;

                while i < textured_anim_none.len()
                {
                    let brush = &mut brushes[textured_anim_none[i]];

                    if brush.texture.as_ref().unwrap().name() == animation.texture
                    {
                        brush.set_texture_animation(animation.animation.clone());
                        textured_anim_none.swap_remove(i);
                        continue;
                    }

                    i += 1;
                }
            }
        }

        let mut things = hv_hash_map![];

        for _ in 0..header.things
        {
            let thing =
                match ciborium::from_reader::<crate::map::thing::ThingInstance, _>(&mut file)
                {
                    Ok(thing) => ThingViewer::new(thing),
                    Err(_) => return Err("Error reading ThingInstance")
                };

            things.asserted_insert((thing.id, thing));
        }

        let mut brushes_map = hv_hash_map![];

        for brush in brushes
        {
            brushes_map.asserted_insert((brush.id, brush));
        }

        Ok(Self(brushes_map, things))
    }
}

//=======================================================================//
// UI
//
//=======================================================================//

#[cfg(feature = "ui")]
pub(in crate::map) mod ui_mod
{
    //=======================================================================//
    // IMPORTS
    //
    //=======================================================================//

    use std::{hash::Hash, ops::RangeInclusive};

    use bevy::{
        input::mouse::MouseWheel,
        prelude::*,
        render::{camera::RenderTarget, render_resource::Extent3d},
        sprite::Mesh2dHandle,
        window::{PrimaryWindow, WindowCloseRequested, WindowMode},
        winit::WinitSettings
    };
    use bevy_egui::{
        egui,
        EguiContext,
        EguiContextQuery,
        EguiContexts,
        EguiInput,
        EguiPlugin,
        EguiSet,
        EguiUserTextures
    };
    use glam::Vec2;
    use hill_vacuum_shared::{continue_if_no_match, return_if_err, return_if_none, NextValue};

    use super::thing::HardcodedThings;
    use crate::{
        config::Config,
        map::{
            camera::init_camera_transform,
            drawer::{
                color::Color,
                drawing_resources::DrawingResources,
                texture_loader::{TextureLoader, TextureLoadingProgress}
            },
            editor::{
                state::clipboard::{
                    PaintToolPropCamera,
                    Prop,
                    PropCamera,
                    PropCameras,
                    PropCamerasMut
                },
                Editor,
                Placeholder
            },
            properties::{BrushProperties, ThingProperties}
        },
        utils::{
            hull::{EntityHull, Hull},
            misc::Toggle
        },
        EditorState
    };

    //=======================================================================//
    // CONSTANTS
    //
    //=======================================================================//

    /// The size of half of the map square.
    pub(in crate::map) const MAP_HALF_SIZE: f32 = 16384f32;
    /// The size of the map square.
    pub(in crate::map) const MAP_SIZE: f32 = MAP_HALF_SIZE * 2f32;
    /// The range of the map dimensions.
    pub(in crate::map) const MAP_RANGE: RangeInclusive<f32> = -MAP_HALF_SIZE..=MAP_HALF_SIZE;
    /// The [`Hull`] representing the map's area.
    const MAP_RECT: Hull = unsafe {
        std::mem::transmute::<_, Hull>([
            MAP_HALF_SIZE,
            -MAP_HALF_SIZE,
            -MAP_HALF_SIZE,
            MAP_HALF_SIZE
        ])
    };
    /// The general offset of the tooltips.
    pub(in crate::map) const TOOLTIP_OFFSET: egui::Vec2 = egui::Vec2::new(0f32, -12.5);
    /// The rows of cameras used to take screenshots of the props placed around the map area.
    pub(in crate::map) const PROP_CAMERAS_ROWS: usize = 2;
    /// The amount of prop screenshot taking cameras placed around the map.
    pub(in crate::map) const PROP_CAMERAS_AMOUNT: usize =
        8 * (PROP_CAMERAS_ROWS * (PROP_CAMERAS_ROWS + 1)) / 2;

    //=======================================================================//
    // TRAIT
    //
    //=======================================================================//

    /// A trait to determine wherever an entity fits within the map's bounds.
    pub(crate) trait OutOfBounds
    {
        /// Whether the entity fits within the map bounds.
        #[must_use]
        fn out_of_bounds(&self) -> bool;
    }

    impl OutOfBounds for Hull
    {
        #[inline]
        fn out_of_bounds(&self) -> bool
        {
            self.top() > MAP_RECT.top() ||
                self.bottom() < MAP_RECT.bottom() ||
                self.left() < MAP_RECT.left() ||
                self.right() > MAP_RECT.right()
        }
    }

    impl OutOfBounds for Vec2
    {
        #[inline]
        fn out_of_bounds(&self) -> bool { !MAP_RECT.contains_point(*self) }
    }

    impl OutOfBounds for f32
    {
        #[inline]
        fn out_of_bounds(&self) -> bool { self.abs() > MAP_HALF_SIZE }
    }

    impl<T: EntityHull> OutOfBounds for T
    {
        fn out_of_bounds(&self) -> bool { self.hull().out_of_bounds() }
    }

    //=======================================================================//

    impl Toggle for WindowMode
    {
        /// Switches the [`WindowMode`] from windowed to borderless fullscreen, and viceversa.
        #[inline]
        fn toggle(&mut self)
        {
            *self = match self
            {
                WindowMode::Windowed => WindowMode::BorderlessFullscreen,
                WindowMode::BorderlessFullscreen => WindowMode::Windowed,
                _ => unreachable!()
            };
        }
    }

    //=======================================================================//
    // ENUMS
    //
    //=======================================================================//

    /// The two execution steps of the running application.
    #[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
    enum EditorSet
    {
        /// Update entities.
        Update,
        /// Draw visible entities.
        Draw
    }

    //=======================================================================//
    // TYPES
    //
    //=======================================================================//

    /// The query of the main camera.
    type MainCameraQuery<'world, 'state, 'a> = Query<
        'world,
        'state,
        &'a Transform,
        (With<Camera>, Without<PropCamera>, Without<PaintToolPropCamera>)
    >;

    //=======================================================================//

    /// The query of the mutable main camera.
    type MainCameraQueryMut<'world, 'state, 'a> = Query<
        'world,
        'state,
        &'a mut Transform,
        (With<Camera>, Without<PropCamera>, Without<PaintToolPropCamera>)
    >;

    //=======================================================================//

    /// The query of the camera used by the paint tool.
    type PaintToolCameraQuery<'world, 'state, 'a> =
        Query<'world, 'state, &'a Transform, (With<PaintToolPropCamera>, Without<PropCamera>)>;

    //=======================================================================//

    /// The query of the mutable camera used by the paint tool.
    type PaintToolCameraQueryMut<'world, 'state, 'a> = Query<
        'world,
        'state,
        (&'a mut Camera, &'a mut Transform),
        (With<PaintToolPropCamera>, Without<PropCamera>)
    >;

    //=======================================================================//

    /// The plugin that builds the map editor.
    pub(crate) struct MapEditorPlugin;

    impl Plugin for MapEditorPlugin
    {
        #[inline]
        fn build(&self, app: &mut App)
        {
            app
            // UI
            .add_plugins(EguiPlugin)
            .add_systems(PreUpdate, clean_egui_inputs.after(EguiSet::ProcessInput).before(EguiSet::BeginFrame))
            // Init resources
            .insert_non_send_resource(unsafe { Editor::placeholder() })
            .init_state::<TextureLoadingProgress>()
            .insert_resource(ClearColor(Color::Clear.default_bevy_color()))
            .insert_resource(WinitSettings::default())
            .init_resource::<TextureLoader>()
            // Setup
            .add_systems(PostStartup, initialize)
            // Texture loading
            .add_systems(
                Update,
                (load_textures, texture_loading_ui).chain().run_if(not(in_state(TextureLoadingProgress::Complete)))
            )
            .add_systems(
                OnEnter(TextureLoadingProgress::Complete),
                store_loaded_textures
            )
            // Handle entity creation and editing
            .add_systems(
                Update,
                (
                    update_state,
                    update_active_tool
                )
                .chain()
                .in_set(EditorSet::Update)
                .run_if(in_state(EditorState::Run))
            )
            .add_systems(
                Update,
                draw
                    .in_set(EditorSet::Draw)
                    .after(EditorSet::Update)
                    .run_if(in_state(EditorState::Run))
            )
            // Shutdown
            .add_systems(OnEnter(EditorState::ShutDown), cleanup);
        }
    }

    //=======================================================================//
    // FUNCTIONS
    //
    //=======================================================================//

    /// Initializes the editor.
    #[allow(clippy::needless_pass_by_value)]
    #[allow(clippy::cast_precision_loss)]
    #[inline]
    fn initialize(
        mut commands: Commands,
        mut images: ResMut<Assets<Image>>,
        mut egui_contexts: Query<EguiContextQuery>
    )
    {
        /// Spawns a [`bevy::render::camera::Camera`] with the added `marker`.
        macro_rules! camera {
            ($marker:ident) => {
                #[must_use]
                #[inline]
                fn prop_camera(images: &mut Assets<Image>, pos: Vec2) -> (Camera2dBundle, $marker)
                {
                    (
                        Camera2dBundle {
                            camera: Camera {
                                is_active: false,
                                target: RenderTarget::Image(images.add(Prop::image(Extent3d {
                                    width:                 1,
                                    height:                1,
                                    depth_or_array_layers: 1
                                }))),
                                ..Default::default()
                            },
                            transform: Transform::from_translation(pos.extend(0f32)),
                            projection: OrthographicProjection {
                                near: -1000f32,
                                far: 10000f32,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        $marker
                    )
                }
            };
        }

        let mut context = egui_contexts.iter_mut().next_value();

        // Cameras.
        commands.spawn(Camera2dBundle {
            transform: init_camera_transform(),
            projection: OrthographicProjection {
                near: f32::MIN,
                far: f32::MAX,
                ..Default::default()
            },
            ..Default::default()
        });

        let mut prop_cameras_amount = 0;
        let mut y = 0f32;

        for i in 0..PROP_CAMERAS_ROWS
        {
            camera!(PropCamera);

            let plus_one = i + 1;
            let start = MAP_SIZE * (plus_one as f32);
            y = -start;

            for _ in 0..=(plus_one * 2)
            {
                commands.spawn(prop_camera(&mut images, Vec2::new(-start, y)));
                commands.spawn(prop_camera(&mut images, Vec2::new(start, y)));

                y += MAP_SIZE;
                prop_cameras_amount += 2;
            }

            let mut x = -start + MAP_SIZE;

            for _ in 0..=(i * 2)
            {
                commands.spawn(prop_camera(&mut images, Vec2::new(x, start)));
                commands.spawn(prop_camera(&mut images, Vec2::new(x, -start)));

                x += MAP_SIZE;
                prop_cameras_amount += 2;
            }
        }

        assert!(prop_cameras_amount == PROP_CAMERAS_AMOUNT, "Incoherent prop cameras.");

        camera!(PaintToolPropCamera);
        commands.spawn(prop_camera(&mut images, Vec2::new(0f32, y + MAP_SIZE)));

        // Extract necessary values.
        let ctx = context.ctx.get_mut();

        // Initialize the labels.
        let egui::FullOutput {
            platform_output,
            textures_delta,
            ..
        } = ctx.run(egui::RawInput::default(), |ctx| {
            DrawingResources::init(ctx);
        });
        context.render_output.textures_delta.append(textures_delta);
        context.egui_output.platform_output = platform_output.clone();

        // Set looks.
        let mut style = (*ctx.style()).clone();
        for font in style.text_styles.values_mut()
        {
            font.size += 2f32;
        }
        ctx.set_style(style);

        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = egui::Color32::WHITE.into();
        visuals.faint_bg_color = egui::Color32::from_gray(35);
        ctx.set_visuals(visuals);
    }

    //=======================================================================//

    /// Removes tab from the egui inputs.
    #[allow(clippy::needless_pass_by_value)]
    #[inline]
    fn clean_egui_inputs(mut input: Query<&mut EguiInput>, editor: NonSend<Editor>)
    {
        let events = &mut input.get_single_mut().unwrap().0.events;
        let mut iter = events.iter_mut().enumerate();
        let mut index = None;
        let mut add_escape = false;

        for (i, ev) in iter.by_ref()
        {
            let (key, modifiers) = continue_if_no_match!(
                ev,
                egui::Event::Key {
                    key,
                    modifiers,
                    pressed: true,
                    ..
                },
                (key, modifiers)
            );
            *modifiers = egui::Modifiers::NONE;

            match key
            {
                egui::Key::Tab =>
                {
                    index = i.into();
                    break;
                },
                egui::Key::F4 => add_escape = true,
                _ => ()
            };
        }

        for (_, ev) in iter
        {
            let (key, modifiers) = continue_if_no_match!(
                ev,
                egui::Event::Key {
                    key,
                    modifiers,
                    pressed: true,
                    ..
                },
                (key, modifiers)
            );
            *modifiers = egui::Modifiers::NONE;

            if *key == egui::Key::F4
            {
                add_escape = true;
            }
        }

        // If F4 is pressed to close an egui window add an artificial escape press to surrender
        // focus of the text editor being used (if any) before the window is closed.
        if add_escape && editor.is_window_focused()
        {
            events.push(egui::Event::Key {
                key:          egui::Key::Escape,
                physical_key: egui::Key::Escape.into(),
                pressed:      true,
                repeat:       false,
                modifiers:    egui::Modifiers::NONE
            });
        }

        events.swap_remove(return_if_none!(index));
    }

    //=======================================================================//

    /// Stores the loaded textures in the [`Editor`].
    #[allow(clippy::needless_pass_by_value)]
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn store_loaded_textures(
        mut window: Query<&mut Window, With<PrimaryWindow>>,
        mut prop_cameras: PropCamerasMut,
        asset_server: Res<AssetServer>,
        mut images: ResMut<Assets<Image>>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<ColorMaterial>>,
        mut user_textures: ResMut<EguiUserTextures>,
        mut editor: NonSendMut<Editor>,
        config: Res<Config>,
        mut texture_loader: ResMut<TextureLoader>,
        hardcoded_things: Option<Res<HardcodedThings>>,
        brush_properties: Option<ResMut<BrushProperties>>,
        thing_properties: Option<ResMut<ThingProperties>>,
        state: Res<State<EditorState>>,
        mut next_state: ResMut<NextState<EditorState>>
    )
    {
        if *state.get() == EditorState::SplashScreen
        {
            *editor = Editor::new(
                window.single_mut().as_mut(),
                &mut prop_cameras,
                &asset_server,
                &mut images,
                &mut meshes,
                &mut materials,
                &mut user_textures,
                &config,
                &mut texture_loader,
                hardcoded_things,
                brush_properties,
                thing_properties
            );

            next_state.set(EditorState::Run);
            return;
        }

        editor.reload_textures(
            &mut prop_cameras,
            &mut images,
            &mut materials,
            &mut user_textures,
            texture_loader.loaded_textures()
        );
    }

    //=======================================================================//

    /// Updates the editor state.
    #[allow(clippy::needless_pass_by_value)]
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn update_state(
        mut window: Query<&mut Window, With<PrimaryWindow>>,
        mut images: ResMut<Assets<Image>>,
        mut materials: ResMut<Assets<ColorMaterial>>,
        mut camera: MainCameraQueryMut,
        mut prop_cameras: PropCamerasMut,
        mouse_buttons: Res<ButtonInput<MouseButton>>,
        mut mouse_wheel: EventReader<MouseWheel>,
        mut key_inputs: ResMut<ButtonInput<KeyCode>>,
        time: Res<Time>,
        mut close_events: EventReader<WindowCloseRequested>,
        mut egui_contexts: Query<
            (&'static mut EguiContext, Option<&'static PrimaryWindow>),
            With<Window>
        >,
        mut user_textures: ResMut<EguiUserTextures>,
        mut editor: NonSendMut<Editor>,
        mut config: ResMut<Config>,
        mut next_editor_state: ResMut<NextState<EditorState>>,
        mut next_tex_load: ResMut<NextState<TextureLoadingProgress>>
    )
    {
        let mut window = return_if_err!(window.get_single_mut());
        let egui_context = egui_contexts
            .iter_mut()
            .find_map(|(ctx, pw)| pw.map(|_| ctx))
            .unwrap()
            .into_inner()
            .get_mut();
        let mut camera = camera.single_mut();

        if close_events.read().next().is_some() &&
            editor.quit(
                &mut window,
                &mut images,
                &mut materials,
                &mut camera,
                &mut prop_cameras,
                &time,
                egui_context,
                &mut user_textures,
                &mouse_buttons,
                &mut key_inputs,
                &mut config,
                &mut next_editor_state,
                &mut next_tex_load
            )
        {
            return;
        }

        editor.update(
            &mut window,
            &mut images,
            &mut materials,
            &mut camera,
            &mut prop_cameras,
            &time,
            egui_context,
            &mut user_textures,
            &mouse_buttons,
            &mut mouse_wheel,
            &mut key_inputs,
            &mut config,
            &mut next_editor_state,
            &mut next_tex_load
        );
    }

    //=======================================================================//

    /// Updates the active tool.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::needless_pass_by_value)]
    #[inline]
    fn update_active_tool(
        window: Query<&Window, With<PrimaryWindow>>,
        mut images: ResMut<Assets<Image>>,
        mut camera: MainCameraQueryMut,
        mut prop_cameras: PropCamerasMut,
        mut paint_tool_camera: PaintToolCameraQueryMut,
        time: Res<Time>,
        mut user_textures: ResMut<EguiUserTextures>,
        mut editor: NonSendMut<Editor>
    )
    {
        let mut paint_tool_camera = paint_tool_camera.single_mut();

        editor.update_active_tool(
            return_if_err!(window.get_single()),
            &mut images,
            camera.get_single_mut().unwrap().as_mut(),
            &mut prop_cameras,
            (paint_tool_camera.0.as_mut(), paint_tool_camera.1.as_mut()),
            &time,
            &mut user_textures
        );
    }

    //=======================================================================//

    /// Draws the visible portion of the map.
    #[allow(clippy::needless_pass_by_value)]
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn draw(
        mut commands: Commands,
        window: Query<&Window, With<PrimaryWindow>>,
        camera: MainCameraQuery,
        prop_cameras: PropCameras,
        paint_tool_camera: PaintToolCameraQuery,
        mut meshes: ResMut<Assets<Mesh>>,
        time: Res<Time>,
        mut egui_context: EguiContexts,
        meshes_query: Query<Entity, With<Mesh2dHandle>>,
        mut editor: NonSendMut<Editor>,
        config: Res<Config>
    )
    {
        editor.draw(
            &mut commands,
            return_if_err!(window.get_single()),
            camera.single(),
            &prop_cameras,
            paint_tool_camera.single(),
            &time,
            &mut meshes,
            egui_context.ctx_mut(),
            &meshes_query,
            &config.colors
        );
    }

    //=======================================================================//

    /// Shutdown cleanup.
    #[allow(clippy::needless_pass_by_value)]
    #[inline]
    fn cleanup(mut meshes: ResMut<Assets<Mesh>>, editor: NonSend<Editor>)
    {
        editor.cleanup(&mut meshes);
    }

    //=======================================================================//

    /// Loads the textures from the assets files.
    #[allow(clippy::needless_pass_by_value)]
    #[inline]
    fn load_textures(
        mut images: ResMut<Assets<Image>>,
        mut user_textures: ResMut<EguiUserTextures>,
        mut texture_loader: ResMut<TextureLoader>,
        mut load_state: ResMut<NextState<TextureLoadingProgress>>
    )
    {
        texture_loader.load(&mut images, &mut user_textures, &mut load_state);
    }

    //=======================================================================//

    /// The UI of the texture loading process.
    #[allow(clippy::needless_pass_by_value)]
    #[inline]
    fn texture_loading_ui(
        window: Query<&Window, With<PrimaryWindow>>,
        mut egui_context: EguiContexts,
        texture_loader: Res<TextureLoader>
    )
    {
        texture_loader.ui(window.single(), egui_context.ctx_mut());
    }
}

#[cfg(feature = "ui")]
pub(crate) use ui_mod::*;
