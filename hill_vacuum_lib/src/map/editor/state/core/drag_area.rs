//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::Vec2;
use shared::return_if_no_match;

use crate::utils::{hull::Hull, identifiers::EntityId};

//=======================================================================//
// MACROS
//
//=======================================================================//

macro_rules! update {
    (
        $drag_area:expr,
        $p:expr,
        $left_mouse_pressed:expr,
        $none:expr,
        $initiated:block,
        $hull:ident,
        $formed:block
    ) => {
        if $drag_area.none()
        {
            if $none
            {
                $drag_area.update_extremes($p);
            }
        }
        else if $drag_area.initiated()
        {
            $drag_area.update_extremes($p);

            if !$left_mouse_pressed
            {
                $initiated;
                $drag_area.reset();
            }
        }
        else
        {
            $drag_area.update_extremes($p);

            if !$left_mouse_pressed
            {
                let $hull = $drag_area.hull().unwrap();
                $formed;
                $drag_area.reset();
            }
        }
    };
}

pub(in crate::map::editor::state::core) use update;

//=======================================================================//
// TRAITS
//
//=======================================================================//

#[allow(clippy::module_name_repetitions)]
pub trait DragAreaTrait
{
    #[must_use]
    fn from_origin(origin: Vec2) -> Self
    where
        Self: Sized;

    #[must_use]
    fn from_extremes(origin: Vec2, extreme: Vec2) -> Option<Self>
    where
        Self: Sized;

    #[must_use]
    fn none(&self) -> bool;

    #[must_use]
    fn initiated(&self) -> bool;

    #[must_use]
    fn formed(&self) -> bool;

    #[must_use]
    fn origin(&self) -> Option<Vec2>;

    #[must_use]
    fn extreme(&self) -> Option<Vec2>;

    #[must_use]
    fn hull(&self) -> Option<Hull>;

    fn update_extremes(&mut self, p: Vec2);

    fn reset(&mut self);
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[derive(Clone, Copy, Debug, Default)]
enum DragAreaCore
{
    #[default]
    None,
    Initiated(Vec2),
    Formed(Vec2, Vec2)
}

//=======================================================================//
// TYPES
//
//=======================================================================//

#[must_use]
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::map::editor::state::core) struct DragArea(DragAreaCore);

impl DragAreaTrait for DragArea
{
    //==============================================================
    // New

    #[inline]
    fn from_origin(origin: Vec2) -> Self { Self(DragAreaCore::Initiated(origin)) }

    #[inline]
    fn from_extremes(origin: Vec2, extreme: Vec2) -> Option<Self>
    {
        valid_points(origin, extreme).then_some(Self(DragAreaCore::Formed(origin, extreme)))
    }

    //==============================================================
    // Info

    #[inline]
    fn none(&self) -> bool { matches!(self.0, DragAreaCore::None) }

    #[inline]
    fn initiated(&self) -> bool { matches!(self.0, DragAreaCore::Initiated(_)) }

    #[inline]
    fn formed(&self) -> bool { matches!(self.0, DragAreaCore::Formed(..)) }

    #[inline]
    fn origin(&self) -> Option<Vec2>
    {
        return_if_no_match!(
            self.0,
            DragAreaCore::Initiated(o) | DragAreaCore::Formed(o, _),
            Some(o),
            None
        )
    }

    #[inline]
    fn extreme(&self) -> Option<Vec2>
    {
        return_if_no_match!(self.0, DragAreaCore::Formed(_, e), Some(e), None)
    }

    #[inline]
    fn hull(&self) -> Option<Hull>
    {
        match self.0
        {
            DragAreaCore::None | DragAreaCore::Initiated(_) => None,
            DragAreaCore::Formed(o, e) => Some(Hull::from_opposite_vertexes(o, e).unwrap())
        }
    }

    //==============================================================
    // Update

    #[inline]
    fn update_extremes(&mut self, p: Vec2)
    {
        match &mut self.0
        {
            DragAreaCore::None =>
            {
                *self = Self(DragAreaCore::Initiated(p));
            },
            DragAreaCore::Initiated(o) =>
            {
                if valid_points(*o, p)
                {
                    *self = Self(DragAreaCore::Formed(*o, p));
                }
            },
            DragAreaCore::Formed(o, e) =>
            {
                if valid_points(*o, p)
                {
                    *e = p;
                }
            }
        };
    }

    #[inline]
    fn reset(&mut self) { *self = DragArea::default(); }
}

//=======================================================================//

#[must_use]
#[derive(Clone, Copy, Debug)]
pub(in crate::map::editor::state::core) struct DragAreaHighlightedEntity<T>(DragArea, Option<T>)
where
    T: EntityId + Clone + Copy;

impl<T> Default for DragAreaHighlightedEntity<T>
where
    T: EntityId + Clone + Copy
{
    fn default() -> Self { Self(DragArea::default(), None) }
}

impl<T> From<Option<T>> for DragAreaHighlightedEntity<T>
where
    T: EntityId + Clone + Copy
{
    #[inline]
    fn from(value: Option<T>) -> Self { Self(DragArea::default(), value) }
}

impl<T> From<DragAreaHighlightedEntity<T>> for DragArea
where
    T: EntityId + Clone + Copy
{
    #[inline]
    fn from(value: DragAreaHighlightedEntity<T>) -> Self { value.0 }
}

impl<T> From<DragArea> for DragAreaHighlightedEntity<T>
where
    T: EntityId + Clone + Copy
{
    #[inline]
    fn from(value: DragArea) -> Self { Self(value, None) }
}

impl<T> DragAreaTrait for DragAreaHighlightedEntity<T>
where
    T: EntityId + Clone + Copy
{
    #[inline]
    fn from_origin(origin: Vec2) -> Self { Self(DragArea::from_origin(origin), None) }

    #[inline]
    fn from_extremes(origin: Vec2, extreme: Vec2) -> Option<Self>
    {
        DragArea::from_extremes(origin, extreme).map(|da| Self(da, None))
    }

    #[inline]
    fn origin(&self) -> Option<Vec2> { self.0.origin() }

    #[inline]
    fn extreme(&self) -> Option<Vec2> { self.0.extreme() }

    #[inline]
    fn hull(&self) -> Option<crate::utils::hull::Hull> { self.0.hull() }

    #[inline]
    fn update_extremes(&mut self, p: Vec2)
    {
        self.0.update_extremes(p);

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

impl<T> DragAreaHighlightedEntity<T>
where
    T: EntityId + Clone + Copy
{
    #[inline]
    #[must_use]
    pub const fn highlighted_entity(&self) -> Option<T> { self.1 }

    #[inline]
    #[must_use]
    pub const fn has_highlighted_entity(&self) -> bool { self.1.is_some() }

    #[inline]
    pub fn set_highlighted_entity(&mut self, entity: Option<T>) { self.1 = entity; }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

#[inline]
#[must_use]
fn valid_points(a: Vec2, b: Vec2) -> bool { (a.x - b.x).abs() >= 2f32 && (a.y - b.y).abs() >= 2f32 }
