pub(in crate::map) mod prop;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{
    fs::File,
    io::{BufReader, BufWriter}
};

use arrayvec::ArrayVec;
use bevy::{
    asset::Assets,
    ecs::{
        change_detection::Mut,
        component::Component,
        entity::Entity,
        query::{With, Without},
        system::Query
    },
    image::Image,
    render::camera::{Camera, RenderTarget},
    transform::components::Transform
};
use bevy_egui::{
    egui::{
        self,
        text::{CCursor, CCursorRange, CursorRange},
        TextBuffer
    },
    EguiUserTextures
};
use glam::{UVec2, Vec2};
use hill_vacuum_shared::return_if_none;
use prop::{Prop, PropScreenshotTimer, PropViewer};
use serde::{Deserialize, Serialize};

use super::{
    edits_history::EditsHistory,
    grid::Grid,
    inputs_presses::InputsPresses,
    manager::EntitiesManager,
    ui::singleline_textedit
};
use crate::{
    map::{
        brush::{BrushData, BrushDataViewer},
        camera::scale_viewport,
        drawer::{color::Color, drawing_resources::DrawingResources, TextureSize},
        editor::DrawBundle,
        path::Path,
        thing::{catalog::ThingsCatalog, ThingInstanceData, ThingInstanceDataViewer},
        MapHeader,
        OutOfBounds,
        Viewer,
        PROP_CAMERAS_AMOUNT
    },
    utils::{
        hull::Hull,
        identifiers::{EntityId, Id},
        misc::ReplaceValue
    }
};

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

/// The size of the image of the prop screenshot.
pub(in crate::map) const PROP_SCREENSHOT_SIZE: UVec2 = UVec2::new(196, 196);

//=======================================================================//
// MACROS
//
//=======================================================================//

/// Returns a prop screenshot camera if `camera_id` contains a value, the paint tool camera if it
/// does not.
macro_rules! draw_camera {
    ($bundle:ident, $camera_id:ident) => {
        match $camera_id
        {
            Some(id) => $bundle.prop_cameras.get(id).unwrap().1,
            None => $bundle.paint_tool_camera
        }
    };
}

use draw_camera;

//=======================================================================//
// TRAITS
//
//=======================================================================//

/// The trait an entity must satisfy in order to be stored
/// in the clipboard.
pub(in crate::map) trait CopyToClipboard
{
    /// Returns a representation of `self` as [`ClipboardData`].
    fn copy_to_clipboard(&self) -> ClipboardData;
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[must_use]
#[derive(Serialize, Deserialize)]
pub(in crate::map) enum ClipboardDataViewer
{
    /// A brush.
    Brush(BrushDataViewer, Id),
    /// A [`ThingInstance`].
    Thing(ThingInstanceDataViewer, Id)
}

//=======================================================================//

/// The data that can be stored in the Clipboard.
#[must_use]
#[derive(Clone)]
pub(in crate::map) enum ClipboardData
{
    /// A brush.
    Brush(BrushData, Id),
    /// A [`ThingInstance`].
    Thing(ThingInstanceData, Id)
}

impl Viewer for ClipboardData
{
    type Item = ClipboardDataViewer;

    #[inline]
    fn from_viewer(value: Self::Item) -> Self
    {
        match value
        {
            Self::Item::Brush(data, id) => Self::Brush(BrushData::from_viewer(data), id),
            Self::Item::Thing(data, id) => Self::Thing(ThingInstanceData::from_viewer(data), id)
        }
    }

    #[inline]
    fn to_viewer(self) -> Self::Item
    {
        match self
        {
            Self::Brush(data, id) => Self::Item::Brush(data.to_viewer(), id),
            Self::Thing(data, id) => Self::Item::Thing(data.to_viewer(), id)
        }
    }
}

impl CopyToClipboard for ClipboardData
{
    #[inline]
    fn copy_to_clipboard(&self) -> ClipboardData { todo!() }
}

impl EntityId for ClipboardData
{
    #[inline]
    fn id(&self) -> Id { *self.id_as_ref() }

    #[inline]
    fn id_as_ref(&self) -> &Id
    {
        match self
        {
            ClipboardData::Brush(_, id) | ClipboardData::Thing(_, id) => id
        }
    }
}

impl ClipboardData
{
    /// Whether `self` is out of bounds if moved by the amount `delta`.
    #[inline]
    #[must_use]
    fn out_of_bounds_moved(
        &self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid,
        delta: Vec2
    ) -> bool
    {
        (self.hull(drawing_resources, things_catalog, grid) + delta).out_of_bounds()
    }

    #[inline]
    fn hull<T: TextureSize>(
        &self,
        resources: &T,
        things_catalog: &ThingsCatalog,
        grid: &Grid
    ) -> Hull
    {
        match self
        {
            ClipboardData::Brush(data, _) => data.hull(resources, grid),
            ClipboardData::Thing(data, _) => data.hull(things_catalog)
        }
    }

    /// Draws the [`ClipboardData`] at its position moved by `delta`
    #[inline]
    fn draw(&self, bundle: &mut DrawBundle, delta: Vec2)
    {
        match self
        {
            ClipboardData::Brush(data, _) =>
            {
                data.draw_prop(bundle.drawer, Color::NonSelectedEntity, delta);
            },
            ClipboardData::Thing(data, _) =>
            {
                data.draw_prop(bundle.drawer, bundle.things_catalog, delta);
            }
        };
    }
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// Marker for the cameras used to create the image screenshot of the imported props.
#[derive(Component)]
pub(in crate::map) struct PropCamera;

impl Default for PropCamera
{
    #[inline]
    fn default() -> Self { Self {} }
}

//=======================================================================//

/// Marker for the camera used to create the image screenshot of the paint tool created prop.
#[derive(Component)]
pub(in crate::map) struct PaintToolPropCamera;

impl Default for PaintToolPropCamera
{
    #[inline]
    fn default() -> Self { Self {} }
}

//=======================================================================//

/// A query to the cameras used to create [`Prop`] screenshots.
pub(in crate::map) type PropCameras<'world, 'state, 'a> = Query<
    'world,
    'state,
    (&'a Camera, &'a Transform),
    (With<PropCamera>, Without<PaintToolPropCamera>)
>;

//=======================================================================//

/// A query to the mutable cameras used to create [`Prop`] screenshots.
pub(in crate::map) type PropCamerasMut<'world, 'state, 'a> = Query<
    'world,
    'state,
    (Entity, &'a mut Camera, &'a mut Transform),
    (With<PropCamera>, Without<PaintToolPropCamera>)
>;

//=======================================================================//

#[derive(Clone, Copy)]
pub(in crate::map) struct UiProp
{
    pub index:  usize,
    pub tex_id: egui::TextureId
}

//=======================================================================//

/// A clipboard where data to be pasted around the map is stored.
pub(in crate::map) struct Clipboard
{
    /// The copy-paste stored entities.
    copy_paste: Prop,
    duplicate: Prop,
    /// The quick prop created with the paint tool.
    quick_prop: Prop,
    /// The slotted [`Prop`]s.
    props: Vec<Prop>,
    /// The index of the [`Prop`] selected in the UI, if any.
    selected_prop: Option<usize>,
    /// The text copied from the UI fields.
    ui_text: String,
    /// The copied platform path, if any.
    platform_path: Option<Path>,
    /// Whether the stored [`Prop`]s were edited.
    props_changed: bool,
    /// The [`Prop`]s which have an assigned camera to take their screenshot.
    props_with_assigned_camera: ArrayVec<(PropScreenshotTimer, usize), PROP_CAMERAS_AMOUNT>,
    /// The [`Prop`]s with no assigned camera to take their screenshot.
    props_with_no_camera: Vec<usize>,
    /// The frames that must pass before the [`Prop`] screenshots can be taken.
    props_import_wait_frames: usize,
    /// The function used to run the frame update.
    update_func: fn(
        &mut Self,
        &mut Assets<Image>,
        &mut PropCamerasMut,
        &mut EguiUserTextures,
        &DrawingResources,
        &ThingsCatalog,
        &Grid
    )
}

impl Clipboard
{
    /// The frames that must pass before the [`Prop`] screenshots can be taken.
    const IMPORTS_WAIT_FRAMES: usize = 2;

    //==============================================================
    // New

    /// Returns an empty clipboard.
    #[inline]
    #[must_use]
    pub(in crate::map::editor) fn new() -> Self
    {
        Self {
            copy_paste: Prop::default(),
            duplicate: Prop::default(),
            quick_prop: Prop::default(),
            props: Vec::new(),
            selected_prop: None,
            ui_text: String::new(),
            platform_path: None,
            props_changed: false,
            props_with_assigned_camera: ArrayVec::new(),
            props_with_no_camera: Vec::new(),
            props_import_wait_frames: Self::IMPORTS_WAIT_FRAMES,
            update_func: Self::delay_update
        }
    }

    /// Creates a new [`Clipboard`] from the data stored in `file`.
    #[inline]
    pub(in crate::map::editor::state) fn from_file<T: TextureSize>(
        images: &mut Assets<Image>,
        prop_cameras: &mut PropCamerasMut,
        user_textures: &mut EguiUserTextures,
        resources: &T,
        catalog: &ThingsCatalog,
        grid: &Grid,
        header: &MapHeader,
        file: &mut BufReader<File>
    ) -> Result<Self, &'static str>
    {
        let mut clip = Self {
            copy_paste: Prop::default(),
            duplicate: Prop::default(),
            quick_prop: Prop::default(),
            props: Vec::new(),
            selected_prop: None,
            ui_text: String::new(),
            platform_path: None,
            props_changed: false,
            props_with_assigned_camera: ArrayVec::new(),
            props_with_no_camera: Vec::new(),
            props_import_wait_frames: Self::IMPORTS_WAIT_FRAMES,
            update_func: Self::delay_update
        };

        match clip.import_props(
            images,
            prop_cameras,
            user_textures,
            resources,
            catalog,
            grid,
            header.props,
            file
        )
        {
            Ok(()) => Ok(clip),
            Err(err) => Err(err)
        }
    }

    /// Import the [`Prop`]s in `file`.
    #[inline]
    pub(in crate::map::editor::state) fn import_props<T: TextureSize>(
        &mut self,
        images: &mut Assets<Image>,
        prop_cameras: &mut PropCamerasMut,
        user_textures: &mut EguiUserTextures,
        resources: &T,
        things_catalog: &ThingsCatalog,
        grid: &Grid,
        props_amount: usize,
        file: &mut BufReader<File>
    ) -> Result<(), &'static str>
    {
        let mut props = Vec::new();

        for _ in 0..props_amount
        {
            let mut prop = Prop::from_viewer(
                ciborium::from_reader::<PropViewer, _>(&mut *file)
                    .map_err(|_| "Error loading props")?
            );
            _ = prop.reload_things(resources, things_catalog, grid);
            props.push(prop);
        }

        self.props_changed = true;

        let mut prop_cameras = prop_cameras.iter_mut().filter(|camera| !camera.1.is_active);

        for prop in props
        {
            let index = self.props.len();
            self.props.push(prop);
            self.queue_prop_screenshot(
                images,
                user_textures,
                prop_cameras.next(),
                resources,
                things_catalog,
                grid,
                index
            );
        }

        Ok(())
    }

    /// Queues a [`Prop`] screenshot.
    #[inline]
    fn queue_prop_screenshot<T: TextureSize>(
        &mut self,
        images: &mut Assets<Image>,
        user_textures: &mut EguiUserTextures,
        camera: Option<(Entity, Mut<Camera>, Mut<Transform>)>,
        resources: &T,
        things_catalog: &ThingsCatalog,
        grid: &Grid,
        index: usize
    )
    {
        if self.props_with_assigned_camera.is_full()
        {
            assert!(
                camera.is_none(),
                "Assigned cameras vector is full but there are still available cameras."
            );
            self.props_with_no_camera.push(index);
            return;
        }

        let mut camera = camera.unwrap();

        Self::assign_camera_to_prop(
            images,
            &mut (&mut camera.1, &mut camera.2),
            user_textures,
            resources,
            things_catalog,
            grid,
            &mut self.props[index]
        );
        self.props_with_assigned_camera
            .push((PropScreenshotTimer::new(camera.0.into()), index));
    }

    #[inline]
    pub(in crate::map::editor::state) fn queue_all_props_screenshots(
        &mut self,
        images: &mut Assets<Image>,
        prop_cameras: &mut PropCamerasMut,
        user_textures: &mut EguiUserTextures,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid
    )
    {
        let mut prop_cameras = prop_cameras.iter_mut().filter(|camera| !camera.1.is_active);

        for i in 0..self.props.len()
        {
            self.queue_prop_screenshot(
                images,
                user_textures,
                prop_cameras.next(),
                drawing_resources,
                things_catalog,
                grid,
                i
            );
        }
    }

    //==============================================================
    // Info

    /// Whether `self` was edited.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) const fn props_changed(&self) -> bool { self.props_changed }

    /// Returns true if there is data to be pasted.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn has_copy_data(&self) -> bool { self.copy_paste.has_data() }

    /// The amount of slotted props stored.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn props_amount(&self) -> usize { self.props.len() }

    /// The index of the selected [`Prop`], if any.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) const fn selected_prop_index(&self) -> Option<usize>
    {
        self.selected_prop
    }

    /// Whether the quick prop stored contains entities.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn has_quick_prop(&self) -> bool
    {
        self.quick_prop.has_data()
    }

    /// Whether there are no [`Prop`]s stored.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn no_props(&self) -> bool
    {
        self.props.is_empty() && !self.has_quick_prop()
    }

    //==============================================================
    // Update

    /// Delays the update of the [`Clipboard`]. During the first few frames it is not possible to
    /// take a [`Prop`] screenshot.
    #[inline]
    fn delay_update(
        &mut self,
        _: &mut Assets<Image>,
        _: &mut PropCamerasMut,
        _: &mut EguiUserTextures,
        _: &DrawingResources,
        _: &ThingsCatalog,
        _: &Grid
    )
    {
        if self.props_import_wait_frames != 0
        {
            self.props_import_wait_frames -= 1;
            return;
        }

        self.update_func = Self::regular_update;
    }

    /// An update to take the screenshots of the queued [`Prop`]s.
    #[inline]
    fn regular_update(
        &mut self,
        images: &mut Assets<Image>,
        prop_cameras: &mut PropCamerasMut,
        user_textures: &mut EguiUserTextures,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid
    )
    {
        let mut i = 0;

        while i < self.props_with_assigned_camera.len()
        {
            if self.props_with_assigned_camera[i].0.update(prop_cameras)
            {
                _ = self.props_with_assigned_camera.swap_remove(i);
                continue;
            }

            i += 1;
        }

        for mut camera in prop_cameras.iter_mut().take(
            self.props_with_no_camera
                .len()
                .min(PROP_CAMERAS_AMOUNT - self.props_with_assigned_camera.len())
        )
        {
            let index = self.props_with_no_camera.pop().unwrap();

            Self::assign_camera_to_prop(
                images,
                &mut (&mut camera.1, &mut camera.2),
                user_textures,
                drawing_resources,
                things_catalog,
                grid,
                &mut self.props[index]
            );

            self.props_with_assigned_camera
                .push((PropScreenshotTimer::new(camera.0.into()), index));
        }
    }

    /// Updates `self`.
    #[inline]
    pub(in crate::map::editor::state) fn update(
        &mut self,
        images: &mut Assets<Image>,
        prop_cameras: &mut PropCamerasMut,
        user_textures: &mut EguiUserTextures,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid
    )
    {
        (self.update_func)(
            self,
            images,
            prop_cameras,
            user_textures,
            drawing_resources,
            things_catalog,
            grid
        );
    }

    /// Assigns a camera to a [`Prop`] to take its screenshot.
    #[allow(clippy::cast_precision_loss)]
    #[inline]
    pub(in crate::map::editor::state) fn assign_camera_to_prop<T: TextureSize>(
        images: &mut Assets<Image>,
        prop_camera: &mut (&mut Camera, &mut Transform),
        user_textures: &mut EguiUserTextures,
        resources: &T,
        things_catalog: &ThingsCatalog,
        grid: &Grid,
        prop: &mut Prop
    )
    {
        assert!(
            !prop_camera.0.is_active.replace_value(true),
            "Tried to assign a prop screenshot to an active camera."
        );

        scale_viewport(
            prop_camera.1,
            (PROP_SCREENSHOT_SIZE.x as f32, PROP_SCREENSHOT_SIZE.y as f32),
            &prop
                .hull(resources, things_catalog, grid)
                .transformed(|vx| grid.transform_point(vx)),
            32f32
        );

        let image = images.add(Prop::empty_image());
        prop_camera.0.target = RenderTarget::Image(image.clone_weak());
        prop.screenshot = user_textures.add_image(image).into();
    }

    /// Writes the serialized [`Prop`]s in `writer`.
    #[inline]
    pub(in crate::map::editor::state) fn export_props(
        &self,
        writer: &mut BufWriter<&mut Vec<u8>>
    ) -> Result<(), &'static str>
    {
        for prop in self.props.iter().map(|prop| prop.clone().to_viewer())
        {
            ciborium::ser::into_writer(&prop, &mut *writer).map_err(|_| "Error saving prop")?;
        }

        Ok(())
    }

    /// Queues the screenshots of the [`Prop`]s that must be retaken after a things reload.
    #[inline]
    pub(in crate::map::editor::state) fn reload_things(
        &mut self,
        images: &mut Assets<Image>,
        user_textures: &mut EguiUserTextures,
        prop_cameras: &mut PropCamerasMut,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid
    )
    {
        let mut prop_cameras = prop_cameras.iter_mut();

        for i in 0..self.props.len()
        {
            let prop = &mut self.props[i];

            if prop.reload_things(drawing_resources, things_catalog, grid)
            {
                self.queue_prop_screenshot(
                    images,
                    user_textures,
                    prop_cameras.next(),
                    drawing_resources,
                    things_catalog,
                    grid,
                    i
                );
            }
        }
    }

    /// Queues the screenshots of the [`Prop`]s that must be retaken after a texture reload.
    #[inline]
    pub(in crate::map::editor::state) fn finish_textures_reload(
        &mut self,
        images: &mut Assets<Image>,
        user_textures: &mut EguiUserTextures,
        prop_cameras: &mut PropCamerasMut,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid
    )
    {
        let mut prop_cameras = prop_cameras.iter_mut();

        for i in 0..self.props.len()
        {
            let prop = &mut self.props[i];

            if prop.reload_textures(drawing_resources, things_catalog, grid)
            {
                self.queue_prop_screenshot(
                    images,
                    user_textures,
                    prop_cameras.next(),
                    drawing_resources,
                    things_catalog,
                    grid,
                    i
                );
            }
        }
    }

    /// Resets the state changed flag.
    #[inline]
    pub(in crate::map::editor::state) fn reset_props_changed(&mut self)
    {
        self.props_changed = false;
    }

    /// Sets the index of the selected slotted [`Prop`].
    /// # Panics
    /// Panics if `slot` is equal or higher than the length of the slotted [`Prop`]s.
    #[inline]
    pub(in crate::map::editor::state) fn set_selected_prop_index(&mut self, slot: usize)
    {
        assert!(
            slot < self.props.len(),
            "Slot {slot} is out of bounds, length of slotted props is {}",
            self.props.len()
        );
        self.selected_prop = Some(slot);
    }

    //==============================================================
    // Entities

    /// Stores the entities in `iter` as a copy-paste [`Prop`].
    #[inline]
    pub(in crate::map::editor::state) fn copy<'a, E>(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid,
        iter: impl Iterator<Item = E>
    ) where
        E: CopyToClipboard + 'a
    {
        self.copy_paste.fill(
            drawing_resources,
            things_catalog,
            grid,
            iter.map(|e| e.copy_to_clipboard())
        );
    }

    /// Pastes the copied entities.
    #[inline]
    pub(in crate::map::editor::state) fn paste(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        cursor_pos: Vec2
    )
    {
        self.copy_paste.spawn_copy(
            drawing_resources,
            things_catalog,
            manager,
            edits_history,
            grid,
            cursor_pos
        );
    }

    #[inline]
    pub(in crate::map::editor::state) fn duplicate(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        delta: Vec2
    )
    {
        self.duplicate.fill(
            drawing_resources,
            things_catalog,
            grid,
            manager.selected_entities().map(|e| e.copy_to_clipboard())
        );

        manager.deselect_selected_entities(edits_history);

        self.duplicate.spawn(
            drawing_resources,
            things_catalog,
            manager,
            edits_history,
            grid,
            delta
        );
    }

    /// Stores `prop` as the quick [`Prop`].
    #[inline]
    pub(in crate::map::editor::state) fn create_quick_prop(&mut self, prop: Prop)
    {
        self.quick_prop = prop;
    }

    /// Inserts a slotted [`Prop`] at the specified `slot`.
    #[inline]
    pub(in crate::map::editor::state) fn insert_prop(&mut self, prop: Prop, slot: usize)
    {
        assert!(prop.screenshot.is_some(), "Tried to insert prop without a screenshot.");

        self.props_changed = true;

        if slot >= self.props.len()
        {
            self.props.push(prop);
            return;
        }

        self.props[slot] = prop;

        if let Some(i) = self
            .props_with_assigned_camera
            .iter()
            .position(|(_, idx)| *idx == slot)
        {
            _ = self.props_with_assigned_camera.remove(i);
        }
        else if let Some(i) = self.props_with_no_camera.iter().position(|idx| *idx == slot)
        {
            self.props_with_no_camera.remove(i);
        }
    }

    /// Deletes the [`Prop`] stored at the selected index, if any.
    #[inline]
    pub(in crate::map::editor::state) fn delete_selected_prop(
        &mut self,
        prop_cameras: &mut PropCamerasMut
    )
    {
        let selected_prop = return_if_none!(self.selected_prop);
        self.props_changed = true;
        let no_screenshot = self.props.remove(selected_prop).screenshot.is_none();

        self.selected_prop = if self.props.is_empty()
        {
            None
        }
        else
        {
            selected_prop.min(self.props.len() - 1).into()
        };

        if !no_screenshot
        {
            return;
        }

        if let Some((i, (timer, _))) = self
            .props_with_assigned_camera
            .iter()
            .copied()
            .enumerate()
            .find(|(_, (_, idx))| *idx == selected_prop)
        {
            prop_cameras.get_mut(timer.id()).unwrap().1.is_active = false;

            _ = self.props_with_assigned_camera.remove(i);

            for (_, idx) in self.props_with_assigned_camera.iter_mut().skip(i)
            {
                *idx -= 1;
            }

            let i = return_if_none!(self
                .props_with_no_camera
                .iter()
                .position(|idx| *idx > selected_prop));

            for idx in self.props_with_no_camera.iter_mut().skip(i)
            {
                *idx -= 1;
            }

            return;
        }

        let i =
            return_if_none!(self.props_with_no_camera.iter().position(|idx| *idx == selected_prop));

        self.props_with_no_camera.remove(i);

        for idx in self.props_with_no_camera.iter_mut().skip(i)
        {
            *idx -= 1;
        }

        let i = return_if_none!(self
            .props_with_assigned_camera
            .iter()
            .position(|(_, idx)| *idx > selected_prop));

        for (_, idx) in self.props_with_assigned_camera.iter_mut().skip(i)
        {
            *idx -= 1;
        }
    }

    /// Spawns the quick [`Prop`] on the map.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn spawn_quick_prop(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        cursor_pos: Vec2
    ) -> bool
    {
        if self.has_quick_prop()
        {
            self.quick_prop.paint_copy(
                drawing_resources,
                things_catalog,
                manager,
                edits_history,
                grid,
                cursor_pos
            );

            return true;
        }

        false
    }

    /// Spawns the selected [`Prop`] on the map.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn spawn_selected_prop(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        cursor_pos: Vec2
    ) -> bool
    {
        self.props[return_if_none!(self.selected_prop, false)].paint_copy(
            drawing_resources,
            things_catalog,
            manager,
            edits_history,
            grid,
            cursor_pos
        );

        true
    }

    //==============================================================
    // UI text

    /// Applies the requested copy/paste/cut operation, if any, on the selected buffer in the
    /// selected range of characters.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn copy_paste_text_editor(
        &mut self,
        inputs: &InputsPresses,
        ui: &mut egui::Ui,
        buffer: &mut String,
        width: f32
    ) -> egui::Response
    {
        let mut output = singleline_textedit(buffer, width).show(ui);

        if !output.response.has_focus()
        {
            return output.response;
        }

        let range = if inputs.copy_just_pressed()
        {
            self.copy_ui_text(buffer, output.cursor_range.as_ref());
            None
        }
        else if inputs.paste_just_pressed()
        {
            self.paste_ui_text(buffer, output.cursor_range.as_ref())
        }
        else if inputs.cut_just_pressed()
        {
            self.cut_ui_text(buffer, output.cursor_range.as_ref())
        }
        else
        {
            None
        };

        if range.is_some()
        {
            output.state.cursor.set_char_range(range);
            output.state.store(ui.ctx(), output.response.id);
        }

        output.response
    }

    /// Copies the selected UI text.
    #[inline]
    fn copy_ui_text(&mut self, buffer: &String, cursor_range: Option<&CursorRange>)
    {
        let cursor_range = return_if_none!(cursor_range);

        if cursor_range.is_empty()
        {
            return;
        }

        self.ui_text.clear();
        self.ui_text
            .push_str(buffer.char_range(cursor_range.as_sorted_char_range()));
    }

    /// Pastes the copied UI text.
    #[inline]
    #[must_use]
    fn paste_ui_text(
        &self,
        buffer: &mut String,
        cursor_range: Option<&CursorRange>
    ) -> Option<CCursorRange>
    {
        if self.ui_text.is_empty()
        {
            return None;
        }

        let cursor_range = return_if_none!(cursor_range, None);
        let range = cursor_range.as_sorted_char_range();

        buffer.delete_char_range(range.clone());
        buffer.insert_str(range.start, &self.ui_text);

        CCursorRange::one(CCursor::new(range.end + self.ui_text.len())).into()
    }

    /// Cuts the selected UI text.
    #[inline]
    #[must_use]
    fn cut_ui_text(
        &mut self,
        buffer: &mut String,
        cursor_range: Option<&CursorRange>
    ) -> Option<CCursorRange>
    {
        let cursor_range = return_if_none!(cursor_range, None);

        if cursor_range.is_empty()
        {
            return None;
        }

        let range = cursor_range.as_sorted_char_range();

        self.ui_text.clear();
        self.ui_text.push_str(buffer.char_range(range.clone()));

        let new_range = CCursorRange::one(CCursor::new(range.start)).into();
        buffer.delete_char_range(range);
        new_range
    }

    //==============================================================
    // Platform path

    /// Copies the [`Path`] of the brush with [`Id`] `identifier`.
    #[inline]
    pub(in crate::map::editor::state) fn copy_platform_path(
        &mut self,
        manager: &mut EntitiesManager,
        identifier: Id
    )
    {
        let mut path = manager.moving(identifier).path().unwrap().clone();
        path.deselect_nodes_no_indexes();
        self.platform_path = path.into();
    }

    /// Pastes the copied [`Path`] in the brush with [`Id`] `identifier`.
    #[inline]
    pub(in crate::map::editor::state) fn paste_platform_path(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        identifier: Id
    )
    {
        let path = return_if_none!(&self.platform_path);

        if let Some(p) = manager.moving(identifier).path()
        {
            if *p == *path
            {
                return;
            }

            manager.replace_selected_path(
                drawing_resources,
                things_catalog,
                edits_history,
                grid,
                identifier,
                path.clone()
            );
            return;
        }

        manager.create_path(
            drawing_resources,
            things_catalog,
            edits_history,
            grid,
            identifier,
            path.clone()
        );
    }

    /// Cuts the [`Path`] of the brush with [`Id`] `identifier`.
    #[inline]
    pub(in crate::map::editor::state) fn cut_platform_path(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        identifier: Id
    )
    {
        self.copy_platform_path(manager, identifier);
        manager.remove_selected_path(
            drawing_resources,
            things_catalog,
            edits_history,
            grid,
            identifier
        );
    }

    //==============================================================
    // Iterators

    /// Returns a [`Chunks`] iterator to the slotted [`Prop`]s with size `chunk_size`.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn ui_iter(
        &self
    ) -> impl ExactSizeIterator<Item = UiProp> + '_
    {
        self.props.iter().enumerate().map(|(index, prop)| {
            UiProp {
                index,
                tex_id: prop.screenshot()
            }
        })
    }

    /// Draws the [`Prop`]s to photograph for the preview.
    #[inline]
    pub(in crate::map::editor::state) fn draw_props_to_photograph(&self, bundle: &mut DrawBundle)
    {
        for (timer, idx) in &self.props_with_assigned_camera
        {
            self.props[*idx].draw(bundle, (timer.id()).into());
        }
    }
}
