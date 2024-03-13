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
use serde::{Deserialize, Serialize};
use shared::{continue_if_none, match_or_panic, return_if_no_match, return_if_none, NextValue};

use super::{editor_state::InputsPresses, edits_history::EditsHistory, manager::EntitiesManager};
use crate::{
    map::{
        brush::{convex_polygon::ConvexPolygon, mover::MoverParts},
        camera::scale_viewport,
        drawer::{color::Color, drawing_resources::DrawingResources},
        editor::{
            draw_camera,
            state::error_message,
            DrawBundle,
            StateUpdateBundle,
            ToolUpdateBundle
        },
        hv_vec,
        thing::{catalog::ThingsCatalog, ThingId},
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
    // A brush that may still exist in the world.
    Brush(ConvexPolygon, Id, MoverParts),
    // A thing that may still exist in the world.
    Thing(ThingId, Id, Hull)
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
            ClipboardData::Brush(_, id, _) | ClipboardData::Thing(_, id, _) => id
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
            ClipboardData::Brush(cp, _, mp) =>
            {
                let mut hull = cp.hull();

                if let Some(h) = cp.sprite_hull()
                {
                    hull = hull.merged(&h);
                }

                match mp.path_hull(cp.center())
                {
                    Some(h) => hull.merged(&h),
                    None => hull
                }
            },
            ClipboardData::Thing(_, _, hull) => *hull
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
    fn out_of_bounds(&self, delta: Vec2) -> bool
    {
        match self
        {
            ClipboardData::Brush(poly, _, parts) =>
            {
                !poly.check_move(delta, true) ||
                    parts.path_hull_out_of_bounds(poly.center() + delta)
            },
            ClipboardData::Thing(_, _, hull) => (*hull + delta).out_of_bounds()
        }
    }

    /// Draws the [`ClipboardData`] at its position moved by `delta`
    #[inline]
    fn draw(
        &self,
        bundle: &mut DrawBundle,
        manager: &EntitiesManager,
        delta: Vec2,
        camera_id: Option<bevy::prelude::Entity>
    )
    {
        match self
        {
            ClipboardData::Brush(polygon, _, mover) =>
            {
                polygon.draw_prop(
                    draw_camera!(bundle, camera_id),
                    &mut bundle.drawer,
                    Color::NonSelectedEntity,
                    delta
                );

                return_if_no_match!(mover, MoverParts::Other(Some(path), _), path)
                    .draw_prop(&mut bundle.drawer, polygon.center() + delta);
            },
            ClipboardData::Thing(_, id, _) =>
            {
                manager
                    .thing(*id)
                    .draw_prop(&mut bundle.drawer, bundle.things_catalog, delta);
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

pub(in crate::map) type PropCameras<'world, 'state, 'a> = Query<
    'world,
    'state,
    (&'a Camera, &'a Transform),
    (With<PropCamera>, Without<PaintToolPropCamera>)
>;

//=======================================================================//

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
                ClipboardData::Brush(_, _, MoverParts::None) | ClipboardData::Thing(..) =>
                {
                    self.data.len()
                },
                ClipboardData::Brush(_, _, MoverParts::Anchored(_)) =>
                {
                    anchored += 1;
                    anchors
                },
                ClipboardData::Brush(_, _, MoverParts::Other(_, ids)) =>
                {
                    if ids.is_some()
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

        for mover in anchor_brushes
            .iter_mut()
            .map(|item| match_or_panic!(item, ClipboardData::Brush(_, _, mover), mover))
        {
            assert!(mover.has_anchors(), "Mover has no anchors.");
            let mut to_remove = hv_vec![];

            for id in mover.anchors().unwrap().iter().copied()
            {
                if anchored_brushes.iter().any(|item| item.id() == id)
                {
                    continue;
                }

                to_remove.push(id);
            }

            for id in to_remove
            {
                mover.remove_anchor(id);
            }
        }

        for mover in anchored_brushes.iter_mut().map(|item| {
            match_or_panic!(
                item,
                ClipboardData::Brush(_, _, mover @ MoverParts::Anchored(_)),
                mover
            )
        })
        {
            *mover = MoverParts::None;
        }

        anchor_brushes.sort_by(|a, b| {
            match (a, b)
            {
                (ClipboardData::Brush(_, _, a), ClipboardData::Brush(_, _, b)) =>
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
            if !match_or_panic!(item, ClipboardData::Brush(_, _, mover), mover).has_anchors()
            {
                break;
            }

            self.anchor_owners += 1;
        }

        self.data_center = Hull::from_hulls_iter(self.data.iter().map(EntityHull::hull))
            .unwrap()
            .center();
    }

    //==============================================================
    // Spawn

    /// Spawns a copy of `self` moved by `delta`.
    #[inline]
    fn spawn(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        delta: Vec2
    )
    {
        #[inline]
        fn spawn_regular(
            prop: &mut Prop,
            drawing_resources: &DrawingResources,
            things_catalog: &ThingsCatalog,
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
                    things_catalog,
                    item,
                    edits_history,
                    delta
                );

                match item
                {
                    ClipboardData::Brush(_, id, _) | ClipboardData::Thing(_, id, _) => *id = new_id
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
            things_catalog,
            manager,
            edits_history,
            (self.anchored_range.end..self.data.len()).rev(),
            delta
        );

        for i in self.anchored_range.clone().rev()
        {
            let item = &mut self.data[i];
            let old_id = item.id();
            let new_id = manager.spawn_pasted_entity(
                drawing_resources,
                things_catalog,
                item,
                edits_history,
                delta
            );

            match item
            {
                ClipboardData::Brush(_, id, _) | ClipboardData::Thing(_, id, _) => *id = new_id
            };

            let mover =
                continue_if_none!(self.data[0..self.anchor_owners].iter_mut().find_map(|item| {
                    let mover = match_or_panic!(item, ClipboardData::Brush(_, _, mover), mover);
                    mover.contains_anchor(old_id).then_some(mover)
                }));

            mover.remove_anchor(old_id);
            mover.insert_anchor(new_id);
        }

        spawn_regular(
            self,
            drawing_resources,
            things_catalog,
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
        things_catalog: &ThingsCatalog,
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

        self.spawn(drawing_resources, things_catalog, manager, edits_history, delta);
    }

    /// Spawns a copy of `self` as if it were a brush of a image editing software.
    #[inline]
    fn paint_copy(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        cursor_pos: Vec2
    )
    {
        self.spawn(
            drawing_resources,
            things_catalog,
            manager,
            edits_history,
            self.spawn_delta(cursor_pos)
        );
    }

    //==============================================================
    // Draw

    /// Draws `self` for the image preview screenshot.
    #[inline]
    pub(in crate::map::editor::state) fn draw(
        &self,
        bundle: &mut DrawBundle,
        manager: &EntitiesManager,
        camera_id: Option<bevy::prelude::Entity>
    )
    {
        let delta = draw_camera!(bundle, camera_id).translation.truncate() - self.data_center;

        for item in &self.data
        {
            item.draw(bundle, manager, delta, camera_id);
        }

        bundle.drawer.prop_square_highlight(
            self.data_center + delta - self.pivot,
            Color::Hull,
            camera_id
        );
    }
}

//=======================================================================//

#[must_use]
#[derive(Clone, Copy, Debug)]
pub(in crate::map::editor::state) struct PropScreenshotTimer(usize, Option<bevy::prelude::Entity>);

impl PropScreenshotTimer
{
    #[inline]
    pub fn new(camera_id: Option<bevy::prelude::Entity>) -> Self { Self(2, camera_id) }

    #[inline]
    #[must_use]
    pub fn id(&self) -> bevy::prelude::Entity { self.1.unwrap() }

    #[inline]
    pub fn update(&mut self, prop_cameras: &mut PropCamerasMut) -> bool
    {
        self.0 -= 1;

        if self.0 != 0
        {
            return false;
        }

        assert!(std::mem::replace(
            &mut prop_cameras
                .get_mut(return_if_none!(self.1, true))
                .unwrap()
                .1
                .is_active,
            false
        ));

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
    imported_props_with_assigned_camera:
        ArrayVec<(PropScreenshotTimer, usize), PROP_CAMERAS_AMOUNT>,
    imported_props_with_no_camera: HvVec<usize>,
    props_import_wait_frames: usize,
    update_func: fn(&mut Self, &mut Assets<Image>, &mut PropCamerasMut, &mut EguiUserTextures)
}

impl Clipboard
{
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
            imported_props_with_assigned_camera: ArrayVec::new(),
            imported_props_with_no_camera: hv_vec![],
            props_import_wait_frames: Self::IMPORTS_WAIT_FRAMES,
            update_func: Self::delay_update
        }
    }

    #[inline]
    pub fn from_file(
        images: &mut Assets<Image>,
        prop_cameras: &mut PropCamerasMut,
        user_textures: &mut EguiUserTextures,
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
            imported_props_with_assigned_camera: ArrayVec::new(),
            imported_props_with_no_camera: hv_vec![],
            props_import_wait_frames: Self::IMPORTS_WAIT_FRAMES,
            update_func: Self::delay_update
        };

        match clip.import_props(images, prop_cameras, user_textures, header.props, file)
        {
            Ok(()) => Ok(clip),
            Err(err) => Err(err)
        }
    }

    #[inline]
    pub fn import_props(
        &mut self,
        images: &mut Assets<Image>,
        prop_cameras: &mut PropCamerasMut,
        user_textures: &mut EguiUserTextures,
        props_amount: usize,
        file: &mut BufReader<File>
    ) -> Result<(), &'static str>
    {
        let mut props = hv_vec![];

        for _ in 0..props_amount
        {
            match ciborium::from_reader(&mut *file)
            {
                Ok(prop) => props.push(prop),
                Err(_) => return Err("Error loading props")
            };
        }

        self.props_changed = true;

        let mut prop_cameras = prop_cameras.iter_mut().filter(|camera| !camera.1.is_active);

        for prop in props
        {
            let camera = prop_cameras.next();
            let idx = self.props.len();
            self.props.push(prop);

            if self.imported_props_with_assigned_camera.is_full()
            {
                assert!(camera.is_none());
                self.imported_props_with_no_camera.push(idx);
                continue;
            }

            let mut camera = camera.unwrap();

            Self::assign_camera_to_prop(
                images,
                &mut (&mut camera.1, &mut camera.2),
                user_textures,
                &mut self.props[idx]
            );
            self.imported_props_with_assigned_camera
                .push((PropScreenshotTimer::new(camera.0.into()), idx));
        }

        Ok(())
    }

    #[inline]
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

    #[inline]
    fn regular_update(
        &mut self,
        images: &mut Assets<Image>,
        prop_cameras: &mut PropCamerasMut,
        user_textures: &mut EguiUserTextures
    )
    {
        let mut i = 0;

        while i < self.imported_props_with_assigned_camera.len()
        {
            if self.imported_props_with_assigned_camera[i].0.update(prop_cameras)
            {
                _ = self.imported_props_with_assigned_camera.swap_remove(i);
                continue;
            }

            i += 1;
        }

        let mut prop_cameras = prop_cameras.iter_mut();

        for _ in 0..self
            .imported_props_with_no_camera
            .len()
            .min(PROP_CAMERAS_AMOUNT - self.imported_props_with_assigned_camera.len())
        {
            let mut camera = prop_cameras.next_value();
            let index = self.imported_props_with_no_camera.pop().unwrap();

            Self::assign_camera_to_prop(
                images,
                &mut (&mut camera.1, &mut camera.2),
                user_textures,
                &mut self.props[index]
            );

            self.imported_props_with_assigned_camera
                .push((PropScreenshotTimer::new(camera.0.into()), index));
        }
    }

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

    #[allow(clippy::cast_precision_loss)]
    #[inline]
    pub fn assign_camera_to_prop(
        images: &mut Assets<Image>,
        prop_camera: &mut (&mut bevy::prelude::Camera, &mut Transform),
        user_textures: &mut EguiUserTextures,
        prop: &mut Prop
    )
    {
        assert!(!std::mem::replace(&mut prop_camera.0.is_active, true));

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

    //==============================================================
    // Info

    /// Whever `self` was edited.
    #[inline]
    #[must_use]
    pub fn props_changed(&self) -> bool { self.props_changed }

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
        self.copy_paste.spawn_copy(
            bundle.drawing_resources,
            bundle.things_catalog,
            manager,
            edits_history,
            cursor_pos
        );
    }

    /// Stores `prop` as the quick [`Prop`].
    #[inline]
    pub fn create_quick_prop(&mut self, prop: Prop) { self.quick_prop = prop; }

    /// Inserts a slotted [`Prop`] at the specified `slot`.
    #[inline]
    pub fn insert_prop(&mut self, prop: Prop, slot: usize)
    {
        assert!(prop.screenshot.is_some());

        self.props_changed = true;

        if slot >= self.props.len()
        {
            self.props.push(prop);
            return;
        }

        self.props[slot] = prop;

        if let Some(i) = self
            .imported_props_with_assigned_camera
            .iter()
            .position(|(_, idx)| *idx == slot)
        {
            _ = self.imported_props_with_assigned_camera.remove(i);
        }
        else if let Some(i) =
            self.imported_props_with_no_camera.iter().position(|idx| *idx == slot)
        {
            self.imported_props_with_no_camera.remove(i);
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
            .imported_props_with_assigned_camera
            .iter()
            .copied()
            .enumerate()
            .find(|(_, (_, idx))| *idx == selected_prop)
        {
            prop_cameras.get_mut(timer.id()).unwrap().1.is_active = false;

            _ = self.imported_props_with_assigned_camera.remove(i);

            for (_, idx) in self.imported_props_with_assigned_camera.iter_mut().skip(i)
            {
                *idx -= 1;
            }

            let i = return_if_none!(self
                .imported_props_with_no_camera
                .iter()
                .position(|idx| *idx > selected_prop));

            for idx in self.imported_props_with_no_camera.iter_mut().skip(i)
            {
                *idx -= 1;
            }

            return;
        }

        let i = return_if_none!(self
            .imported_props_with_no_camera
            .iter()
            .position(|idx| *idx == selected_prop));

        self.imported_props_with_no_camera.remove(i);

        for idx in self.imported_props_with_no_camera.iter_mut().skip(i)
        {
            *idx -= 1;
        }

        let i = return_if_none!(self
            .imported_props_with_assigned_camera
            .iter()
            .position(|(_, idx)| *idx > selected_prop));

        for (_, idx) in self.imported_props_with_assigned_camera.iter_mut().skip(i)
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
        self.quick_prop.paint_copy(
            bundle.drawing_resources,
            bundle.things_catalog,
            manager,
            edits_history,
            cursor_pos
        );
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
            bundle.things_catalog,
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
        self.platform_path = manager.moving(identifier).path().unwrap().clone().into();
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
        self.platform_path = manager.moving(identifier).path().unwrap().clone().into();
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

    #[inline]
    pub fn draw_props_to_photograph(&self, bundle: &mut DrawBundle, manager: &EntitiesManager)
    {
        for (timer, idx) in self
            .imported_props_with_assigned_camera
            .iter()
            .take(PROP_CAMERAS_AMOUNT)
        {
            self.props[*idx].draw(bundle, manager, (timer.id()).into());
        }
    }
}
