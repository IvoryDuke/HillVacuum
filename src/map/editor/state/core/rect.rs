//=======================================================================//
// IMPORTS
//
//=======================================================================//

use glam::Vec2;
use hill_vacuum_shared::return_if_no_match;

use crate::{
    map::editor::ToolUpdateBundle,
    utils::{hull::Hull, identifiers::EntityId, misc::Camera}
};

//=======================================================================//
// MACROS
//
//=======================================================================//

macro_rules! impl_update {
    () => {
        #[inline]
        #[must_use]
        fn drag_selection<'a, U, N, I, F, E>(
            &mut self,
            bundle: &mut ToolUpdateBundle,
            cursor_pos: Vec2,
            extra: E,
            mut n: N,
            mut i: I,
            mut f: F
        ) -> Option<U>
        where
            E: 'a,
            N: FnMut(&mut Self, &mut ToolUpdateBundle, E) -> LeftMouse<U>,
            I: FnMut(&mut ToolUpdateBundle, E) -> Option<U>,
            F: FnMut(&mut ToolUpdateBundle, &Hull, E) -> Option<U>
        {
            match self.core()
            {
                RectCore::None =>
                {
                    match n(self, bundle, extra).into()
                    {
                        LeftMouse::Value(v) => return v.into(),
                        LeftMouse::Pressed => self.update_extremes(bundle.camera, cursor_pos),
                        LeftMouse::NotPressed => ()
                    };
                },
                RectCore::Initiated(_) =>
                {
                    if !bundle.inputs.left_mouse.pressed()
                    {
                        let value = i(bundle, extra);
                        self.reset();
                        return value;
                    }

                    self.update_extremes(bundle.camera, cursor_pos);
                },
                RectCore::Formed(..) =>
                {
                    if !bundle.inputs.left_mouse.pressed()
                    {
                        let value = f(bundle, &self.hull().unwrap(), extra);
                        self.reset();
                        return value;
                    }

                    self.update_extremes(bundle.camera, cursor_pos);
                }
            };

            None
        }
    };
}

//=======================================================================//
// TRAITS
//
//=======================================================================//

/// A trait for structs that are built around a [`Rect`].
pub(crate) trait RectTrait
{
    /// Returns a new `Self` created from just an origin point.
    #[must_use]
    fn from_origin(origin: Vec2) -> Self
    where
        Self: Sized;

    /// Returns the origin of the surface, if any.
    #[must_use]
    fn origin(&self) -> Option<Vec2>;

    /// Returns the point opposite to the origin, if any.
    #[must_use]
    fn extreme(&self) -> Option<Vec2>;

    /// Returns the [`Hull`] representing the surface of the drag area, if any.
    #[must_use]
    fn hull(&self) -> Option<Hull>;

    /// Updates the extremities of the surface from `p`.
    fn update_extremes<T: Camera>(&mut self, camera: &T, p: Vec2);

    #[must_use]
    fn drag_selection<'a, U, N, I, F, E>(
        &mut self,
        bundle: &mut ToolUpdateBundle,
        cursor_pos: Vec2,
        extra: E,
        n: N,
        i: I,
        f: F
    ) -> Option<U>
    where
        E: 'a,
        N: FnMut(&mut Self, &mut ToolUpdateBundle, E) -> LeftMouse<U>,
        I: FnMut(&mut ToolUpdateBundle, E) -> Option<U>,
        F: FnMut(&mut ToolUpdateBundle, &Hull, E) -> Option<U>;
}

//=======================================================================//

trait RectPrivate
{
    fn core(&self) -> &RectCore;

    /// Whether `self` represents a formed drag area.
    #[must_use]
    fn formed(&self) -> bool;

    /// Resets the drag area.
    fn reset(&mut self);
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The core of a [`Rect`].
#[must_use]
#[derive(Clone, Copy, Default)]
pub(in crate::map::editor::state::core) enum RectCore
{
    /// No area.
    #[default]
    None,
    /// Just the starting point.
    Initiated(Vec2),
    /// A rectangular surface.
    Formed(Vec2, Vec2)
}

//=======================================================================//

pub(in crate::map::editor::state::core) enum LeftMouse<T>
{
    Value(T),
    Pressed,
    NotPressed
}

impl<T> From<bool> for LeftMouse<T>
{
    #[inline]
    fn from(value: bool) -> Self
    {
        #[allow(clippy::match_bool)]
        match value
        {
            true => Self::Pressed,
            false => Self::NotPressed
        }
    }
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// A rectangular area generated from two points that can be empty.
#[must_use]
#[derive(Clone, Copy, Default)]
pub(in crate::map::editor::state::core) struct Rect(RectCore);

impl RectPrivate for Rect
{
    #[inline]
    fn core(&self) -> &RectCore { &self.0 }

    #[inline]
    fn formed(&self) -> bool { matches!(self.0, RectCore::Formed(..)) }

    #[inline]
    fn reset(&mut self) { *self = Rect::default(); }
}

impl RectTrait for Rect
{
    impl_update!();

    #[inline]
    fn from_origin(origin: Vec2) -> Self { Self(RectCore::Initiated(origin)) }

    #[inline]
    fn origin(&self) -> Option<Vec2>
    {
        return_if_no_match!(self.0, RectCore::Initiated(o) | RectCore::Formed(o, _), Some(o), None)
    }

    #[inline]
    fn extreme(&self) -> Option<Vec2>
    {
        return_if_no_match!(self.0, RectCore::Formed(_, e), Some(e), None)
    }

    #[inline]
    fn hull(&self) -> Option<Hull>
    {
        match self.0
        {
            RectCore::None | RectCore::Initiated(_) => None,
            RectCore::Formed(o, e) => Some(Hull::from_opposite_vertexes(o, e))
        }
    }

    #[inline]
    fn update_extremes<T: Camera>(&mut self, _: &T, p: Vec2)
    {
        match &mut self.0
        {
            RectCore::None =>
            {
                *self = Self(RectCore::Initiated(p));
            },
            RectCore::Initiated(o) =>
            {
                *self = Self(RectCore::Formed(*o, p));
            },
            RectCore::Formed(_, e) => *e = p
        };
    }
}

//=======================================================================//

/// A [`Rect`] that can store an [`Id`] that represents an entity to highlight.
#[must_use]
#[derive(Clone, Copy)]
pub(in crate::map::editor::state::core) struct RectHighlightedEntity<T>(Rect, Option<T>)
where
    T: EntityId + Clone + Copy;

impl<T> Default for RectHighlightedEntity<T>
where
    T: EntityId + Clone + Copy
{
    fn default() -> Self { Self(Rect::default(), None) }
}

impl<T> From<Option<T>> for RectHighlightedEntity<T>
where
    T: EntityId + Clone + Copy
{
    #[inline]
    fn from(value: Option<T>) -> Self { Self(Rect::default(), value) }
}

impl<T> From<RectHighlightedEntity<T>> for Rect
where
    T: EntityId + Clone + Copy
{
    #[inline]
    fn from(value: RectHighlightedEntity<T>) -> Self { value.0 }
}

impl<T> From<Rect> for RectHighlightedEntity<T>
where
    T: EntityId + Clone + Copy
{
    #[inline]
    fn from(value: Rect) -> Self { Self(value, None) }
}

impl<T> RectPrivate for RectHighlightedEntity<T>
where
    T: EntityId + Clone + Copy
{
    #[inline]
    fn reset(&mut self) { self.0.reset(); }

    #[inline]
    fn core(&self) -> &RectCore { &self.0 .0 }

    #[inline]
    fn formed(&self) -> bool { self.0.formed() }
}

impl<T> RectTrait for RectHighlightedEntity<T>
where
    T: EntityId + Clone + Copy
{
    impl_update!();

    #[inline]
    fn from_origin(origin: Vec2) -> Self { Self(Rect::from_origin(origin), None) }

    #[inline]
    fn origin(&self) -> Option<Vec2> { self.0.origin() }

    #[inline]
    fn extreme(&self) -> Option<Vec2> { self.0.extreme() }

    #[inline]
    fn hull(&self) -> Option<crate::utils::hull::Hull> { self.0.hull() }

    #[inline]
    fn update_extremes<U: Camera>(&mut self, camera: &U, p: Vec2)
    {
        if let Some(o) = self.origin()
        {
            let delta = camera.scale() * 2f32;

            if (o.x - p.x).abs() < delta && (o.y - p.y).abs() < delta
            {
                return;
            }
        }

        self.0.update_extremes(camera, p);

        if self.0.formed()
        {
            self.1 = None;
        }
    }
}

impl<T> RectHighlightedEntity<T>
where
    T: EntityId + Clone + Copy
{
    /// Returns the highlighted entity, if any.
    #[inline]
    #[must_use]
    pub const fn highlighted_entity(&self) -> Option<T> { self.1 }

    /// Whether there is an highlighted entity.
    #[inline]
    #[must_use]
    pub const fn has_highlighted_entity(&self) -> bool { self.1.is_some() }

    /// Sets the highlighted entity, if any.
    #[inline]
    pub fn set_highlighted_entity(&mut self, entity: Option<T>) { self.1 = entity; }
}
