//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{
    cmp::Ordering,
    fs::File,
    io::{BufReader, BufWriter},
    iter::Rev,
    ops::Range
};

use arrayvec::ArrayVec;
use bevy::{
    asset::Assets,
    ecs::{
        component::Component,
        query::{With, Without},
        system::Query
    },
    math::UVec2,
    prelude::Vec2,
    render::{
        camera::{Camera, RenderTarget},
        render_resource::{
            Extent3d,
            TextureDescriptor,
            TextureDimension,
            TextureFormat,
            TextureUsages
        },
        texture::Image
    },
    transform::components::Transform
};
use bevy_egui::{
    egui::{
        self,
        text::{CCursor, CCursorRange, CursorRange},
        text_edit::TextEditOutput,
        TextBuffer
    },
    EguiUserTextures
};
use hill_vacuum_shared::{
    continue_if_no_match,
    continue_if_none,
    match_or_panic,
    return_if_none,
    NextValue
};
use serde::{Deserialize, Serialize};

use super::{editor_state::InputsPresses, edits_history::EditsHistory, manager::EntitiesManager};
use crate::{
    error_message,
    map::{
        brush::BrushData,
        camera::scale_viewport,
        drawer::{color::Color, drawing_resources::DrawingResources},
        editor::{DrawBundle, StateUpdateBundle, ToolUpdateBundle},
        hv_vec,
        thing::{catalog::ThingsCatalog, ThingInstanceData},
        HvVec,
        MapHeader,
        OutOfBounds,
        PROP_CAMERAS_AMOUNT
    },
    utils::{
        hull::{EntityHull, Hull},
        identifiers::{EntityCenter, EntityId, Id}
    },
    Path
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

/// The data that can be stored in the Clipboard.
#[must_use]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::map) enum ClipboardData
{
    /// A [`Brush`].
    Brush(BrushData, Id),
    /// A [`ThingInstance`].
    Thing(ThingInstanceData, Id)
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

impl EntityHull for ClipboardData
{
    #[inline]
    fn hull(&self) -> Hull
    {
        match self
        {
            ClipboardData::Brush(data, _) =>
            {
                let mut hull = data.polygon_hull();

                if let Some(h) = data.sprite_hull()
                {
                    hull = hull.merged(&h);
                }

                match data.path_hull()
                {
                    Some(h) => hull.merged(&h),
                    None => hull
                }
            },
            ClipboardData::Thing(data, _) =>
            {
                let hull = data.hull();

                match data.path_hull()
                {
                    Some(h) => hull.merged(&h),
                    None => hull
                }
            }
        }
    }
}

impl EntityCenter for ClipboardData
{
    #[inline]
    fn center(&self) -> Vec2 { self.hull().center() }
}

impl ClipboardData
{
    /// Whever `self` is out of bounds if moved by the amount `delta`.
    #[inline]
    #[must_use]
    fn out_of_bounds(&self, delta: Vec2) -> bool { (self.hull() + delta).out_of_bounds() }

    /// Draws the [`ClipboardData`] at its position moved by `delta`
    #[inline]
    fn draw(&self, bundle: &mut DrawBundle, delta: Vec2, camera_id: Option<bevy::prelude::Entity>)
    {
        match self
        {
            ClipboardData::Brush(data, _) =>
            {
                data.draw_prop(
                    draw_camera!(bundle, camera_id),
                    &mut bundle.drawer,
                    Color::NonSelectedEntity,
                    delta
                );
            },
            ClipboardData::Thing(data, _) =>
            {
                data.draw_prop(&mut bundle.drawer, bundle.things_catalog, delta);
            }
        };
    }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// Marker for the cameras used to create the image screenshot of the imported props.
#[derive(Component)]
pub(in crate::map) struct PropCamera;

//=======================================================================//

/// Marker for the camera used to create the image screenshot of the paint tool created prop.
#[derive(Component)]
pub(in crate::map) struct PaintToolPropCamera;

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
    (bevy::prelude::Entity, &'a mut bevy::prelude::Camera, &'a mut Transform),
    (With<PropCamera>, Without<PaintToolPropCamera>)
>;

//=======================================================================//

/// An agglomeration of entities that can be spawned around the map.
#[must_use]
#[derive(Debug, Serialize, Deserialize)]
pub(in crate::map) struct Prop
{
    /// The entities in their [`ClipboardData`] representation.
    data:           HvVec<ClipboardData>,
    /// The center of the area covered by the entities.
    data_center:    Vec2,
    /// The point used as reference for the spawn process.
    pivot:          Vec2,
    /// The amount of [`ClipboardData`] that owns anchored brushes.
    anchor_owners:  usize,
    /// The range of indexes of `data` in which anchored brushes are stored.
    anchored_range: Range<usize>,
    /// The optional texture screenshot.
    screenshot:     Option<egui::TextureId>
}

impl Default for Prop
{
    #[inline]
    fn default() -> Self
    {
        Self {
            data:           hv_vec![capacity; 30],
            data_center:    Vec2::ZERO,
            pivot:          Vec2::ZERO,
            anchor_owners:  0,
            anchored_range: 0..0,
            screenshot:     None
        }
    }
}

impl EntityHull for Prop
{
    #[inline]
    fn hull(&self) -> Hull
    {
        Hull::from_hulls_iter(self.data.iter().map(EntityHull::hull)).unwrap()
    }
}

impl Prop
{
    //==============================================================
    // New

    /// Creates a new [`Prop`] for with the entities contained in `iter`, pivot placed at
    /// `cursor_pos`, and `screenshot`.
    #[inline]
    pub(in crate::map::editor::state) fn new<'a, D>(
        iter: impl Iterator<Item = &'a D>,
        cursor_pos: Vec2,
        screenshot: Option<egui::TextureId>
    ) -> Self
    where
        D: CopyToClipboard + ?Sized + 'a
    {
        let mut new = Self::default();
        new.fill(iter);
        new.pivot = new.data_center - cursor_pos;
        new.screenshot = screenshot;
        new
    }

    /// Returns a new [`Image`] to be used for a screenshot.
    #[inline]
    #[must_use]
    pub fn image(size: Extent3d) -> Image
    {
        let mut image = Image {
            texture_descriptor: TextureDescriptor {
                label: None,
                size,
                dimension: TextureDimension::D2,
                format: TextureFormat::Bgra8UnormSrgb,
                mip_level_count: 1,
                sample_count: 1,
                usage: TextureUsages::TEXTURE_BINDING |
                    TextureUsages::COPY_DST |
                    TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[]
            },
            ..Default::default()
        };
        image.resize(size);

        image
    }

    /// An empty [`Image`] set up to store the [`Prop`] screenshot.
    #[inline]
    #[must_use]
    pub fn empty_image() -> Image
    {
        Self::image(Extent3d {
            width:                 PROP_SCREENSHOT_SIZE.x,
            height:                PROP_SCREENSHOT_SIZE.y,
            depth_or_array_layers: 1
        })
    }

    //==============================================================
    // Info

    /// Whever `self` contains copied entities.
    #[inline]
    #[must_use]
    fn has_data(&self) -> bool { !self.data.is_empty() }

    /// Returns a reference to the screenshot image id.
    /// # Panics
    /// Panics if `self` has no stored screenshot.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn screenshot(&self) -> egui::TextureId
    {
        self.screenshot.unwrap()
    }

    /// The amount copies of the entities contained in `self` must be moved to be spawned on the map
    /// with the pivot placed at `cursor_pos`.
    #[inline]
    #[must_use]
    fn spawn_delta(&self, cursor_pos: Vec2) -> Vec2 { cursor_pos - self.data_center + self.pivot }

    //==============================================================
    // Update

    /// Fills `self` with copies of the entities provided by `iter`.
    #[inline]
    fn fill<'a, D>(&mut self, iter: impl Iterator<Item = &'a D>)
    where
        D: CopyToClipboard + ?Sized + 'a
    {
        self.data.clear();
        self.anchor_owners = 0;
        self.anchored_range = 0..0;

        let mut anchors = 0;
        let mut anchored = 0;

        for item in iter.map(CopyToClipboard::copy_to_clipboard)
        {
            let index = match &item
            {
                ClipboardData::Thing(..) => self.data.len(),
                ClipboardData::Brush(data, _) =>
                {
                    if data.is_anchored()
                    {
                        anchored += 1;
                        anchors
                    }
                    else if data.has_anchors()
                    {
                        anchors += 1;
                        0
                    }
                    else
                    {
                        self.data.len()
                    }
                }
            };

            self.data.insert(index, item);
        }

        let (anchor_brushes, anchored_brushes) = self.data.split_at_mut(anchors);
        let anchored_brushes = &mut anchored_brushes[..anchored];
        self.anchored_range = anchors..anchors + anchored;

        for data in anchor_brushes
            .iter_mut()
            .map(|item| match_or_panic!(item, ClipboardData::Brush(data, _), data))
        {
            assert!(data.has_anchors(), "Mover has no anchors.");
            let mut to_remove = hv_vec![];

            for id in data.anchors().unwrap().iter().copied()
            {
                if anchored_brushes.iter().any(|item| item.id() == id)
                {
                    continue;
                }

                to_remove.push(id);
            }

            for id in to_remove
            {
                data.remove_anchor(id);
            }
        }

        for data in anchored_brushes
            .iter_mut()
            .map(|item| match_or_panic!(item, ClipboardData::Brush(data, _), data))
        {
            data.disanchor();
        }

        anchor_brushes.sort_by(|a, b| {
            match (a, b)
            {
                (ClipboardData::Brush(a, _), ClipboardData::Brush(b, _)) =>
                {
                    a.has_anchors().cmp(&b.has_anchors()).reverse()
                },
                (ClipboardData::Brush(..), ClipboardData::Thing(..)) => Ordering::Less,
                (ClipboardData::Thing(..), ClipboardData::Brush(..)) => Ordering::Greater,
                (ClipboardData::Thing(..), ClipboardData::Thing(..)) => Ordering::Equal
            }
        });

        for item in anchor_brushes
        {
            if !match_or_panic!(item, ClipboardData::Brush(data, _), data).has_anchors()
            {
                break;
            }

            self.anchor_owners += 1;
        }

        self.reset_data_center();
    }

    /// Resets the center of `self`.
    #[inline]
    fn reset_data_center(&mut self)
    {
        self.data_center = Hull::from_hulls_iter(self.data.iter().map(EntityHull::hull))
            .unwrap()
            .center();
    }

    /// Updates the things of the contained [`ThingInstance`]s after a things reload. Returns whever
    /// any thing were changed.
    #[inline]
    #[must_use]
    fn reload_things(&mut self, catalog: &ThingsCatalog) -> bool
    {
        let mut changed = false;

        for data in &mut self.data
        {
            let data = continue_if_no_match!(data, ClipboardData::Thing(data, _), data);
            _ = data.set_thing(catalog.thing_or_error(data.thing()));
            changed = true;
        }

        if changed
        {
            self.reset_data_center();
            return true;
        }

        false
    }

    /// Updates the textures of the contained [`Brush`]es after a texture reload. Returns whever any
    /// textures were changed.
    #[inline]
    #[must_use]
    fn reload_textures(&mut self, drawing_resources: &DrawingResources) -> bool
    {
        let mut changed = false;

        for data in &mut self.data
        {
            if let ClipboardData::Brush(data, _) = data
            {
                if !data.has_texture()
                {
                    continue;
                }

                _ = data.set_texture(
                    drawing_resources,
                    drawing_resources
                        .texture_or_error(data.texture_name().unwrap())
                        .name()
                );
            }

            changed = true;
        }

        if changed
        {
            self.reset_data_center();
            return true;
        }

        false
    }

    //==============================================================
    // Spawn

    /// Spawns a copy of `self` moved by `delta`.
    #[inline]
    fn spawn(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        delta: Vec2
    )
    {
        /// Spawns the entities stored in `prop`.
        #[inline]
        fn spawn_regular(
            prop: &mut Prop,
            drawing_resources: &DrawingResources,
            manager: &mut EntitiesManager,
            edits_history: &mut EditsHistory,
            range: Rev<Range<usize>>,
            delta: Vec2
        )
        {
            for i in range
            {
                let item = &mut prop.data[i];
                let new_id = manager.spawn_pasted_entity(
                    drawing_resources,
                    edits_history,
                    item.clone(),
                    delta
                );

                match item
                {
                    ClipboardData::Brush(_, id) | ClipboardData::Thing(_, id) => *id = new_id
                };
            }
        }

        assert!(self.has_data(), "Prop contains no entities.");

        if self.data.iter().any(|item| item.out_of_bounds(delta))
        {
            error_message("Cannot spawn copy: out of bounds");
            return;
        }

        spawn_regular(
            self,
            drawing_resources,
            manager,
            edits_history,
            (self.anchored_range.end..self.data.len()).rev(),
            delta
        );

        for i in self.anchored_range.clone().rev()
        {
            let item = &mut self.data[i];
            let old_id = item.id();
            let new_id =
                manager.spawn_pasted_entity(drawing_resources, edits_history, item.clone(), delta);

            match item
            {
                ClipboardData::Brush(_, id) | ClipboardData::Thing(_, id) => *id = new_id
            };

            let data =
                continue_if_none!(self.data[0..self.anchor_owners].iter_mut().find_map(|item| {
                    let data = match_or_panic!(item, ClipboardData::Brush(data, _), data);
                    data.contains_anchor(old_id).then_some(data)
                }));

            data.remove_anchor(old_id);
            data.insert_anchor(new_id);
        }

        spawn_regular(
            self,
            drawing_resources,
            manager,
            edits_history,
            (0..self.anchored_range.start).rev(),
            delta
        );
    }

    /// Spawns a copy of `self` the copy-paste way.
    #[inline]
    fn spawn_copy(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        cursor_pos: Vec2
    )
    {
        let mut delta = self.spawn_delta(cursor_pos);

        // If the pasted and the original overlap pull them apart.
        if self.data.len() == 1 && manager.entity_exists(self.data[0].id())
        {
            let hull = self.data[0].hull();

            if let Some(overlap_vector) = hull.overlap_vector(&(hull + delta))
            {
                delta += overlap_vector;
            }
        }

        self.spawn(drawing_resources, manager, edits_history, delta);
    }

    /// Spawns a copy of `self` as if it were a brush of a image editing software.
    #[inline]
    fn paint_copy(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        cursor_pos: Vec2
    )
    {
        self.spawn(drawing_resources, manager, edits_history, self.spawn_delta(cursor_pos));
    }

    //==============================================================
    // Draw

    /// Draws `self` for the image preview screenshot.
    #[inline]
    pub(in crate::map::editor::state) fn draw(
        &self,
        bundle: &mut DrawBundle,
        camera_id: Option<bevy::prelude::Entity>
    )
    {
        let delta = draw_camera!(bundle, camera_id).translation.truncate() - self.data_center;

        for item in &self.data
        {
            item.draw(bundle, delta, camera_id);
        }

        bundle
            .drawer
            .prop_pivot(self.data_center + delta - self.pivot, Color::Hull, camera_id);
    }
}

//=======================================================================//

/// A timer that disables the camera assigned to a [`Prop`] to take its screenshot once the time has
/// finished.
#[must_use]
#[derive(Clone, Copy, Debug)]
pub(in crate::map::editor::state) struct PropScreenshotTimer(usize, Option<bevy::prelude::Entity>);

impl PropScreenshotTimer
{
    /// Returns a new [`PropScreenshotTimer`].
    #[inline]
    pub const fn new(camera_id: Option<bevy::prelude::Entity>) -> Self { Self(3, camera_id) }

    /// Returns the [`Entity`] of the assigned camera.
    #[inline]
    #[must_use]
    pub fn id(&self) -> bevy::prelude::Entity { self.1.unwrap() }

    /// Updates the assigned camera, deactivating it once the time has finished. Returns whever the
    /// camera has been disabled.
    #[inline]
    pub fn update(&mut self, prop_cameras: &mut PropCamerasMut) -> bool
    {
        self.0 -= 1;

        if self.0 != 0
        {
            return false;
        }

        assert!(
            std::mem::replace(
                &mut prop_cameras
                    .get_mut(return_if_none!(self.1, true))
                    .unwrap()
                    .1
                    .is_active,
                false
            ),
            "Camera was disabled."
        );

        true
    }
}

//=======================================================================//

/// A clipboard where data to be pasted around the map is stored.
pub(in crate::map::editor::state) struct Clipboard
{
    /// The copy-paste stored entities.
    copy_paste: Prop,
    /// The quick prop created with the paint tool.
    quick_prop: Prop,
    /// The slotted [`Prop`]s.
    props: HvVec<Prop>,
    /// The index of the [`Prop`] selected in the UI, if any.
    selected_prop: Option<usize>,
    /// The text copied from the UI fields.
    ui_text: String,
    /// The copied platform path, if any.
    platform_path: Option<Path>,
    /// Whever the stored [`Prop`]s were edited.
    props_changed: bool,
    /// The [`Prop`]s which have an assigned camera to take their screenshot.
    props_with_assigned_camera: ArrayVec<(PropScreenshotTimer, usize), PROP_CAMERAS_AMOUNT>,
    /// The [`Prop`]s with no assigned camera to take their screenshot.
    props_with_no_camera: HvVec<usize>,
    /// The frames that must pass before the [`Prop`] screenshots can be taken.
    props_import_wait_frames: usize,
    /// The function used to run the frame update.
    update_func: fn(&mut Self, &mut Assets<Image>, &mut PropCamerasMut, &mut EguiUserTextures)
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
    pub fn new() -> Self
    {
        Self {
            copy_paste: Prop::default(),
            quick_prop: Prop::default(),
            props: HvVec::new(),
            selected_prop: None,
            ui_text: String::new(),
            platform_path: None,
            props_changed: false,
            props_with_assigned_camera: ArrayVec::new(),
            props_with_no_camera: hv_vec![],
            props_import_wait_frames: Self::IMPORTS_WAIT_FRAMES,
            update_func: Self::delay_update
        }
    }

    /// Creates a new [`Clipboard`] from the data stored in `file`.
    #[inline]
    pub fn from_file(
        images: &mut Assets<Image>,
        prop_cameras: &mut PropCamerasMut,
        user_textures: &mut EguiUserTextures,
        catalog: &ThingsCatalog,
        header: &MapHeader,
        file: &mut BufReader<File>
    ) -> Result<Self, &'static str>
    {
        let mut clip = Self {
            copy_paste: Prop::default(),
            quick_prop: Prop::default(),
            props: HvVec::new(),
            selected_prop: None,
            ui_text: String::new(),
            platform_path: None,
            props_changed: false,
            props_with_assigned_camera: ArrayVec::new(),
            props_with_no_camera: hv_vec![],
            props_import_wait_frames: Self::IMPORTS_WAIT_FRAMES,
            update_func: Self::delay_update
        };

        match clip.import_props(images, prop_cameras, user_textures, catalog, header.props, file)
        {
            Ok(()) => Ok(clip),
            Err(err) => Err(err)
        }
    }

    /// Import the [`Prop`]s in `file`.
    #[inline]
    pub fn import_props(
        &mut self,
        images: &mut Assets<Image>,
        prop_cameras: &mut PropCamerasMut,
        user_textures: &mut EguiUserTextures,
        catalog: &ThingsCatalog,
        props_amount: usize,
        file: &mut BufReader<File>
    ) -> Result<(), &'static str>
    {
        let mut props = hv_vec![];

        for _ in 0..props_amount
        {
            match ciborium::from_reader::<Prop, _>(&mut *file)
            {
                Ok(mut prop) =>
                {
                    _ = prop.reload_things(catalog);
                    props.push(prop);
                },
                Err(_) => return Err("Error loading props")
            };
        }

        self.props_changed = true;

        let mut prop_cameras = prop_cameras.iter_mut().filter(|camera| !camera.1.is_active);

        for prop in props
        {
            let index = self.props.len();
            self.props.push(prop);
            self.queue_prop_screenshot(images, user_textures, prop_cameras.next(), index);
        }

        Ok(())
    }

    /// Queues a [`Prop`] screenshot.
    #[inline]
    fn queue_prop_screenshot(
        &mut self,
        images: &mut Assets<Image>,
        user_textures: &mut EguiUserTextures,
        camera: Option<(
            bevy::prelude::Entity,
            bevy::prelude::Mut<bevy::prelude::Camera>,
            bevy::prelude::Mut<bevy::prelude::Transform>
        )>,
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
            &mut self.props[index]
        );
        self.props_with_assigned_camera
            .push((PropScreenshotTimer::new(camera.0.into()), index));
    }

    //==============================================================
    // Info

    /// Whever `self` was edited.
    #[inline]
    #[must_use]
    pub const fn props_changed(&self) -> bool { self.props_changed }

    /// Returns true if there is data to be pasted.
    #[inline]
    #[must_use]
    pub fn has_copy_data(&self) -> bool { self.copy_paste.has_data() }

    /// The amount of slotted props stored.
    #[inline]
    #[must_use]
    pub fn props_amount(&self) -> usize { self.props.len() }

    /// The index of the selected [`Prop`], if any.
    #[inline]
    #[must_use]
    pub const fn selected_prop_index(&self) -> Option<usize> { self.selected_prop }

    /// Whever the quick prop stored contains entities.
    #[inline]
    #[must_use]
    pub fn has_quick_prop(&self) -> bool { self.quick_prop.has_data() }

    /// Whever there are no [`Prop`]s stored.
    #[inline]
    #[must_use]
    pub fn no_props(&self) -> bool { self.props.is_empty() && !self.has_quick_prop() }

    //==============================================================
    // Update

    /// Delays the update of the [`Clipboard`]. During the first few frames it is not possible to
    /// take a [`Prop`] screenshot.
    #[inline(always)]
    fn delay_update(
        &mut self,
        _: &mut Assets<Image>,
        _: &mut PropCamerasMut,
        _: &mut EguiUserTextures
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
    #[inline(always)]
    fn regular_update(
        &mut self,
        images: &mut Assets<Image>,
        prop_cameras: &mut PropCamerasMut,
        user_textures: &mut EguiUserTextures
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

        let mut prop_cameras = prop_cameras.iter_mut();

        for _ in 0..self
            .props_with_no_camera
            .len()
            .min(PROP_CAMERAS_AMOUNT - self.props_with_assigned_camera.len())
        {
            let mut camera = prop_cameras.next_value();
            let index = self.props_with_no_camera.pop().unwrap();

            Self::assign_camera_to_prop(
                images,
                &mut (&mut camera.1, &mut camera.2),
                user_textures,
                &mut self.props[index]
            );

            self.props_with_assigned_camera
                .push((PropScreenshotTimer::new(camera.0.into()), index));
        }
    }

    /// Updates `self`.
    #[inline]
    pub fn update(
        &mut self,
        images: &mut Assets<Image>,
        prop_cameras: &mut PropCamerasMut,
        user_textures: &mut EguiUserTextures
    )
    {
        (self.update_func)(self, images, prop_cameras, user_textures);
    }

    /// Assigns a camera to a [`Prop`] to take its screenshot.
    #[allow(clippy::cast_precision_loss)]
    #[inline]
    pub fn assign_camera_to_prop(
        images: &mut Assets<Image>,
        prop_camera: &mut (&mut bevy::prelude::Camera, &mut Transform),
        user_textures: &mut EguiUserTextures,
        prop: &mut Prop
    )
    {
        assert!(
            !std::mem::replace(&mut prop_camera.0.is_active, true),
            "Tried to assign a prop screenshot to an active camera."
        );

        let hull = prop.hull();

        scale_viewport(
            prop_camera.1,
            (PROP_SCREENSHOT_SIZE.x as f32, PROP_SCREENSHOT_SIZE.y as f32),
            &hull,
            32f32
        );

        let image = images.add(Prop::empty_image());
        prop_camera.0.target = RenderTarget::Image(image.clone_weak());
        prop.screenshot = user_textures.add_image(image).into();
    }

    /// Writes the serialized [`Prop`]s in `writer`.
    #[inline]
    pub fn export_props(&self, writer: &mut BufWriter<&mut Vec<u8>>) -> Result<(), &'static str>
    {
        for prop in &self.props
        {
            if ciborium::ser::into_writer(prop, &mut *writer).is_err()
            {
                return Err("Error saving prop");
            }
        }

        Ok(())
    }

    /// Queues the screenshots of the [`Prop`]s that must be retaken after a things reload.
    #[inline]
    pub fn reload_things(&mut self, bundle: &mut StateUpdateBundle)
    {
        let mut prop_cameras = bundle.prop_cameras.iter_mut();

        for i in 0..self.props.len()
        {
            let prop = &mut self.props[i];

            if prop.reload_things(bundle.things_catalog)
            {
                self.queue_prop_screenshot(
                    bundle.images,
                    bundle.user_textures,
                    prop_cameras.next(),
                    i
                );
            }
        }
    }

    /// Queues the screenshots of the [`Prop`]s that must be retaken after a texture reload.
    #[inline]
    pub fn reload_textures(
        &mut self,
        images: &mut Assets<Image>,
        user_textures: &mut EguiUserTextures,
        prop_cameras: &mut PropCamerasMut,
        drawing_resources: &DrawingResources
    )
    {
        let mut prop_cameras = prop_cameras.iter_mut();

        for i in 0..self.props.len()
        {
            let prop = &mut self.props[i];

            if prop.reload_textures(drawing_resources)
            {
                self.queue_prop_screenshot(images, user_textures, prop_cameras.next(), i);
            }
        }
    }

    /// Resets the state changed flag.
    #[inline]
    pub fn reset_props_changed(&mut self) { self.props_changed = false; }

    /// Sets the index of the selected slotted [`Prop`].
    /// # Panics
    /// Panics if `slot` is equal or higher than the length of the slotted [`Prop`]s.
    #[inline]
    pub fn set_selected_prop_index(&mut self, slot: usize)
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
    pub fn copy<'a, D>(&mut self, iter: impl Iterator<Item = &'a D>)
    where
        D: CopyToClipboard + ?Sized + 'a
    {
        self.copy_paste.fill(iter);
    }

    /// Pastes the copied entities.
    #[inline]
    pub fn paste(
        &mut self,
        bundle: &StateUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        cursor_pos: Vec2
    )
    {
        self.copy_paste
            .spawn_copy(bundle.drawing_resources, manager, edits_history, cursor_pos);
    }

    /// Stores `prop` as the quick [`Prop`].
    #[inline]
    pub fn create_quick_prop(&mut self, prop: Prop) { self.quick_prop = prop; }

    /// Inserts a slotted [`Prop`] at the specified `slot`.
    #[inline]
    pub fn insert_prop(&mut self, prop: Prop, slot: usize)
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
    pub fn delete_selected_prop(&mut self, prop_cameras: &mut PropCamerasMut)
    {
        self.props_changed = true;

        let selected_prop = return_if_none!(self.selected_prop);
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
    pub fn spawn_quick_prop(
        &mut self,
        bundle: &ToolUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        cursor_pos: Vec2
    )
    {
        self.quick_prop
            .paint_copy(bundle.drawing_resources, manager, edits_history, cursor_pos);
    }

    /// Spawns the selected [`Prop`] on the map.
    #[inline]
    pub fn spawn_selected_prop(
        &mut self,
        bundle: &ToolUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        cursor_pos: Vec2
    )
    {
        self.props[self.selected_prop.unwrap()].paint_copy(
            bundle.drawing_resources,
            manager,
            edits_history,
            cursor_pos
        );
    }

    //==============================================================
    // UI text

    /// Applies the requested copy/paste/cut operation, if any, on the selected buffer in the
    /// selected range of characters.
    #[inline]
    #[must_use]
    pub fn copy_paste_text_editor(
        &mut self,
        inputs: &InputsPresses,
        ui: &egui::Ui,
        buffer: &mut String,
        mut output: TextEditOutput
    ) -> egui::Response
    {
        if !output.response.has_focus()
        {
            return output.response;
        }

        let range = if inputs.copy_just_pressed()
        {
            self.copy_ui_text(buffer, &output.cursor_range);
            None
        }
        else if inputs.paste_just_pressed()
        {
            self.paste_ui_text(buffer, &output.cursor_range)
        }
        else if inputs.cut_just_pressed()
        {
            self.cut_ui_text(buffer, &output.cursor_range)
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
    fn copy_ui_text(&mut self, buffer: &String, cursor_range: &Option<CursorRange>)
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
        cursor_range: &Option<CursorRange>
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
        cursor_range: &Option<CursorRange>
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
    pub fn copy_platform_path(&mut self, manager: &mut EntitiesManager, identifier: Id)
    {
        let mut path = manager.moving(identifier).path().unwrap().clone();
        path.deselect_nodes_no_indexes();
        self.platform_path = path.into();
    }

    /// Pastes the copied [`Path`] in the [`Brush`] with [`Id`] `identifier`.
    #[inline]
    pub fn paste_platform_path(
        &mut self,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
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

            manager.replace_selected_path(identifier, edits_history, path.clone());
            return;
        }

        manager.create_path(identifier, path.clone(), edits_history);
    }

    /// Cuts the [`Path`] of the brush with [`Id`] `identifier`.
    #[inline]
    pub fn cut_platform_path(
        &mut self,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        identifier: Id
    )
    {
        self.copy_platform_path(manager, identifier);
        manager.remove_selected_path(identifier, edits_history);
    }

    //==============================================================
    // Iterators

    /// Returns a [`Chunks`] iterator to the slotted [`Prop`]s with size `chunk_size`.
    #[inline]
    #[must_use]
    pub fn chunked_props(
        &self,
        chunk_size: usize
    ) -> impl ExactSizeIterator<Item = impl Iterator<Item = (usize, egui::TextureId)> + '_>
    {
        self.props.chunks(chunk_size).enumerate().map(move |(index, props)| {
            let mut index = index * chunk_size;

            props.iter().map(move |prop| {
                let value = (index, prop.screenshot());
                index += 1;
                value
            })
        })
    }

    /// Draws the [`Prop`]s to photograph for the preview.
    #[inline]
    pub fn draw_props_to_photograph(&self, bundle: &mut DrawBundle)
    {
        for (timer, idx) in &self.props_with_assigned_camera
        {
            self.props[*idx].draw(bundle, (timer.id()).into());
        }
    }
}
