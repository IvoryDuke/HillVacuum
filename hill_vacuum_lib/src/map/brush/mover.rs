//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::Vec2;
use serde::{Deserialize, Serialize};
use shared::{match_or_panic, return_if_no_match};

use super::path::Path;
use crate::{
    map::{brush::calc_path_hull, hv_hash_set, AssertedInsertRemove, Ids, OutOfBounds},
    utils::{
        hull::Hull,
        identifiers::Id,
        misc::{NoneIfEmpty, TakeValue}
    }
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[must_use]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub enum Mover
{
    #[default]
    None,
    Anchors(Ids),
    Motor(Motor),
    Anchored(Id)
}

impl From<Motor> for Mover
{
    #[inline]
    fn from(value: Motor) -> Self { Self::Motor(value) }
}

impl From<MoverParts> for Mover
{
    #[inline]
    fn from(value: MoverParts) -> Self { Self::from_parts(value) }
}

impl Mover
{
    #[inline]
    #[must_use]
    pub(in crate::map) const fn has_motor(&self) -> bool { matches!(self, Self::Motor(_)) }

    #[inline]
    #[must_use]
    pub(in crate::map) fn has_anchors(&self) -> bool
    {
        match self.anchors()
        {
            Some(ids) => !ids.is_empty(),
            None => false
        }
    }

    #[inline]
    #[must_use]
    pub(in crate::map) const fn is_anchored(&self) -> Option<Id>
    {
        return_if_no_match!(self, Self::Anchored(id), Some(*id), None)
    }

    #[inline]
    pub(in crate::map) const fn path(&self) -> &Path
    {
        &match_or_panic!(self, Self::Motor(motor), motor).path
    }

    #[inline]
    pub(in crate::map::brush) fn path_mut(&mut self) -> &mut Path
    {
        &mut match_or_panic!(self, Self::Motor(motor), motor).path
    }

    #[inline]
    pub(in crate::map::brush) fn anchors_iter(
        &self
    ) -> Option<impl ExactSizeIterator<Item = &Id> + Clone>
    {
        self.anchors().map(Ids::iter)
    }

    #[inline]
    const fn anchors(&self) -> Option<&Ids>
    {
        match self
        {
            Self::None | Self::Anchored(_) => None,
            Self::Anchors(ids) => Some(ids),
            Self::Motor(motor) => Some(&motor.anchored_brushes)
        }
    }

    #[inline]
    #[must_use]
    pub(in crate::map) fn contains_anchor(&self, identifier: Id) -> bool
    {
        match self.anchors()
        {
            Some(ids) => ids.contains(&identifier),
            None => false
        }
    }

    #[inline]
    pub(in crate::map::brush) fn insert_anchor(&mut self, identifier: Id)
    {
        match self
        {
            Self::None => *self = Self::Anchors(hv_hash_set![identifier]),
            Self::Anchors(ids) => ids.asserted_insert(identifier),
            Self::Motor(motor) => motor.insert_anchor(identifier),
            Self::Anchored(_) => panic!("Tried to insert an anchor in an anchored brush.")
        };
    }

    #[inline]
    pub(in crate::map::brush) fn remove_anchor(&mut self, identifier: Id)
    {
        match self
        {
            Self::Anchors(ids) =>
            {
                ids.asserted_remove(&identifier);

                if ids.is_empty()
                {
                    *self = Self::None;
                }
            },
            Self::Motor(motor) => motor.remove_anchor(identifier),
            _ => panic!("Brush does not have anchors.")
        }
    }

    #[inline]
    pub(in crate::map::brush) fn create_motor(&mut self, path: Path)
    {
        match self
        {
            Self::None => *self = Self::Motor(Motor::new(path, None)),
            Self::Anchors(ids) => *self = Self::Motor(Motor::new(path, Some(ids.take_value()))),
            _ => panic!("Unsuitable circumstances for a motor creation.")
        };
    }

    #[inline]
    pub(in crate::map::brush) fn take_motor(&mut self) -> Motor
    {
        let mut motor = match_or_panic!(std::mem::take(self), Self::Motor(motor), motor);

        if motor.anchored_brushes.is_empty()
        {
            return motor;
        }

        *self = Self::Anchors(motor.anchored_brushes.take_value());
        motor
    }

    #[inline]
    pub(in crate::map::brush) fn set_motor(&mut self, mut motor: Motor)
    {
        assert!(motor.anchored_brushes.is_empty(), "Brush's Motor has anchored brushes.");

        match self
        {
            Self::None => *self = motor.into(),
            Self::Anchors(ids) =>
            {
                motor.anchored_brushes = ids.take_value();
                *self = Self::Motor(motor);
            },
            Self::Motor(_) | Self::Anchored(_) =>
            {
                panic!("Unsuitable circumstance for setting a motor.")
            }
        };
    }

    #[inline]
    pub(in crate::map::brush) fn apply_motor(&mut self, motor: Motor)
    {
        assert!(matches!(self, Self::None), "Tried to apply motor on an unsuitable brush.");
        *self = motor.into();
    }

    #[inline]
    fn from_parts(parts: MoverParts) -> Self
    {
        match parts
        {
            MoverParts::None => Self::None,
            MoverParts::Anchored(id) => Self::Anchored(id),
            MoverParts::Other(path, ids) =>
            {
                match (path, ids)
                {
                    (None, None) => unreachable!(),
                    (None, Some(ids)) => Self::Anchors(ids),
                    (Some(path), None) =>
                    {
                        Self::Motor(Motor {
                            path,
                            anchored_brushes: hv_hash_set![]
                        })
                    },
                    (Some(path), Some(anchored_brushes)) =>
                    {
                        Self::Motor(Motor {
                            path,
                            anchored_brushes
                        })
                    },
                }
            },
        }
    }
}

//=======================================================================//

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::map) enum MoverParts
{
    None,
    Anchored(Id),
    Other(Option<Path>, Option<Ids>)
}

impl From<Mover> for MoverParts
{
    #[inline]
    #[must_use]
    fn from(value: Mover) -> Self
    {
        match value
        {
            Mover::None => Self::None,
            Mover::Anchors(ids) => Self::Other(None, Some(ids)),
            Mover::Motor(motor) =>
            {
                let (path, anchors) = motor.into_parts();
                Self::Other(Some(path), anchors.none_if_empty())
            },
            Mover::Anchored(id) => Self::Anchored(id)
        }
    }
}

impl MoverParts
{
    #[inline]
    #[must_use]
    pub fn path_hull_out_of_bounds(&self, center: Vec2) -> bool
    {
        calc_path_hull(return_if_no_match!(self, Self::Other(Some(path), _), path, false), center)
            .out_of_bounds()
    }

    #[inline]
    #[must_use]
    pub fn path_hull(&self, center: Vec2) -> Option<Hull>
    {
        calc_path_hull(return_if_no_match!(self, Self::Other(Some(path), _), path, None), center)
            .into()
    }

    #[inline]
    #[must_use]
    pub const fn has_anchors(&self) -> bool { self.anchors().is_some() }

    #[inline]
    pub const fn anchors(&self) -> Option<&Ids>
    {
        match self
        {
            Self::None | Self::Anchored(_) => None,
            Self::Other(_, ids) => ids.as_ref()
        }
    }

    #[inline]
    #[must_use]
    pub fn contains_anchor(&self, identifier: Id) -> bool
    {
        match self.anchors()
        {
            Some(ids) => ids.contains(&identifier),
            None => false
        }
    }

    #[inline]
    pub fn insert_anchor(&mut self, identifier: Id)
    {
        match self
        {
            Self::None => *self = Self::Other(None, Some(hv_hash_set![identifier])),
            Self::Anchored(_) => panic!("Tried to insert an anchor in an anchored brush."),
            Self::Other(_, anchors) =>
            {
                match anchors
                {
                    Some(ids) => ids.asserted_insert(identifier),
                    None => *anchors = hv_hash_set![identifier].into()
                };
            }
        };
    }

    #[inline]
    pub fn remove_anchor(&mut self, identifier: Id)
    {
        match self
        {
            Self::None | Self::Anchored(_) => panic!("Brush does not have anchors."),
            Self::Other(path, anchors) =>
            {
                match anchors
                {
                    Some(ids) =>
                    {
                        ids.asserted_remove(&identifier);

                        if ids.is_empty()
                        {
                            *anchors = None;
                        }
                    },
                    None => panic!("Brush does not contain the anchor.")
                };

                if path.is_none() && anchors.is_none()
                {
                    *self = Self::None;
                }
            }
        };
    }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

#[must_use]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Motor
{
    path:             Path,
    anchored_brushes: Ids
}

impl From<Path> for Motor
{
    #[inline]
    fn from(value: Path) -> Self
    {
        Self {
            path:             value,
            anchored_brushes: hv_hash_set![]
        }
    }
}

impl Motor
{
    #[inline]
    pub(in crate::map) fn new(path: Path, anchors: Option<Ids>) -> Self
    {
        Self {
            path,
            anchored_brushes: anchors.unwrap_or_default()
        }
    }

    #[inline]
    pub fn path(&self) -> &Path { &self.path }

    #[inline]
    #[must_use]
    pub fn anchored_brushes(&self) -> &Ids { &self.anchored_brushes }

    #[inline]
    pub(in crate::map) fn take_path(self) -> Path { self.path }

    #[inline]
    fn insert_anchor(&mut self, identifier: Id)
    {
        self.anchored_brushes.asserted_insert(identifier);
    }

    #[inline]
    fn remove_anchor(&mut self, identifier: Id)
    {
        self.anchored_brushes.asserted_remove(&identifier);
    }

    #[inline]
    fn into_parts(self) -> (Path, Ids) { (self.path, self.anchored_brushes) }
}
