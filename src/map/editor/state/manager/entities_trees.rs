//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::sync::{RwLock, RwLockReadGuard};

use bevy::{transform::components::Transform, window::Window};
use glam::Vec2;

use super::quad_tree::InsertResult;
use crate::{
    map::{
        brush::Brush,
        drawer::{drawing_resources::DrawingResources, TextureSize},
        editor::state::{
            grid::Grid,
            manager::quad_tree::{QuadTree, QuadTreeIds}
        },
        path::Moving,
        thing::{catalog::ThingsCatalog, ThingInstance, ThingInterface},
        MAP_SIZE
    },
    utils::{hull::Hull, identifiers::EntityId, math::AroundEqual, misc::Camera},
    Id
};

//=======================================================================//
// MACROS
//
//=======================================================================//

/// Generates the function returning the visible `entities`.
macro_rules! visible_iters {
    ($($entities:ident),+) => { paste::paste! { $(
        #[inline]
        pub fn [< visible_ $entities >](
            &self,
            camera: &Transform,
            window: &Window,
            grid: &Grid
        ) -> VisibleIds<'_>
        {
            self.[< visible_ $entities >].write().unwrap().update(camera, window, grid, |ids, viewport| {
                self.[< $entities _tree >]
                    .entities_intersect_range(ids, &viewport);
            });

            self.[< visible_ $entities >].read().unwrap()
        }
    )+}}
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The [`QuadTrees`] used by the [`EntitiesManager`].
pub(in crate::map::editor::state::manager) struct Trees
{
    /// All brushes.
    brushes_tree:     QuadTree,
    /// All [`Path`]s.
    paths_tree:       QuadTree,
    /// All brush attachments.
    anchors_tree:     QuadTree,
    /// All sprites.
    sprites_tree:     QuadTree,
    /// All [`ThingInstance`]s.
    things_tree:      QuadTree,
    /// The brushes at a certain position.
    brushes_at_pos:   RwLock<QuadTreeIdsNearPos>,
    /// The visible brushes.
    visible_brushes:  RwLock<VisibleQuadTreeIds>,
    /// The brushes in a certain range.
    brushes_in_range: RwLock<QuadTreeIds>,
    /// The visible [`Path`]s.
    visible_paths:    RwLock<VisibleQuadTreeIds>,
    /// The [`Path`]s at a certain pos.
    paths_at_pos:     RwLock<QuadTreeIdsNearPos>,
    /// The visible attachments.
    visible_anchors:  RwLock<VisibleQuadTreeIds>,
    /// The sprites at a certain position.
    sprites_at_pos:   RwLock<QuadTreeIdsNearPos>,
    /// The visible sprites.
    visible_sprites:  RwLock<VisibleQuadTreeIds>,
    /// The sprites in a certain range.
    sprites_in_range: RwLock<QuadTreeIds>,
    /// The [`ThingInstance`]s at a certain pos.
    things_at_pos:    RwLock<QuadTreeIdsNearPos>,
    /// The visible [`ThingInstance`].
    visible_things:   RwLock<VisibleQuadTreeIds>,
    /// The [`ThingInstance`] in a certain range.
    things_in_range:  RwLock<QuadTreeIds>
}

impl Trees
{
    visible_iters!(brushes, paths, sprites, anchors, things);

    /// Returns a new [`Trees`].
    #[inline]
    #[must_use]
    pub fn new() -> Self
    {
        Self {
            brushes_tree:     QuadTree::new(),
            paths_tree:       QuadTree::new(),
            anchors_tree:     QuadTree::new(),
            sprites_tree:     QuadTree::with_size(MAP_SIZE * 2f32),
            brushes_at_pos:   QuadTreeIdsNearPos::new().into(),
            visible_brushes:  VisibleQuadTreeIds::new().into(),
            brushes_in_range: QuadTreeIds::new().into(),
            visible_paths:    VisibleQuadTreeIds::new().into(),
            paths_at_pos:     QuadTreeIdsNearPos::new().into(),
            visible_anchors:  VisibleQuadTreeIds::new().into(),
            sprites_at_pos:   QuadTreeIdsNearPos::new().into(),
            visible_sprites:  VisibleQuadTreeIds::new().into(),
            sprites_in_range: QuadTreeIds::new().into(),
            things_tree:      QuadTree::new(),
            things_at_pos:    QuadTreeIdsNearPos::new().into(),
            visible_things:   VisibleQuadTreeIds::new().into(),
            things_in_range:  QuadTreeIds::new().into()
        }
    }

    /// Inserts the anchor [`Hull`] of `brush`.
    #[inline]
    #[must_use]
    pub fn insert_anchor_hull(&mut self, brush: &Brush, hull: &Hull) -> InsertResult
    {
        self.set_anchors_dirty();
        self.anchors_tree.insert_entity(brush, |_| *hull)
    }

    /// Removes the anchor [`Hull`] of the `brush`.
    #[inline]
    #[must_use]
    pub fn remove_anchor_hull(&mut self, brush: &Brush) -> bool
    {
        self.set_anchors_dirty();
        self.anchors_tree.remove_entity(brush)
    }

    /// Inserts the [`Hull`] of `brush`.
    #[inline]
    #[must_use]
    pub fn insert_brush_hull(&mut self, brush: &Brush) -> InsertResult
    {
        self.set_brushes_dirty();
        self.brushes_tree.insert_entity(brush, Brush::polygon_hull)
    }

    /// Removes the [`Hull`] of `brush`.
    #[inline]
    #[must_use]
    pub fn remove_brush_hull(&mut self, brush: &Brush) -> bool
    {
        self.set_brushes_dirty();
        self.brushes_tree.remove_entity(brush)
    }

    /// Inserts the [`Path`] [`Hull`] of `entity`.
    #[inline]
    #[must_use]
    pub fn insert_path_hull<T: EntityId + Moving>(&mut self, entity: &T) -> InsertResult
    {
        self.set_paths_dirty();
        self.paths_tree
            .insert_entity(entity, |entity| entity.path_hull().unwrap())
    }

    /// Removes the [`Path`] [`Hull`] of `entity`.
    #[inline]
    #[must_use]
    pub fn remove_path_hull<T: EntityId + ?Sized>(&mut self, entity: &T) -> bool
    {
        self.set_paths_dirty();
        self.paths_tree.remove_entity(entity)
    }

    #[inline]
    pub fn rebuild_sprite_quad_tree<'a, I: Iterator<Item = &'a Brush>>(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        iter: I
    )
    {
        self.sprites_tree.clear();

        for brush in iter
        {
            assert!(
                self.insert_sprite_hull(drawing_resources, grid, brush).inserted(),
                "Sprite hull was already in the quad tree."
            );
        }
    }

    /// Inserts the [`Hull`] of the sprite of `brush`.
    #[inline]
    #[must_use]
    pub fn insert_sprite_hull<T: TextureSize>(
        &mut self,
        resources: &T,
        grid: &Grid,
        brush: &Brush
    ) -> InsertResult
    {
        self.set_sprites_dirty();
        self.sprites_tree
            .insert_entity(brush, |brush| brush.sprite_and_anchor_hull(resources, grid).unwrap())
    }

    /// Removes the [`Hull`] of the sprite of `brush`.
    #[inline]
    #[must_use]
    pub fn remove_sprite_hull(&mut self, brush: &Brush) -> bool
    {
        self.set_sprites_dirty();
        self.sprites_tree.remove_entity(brush)
    }

    /// Inserts the [`Hull`] of `thing`.
    #[inline]
    #[must_use]
    pub fn insert_thing_hull(
        &mut self,
        things_catalog: &ThingsCatalog,
        thing: &ThingInstance
    ) -> InsertResult
    {
        self.set_things_dirty();
        self.things_tree
            .insert_entity(thing, |thing| thing.thing_hull(things_catalog))
    }

    /// Removes the [`Hull`] of `thing`.
    #[inline]
    #[must_use]
    pub fn remove_thing_hull(&mut self, thing: &ThingInstance) -> bool
    {
        self.set_things_dirty();
        self.things_tree.remove_entity(thing)
    }

    /// Marks the brush [`DirtyQuadTreeIdsNearPos`]es as dirty.
    #[inline]
    fn set_brushes_dirty(&mut self)
    {
        self.brushes_at_pos.write().unwrap().set_dirty();
        self.visible_brushes.write().unwrap().set_dirty();
    }

    /// Marks the paths [`DirtyQuadTreeIdsNearPos`]es as dirty.
    #[inline]
    fn set_paths_dirty(&mut self)
    {
        self.paths_at_pos.write().unwrap().set_dirty();
        self.visible_paths.write().unwrap().set_dirty();
    }

    /// Marks the sprites [`DirtyQuadTreeIdsNearPos`]es as dirty.
    #[inline]
    fn set_sprites_dirty(&mut self)
    {
        self.sprites_at_pos.write().unwrap().set_dirty();
        self.visible_sprites.write().unwrap().set_dirty();
    }

    /// Marks the things [`DirtyQuadTreeIdsNearPos`]es as dirty.
    #[inline]
    fn set_things_dirty(&mut self)
    {
        self.things_at_pos.write().unwrap().set_dirty();
        self.visible_things.write().unwrap().set_dirty();
    }

    /// Sets the attachments [`DirtyQuadTreeIdsNearPos`] as dirty.
    #[inline]
    pub fn set_anchors_dirty(&mut self) { self.visible_anchors.write().unwrap().set_dirty(); }

    /// Stores the [`Id`]s of the brushes at `cursor_pos` (or near it if `camera_scale` contains
    /// a value) and returns their container.
    #[inline]
    pub fn brushes_at_pos(&self, cursor_pos: Vec2, camera_scale: Option<f32>) -> IdsNearPos<'_>
    {
        self.brushes_at_pos.write().unwrap().update(
            cursor_pos,
            camera_scale,
            |ids, pos, camera_scale| {
                if let Some(scale) = camera_scale
                {
                    self.brushes_tree.entities_near_pos(ids, pos, scale);
                    return;
                }

                self.brushes_tree.entities_at_pos(ids, pos);
            }
        );

        self.brushes_at_pos.read().unwrap()
    }

    /// Stores the [`Id`]s of the brushes in `range` and returns their container.
    #[inline]
    pub fn brushes_in_range(&self, range: &Hull) -> Ids<'_>
    {
        self.brushes_tree
            .entities_in_range(&mut self.brushes_in_range.write().unwrap(), range);
        self.brushes_in_range.read().unwrap()
    }

    /// Stores the [`Id`]s of the entities that own the [`Path`]s at `cursor_pos` (or near it if
    /// `camera_scale` contains a value) and returns their container.
    #[inline]
    pub fn paths_at_pos(&self, cursor_pos: Vec2, camera_scale: f32) -> IdsNearPos<'_>
    {
        self.paths_at_pos.write().unwrap().update(
            cursor_pos,
            camera_scale.into(),
            |ids, pos, camera_scale| {
                self.paths_tree.entities_near_pos(ids, pos, camera_scale.unwrap());
            }
        );

        self.paths_at_pos.read().unwrap()
    }

    /// Stores the [`Id`]s of the [`ThingInstance`]s at `cursor_pos` (or near it if `camera_scale`
    /// contains a value) and returns their container.
    #[inline]
    pub fn sprites_at_pos(&self, cursor_pos: Vec2) -> IdsNearPos<'_>
    {
        self.sprites_at_pos
            .write()
            .unwrap()
            .update(cursor_pos, None, |ids, pos, _| {
                self.sprites_tree.entities_at_pos(ids, pos);
            });

        self.sprites_at_pos.read().unwrap()
    }

    /// Stores the [`Id`]s of the brushes that own the sprites in `range` and returns their
    /// container.
    #[inline]
    pub fn sprites_in_range(&self, range: &Hull) -> Ids<'_>
    {
        self.sprites_tree
            .entities_in_range(&mut self.sprites_in_range.write().unwrap(), range);
        self.sprites_in_range.read().unwrap()
    }

    /// Stores the [`Id`]s of the [`ThingInstance`]s at `cursor_pos` (or near it if `camera_scale`
    /// contains a value) and returns their container.
    #[inline]
    pub fn things_at_pos(&self, cursor_pos: Vec2, camera_scale: Option<f32>) -> IdsNearPos<'_>
    {
        self.things_at_pos.write().unwrap().update(
            cursor_pos,
            camera_scale,
            |ids, pos, camera_scale| {
                if let Some(scale) = camera_scale
                {
                    self.things_tree.entities_near_pos(ids, pos, scale);
                    return;
                }

                self.things_tree.entities_at_pos(ids, pos);
            }
        );

        self.things_at_pos.read().unwrap()
    }

    /// Stores the [`Id`]s of the [`ThingInstance`]s in `range` and returns their container.
    #[inline]
    pub fn things_in_range(&self, range: &Hull) -> Ids<'_>
    {
        self.things_tree
            .entities_in_range(&mut self.things_in_range.write().unwrap(), range);
        self.things_in_range.read().unwrap()
    }
}

//=======================================================================//

type Ids<'a> = RwLockReadGuard<'a, QuadTreeIds>;

//=======================================================================//

type IdsNearPos<'a> = RwLockReadGuard<'a, QuadTreeIdsNearPos>;

//=======================================================================//

/// A container of [`Id`]s of entities at a certain pos with a dirty flag.
pub(in crate::map::editor::state::manager) struct QuadTreeIdsNearPos
{
    /// The [`Id`]s.
    ids:               QuadTreeIds,
    /// The dirty flag.
    dirty:             bool,
    /// The last tested position.
    last_pos:          Vec2,
    /// The last tested camera scale, if any.
    last_camera_scale: Option<f32>
}

impl QuadTreeIdsNearPos
{
    /// Returns a new [`DirtyQuadTreeIdsNearPos`].
    #[inline]
    #[must_use]
    fn new() -> Self
    {
        Self {
            ids:               QuadTreeIds::new(),
            dirty:             true,
            last_pos:          Vec2::INFINITY,
            last_camera_scale: None
        }
    }

    #[inline]
    pub fn ids(&self) -> hashbrown::hash_map::Keys<'_, Id, Hull> { self.ids.ids() }

    /// Sets the dirty flag to true.
    #[inline]
    fn set_dirty(&mut self) { self.dirty = true; }

    /// Updates the contained [`Id`]s if necessary.
    #[inline]
    fn update<F: FnOnce(&mut QuadTreeIds, Vec2, Option<f32>)>(
        &mut self,
        pos: Vec2,
        camera_scale: Option<f32>,
        f: F
    )
    {
        if !self.dirty &&
            self.last_pos.around_equal_narrow(&pos) &&
            self.last_camera_scale == camera_scale
        {
            return;
        }

        self.last_pos = pos;
        self.last_camera_scale = camera_scale;
        self.ids.clear();

        f(&mut self.ids, self.last_pos, self.last_camera_scale);
        self.dirty = false;
    }
}

//=======================================================================//

type VisibleIds<'a> = RwLockReadGuard<'a, VisibleQuadTreeIds>;

//=======================================================================//

/// A container of [`Id`]s of visible entities with a dirty flag.
pub(in crate::map::editor::state::manager) struct VisibleQuadTreeIds
{
    /// The [`Id`]s.
    ids:           QuadTreeIds,
    /// The dirty flag.
    dirty:         bool,
    /// The last tested viewport.
    last_viewport: Hull
}

impl VisibleQuadTreeIds
{
    /// Returns a new [`VisibleQuadTreeIds`].
    #[inline]
    #[must_use]
    fn new() -> Self
    {
        Self {
            ids:           QuadTreeIds::new(),
            dirty:         true,
            last_viewport: Hull::new(MAP_SIZE, MAP_SIZE - 64f32, MAP_SIZE - 64f32, MAP_SIZE)
                .unwrap()
        }
    }

    #[inline]
    pub fn ids(&self) -> hashbrown::hash_map::Keys<'_, Id, Hull> { self.ids.ids() }

    /// Sets the dirty flag to true.
    #[inline]
    fn set_dirty(&mut self) { self.dirty = true; }

    /// Updates the contained [`Id`]s if necessary.
    #[inline]
    fn update<F: FnOnce(&mut QuadTreeIds, &Hull)>(
        &mut self,
        camera: &Transform,
        window: &Window,
        grid: &Grid,
        f: F
    )
    {
        let viewport = camera.viewport(window, grid);

        if !self.dirty && self.last_viewport.around_equal_narrow(&viewport)
        {
            return;
        }

        self.last_viewport = viewport;
        self.ids.clear();

        f(&mut self.ids, &self.last_viewport);
        self.dirty = false;
    }
}
