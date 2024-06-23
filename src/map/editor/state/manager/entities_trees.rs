//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::cell::{Ref, RefCell};

use bevy::prelude::{Transform, Vec2, Window};

use crate::{
    map::{
        brush::Brush,
        editor::state::manager::quad_tree::{QuadTree, QuadTreeIds},
        path::Moving,
        thing::ThingInstance
    },
    utils::{
        hull::{EntityHull, Hull},
        identifiers::{EntityId, Id},
        math::AroundEqual,
        misc::Camera
    }
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
            window: &Window
        ) -> Ref<'_, QuadTreeIds>
        {
            self.[< visible_ $entities >].borrow_mut().update(camera, window, |ids, viewport| {
                self.[< $entities _tree >]
                    .entities_intersect_range(ids, &viewport);
            });

            Ref::map(self.[< visible_ $entities >].borrow(), |v| &v.ids)
        }
)+ }}
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The [`QuadTrees`] used by the [`EntitiesManager`].
pub(in crate::map::editor::state::manager) struct Trees
{
    /// All [`Brush`]es.
    brushes_tree:     QuadTree,
    /// All [`Path`]s.
    paths_tree:       QuadTree,
    /// All [`Brush`] anchors.
    anchors_tree:     QuadTree,
    /// All sprites.
    sprites_tree:     QuadTree,
    /// All [`ThingInstance`]s.
    things_tree:      QuadTree,
    /// The [`Brush`]es at a certain position.
    brushes_at_pos:   RefCell<QuadTreeIdsNearPos>,
    /// The visible [`Brush`]es.
    visible_brushes:  RefCell<VisibleQuadTreeIds>,
    /// The [`Brush`]es in a certain range.
    brushes_in_range: RefCell<QuadTreeIds>,
    /// The visible [`Path`]s.
    visible_paths:    RefCell<VisibleQuadTreeIds>,
    /// The [`Path`]s at a certain pos.
    paths_at_pos:     RefCell<QuadTreeIdsNearPos>,
    /// The visible anchors.
    visible_anchors:  RefCell<VisibleQuadTreeIds>,
    /// The sprites at a certain position.
    sprites_at_pos:   RefCell<QuadTreeIdsNearPos>,
    /// The visible sprites.
    visible_sprites:  RefCell<VisibleQuadTreeIds>,
    /// The sprites in a certain range.
    sprites_in_range: RefCell<QuadTreeIds>,
    /// The [`ThingInstance`]s at a certain pos.
    things_at_pos:    RefCell<QuadTreeIdsNearPos>,
    /// The visible [`ThingInstance`].
    visible_things:   RefCell<VisibleQuadTreeIds>,
    /// The [`ThingInstance`] in a certain range.
    things_in_range:  RefCell<QuadTreeIds>
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
            sprites_tree:     QuadTree::new(),
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

    /// Inserts the anchor [`Hull`] of the [`Brush`] with [`Id`] `owner_id`.
    #[inline]
    pub fn insert_anchor_hull(&mut self, owner_id: Id, hull: &Hull)
    {
        self.anchors_tree.insert_hull(owner_id, hull);
        self.set_anchors_dirty();
    }

    /// Removes the anchor [`Hull`] of the [`Brush`] with [`Id`] `owner_id`.
    #[inline]
    pub fn remove_anchor_hull(&mut self, owner_id: Id, hull: &Hull)
    {
        self.anchors_tree.remove_hull(owner_id, hull);
        self.set_anchors_dirty();
    }

    /// Inserts the [`Hull`] of `brush`.
    #[inline]
    pub fn insert_brush_hull(&mut self, brush: &Brush)
    {
        self.brushes_tree.insert_entity(brush);
        self.set_brushes_dirty();
    }

    /// Removes the [`Hull`] of `brush`.
    #[inline]
    pub fn remove_brush_hull(&mut self, brush: &Brush)
    {
        self.brushes_tree.remove_entity(brush);
        self.set_brushes_dirty();
    }

    /// Replaces the [`Hull`] of the [`Brush`] with [`Id`] `identifier`.
    #[inline]
    pub fn replace_brush_hull(&mut self, identifier: Id, current_hull: &Hull, previous_hull: &Hull)
    {
        self.brushes_tree
            .replace_hull(identifier, current_hull, previous_hull);
        self.set_brushes_dirty();
    }

    /// Inserts the [`Path`] [`Hull`] of `entity`.
    #[inline]
    pub fn insert_path_hull<P: EntityId + Moving>(&mut self, entity: &P)
    {
        self.paths_tree.insert_hull(entity.id(), &entity.path_hull().unwrap());
        self.set_paths_dirty();
    }

    /// Removes the [`Path`] [`Hull`] of `entity`.
    #[inline]
    pub fn remove_path_hull<P: EntityId + ?Sized>(&mut self, entity: &P, hull: &Hull)
    {
        self.paths_tree.remove_hull(entity.id(), hull);
        self.set_paths_dirty();
    }

    /// Replaces the [`Path`] [`Hull`] of `entity`.
    #[inline]
    pub fn replace_path_hull<P: EntityId + Moving>(
        &mut self,
        entity: &P,
        current_hull: &Hull,
        previous_hull: &Hull
    )
    {
        self.paths_tree.replace_hull(entity.id(), current_hull, previous_hull);
        self.set_paths_dirty();
    }

    /// Inserts the [`Hull`] of the sprite of `brush`.
    #[inline]
    pub fn insert_sprite_hull(&mut self, brush: &Brush)
    {
        self.sprites_tree
            .insert_hull(brush.id(), &brush.sprite_and_anchor_hull().unwrap());
        self.set_sprites_dirty();
    }

    /// Removes the [`Hull`] of the sprite of `brush`.
    #[inline]
    pub fn remove_sprite_hull(&mut self, brush: &Brush, hull: &Hull)
    {
        self.sprites_tree.remove_hull(brush.id(), hull);
        self.set_sprites_dirty();
    }

    /// Replaces the [`Hull`] of the sprite of `brush`.
    #[inline]
    pub fn replace_sprite_hull(&mut self, brush: &Brush, current_hull: &Hull, previous_hull: &Hull)
    {
        self.sprites_tree
            .replace_hull(brush.id(), current_hull, previous_hull);
        self.set_sprites_dirty();
    }

    /// Inserts the [`Hull`] of `thing`.
    #[inline]
    pub fn insert_thing_hull(&mut self, thing: &ThingInstance)
    {
        self.things_tree.insert_hull(thing.id(), &thing.hull());
        self.set_things_dirty();
    }

    /// Removes the [`Hull`] of `thing`.
    #[inline]
    pub fn remove_thing_hull(&mut self, thing: &ThingInstance)
    {
        self.things_tree.remove_hull(thing.id(), &thing.hull());
        self.set_things_dirty();
    }

    /// Replaces the [`Hull`] of `thing`.
    #[inline]
    pub fn replace_thing_hull(&mut self, thing: &ThingInstance, previous_hull: &Hull)
    {
        self.things_tree
            .replace_hull(thing.id(), &thing.hull(), previous_hull);
        self.set_things_dirty();
    }

    /// Marks the brush [`DirtyQuadTreeIdsNearPos`]es as dirty.
    #[inline]
    fn set_brushes_dirty(&mut self)
    {
        self.brushes_at_pos.borrow_mut().set_dirty();
        self.visible_brushes.borrow_mut().set_dirty();
    }

    /// Marks the paths [`DirtyQuadTreeIdsNearPos`]es as dirty.
    #[inline]
    fn set_paths_dirty(&mut self)
    {
        self.paths_at_pos.borrow_mut().set_dirty();
        self.visible_paths.borrow_mut().set_dirty();
    }

    /// Marks the sprites [`DirtyQuadTreeIdsNearPos`]es as dirty.
    #[inline]
    fn set_sprites_dirty(&mut self)
    {
        self.sprites_at_pos.borrow_mut().set_dirty();
        self.visible_sprites.borrow_mut().set_dirty();
    }

    /// Marks the things [`DirtyQuadTreeIdsNearPos`]es as dirty.
    #[inline]
    fn set_things_dirty(&mut self)
    {
        self.things_at_pos.borrow_mut().set_dirty();
        self.visible_things.borrow_mut().set_dirty();
    }

    /// Sets the anchors [`DirtyQuadTreeIdsNearPos`] as dirty.
    #[inline]
    pub fn set_anchors_dirty(&mut self) { self.visible_anchors.borrow_mut().set_dirty(); }

    /// Stores the [`Id`]s of the [`Brush`]es at `cursor_pos` (or near it if `camera_scale` contains
    /// a value) and returns their container.
    #[inline]
    pub fn brushes_at_pos(
        &self,
        cursor_pos: Vec2,
        camera_scale: Option<f32>
    ) -> Ref<'_, QuadTreeIds>
    {
        self.brushes_at_pos.borrow_mut().update(
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

        Ref::map(self.brushes_at_pos.borrow(), |v| &v.ids)
    }

    /// Stores the [`Id`]s of the [`Brush`]es in `range` and returns their container.
    #[inline]
    pub fn brushes_in_range(&self, range: &Hull) -> Ref<'_, QuadTreeIds>
    {
        self.brushes_tree
            .entities_in_range(&mut self.brushes_in_range.borrow_mut(), range);
        self.brushes_in_range.borrow()
    }

    /// Stores the [`Id`]s of the entities that own the [`Path`]s at `cursor_pos` (or near it if
    /// `camera_scale` contains a value) and returns their container.
    #[inline]
    pub fn paths_at_pos(&self, cursor_pos: Vec2, camera_scale: f32) -> Ref<'_, QuadTreeIds>
    {
        self.paths_at_pos.borrow_mut().update(
            cursor_pos,
            camera_scale.into(),
            |ids, pos, camera_scale| {
                self.paths_tree.entities_near_pos(ids, pos, camera_scale.unwrap());
            }
        );

        Ref::map(self.paths_at_pos.borrow(), |v| &v.ids)
    }

    /// Stores the [`Id`]s of the [`ThingInstance`]s at `cursor_pos` (or near it if `camera_scale`
    /// contains a value) and returns their container.
    #[inline]
    pub fn sprites_at_pos(&self, cursor_pos: Vec2) -> Ref<'_, QuadTreeIds>
    {
        self.sprites_at_pos
            .borrow_mut()
            .update(cursor_pos, None, |ids, pos, _| {
                self.sprites_tree.entities_at_pos(ids, pos);
            });

        Ref::map(self.sprites_at_pos.borrow(), |v| &v.ids)
    }

    /// Stores the [`Id`]s of the [`Brush`]es that own the sprites in `range` and returns their
    /// container.
    #[inline]
    pub fn sprites_in_range(&self, range: &Hull) -> Ref<'_, QuadTreeIds>
    {
        self.sprites_tree
            .entities_in_range(&mut self.sprites_in_range.borrow_mut(), range);
        self.sprites_in_range.borrow()
    }

    /// Stores the [`Id`]s of the [`ThingInstance`]s at `cursor_pos` (or near it if `camera_scale`
    /// contains a value) and returns their container.
    #[inline]
    pub fn things_at_pos(&self, cursor_pos: Vec2, camera_scale: Option<f32>)
        -> Ref<'_, QuadTreeIds>
    {
        self.things_at_pos.borrow_mut().update(
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

        Ref::map(self.things_at_pos.borrow(), |v| &v.ids)
    }

    /// Stores the [`Id`]s of the [`ThingInstance`]s in `range` and returns their container.
    #[inline]
    pub fn things_in_range(&self, range: &Hull) -> Ref<'_, QuadTreeIds>
    {
        self.things_tree
            .entities_in_range(&mut self.things_in_range.borrow_mut(), range);
        self.things_in_range.borrow()
    }

    //==============================================================
    // Draw

    #[cfg(feature = "debug")]
    /// Draws the [`QuadTree`]s of the [`Brush`]es and sprites.
    #[inline]
    pub fn draw(&self, gizmos: &mut bevy::prelude::Gizmos, viewport: &Hull, camera_scale: f32)
    {
        self.brushes_tree.draw(gizmos, viewport, camera_scale);
        self.sprites_tree.draw(gizmos, viewport, camera_scale);
    }
}

//=======================================================================//

/// A container of [`Id`]s of entities at a certain pos with a dirty flag.
#[derive(Debug)]
struct QuadTreeIdsNearPos
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

    /// Sets the dirty flag to true.
    #[inline]
    fn set_dirty(&mut self) { self.dirty = true; }

    /// Updates the contained [`Id`]s if necessary.
    #[inline]
    pub fn update<F: FnMut(&mut QuadTreeIds, Vec2, Option<f32>)>(
        &mut self,
        pos: Vec2,
        camera_scale: Option<f32>,
        mut f: F
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

/// A container of [`Id`]s of visible entities with a dirty flag.
#[derive(Debug)]
struct VisibleQuadTreeIds
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
            last_viewport: Hull::new(
                f32::INFINITY,
                f32::INFINITY - 64f32,
                f32::INFINITY - 64f32,
                f32::INFINITY
            )
        }
    }

    /// Sets the dirty flag to true.
    #[inline]
    fn set_dirty(&mut self) { self.dirty = true; }

    /// Updates the contained [`Id`]s if necessary.
    #[inline]
    pub fn update<F: FnMut(&mut QuadTreeIds, &Hull)>(
        &mut self,
        camera: &Transform,
        window: &Window,
        mut f: F
    )
    {
        /// Generates the world viewport of the `camera`.
        #[inline]
        #[must_use]
        fn viewport(camera: &Transform, window: &Window) -> Hull
        {
            /// Extra space for improved visibility detection.
            const PADDING: f32 = 64f32;

            let hull = camera.viewport_ui_constricted(window);
            let scaled_increment = PADDING * camera.scale();

            Hull::new(
                hull.top() + scaled_increment,
                hull.bottom() - scaled_increment,
                hull.left() - scaled_increment,
                hull.right() + scaled_increment
            )
        }

        let viewport = viewport(camera, window);

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
