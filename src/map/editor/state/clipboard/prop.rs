//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{iter::Rev, ops::Range};

use bevy::{
    ecs::entity::Entity,
    render::{
        render_resource::{
            Extent3d,
            TextureDescriptor,
            TextureDimension,
            TextureFormat,
            TextureUsages
        },
        texture::Image
    }
};
use bevy_egui::egui;
use glam::Vec2;
use hill_vacuum_shared::{continue_if_none, match_or_panic, return_if_none};
use serde::{Deserialize, Serialize};

use super::{
    ClipboardData,
    ClipboardDataViewer,
    CopyToClipboard,
    PropCamerasMut,
    PROP_SCREENSHOT_SIZE
};
use crate::{
    error_message,
    map::{
        drawer::{color::Color, drawing_resources::DrawingResources},
        editor::{
            state::{edits_history::EditsHistory, grid::Grid, manager::EntitiesManager},
            DrawBundle
        },
        thing::catalog::ThingsCatalog,
        Viewer
    },
    utils::{collections::hv_vec, hull::Hull, identifiers::EntityId},
    HvVec
};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

#[must_use]
#[derive(Serialize, Deserialize)]
pub(in crate::map) struct PropViewer
{
    entities:         HvVec<ClipboardDataViewer>,
    attached_brushes: Range<usize>,
    pivot:            Vec2,
    center:           Vec2
}

//=======================================================================//

/// An agglomeration of entities that can be spawned around the map.
#[must_use]
#[derive(Clone)]
pub(in crate::map) struct Prop
{
    /// The entities in their [`ClipboardData`] representation.
    entities: HvVec<ClipboardData>,
    /// The point used as reference for the spawn process.
    pivot: Vec2,
    /// The center of the area covered by the entities.
    center: Vec2,
    /// The range of indexes of `data` in which attached brushes are stored.
    attached_brushes: Range<usize>,
    /// The optional texture screenshot.
    pub(in crate::map::editor::state::clipboard) screenshot: Option<egui::TextureId>
}

impl Default for Prop
{
    #[inline]
    fn default() -> Self
    {
        Self {
            entities:         hv_vec![],
            pivot:            Vec2::ZERO,
            center:           Vec2::ZERO,
            attached_brushes: 0..0,
            screenshot:       None
        }
    }
}

impl Viewer for Prop
{
    type Item = PropViewer;

    #[inline]
    fn from_viewer(value: Self::Item) -> Self
    {
        let Self::Item {
            entities,
            attached_brushes,
            pivot,
            center
        } = value;

        Self {
            entities: hv_vec![collect; entities.into_iter().map(ClipboardData::from_viewer)],
            pivot,
            center,
            attached_brushes,
            screenshot: None
        }
    }

    #[inline]
    fn to_viewer(self) -> Self::Item
    {
        let Self {
            entities,
            attached_brushes,
            pivot,
            center,
            ..
        } = self;

        Self::Item {
            entities: hv_vec![collect; entities.into_iter().map(ClipboardData::to_viewer)],
            attached_brushes,
            pivot,
            center
        }
    }
}

impl Prop
{
    //==============================================================
    // New

    /// Creates a new [`Prop`] with the entities contained in `iter`, pivot placed at
    /// `cursor_pos`, and `screenshot`.
    #[inline]
    pub(in crate::map::editor::state) fn new<'a, E>(
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid,
        iter: impl Iterator<Item = E>,
        cursor_pos: Vec2,
        screenshot: Option<egui::TextureId>
    ) -> Self
    where
        E: CopyToClipboard + 'a
    {
        let mut new = Self::default();
        new.fill(drawing_resources, things_catalog, grid, iter.map(|e| e.copy_to_clipboard()));
        new.pivot = new.center - cursor_pos;
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

    #[inline]
    pub(in crate::map::editor::state::clipboard) fn hull(
        &self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid
    ) -> Hull
    {
        Hull::from_hulls_iter(
            self.entities
                .iter()
                .map(|data| data.hull(drawing_resources, things_catalog, grid))
        )
        .unwrap()
    }

    /// Whether `self` contains copied entities.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state::clipboard) fn has_data(&self) -> bool
    {
        !self.entities.is_empty()
    }

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
    fn spawn_delta(&self, cursor_pos: Vec2) -> Vec2 { cursor_pos - self.center + self.pivot }

    //==============================================================
    // Update

    /// Fills `self` with copies of the entities provided by `iter`.
    #[inline]
    pub(in crate::map::editor::state::clipboard) fn fill(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid,
        iter: impl IntoIterator<Item = ClipboardData>
    )
    {
        self.entities.clear();

        let mut with_attachments = 0;
        let mut attached = 0;

        for item in iter
        {
            let index = match &item
            {
                ClipboardData::Thing(..) => self.entities.len(),
                ClipboardData::Brush(data, _) =>
                {
                    if data.is_attached()
                    {
                        attached += 1;
                        with_attachments
                    }
                    else if data.has_attachments()
                    {
                        with_attachments += 1;
                        0
                    }
                    else
                    {
                        self.entities.len()
                    }
                }
            };

            self.entities.insert(index, item);
        }

        // Clean the groups that contain attached brushes that have not been been added.
        let (owner_brushes, attached_brushes) = self.entities.split_at_mut(with_attachments);
        let attached_brushes = &mut attached_brushes[..attached];
        let mut attached = 0;

        for data in owner_brushes
            .iter_mut()
            .map(|item| match_or_panic!(item, ClipboardData::Brush(data, _), data))
        {
            let attachments = data.attachments().unwrap();
            let attachments_len = attachments.len();
            assert!(attachments_len != 0, "Brush has no attachments.");

            let to_remove = hv_vec![
                collect;
                attachments
                    .iter()
                    .copied()
                    .filter(|id| !attached_brushes.iter().any(|item| item.id() == *id))
            ];
            attached += attachments_len - to_remove.len();

            for id in to_remove
            {
                data.remove_attachment(id);
            }
        }

        for data in attached_brushes
            .iter_mut()
            .map(|item| match_or_panic!(item, ClipboardData::Brush(data, _), data))
        {
            data.detach();
        }

        // Define the actual range of attached brushes after all the detachments.
        let owner_brushes = owner_brushes.len();
        let mut actual_owner_brushes = 0;

        for i in 0..owner_brushes
        {
            if match_or_panic!(&self.entities[i], ClipboardData::Brush(data, _), data)
                .has_attachments()
            {
                actual_owner_brushes += 1;
            }
            else
            {
                let brush = self.entities.remove(i);
                self.entities.push(brush);
            }
        }

        self.attached_brushes = actual_owner_brushes..actual_owner_brushes + attached;
        self.reset_center(drawing_resources, things_catalog, grid);
    }

    /// Resets the center of `self`.
    #[inline]
    fn reset_center(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid
    )
    {
        let grid = grid.with_size(2);

        self.center = Hull::from_hulls_iter(
            self.entities
                .iter()
                .map(|data| data.hull(drawing_resources, things_catalog, &grid))
        )
        .unwrap()
        .center();
    }

    /// Updates the things of the contained [`ThingInstance`]s after a things reload. Returns
    /// whether any [`Thing`]s were changed.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state::clipboard) fn reload_things(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid
    ) -> bool
    {
        if self
            .entities
            .iter()
            .any(|data| matches!(data, ClipboardData::Thing(..)))
        {
            self.reset_center(drawing_resources, things_catalog, grid);
            return true;
        }

        false
    }

    /// Updates the textures of the contained brushes after a texture reload. Returns whether any
    /// textures were changed.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state::clipboard) fn reload_textures(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid
    ) -> bool
    {
        if self
            .entities
            .iter()
            .any(|data| matches!(data, ClipboardData::Brush(..)))
        {
            self.reset_center(drawing_resources, things_catalog, grid);
            return true;
        }

        false
    }

    //==============================================================
    // Spawn

    /// Spawns a copy of `self` moved by `delta`.
    #[inline]
    pub(in crate::map::editor::state::clipboard) fn spawn(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        delta: Vec2
    )
    {
        /// Spawns the entities stored in `prop`.
        #[inline]
        fn spawn_regular(
            prop: &mut Prop,
            drawing_resources: &DrawingResources,
            things_catalog: &ThingsCatalog,
            manager: &mut EntitiesManager,
            edits_history: &mut EditsHistory,
            grid: &Grid,
            range: Rev<Range<usize>>,
            delta: Vec2
        )
        {
            for i in range
            {
                let item = &mut prop.entities[i];
                let new_id = manager.spawn_pasted_entity(
                    drawing_resources,
                    things_catalog,
                    edits_history,
                    grid,
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

        if self
            .entities
            .iter()
            .any(|item| item.out_of_bounds_moved(drawing_resources, things_catalog, grid, delta))
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
            grid,
            (self.attached_brushes.end..self.entities.len()).rev(),
            delta
        );

        for i in self.attached_brushes.clone().rev()
        {
            let item = &mut self.entities[i];
            let old_id = item.id();
            let new_id = manager.spawn_pasted_entity(
                drawing_resources,
                things_catalog,
                edits_history,
                grid,
                item.clone(),
                delta
            );

            match item
            {
                ClipboardData::Brush(_, id) | ClipboardData::Thing(_, id) => *id = new_id
            };

            let data = continue_if_none!(self.entities[0..self.attached_brushes.start]
                .iter_mut()
                .find_map(|item| {
                    let data = match_or_panic!(item, ClipboardData::Brush(data, _), data);
                    data.contains_attachment(old_id).then_some(data)
                }));

            data.remove_attachment(old_id);
            data.insert_attachment(new_id);
        }

        spawn_regular(
            self,
            drawing_resources,
            things_catalog,
            manager,
            edits_history,
            grid,
            (0..self.attached_brushes.start).rev(),
            delta
        );
    }

    /// Spawns a copy of `self` the copy-paste way.
    #[inline]
    pub(in crate::map::editor::state::clipboard) fn spawn_copy(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        cursor_pos: Vec2
    )
    {
        let mut delta = self.spawn_delta(cursor_pos);

        // If the pasted and the original overlap pull them apart.
        if self.entities.len() == 1 && manager.entity_exists(self.entities[0].id())
        {
            let hull = self.entities[0].hull(drawing_resources, things_catalog, grid);

            if let Some(overlap_vector) = hull.overlap_vector(&(hull + delta))
            {
                delta += overlap_vector;
            }
        }

        self.spawn(drawing_resources, things_catalog, manager, edits_history, grid, delta);
    }

    /// Spawns a copy of `self` as if it were a brush of a image editing software.
    #[inline]
    pub(in crate::map::editor::state::clipboard) fn paint_copy(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        cursor_pos: Vec2
    )
    {
        self.spawn(
            drawing_resources,
            things_catalog,
            manager,
            edits_history,
            grid,
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
        camera_id: Option<Entity>
    )
    {
        let delta = bundle.drawer.grid().point_projection(
            crate::map::editor::state::clipboard::draw_camera!(bundle, camera_id)
                .translation
                .truncate()
        ) - self.center;

        for item in &self.entities
        {
            item.draw(bundle, delta);
        }

        bundle
            .drawer
            .prop_pivot(self.center + delta - self.pivot, Color::Hull, camera_id);
    }
}

//=======================================================================//

/// A timer that disables the camera assigned to a [`Prop`] to take its screenshot once the time has
/// finished.
#[must_use]
#[derive(Clone, Copy)]
pub(in crate::map::editor::state) struct PropScreenshotTimer(usize, Option<Entity>);

impl PropScreenshotTimer
{
    /// Returns a new [`PropScreenshotTimer`].
    #[inline]
    pub const fn new(camera_id: Option<Entity>) -> Self { Self(3, camera_id) }

    /// Returns the [`Entity`] of the assigned camera.
    #[inline]
    #[must_use]
    pub fn id(&self) -> Entity { self.1.unwrap() }

    /// Updates the assigned camera, deactivating it once the time has finished. Returns whether the
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
