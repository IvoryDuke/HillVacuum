//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::Vec2;
use hill_vacuum_shared::return_if_no_match;

use crate::utils::{hull::Hull, identifiers::EntityId};

//=======================================================================//
// MACROS
//
//=======================================================================//

/// Updates `rect`.
macro_rules! update {
    (
        $rect:expr,
        $p:expr,
        $camera_scale:expr,
        $left_mouse_pressed:expr,
        $none:expr,
        $initiated:block,
        $hull:ident,
        $formed:block
    ) => {
        if $rect.none()
        {
            if $none
            {
                $rect.update_extremes($p, $camera_scale);
            }
        }
        else if $rect.initiated()
        {
            $rect.update_extremes($p, $camera_scale);

            if !$left_mouse_pressed
            {
                $initiated;
                $rect.reset();
            }
        }
        else
        {
            $rect.update_extremes($p, $camera_scale);

            if !$left_mouse_pressed
            {
                let $hull = $rect.hull().unwrap();
                $formed;
                $rect.reset();
            }
        }
    };
}

pub(in crate::map::editor::state::core) use update;

//=======================================================================//
// TRAITS
//
//=======================================================================//

/// A trait for structs that are built around a [`Rect`].
pub trait RectTrait
{
    /// Returns a new `Self` created from just an origin point.
    #[must_use]
    fn from_origin(origin: Vec2) -> Self
    where
        Self: Sized;

    /// Returns a new `Self` created from two points, if valid.
    #[must_use]
    fn from_extremes(origin: Vec2, extreme: Vec2, camera_scale: f32) -> Option<Self>
    where
        Self: Sized;

    /// Whever `self` represents an uninitiated drag area.
    #[must_use]
    fn none(&self) -> bool;

    /// Whever `self` represents an initiated drag area.
    #[must_use]
    fn initiated(&self) -> bool;

    /// Whever `self` represents a formed drag area.
    #[must_use]
    fn formed(&self) -> bool;

    /// Returns the origin of the surface, if any.
    #[must_use]
    fn origin(&self) -> Option<Vec2>;

    /// Returns the point opposite to the origin, if any.
    #[must_use]
    fn extreme(&self) -> Option<Vec2>;

    /// Returns the [`Hull`] representing the surface of the drag area, if any.
    #[must_use]
    fn hull(&self) -> Option<Hull>;

    /// Updates the extremeties of the surface from `p`.
    fn update_extremes(&mut self, p: Vec2, camera_scale: f32);

    /// Resets the drag area.
    fn reset(&mut self);
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The core of a [`Rect`].
#[derive(Clone, Copy, Debug, Default)]
enum RectCore
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
// TYPES
//
//=======================================================================//

/// A rectangular area generated from two points that can be empty.
#[must_use]
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::map::editor::state::core) struct Rect(RectCore);

impl RectTrait for Rect
{
    //==============================================================
    // New

    #[inline]
    fn from_origin(origin: Vec2) -> Self { Self(RectCore::Initiated(origin)) }

    #[inline]
    fn from_extremes(origin: Vec2, extreme: Vec2, camera_scale: f32) -> Option<Self>
    {
        valid_points(origin, extreme, camera_scale)
            .then_some(Self(RectCore::Formed(origin, extreme)))
    }

    //==============================================================
    // Info

    #[inline]
    fn none(&self) -> bool { matches!(self.0, RectCore::None) }

    #[inline]
    fn initiated(&self) -> bool { matches!(self.0, RectCore::Initiated(_)) }

    #[inline]
    fn formed(&self) -> bool { matches!(self.0, RectCore::Formed(..)) }

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
            RectCore::Formed(o, e) => Some(Hull::from_opposite_vertexes(o, e).unwrap())
        }
    }

    //==============================================================
    // Update

    #[inline]
    fn update_extremes(&mut self, p: Vec2, camera_scale: f32)
    {
        match &mut self.0
        {
            RectCore::None =>
            {
                *self = Self(RectCore::Initiated(p));
            },
            RectCore::Initiated(o) =>
            {
                if valid_points(*o, p, camera_scale)
                {
                    *self = Self(RectCore::Formed(*o, p));
                }
            },
            RectCore::Formed(o, e) =>
            {
                if valid_points(*o, p, camera_scale)
                {
                    *e = p;
                }
            }
        };
    }

    #[inline]
    fn reset(&mut self) { *self = Rect::default(); }
}

//=======================================================================//

/// A [`Rect`] that can store an [`Id`] that represents an entity to highlight.
#[must_use]
#[derive(Clone, Copy, Debug)]
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

impl<T> RectTrait for RectHighlightedEntity<T>
where
    T: EntityId + Clone + Copy
{
    #[inline]
    fn from_origin(origin: Vec2) -> Self { Self(Rect::from_origin(origin), None) }

    #[inline]
    fn from_extremes(origin: Vec2, extreme: Vec2, camera_scale: f32) -> Option<Self>
    {
        Rect::from_extremes(origin, extreme, camera_scale).map(|da| Self(da, None))
    }

    #[inline]
    fn origin(&self) -> Option<Vec2> { self.0.origin() }

    #[inline]
    fn extreme(&self) -> Option<Vec2> { self.0.extreme() }

    #[inline]
    fn hull(&self) -> Option<crate::utils::hull::Hull> { self.0.hull() }

    #[inline]
    fn update_extremes(&mut self, p: Vec2, camera_scale: f32)
    {
        self.0.update_extremes(p, camera_scale);

        if self.0.formed()
        {
            self.1 = None;
        }
    }

    #[inline]
    fn reset(&mut self) { self.0.reset(); }

    #[inline]
    fn none(&self) -> bool { self.0.none() }

    #[inline]
    fn initiated(&self) -> bool { self.0.initiated() }

    #[inline]
    fn formed(&self) -> bool { self.0.formed() }
}

impl<T> RectHighlightedEntity<T>
where
    T: EntityId + Clone + Copy
{
    /// Returns the highlighted entity, if any.
    #[inline]
    #[must_use]
    pub const fn highlighted_entity(&self) -> Option<T> { self.1 }

    /// Whever there is an highlighted entity.
    #[inline]
    #[must_use]
    pub const fn has_highlighted_entity(&self) -> bool { self.1.is_some() }

    /// Sets the highlighted entity, if any.
    #[inline]
    pub fn set_highlighted_entity(&mut self, entity: Option<T>) { self.1 = entity; }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Whever `a` and `b` are valid points to generate a [`Rect`].
#[inline]
#[must_use]
fn valid_points(a: Vec2, b: Vec2, camera_scale: f32) -> bool
{
    (a.x - b.x).abs() * camera_scale >= 2f32 && (a.y - b.y).abs() * camera_scale >= 2f32
}
