pub mod brush;
mod camera;
pub mod containers;
pub mod drawer;
mod editor;
mod ordered_map;
pub mod path;
mod selectable_vector;
pub mod thing;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{fs::File, hash::Hash, io::BufReader, ops::RangeInclusive, path::PathBuf};

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
use serde::{Deserialize, Serialize};
use shared::{continue_if_no_match, return_if_err, return_if_none, NextValue};

use self::{
    brush::{Brush, BrushViewer},
    camera::init_camera_transform,
    containers::{hv_vec, HvHashMap, HvVec},
    drawer::{
        color::Color,
        drawing_resources::DrawingResources,
        texture_loader::{TextureLoader, TextureLoadingProgress}
    },
    editor::{
        state::clipboard::{Prop, PropCameras, PropCamerasMut},
        Editor
    },
    thing::ThingInstance
};
use crate::{
    config::Config,
    map::{
        editor::state::clipboard::{PaintToolPropCamera, PropCamera},
        thing::ThingViewer
    },
    utils::{
        hull::{EntityHull, Hull},
        misc::Toggle
    },
    Animation,
    EditorState,
    HardcodedThings,
    TextureInterface,
    PROP_CAMERAS_AMOUNT,
    PROP_CAMERAS_ROWS
};

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

/// The size of half of the map square.
const MAP_HALF_SIZE: f32 = 16384f32;
/// The size of the map square.
const MAP_SIZE: f32 = MAP_HALF_SIZE * 2f32;
/// The range of the map dimensions.
const MAP_RANGE: RangeInclusive<f32> = -MAP_HALF_SIZE..=MAP_HALF_SIZE;
/// The [`Hull`] representing the map's area.
const MAP_RECT: Hull = unsafe {
    std::mem::transmute::<_, Hull>([MAP_HALF_SIZE, -MAP_HALF_SIZE, -MAP_HALF_SIZE, MAP_HALF_SIZE])
};

/// The general offset of the tooltips.
const TOOLTIP_OFFSET: egui::Vec2 = egui::Vec2::new(0f32, -12.5);
/// Cyan in [`egui::Color32`] format.
const EGUI_CYAN: egui::Color32 = egui::Color32::from_rgb(0, 255, 255);

//=======================================================================//
// TRAIT
//
//=======================================================================//

/// A trait to determine wherever an entity fits within the map's bounds.
pub trait OutOfBounds
{
    /// Whever the entity fits within the map bounds.
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

/// A trait for collections that allows to insert and remove a value but causes the application to
/// panic if the insert or remove was unsuccesful.
trait AssertedInsertRemove<T, U, V, X>
{
    /// Insert `value` in the collection. Panics if the collection already contains `value`.
    fn asserted_insert(&mut self, value: T) -> V;

    /// Remove `value` from the collection. Panics if the collection does not contain `value`.
    fn asserted_remove(&mut self, value: &U) -> X;
}

impl<K, V> AssertedInsertRemove<(K, V), K, (), V> for HvHashMap<K, V>
where
    K: Eq + std::hash::Hash
{
    /// Inserts `value`, a (key, element) pair. Panics if the collection already contains the key.
    #[inline]
    fn asserted_insert(&mut self, value: (K, V))
    {
        assert!(self.insert(value.0, value.1).is_none(), "Key is a already present.");
    }

    /// Remove the element associated with the key `value`. Panics if the collection does not
    /// contain `value`. Returns the removed element.
    #[inline]
    fn asserted_remove(&mut self, value: &K) -> V { self.remove(value).unwrap() }
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

type MainCameraQuery<'world, 'state, 'a> = Query<
    'world,
    'state,
    &'a Transform,
    (With<Camera>, Without<PropCamera>, Without<PaintToolPropCamera>)
>;

//=======================================================================//

type MainCameraQueryMut<'world, 'state, 'a> = Query<
    'world,
    'state,
    &'a mut Transform,
    (With<Camera>, Without<PropCamera>, Without<PaintToolPropCamera>)
>;

//=======================================================================//

type PaintToolCameraQuery<'world, 'state, 'a> =
    Query<'world, 'state, &'a Transform, (With<PaintToolPropCamera>, Without<PropCamera>)>;

//=======================================================================//

type PaintToolCameraQueryMut<'world, 'state, 'a> = Query<
    'world,
    'state,
    (&'a mut Camera, &'a mut Transform),
    (With<PaintToolPropCamera>, Without<PropCamera>)
>;

//=======================================================================//

/// The plugin that builds the map editor.
#[allow(clippy::module_name_repetitions)]
pub struct MapEditorPlugin;

impl Plugin for MapEditorPlugin
{
    #[inline]
    fn build(&self, app: &mut App)
    {
        app
            // UI
            .add_plugins(EguiPlugin)
            .add_systems(PreUpdate, egui_clear_tab.after(EguiSet::ProcessInput).before(EguiSet::BeginFrame))
            // Init resources
            .insert_non_send_resource(Editor::placeholder())
            .insert_state(TextureLoadingProgress::default())
            .insert_resource(ClearColor(Color::Clear.bevy_color()))
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
                (store_loaded_textures, apply_state_transition::<EditorState>).chain()
            )
            // Handle brush creation and editing
            .add_systems(
                Update,
                (
                    update_state,
                    update_active_tool,
                    apply_state_transition::<EditorState>
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
            // Shutdowm
            .add_systems(OnEnter(EditorState::ShutDown), cleanup);
    }
}

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
#[must_use]
pub struct Exporter(pub HvVec<BrushViewer>, pub HvVec<ThingViewer>);

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

        let animations = match DrawingResources::file_animations(header.animations, &mut file)
        {
            Ok(animations) => animations,
            Err(_) => return Err("Error reading default animations")
        };

        let mut brushes = hv_vec![];

        for _ in 0..header.brushes
        {
            let brush = match ciborium::from_reader::<Brush, _>(&mut file)
            {
                Ok(brush) => brush,
                Err(_) => return Err("Error reading Brush")
            };

            brushes.push(BrushViewer::new(brush));
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

        let mut things = hv_vec![];

        for _ in 0..header.things
        {
            let thing = match ciborium::from_reader::<ThingInstance, _>(&mut file)
            {
                Ok(thing) => ThingViewer::new(thing),
                Err(_) => return Err("Error reading ThingInstance")
            };

            things.push(thing);
        }

        Ok(Self(brushes, things))
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
pub(in crate::map) fn initialize(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut egui_contexts: Query<EguiContextQuery>
)
{
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

    assert!(prop_cameras_amount == PROP_CAMERAS_AMOUNT);

    camera!(PaintToolPropCamera);
    commands.spawn(prop_camera(&mut images, Vec2::new(0f32, y + MAP_SIZE)));

    // Extract necessary values.
    let ctx = context.ctx.get_mut();

    // Do a fake frame thing to allow the labels initialization.
    ctx.begin_frame(egui::RawInput::default());
    let full_output = ctx.end_frame();

    let egui::FullOutput {
        platform_output,
        textures_delta,
        ..
    } = full_output;
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
    ctx.set_visuals(visuals);

    DrawingResources::init(ctx);
}

//=======================================================================//

#[inline]
fn egui_clear_tab(mut input: Query<&mut EguiInput>)
{
    let events = &mut input.get_single_mut().unwrap().0.events;
    let mut iter = events.iter_mut().enumerate();
    let mut index = None;

    for (i, ev) in iter.by_ref()
    {
        let (key, modifiers) =
            continue_if_no_match!(ev, egui::Event::Key { key, modifiers, .. }, (key, modifiers));
        *modifiers = egui::Modifiers::NONE;

        if *key == egui::Key::Tab
        {
            index = i.into();
            break;
        }
    }

    for (_, ev) in iter
    {
        *continue_if_no_match!(ev, egui::Event::Key { modifiers, .. }, modifiers) =
            egui::Modifiers::NONE;
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
            hardcoded_things
        );

        next_state.set(EditorState::Run);
        return;
    }

    editor.reload_textures(&mut materials, texture_loader.loaded_textures());
}

//=======================================================================//

/// Updates the editor state.
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::too_many_arguments)]
#[inline]
fn update_state(
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
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
    #[cfg(feature = "debug")] mut gizmos: bevy::gizmos::gizmos::Gizmos
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
        #[cfg(feature = "debug")]
        &mut gizmos
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
