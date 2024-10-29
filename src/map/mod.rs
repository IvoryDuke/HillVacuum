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

use hill_vacuum_proc_macros::EnumIter;
use hill_vacuum_shared::{continue_if_none, return_if_none, NextValue};
use properties::DefaultPropertiesViewer;
use serde::{Deserialize, Serialize};

use crate::{
    utils::{
        collections::{hash_map, HashMap},
        misc::AssertedInsertRemove
    },
    Id,
    TextureInterface
};
#[allow(unused_imports)]
use crate::{Brush, ThingInstance};

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

/// The version of the saved files.
const FILE_VERSION: &str = "0.10";

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[must_use]
#[derive(Debug, Clone, Copy, EnumIter, PartialEq)]
enum FileStructure
{
    Version,
    Header,
    Grid,
    Animations,
    Properties,
    Brushes,
    Things,
    Props
}

impl FileStructure
{
    #[inline]
    pub fn assert(self, value: Self)
    {
        assert!(
            self == value,
            "Mismatching file structure step. Current: {self:?} Requested: {value:?}."
        );
    }
}

//=======================================================================//

/// The settings of the map grid saved into the map files.
#[must_use]
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
enum GridSettings
{
    #[default]
    None,
    Skew(i8),
    Rotate(i16),
    Isometric
    {
        /// How much the vertical lines are skewed.
        skew:  i8,
        /// The angle of rotation of the grid.
        angle: i16
    }
}

impl GridSettings
{
    #[inline]
    #[must_use]
    pub const fn skew(self) -> i8
    {
        match self
        {
            Self::Skew(skew) | Self::Isometric { skew, .. } => skew,
            _ => 0
        }
    }

    #[inline]
    #[must_use]
    pub const fn angle(self) -> i16
    {
        match self
        {
            Self::Rotate(angle) | Self::Isometric { angle, .. } => angle,
            _ => 0
        }
    }
}

//=======================================================================//
// STRUCTS
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

/// The struct used to read a map file and extract the information necessary, for example, to export
/// it to another format.
/// ```
/// let exporter = hill_vacuum::Exporter::new(&std::env::args().collect::<Vec<_>>()[0]);
/// // Your code.
/// ```
#[must_use]
pub struct Exporter
{
    /// The rotation angle of the grid.
    pub grid_angle: i16,
    /// The skew angle of the grid.
    pub grid_skew:  i8,
    /// The [`Brush`]es inside the map.
    pub brushes:    HashMap<Id, crate::Brush>,
    /// The [`ThingInstance`]s inside the map.
    pub things:     HashMap<Id, crate::ThingInstance>
}

impl Exporter
{
    /// Returns a new [`Exporter`] generated from the requested `path`, unless there was an error.
    /// # Errors
    /// Returns an error if there was an issue reading the requested file.
    #[allow(clippy::missing_panics_doc)]
    #[inline]
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, &'static str>
    {
        let file =
            File::open(Into::<PathBuf>::into(path)).map_err(|_| "Could not open the file")?;

        let mut file = BufReader::new(file);
        let mut steps = FileStructure::iter();

        // Version.
        steps.next_value().assert(FileStructure::Version);

        if version_number(&mut file)? != FILE_VERSION
        {
            return Err("Cannot export previous map versions, save the file to upgrade it to the \
                        latest version.");
        }

        // Header.
        steps.next_value().assert(FileStructure::Header);

        let header = ciborium::from_reader::<MapHeader, _>(&mut file)
            .map_err(|_| "Error reading file header")?;

        // Grid.
        steps.next_value().assert(FileStructure::Grid);
        let grid_settings = ciborium::from_reader::<GridSettings, _>(&mut file)
            .map_err(|_| "Error reading grid")?;

        // Animations.
        steps.next_value().assert(FileStructure::Animations);

        let animations = drawer::file_animations(header.animations, &mut file)
            .map_err(|_| "Error reading default animations")?;

        // Properties.
        steps.next_value().assert(FileStructure::Properties);

        for _ in 0..2
        {
            _ = ciborium::from_reader::<DefaultPropertiesViewer, _>(&mut file)
                .map_err(|_| "Error reading default properties")?;
        }

        // Brushes.
        steps.next_value().assert(FileStructure::Brushes);

        let mut brushes = Vec::new();

        for _ in 0..header.brushes
        {
            brushes.push(
                ciborium::from_reader::<crate::Brush, _>(&mut file)
                    .map_err(|_| "Error reading Brush")?
            );
        }

        if !animations.is_empty()
        {
            // Replaces the empty animations of a brush with the texture's default one.
            for texture in brushes.iter_mut().filter_map(|brush| {
                let texture = return_if_none!(brush.texture.as_mut(), None);
                texture.animation().is_none().then_some(texture)
            })
            {
                unsafe {
                    texture.unsafe_set_animation(
                        continue_if_none!(animations.get(texture.name())).clone()
                    );
                }
            }
        }

        // Things.
        steps.next_value().assert(FileStructure::Things);

        let mut things = hash_map![];

        for _ in 0..header.things
        {
            let thing = ciborium::from_reader::<crate::ThingInstance, _>(&mut file)
                .map_err(|_| "Error reading ThingInstance")?;
            things.asserted_insert((thing.id, thing));
        }

        let mut brushes_map = hash_map![];

        for brush in brushes
        {
            brushes_map.asserted_insert((brush.id, brush));
        }

        Ok(Self {
            grid_angle: grid_settings.angle(),
            grid_skew: grid_settings.skew(),
            brushes: brushes_map,
            things
        })
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Reads the version number from `file`.
#[inline]
fn version_number(file: &mut BufReader<File>) -> Result<String, &'static str>
{
    ciborium::from_reader(&mut *file).map_err(|_| "Error reading file version")
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

    use std::ops::RangeInclusive;

    use bevy::{
        input::mouse::MouseWheel,
        prelude::*,
        render::{camera::RenderTarget, render_resource::Extent3d},
        sprite::Mesh2dHandle,
        window::{PrimaryWindow, WindowCloseRequested},
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

    use super::{
        editor::state::{grid::Grid, ui::UiFocus},
        thing::HardcodedThings,
        GridSettings
    };
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
                    prop::Prop,
                    PaintToolPropCamera,
                    PropCamera,
                    PropCameras,
                    PropCamerasMut
                },
                Editor,
                Placeholder
            },
            properties::{BrushUserProperties, ThingUserProperties}
        },
        utils::hull::Hull,
        warning_message,
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
    pub(in crate::map) const TOOLTIP_OFFSET: Vec2 = Vec2::new(0f32, -12.5);
    /// The rows of cameras used to take screenshots of the props placed around the map area.
    pub(in crate::map) const PROP_CAMERAS_ROWS: usize = 2;
    /// The amount of prop screenshot taking cameras placed around the map.
    pub(in crate::map) const PROP_CAMERAS_AMOUNT: usize =
        8 * (PROP_CAMERAS_ROWS * (PROP_CAMERAS_ROWS + 1)) / 2;
    pub(in crate::map) const PREVIOUS_FILE_VERSION: &str = "0.9";
    // /// The string that is appended to the name of a converted `.hv` file.
    pub(in crate::map) const CONVERTED_FILE_APPENDIX: &str = "_010.hv";
    /// The warning that is displayed when trying to convert a no longer supported `.hv` file.
    pub(in crate::map) const UPGRADE_WARNING: &str =
        "This file appears to use a no longer supported format, only files with version 0.9.0 are \
         supported to be upgraded to version 0.10.0.\nTo upgrade the file you will need to open \
         it with the previous HillVacuum version and then open the generated file with this \
         version.\nI apologize for the inconvenience.";

    //=======================================================================//
    // TRAITS
    //
    //=======================================================================//

    /// A trait to determine wherever an entity fits within the map's bounds.
    pub(in crate::map) trait OutOfBounds
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

    //=======================================================================//

    pub(in crate::map) trait BoundToMap
    {
        #[must_use]
        fn bound(&self) -> Self;
    }

    impl BoundToMap for f32
    {
        #[inline]
        fn bound(&self) -> Self { self.clamp(-MAP_HALF_SIZE, MAP_HALF_SIZE) }
    }

    impl BoundToMap for Vec2
    {
        #[inline]
        fn bound(&self) -> Self { Self::new(self.x.bound(), self.y.bound()) }
    }

    //=======================================================================//

    pub(in crate::map) trait Viewer
    {
        type Item;

        fn from_viewer(value: Self::Item) -> Self;

        fn to_viewer(self) -> Self::Item;
    }

    //=======================================================================//
    // ENUMS
    //
    //=======================================================================//

    impl GridSettings
    {
        #[inline]
        pub fn set_skew(&mut self, value: i8)
        {
            let value = value.clamp(*Grid::SKEW_RANGE.start(), *Grid::SKEW_RANGE.end());

            match self
            {
                Self::None =>
                {
                    if value != 0
                    {
                        *self = Self::Skew(value);
                    }
                },
                Self::Skew(skew) =>
                {
                    if value == 0
                    {
                        *self = Self::None;
                    }
                    else
                    {
                        *skew = value;
                    }
                },
                Self::Rotate(angle) =>
                {
                    if value == 0
                    {
                        return;
                    }

                    *self = Self::Isometric {
                        skew:  value,
                        angle: *angle
                    }
                },
                Self::Isometric { skew, angle } =>
                {
                    if value == 0
                    {
                        *self = Self::Rotate(*angle);
                    }
                    else
                    {
                        *skew = value;
                    }
                }
            };
        }

        #[inline]
        pub fn set_angle(&mut self, value: i16)
        {
            let value = value.clamp(*Grid::ANGLE_RANGE.start(), *Grid::ANGLE_RANGE.end());

            match self
            {
                Self::None =>
                {
                    if value != 0
                    {
                        *self = Self::Rotate(value);
                    }
                },
                Self::Skew(skew) =>
                {
                    if value == 0
                    {
                        return;
                    }

                    *self = Self::Isometric {
                        skew:  *skew,
                        angle: value
                    }
                },
                Self::Rotate(angle) =>
                {
                    if value == 0
                    {
                        *self = Self::None;
                    }
                    else
                    {
                        *angle = value;
                    }
                },
                Self::Isometric { skew, angle } =>
                {
                    if value == 0
                    {
                        *self = Self::Skew(*skew);
                    }
                    else
                    {
                        *angle = value;
                    }
                }
            };
        }
    }

    //=======================================================================//
    // STRUCTS
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
            .add_systems(PreUpdate, process_egui_inputs.after(EguiSet::ProcessInput).before(EguiSet::BeginPass))
            // Init resources
            .insert_non_send_resource(unsafe { Editor::placeholder() })
            .init_state::<TextureLoadingProgress>()
            .insert_resource(ClearColor(Color::Clear.default_bevy_color()))
            .insert_resource(WinitSettings::default())
            .init_non_send_resource::<TextureLoader>()
            // Setup
            .add_systems(PostStartup, initialize)
            // Texture loading
            .add_systems(
                Update,
                (load_textures, texture_loading_ui)
                    .chain()
                    .run_if(not(in_state(TextureLoadingProgress::Complete)))
            )
            .add_systems(
                OnEnter(TextureLoadingProgress::Complete),
                store_loaded_textures
            )
            // Handle editor
            .add_systems(First, alt_f4_quit)
            .add_systems(
                Update,
                (update,draw)
                    .chain()
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
        #[inline]
        #[must_use]
        fn prop_camera<T: Default + Component>(
            images: &mut Assets<Image>,
            pos: Vec2
        ) -> (Camera2dBundle, T)
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
                T::default()
            )
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
            let plus_one = i + 1;
            let start = MAP_SIZE * (plus_one as f32);
            y = -start;

            for _ in 0..=(plus_one * 2)
            {
                commands.spawn(prop_camera::<PropCamera>(&mut images, Vec2::new(-start, y)));
                commands.spawn(prop_camera::<PropCamera>(&mut images, Vec2::new(start, y)));

                y += MAP_SIZE;
                prop_cameras_amount += 2;
            }

            let mut x = -start + MAP_SIZE;

            for _ in 0..=(i * 2)
            {
                commands.spawn(prop_camera::<PropCamera>(&mut images, Vec2::new(x, start)));
                commands.spawn(prop_camera::<PropCamera>(&mut images, Vec2::new(x, -start)));

                x += MAP_SIZE;
                prop_cameras_amount += 2;
            }
        }

        assert!(prop_cameras_amount == PROP_CAMERAS_AMOUNT, "Incoherent prop cameras.");

        commands
            .spawn(prop_camera::<PaintToolPropCamera>(&mut images, Vec2::new(0f32, y + MAP_SIZE)));

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

    /// Processes the `egui` inputs for some custom behaviors.
    #[allow(clippy::needless_pass_by_value)]
    #[inline]
    fn process_egui_inputs(mut input: Query<&mut EguiInput>, editor: NonSend<Editor>)
    {
        let ui_focus = editor.is_ui_focused();
        let events = &mut return_if_err!(input.get_single_mut()).0.events;
        let mut iter = events.iter_mut().enumerate();
        let mut index = None;
        let mut checked = false;
        let mut add_escape = false;

        for (i, ev) in &mut iter
        {
            match continue_if_no_match!(
                ev,
                egui::Event::Key {
                    key,
                    pressed: true,
                    ..
                },
                key
            )
            {
                egui::Key::Tab => index = matches!(ui_focus, UiFocus::None).then_some(i),
                egui::Key::F4 => add_escape = matches!(ui_focus, UiFocus::Window),
                _ => continue
            };

            if checked
            {
                break;
            }

            checked = true;
        }

        // If F4 is pressed to close an egui window add an artificial escape press to surrender
        // focus of the text editor being used (if any) before the window is closed.
        if add_escape
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
        mut config: ResMut<Config>,
        mut texture_loader: NonSendMut<TextureLoader>,
        mut hardcoded_things: ResMut<HardcodedThings>,
        mut brush_properties: ResMut<BrushUserProperties>,
        mut thing_properties: ResMut<ThingUserProperties>,
        state: Res<State<EditorState>>,
        mut next_state: ResMut<NextState<EditorState>>
    )
    {
        if *state.get() == EditorState::SplashScreen
        {
            if !config.warning_displayed
            {
                warning_message("Please, if you find any bugs consider reporting them at\nhttps://github.com/IvoryDuke/HillVacuum");
                config.warning_displayed = true;
            }

            *editor = Editor::new(
                window.single_mut().as_mut(),
                &mut prop_cameras,
                &asset_server,
                &mut images,
                &mut meshes,
                &mut materials,
                &mut user_textures,
                &mut config,
                &mut texture_loader,
                &mut hardcoded_things,
                &mut brush_properties,
                &mut thing_properties
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

    /// Handle `Alt+F4` shutdown.
    #[inline]
    fn alt_f4_quit(
        mut window: Query<&mut Window, With<PrimaryWindow>>,
        mut close_events: ResMut<Events<WindowCloseRequested>>,
        mut config: ResMut<Config>,
        mut editor: NonSendMut<Editor>,
        mut next_editor_state: ResMut<NextState<EditorState>>
    )
    {
        if close_events.is_empty()
        {
            return;
        }

        let mut window = return_if_err!(window.get_single_mut());

        if !editor.quit(&mut window, &mut config, &mut next_editor_state)
        {
            close_events.clear();
        }
    }

    //=======================================================================//

    /// Updates the editor state.
    #[allow(clippy::needless_pass_by_value)]
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn update(
        mut window: Query<&mut Window, With<PrimaryWindow>>,
        mut images: ResMut<Assets<Image>>,
        mut materials: ResMut<Assets<ColorMaterial>>,
        mut camera: MainCameraQueryMut,
        mut paint_tool_camera: PaintToolCameraQueryMut,
        mut prop_cameras: PropCamerasMut,
        mouse_buttons: Res<ButtonInput<MouseButton>>,
        mut mouse_wheel: EventReader<MouseWheel>,
        mut key_inputs: ResMut<ButtonInput<KeyCode>>,
        time: Res<Time>,
        mut egui_context: Query<&'static mut EguiContext, With<PrimaryWindow>>,
        mut user_textures: ResMut<EguiUserTextures>,
        mut editor: NonSendMut<Editor>,
        mut config: ResMut<Config>,
        mut next_editor_state: ResMut<NextState<EditorState>>,
        mut next_tex_load: ResMut<NextState<TextureLoadingProgress>>
    )
    {
        let mut window = return_if_err!(window.get_single_mut());
        let mut egui_context = egui_context.single_mut();
        let egui_context = egui_context.get_mut();
        let mut camera = camera.single_mut();

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

        let mut paint_tool_camera = paint_tool_camera.single_mut();

        editor.update_active_tool(
            &window,
            &mut images,
            &mut camera,
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
        mut egui_context: Query<&'static mut EguiContext, With<PrimaryWindow>>,
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
            egui_context.single_mut().get_mut(),
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
        mut texture_loader: NonSendMut<TextureLoader>,
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
        texture_loader: NonSend<TextureLoader>
    )
    {
        texture_loader.ui(window.single(), egui_context.ctx_mut());
    }
}

#[cfg(feature = "ui")]
pub(crate) use ui_mod::*;
