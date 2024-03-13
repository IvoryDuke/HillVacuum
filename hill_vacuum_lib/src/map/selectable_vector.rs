//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::ops::{Add, AddAssign, SubAssign};

use bevy::prelude::Vec2;
use serde::{Deserialize, Serialize};

use crate::{map::HvVec, utils::misc::Toggle};

//=======================================================================//
// MACROS
//
//=======================================================================//

/// Deselects all [`SelectableVector`]s and returns their indexes, if any.
macro_rules! deselect_vectors {
    ($value:expr) => {{
        let mut idxs = crate::map::hv_vec![];

        for (i, value) in $value.iter_mut().enumerate()
        {
            if *value.1
            {
                idxs.push(i.try_into().unwrap());
                *value.1 = false;
            }
        }

        idxs.none_if_empty()
    }};
}

pub(in crate::map) use deselect_vectors;

//=======================================================================//

/// Selects the [`SelectableVector`]s in range of `$x_range` and `$y_range` and returns their
/// indexes, if any.
macro_rules! select_vectors_in_range {
    ($value:expr, $range:ident) => {{
        let mut idxs = crate::map::hv_vec![];

        for (i, value) in $value.iter_mut().enumerate()
        {
            let selection = *value.1;

            if $range.contains_point(value.0)
            {
                *value.1 = true;
            }

            if *value.1 != selection
            {
                idxs.push(i.try_into().unwrap());
            }
        }

        idxs.none_if_empty()
    }};
}

pub(in crate::map) use select_vectors_in_range;

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The result of the selection process of one [`SelectableVector`].
#[must_use]
#[derive(Debug)]
pub(in crate::map) enum VectorSelectionResult
{
    /// The vector has been or was already selected.
    Selected,
    /// The vector was not selected, it was exclusively selected and n >= 0 other vectors were
    /// deselected.
    NotSelected(Vec2, HvVec<u8>),
    /// Nothing occurred.
    None
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// A 2D vertex which can be selected and deselected.
#[derive(Clone, Copy, PartialEq)]
pub struct SelectableVector
{
    /// The vector.
    pub vec:      Vec2,
    /// Whever it is selected or not.
    pub selected: bool
}

impl Add<Vec2> for SelectableVector
{
    type Output = Vec2;

    fn add(self, rhs: Vec2) -> Self::Output { self.vec + rhs }
}

impl AddAssign<Vec2> for SelectableVector
{
    fn add_assign(&mut self, rhs: Vec2) { self.vec += rhs; }
}

impl SubAssign<Vec2> for SelectableVector
{
    fn sub_assign(&mut self, rhs: Vec2) { self.vec -= rhs; }
}

impl From<Vec2> for SelectableVector
{
    #[inline]
    #[must_use]
    fn from(vector: Vec2) -> Self { Self::new(vector) }
}

impl std::fmt::Debug for SelectableVector
{
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        f.debug_struct("Svec")
            .field("vertex", &self.vec)
            .field("selected", &self.selected)
            .finish()
    }
}

impl Toggle for SelectableVector
{
    #[inline]
    fn toggle(&mut self) { self.selected.toggle(); }
}

impl Serialize for SelectableVector
{
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer
    {
        self.vec.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SelectableVector
{
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>
    {
        Vec2::deserialize(deserializer).map(Into::into)
    }
}

impl SelectableVector
{
    /// Creates a new non-selected [`SelectableVertex`] from `vector`.
    #[inline]
    pub const fn new(vector: Vec2) -> Self
    {
        Self {
            vec:      vector,
            selected: false
        }
    }

    /// Creates a new [`SelectableVertex`] with `selected` selection state.
    #[inline]
    pub const fn with_selected(vector: Vec2, selected: bool) -> Self
    {
        Self {
            vec: vector,
            selected
        }
    }
}
