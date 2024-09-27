//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{cmp::Ordering, iter::Rev, ops::Range};

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
use hill_vacuum_shared::{continue_if_no_match, continue_if_none, match_or_panic, return_if_none};
use serde::{Deserialize, Serialize};

use super::{ClipboardData, CopyToClipboard, PropCamerasMut, PROP_SCREENSHOT_SIZE};
use crate::{
    error_message,
    map::{
        drawer::{color::Color, drawing_resources::DrawingResources},
        editor::{
            state::{edits_history::EditsHistory, grid::Grid, manager::EntitiesManager},
            DrawBundle
        },
        thing::catalog::ThingsCatalog
    },
    utils::{collections::hv_vec, hull::Hull, identifiers::EntityId},
    HvVec
};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// An agglomeration of entities that can be spawned around the map.
#[must_use]
#[derive(Debug, Serialize, Deserialize)]
pub(in crate::map) struct Prop
{
    /// The entities in their [`ClipboardData`] representation.
    data: HvVec<ClipboardData>,
    /// The center of the area covered by the entities.
    data_center: Vec2,
    /// The point used as reference for the spawn process.
    pivot: Vec2,
    /// The amount of [`ClipboardData`] that owns attached brushes.
    attachments_owners: usize,
    /// The range of indexes of `data` in which attached brushes are stored.
    attached_range: Range<usize>,
    /// The optional texture screenshot.
    pub(in crate::map::editor::state::clipboard) screenshot: Option<egui::TextureId>
}

impl Default for Prop
{
    #[inline]
    fn default() -> Self
    {
        Self {
            data:               hv_vec![],
            data_center:        Vec2::ZERO,
            pivot:              Vec2::ZERO,
            attachments_owners: 0,
            attached_range:     0..0,
            screenshot:         None
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
    pub(in crate::map::editor::state) fn new<'a, D>(
        drawing_resources: &DrawingResources,
        grid: Grid,
        iter: impl Iterator<Item = &'a D>,
        cursor_pos: Vec2,
        screenshot: Option<egui::TextureId>
    ) -> Self
    where
        D: CopyToClipboard + ?Sized + 'a
    {
        let mut new = Self::default();
        new.fill(drawing_resources, grid, iter);
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

    #[inline]
    pub(in crate::map::editor::state::clipboard) fn hull(
        &self,
        drawing_resources: &DrawingResources,
        grid: Grid
    ) -> Hull
    {
        Hull::from_hulls_iter(self.data.iter().map(|data| data.hull(drawing_resources, grid)))
            .unwrap()
    }

    /// Whether `self` contains copied entities.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state::clipboard) fn has_data(&self) -> bool
    {
        !self.data.is_empty()
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
    fn spawn_delta(&self, cursor_pos: Vec2) -> Vec2 { cursor_pos - self.data_center + self.pivot }

    //==============================================================
    // Update

    /// Fills `self` with copies of the entities provided by `iter`.
    #[inline]
    pub(in crate::map::editor::state::clipboard) fn fill<'a, D>(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: Grid,
        iter: impl Iterator<Item = &'a D>
    ) where
        D: CopyToClipboard + ?Sized + 'a
    {
        self.data.clear();
        self.attachments_owners = 0;
        self.attached_range = 0..0;

        let mut attachments = 0;
        let mut attached = 0;

        for item in iter.map(CopyToClipboard::copy_to_clipboard)
        {
            let index = match &item
            {
                ClipboardData::Thing(..) => self.data.len(),
                ClipboardData::Brush(data, _) =>
                {
                    if data.is_attached()
                    {
                        attached += 1;
                        attachments
                    }
                    else if data.has_attachments()
                    {
                        attachments += 1;
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

        let (owner_brushes, attached_brushes) = self.data.split_at_mut(attachments);
        let attached_brushes = &mut attached_brushes[..attached];
        self.attached_range = attachments..attachments + attached;

        for data in owner_brushes
            .iter_mut()
            .map(|item| match_or_panic!(item, ClipboardData::Brush(data, _), data))
        {
            assert!(data.has_attachments(), "Mover has no attachments.");
            let mut to_remove = hv_vec![];

            for id in data.attachments().unwrap().iter().copied()
            {
                if attached_brushes.iter().any(|item| item.id() == id)
                {
                    continue;
                }

                to_remove.push(id);
            }

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

        owner_brushes.sort_by(|a, b| {
            match (a, b)
            {
                (ClipboardData::Brush(a, _), ClipboardData::Brush(b, _)) =>
                {
                    a.has_attachments().cmp(&b.has_attachments()).reverse()
                },
                (ClipboardData::Brush(..), ClipboardData::Thing(..)) => Ordering::Less,
                (ClipboardData::Thing(..), ClipboardData::Brush(..)) => Ordering::Greater,
                (ClipboardData::Thing(..), ClipboardData::Thing(..)) => Ordering::Equal
            }
        });

        for item in owner_brushes
        {
            if !match_or_panic!(item, ClipboardData::Brush(data, _), data).has_attachments()
            {
                break;
            }

            self.attachments_owners += 1;
        }

        self.reset_data_center(drawing_resources, grid);
    }

    /// Resets the center of `self`.
    #[inline]
    fn reset_data_center(&mut self, drawing_resources: &DrawingResources, grid: Grid)
    {
        self.data_center =
            Hull::from_hulls_iter(self.data.iter().map(|data| data.hull(drawing_resources, grid)))
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
        catalog: &ThingsCatalog,
        grid: Grid
    ) -> bool
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
            self.reset_data_center(drawing_resources, grid);
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
        grid: Grid
    ) -> bool
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
            self.reset_data_center(drawing_resources, grid);
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
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: Grid,
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
            grid: Grid,
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
            .data
            .iter()
            .any(|item| item.out_of_bounds_moved(drawing_resources, grid, delta))
        {
            error_message("Cannot spawn copy: out of bounds");
            return;
        }

        spawn_regular(
            self,
            drawing_resources,
            manager,
            edits_history,
            grid,
            (self.attached_range.end..self.data.len()).rev(),
            delta
        );

        for i in self.attached_range.clone().rev()
        {
            let item = &mut self.data[i];
            let old_id = item.id();
            let new_id = manager.spawn_pasted_entity(
                drawing_resources,
                edits_history,
                grid,
                item.clone(),
                delta
            );

            match item
            {
                ClipboardData::Brush(_, id) | ClipboardData::Thing(_, id) => *id = new_id
            };

            let data = continue_if_none!(self.data[0..self.attachments_owners]
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
            manager,
            edits_history,
            grid,
            (0..self.attached_range.start).rev(),
            delta
        );
    }

    /// Spawns a copy of `self` the copy-paste way.
    #[inline]
    pub(in crate::map::editor::state::clipboard) fn spawn_copy(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: Grid,
        cursor_pos: Vec2
    )
    {
        let mut delta = self.spawn_delta(cursor_pos);

        // If the pasted and the original overlap pull them apart.
        if self.data.len() == 1 && manager.entity_exists(self.data[0].id())
        {
            let hull = self.data[0].hull(drawing_resources, grid);

            if let Some(overlap_vector) = hull.overlap_vector(&(hull + delta))
            {
                delta += overlap_vector;
            }
        }

        self.spawn(drawing_resources, manager, edits_history, grid, delta);
    }

    /// Spawns a copy of `self` as if it were a brush of a image editing software.
    #[inline]
    pub(in crate::map::editor::state::clipboard) fn paint_copy(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: Grid,
        cursor_pos: Vec2
    )
    {
        self.spawn(drawing_resources, manager, edits_history, grid, self.spawn_delta(cursor_pos));
    }

    //==============================================================
    // Draw

    /// Draws `self` for the image preview screenshot.
    #[inline]
    pub(in crate::map::editor::state) fn draw(
        &self,
        bundle: &mut DrawBundle,
        grid: Grid,
        camera_id: Option<Entity>
    )
    {
        let delta = grid.point_projection(
            crate::map::editor::state::clipboard::draw_camera!(bundle, camera_id)
                .translation
                .truncate()
        ) - self.data_center;

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
