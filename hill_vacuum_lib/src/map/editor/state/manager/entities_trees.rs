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

macro_rules! visible_iters {
    ($($ent:ident),+) => { paste::paste! { $(
        #[inline]
        pub fn [< visible_ $ent >](
            &self,
            camera: &Transform,
            window: &Window
        ) -> Ref<'_, QuadTreeIds>
        {
            self.[< visible_ $ent >].borrow_mut().update(camera, window, |ids, viewport| {
                self.[< $ent _tree >]
                    .entities_intersect_range(ids, &viewport);
            });

            Ref::map(self.[< visible_ $ent >].borrow(), |v| &v.ids)
        }
)+ }}
}

//=======================================================================//
// TYPES
//
//=======================================================================//

pub(in crate::map::editor::state::manager) struct Trees
{
    brushes_tree:              QuadTree,
    paths_tree:                QuadTree,
    anchors_tree:              QuadTree,
    sprites_tree:              QuadTree,
    sprite_highlights_tree:    QuadTree,
    things_tree:               QuadTree,
    brushes_at_pos:            RefCell<DirtyQuadTreeIdsNearPos>,
    visible_brushes:           RefCell<VisibleQuadTreeIds>,
    brushes_in_range:          RefCell<QuadTreeIds>,
    brushes_intersect_range:   RefCell<QuadTreeIds>,
    visible_paths:             RefCell<VisibleQuadTreeIds>,
    paths_at_pos:              RefCell<DirtyQuadTreeIdsNearPos>,
    visible_anchors:           RefCell<VisibleQuadTreeIds>,
    sprites_at_pos:            RefCell<DirtyQuadTreeIdsNearPos>,
    visible_sprites:           RefCell<VisibleQuadTreeIds>,
    visible_sprite_highlights: RefCell<VisibleQuadTreeIds>,
    sprites_in_range:          RefCell<QuadTreeIds>,
    things_at_pos:             RefCell<DirtyQuadTreeIdsNearPos>,
    visible_things:            RefCell<VisibleQuadTreeIds>,
    things_in_range:           RefCell<QuadTreeIds>
}

impl Trees
{
    visible_iters!(brushes, paths, sprites, anchors, sprite_highlights, things);

    #[inline]
    #[must_use]
    pub fn new() -> Self
    {
        Self {
            brushes_tree:              QuadTree::new(),
            paths_tree:                QuadTree::new(),
            anchors_tree:              QuadTree::new(),
            sprites_tree:              QuadTree::new(),
            sprite_highlights_tree:    QuadTree::new(),
            brushes_at_pos:            DirtyQuadTreeIdsNearPos::new().into(),
            visible_brushes:           VisibleQuadTreeIds::new().into(),
            brushes_in_range:          QuadTreeIds::new().into(),
            brushes_intersect_range:   QuadTreeIds::new().into(),
            visible_paths:             VisibleQuadTreeIds::new().into(),
            paths_at_pos:              DirtyQuadTreeIdsNearPos::new().into(),
            visible_anchors:           VisibleQuadTreeIds::new().into(),
            sprites_at_pos:            DirtyQuadTreeIdsNearPos::new().into(),
            visible_sprites:           VisibleQuadTreeIds::new().into(),
            visible_sprite_highlights: VisibleQuadTreeIds::new().into(),
            sprites_in_range:          QuadTreeIds::new().into(),
            things_tree:               QuadTree::new(),
            things_at_pos:             DirtyQuadTreeIdsNearPos::new().into(),
            visible_things:            VisibleQuadTreeIds::new().into(),
            things_in_range:           QuadTreeIds::new().into()
        }
    }

    #[inline]
    pub fn insert_anchor_hull(&mut self, owner_id: Id, hull: &Hull)
    {
        self.anchors_tree.insert_hull(owner_id, hull);
        self.set_anchors_dirty();
    }

    #[inline]
    pub fn remove_anchor_hull(&mut self, owner_id: Id, hull: &Hull)
    {
        self.anchors_tree.remove_hull(owner_id, hull);
        self.set_anchors_dirty();
    }

    #[inline]
    pub fn insert_brush_hull(&mut self, brush: &Brush)
    {
        self.brushes_tree.insert_entity(brush);
        self.set_brushes_dirty();
    }

    #[inline]
    pub fn remove_brush_hull(&mut self, brush: &Brush)
    {
        self.brushes_tree.remove_entity(brush);
        self.set_brushes_dirty();
    }

    #[inline]
    pub fn replace_brush_hull(&mut self, identifier: Id, current_hull: &Hull, previous_hull: &Hull)
    {
        self.brushes_tree
            .replace_hull(identifier, current_hull, previous_hull);
        self.set_brushes_dirty();
    }

    #[inline]
    pub fn insert_path_hull(&mut self, brush: &Brush)
    {
        self.paths_tree.insert_hull(brush.id(), &brush.path_hull().unwrap());
        self.set_paths_dirty();
    }

    #[inline]
    pub fn remove_path_hull(&mut self, brush: &Brush, hull: &Hull)
    {
        self.paths_tree.remove_hull(brush.id(), hull);
        self.set_paths_dirty();
    }

    #[inline]
    pub fn replace_path_hull(&mut self, brush: &Brush, current_hull: &Hull, previous_hull: &Hull)
    {
        self.paths_tree.replace_hull(brush.id(), current_hull, previous_hull);
        self.set_paths_dirty();
    }

    #[inline]
    pub fn insert_sprite_hull(&mut self, brush: &Brush)
    {
        self.sprites_tree
            .insert_hull(brush.id(), &brush.sprite_hull().unwrap());
        self.sprite_highlights_tree
            .insert_hull(brush.id(), &brush.sprite_anchor_hull().unwrap());
        self.set_sprites_dirty();
    }

    #[inline]
    pub fn remove_sprite_hull(&mut self, brush: &Brush, hull: &(Hull, Hull))
    {
        self.sprites_tree.remove_hull(brush.id(), &hull.0);
        self.sprite_highlights_tree.remove_hull(brush.id(), &hull.1);
        self.set_sprites_dirty();
    }

    #[inline]
    pub fn replace_sprite_hull(
        &mut self,
        brush: &Brush,
        current_hull: &(Hull, Hull),
        previous_hull: &(Hull, Hull)
    )
    {
        self.sprites_tree
            .replace_hull(brush.id(), &current_hull.0, &previous_hull.0);
        self.sprite_highlights_tree
            .replace_hull(brush.id(), &current_hull.1, &previous_hull.1);
        self.set_sprites_dirty();
    }

    #[inline]
    pub fn replace_sprite_anchor_hull(
        &mut self,
        brush: &Brush,
        current_hull: &Hull,
        previous_hull: &Hull
    )
    {
        self.sprite_highlights_tree
            .replace_hull(brush.id(), current_hull, previous_hull);
        self.visible_sprite_highlights.borrow_mut().set_dirty();
    }

    #[inline]
    pub fn insert_thing_hull(&mut self, thing: &ThingInstance)
    {
        self.things_tree.insert_hull(thing.id(), &thing.hull());
        self.set_things_dirty();
    }

    #[inline]
    pub fn remove_thing_hull(&mut self, thing: &ThingInstance)
    {
        self.things_tree.remove_hull(thing.id(), &thing.hull());
        self.set_things_dirty();
    }

    #[inline]
    pub fn replace_thing_hull(&mut self, thing: &ThingInstance, previous_hull: &Hull)
    {
        self.things_tree
            .replace_hull(thing.id(), &thing.hull(), previous_hull);
        self.set_things_dirty();
    }

    #[inline]
    pub fn set_brushes_dirty(&mut self)
    {
        self.brushes_at_pos.borrow_mut().set_dirty();
        self.visible_brushes.borrow_mut().set_dirty();
    }

    #[inline]
    pub fn set_paths_dirty(&mut self)
    {
        self.paths_at_pos.borrow_mut().set_dirty();
        self.visible_paths.borrow_mut().set_dirty();
    }

    #[inline]
    pub fn set_sprites_dirty(&mut self)
    {
        self.sprites_at_pos.borrow_mut().set_dirty();
        self.visible_sprites.borrow_mut().set_dirty();
        self.visible_sprite_highlights.borrow_mut().set_dirty();
    }

    #[inline]
    pub fn set_things_dirty(&mut self)
    {
        self.things_at_pos.borrow_mut().set_dirty();
        self.visible_things.borrow_mut().set_dirty();
    }

    #[inline]
    pub fn set_anchors_dirty(&mut self) { self.visible_anchors.borrow_mut().set_dirty(); }

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

    #[inline]
    pub fn brushes_in_range(&self, range: &Hull) -> Ref<'_, QuadTreeIds>
    {
        self.brushes_tree
            .entities_in_range(&mut self.brushes_in_range.borrow_mut(), range);
        self.brushes_in_range.borrow()
    }

    #[inline]
    pub fn brushes_intersect_range(&self, range: &Hull) -> Ref<'_, QuadTreeIds>
    {
        self.brushes_tree
            .entities_intersect_range(&mut self.brushes_intersect_range.borrow_mut(), range);
        self.brushes_intersect_range.borrow()
    }

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

    #[inline]
    pub fn paths_intersect_range(&self, range: &Hull) -> Ref<'_, QuadTreeIds>
    {
        self.paths_tree
            .entities_intersect_range(&mut self.brushes_intersect_range.borrow_mut(), range);
        self.brushes_intersect_range.borrow()
    }

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

    #[inline]
    pub fn sprites_in_range(&self, range: &Hull) -> Ref<'_, QuadTreeIds>
    {
        self.sprites_tree
            .entities_in_range(&mut self.sprites_in_range.borrow_mut(), range);
        self.sprites_in_range.borrow()
    }

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
    #[inline]
    pub fn draw(&self, gizmos: &mut bevy::prelude::Gizmos, viewport: &Hull, camera_scale: f32)
    {
        self.brushes_tree.draw(gizmos, viewport, camera_scale);
        self.sprites_tree.draw(gizmos, viewport, camera_scale);
    }
}

//=======================================================================//

#[derive(Debug)]
struct DirtyQuadTreeIdsNearPos
{
    ids:               QuadTreeIds,
    dirty:             bool,
    last_pos:          Vec2,
    last_camera_scale: Option<f32>
}

impl DirtyQuadTreeIdsNearPos
{
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
    fn set_dirty(&mut self) { self.dirty = true; }

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

#[derive(Debug)]
struct VisibleQuadTreeIds
{
    ids:           QuadTreeIds,
    dirty:         bool,
    last_viewport: Hull
}

impl VisibleQuadTreeIds
{
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

    #[inline]
    fn set_dirty(&mut self) { self.dirty = true; }

    #[inline]
    pub fn update<F: FnMut(&mut QuadTreeIds, &Hull)>(
        &mut self,
        camera: &Transform,
        window: &Window,
        mut f: F
    )
    {
        #[inline]
        #[must_use]
        fn viewport(camera: &Transform, window: &Window) -> Hull
        {
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
