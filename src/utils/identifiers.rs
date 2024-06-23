//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::Vec2;
use serde::{Deserialize, Serialize};

//=======================================================================//
// TRAITS
//
//=======================================================================//

/// Trait for all map elements characterized by an Id.
pub trait EntityId
{
    /// Returns the entity [`Id`].
    #[must_use]
    fn id(&self) -> Id;

    /// Returns a reference to the entity [`Id`].
    #[must_use]
    fn id_as_ref(&self) -> &Id;
}

//=======================================================================//

/// Trait for all map elements characterized by a bidimensional center.
pub trait EntityCenter
{
    /// Returns the center of the area of `self`.
    #[must_use]
    fn center(&self) -> Vec2;
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// An unique identifier assigned to each map entity to identify and distinguish them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Id(usize);

impl EntityId for Id
{
    #[inline]
    fn id(&self) -> Id { *self }

    #[inline]
    fn id_as_ref(&self) -> &Id { self }
}

impl Id
{
    /// [`Id`] with wrapped value equal to zero.
    pub const ZERO: Self = Self(0);

    /// Returns the [`Id`] with the highest value.
    #[inline]
    #[must_use]
    pub const fn max(self, other: Self) -> Id
    {
        if self.0 > other.0
        {
            self
        }
        else
        {
            other
        }
    }
}

//=======================================================================//

/// A generator of unique [`Id`]s for the map entities.
pub struct IdGenerator(Id);

impl Default for IdGenerator
{
    #[inline]
    #[must_use]
    fn default() -> Self { Self(Id(0)) }
}

impl IdGenerator
{
    /// Returns a new unique [`Id`].
    #[inline]
    #[must_use]
    pub fn new_id(&mut self) -> Id
    {
        let value = self.0;
        self.0 .0 += 1;
        value
    }

    /// Set the next [`Id`] to be generated to `value`.
    #[inline]
    pub fn reset(&mut self, value: Id) { self.0 = value; }
}
