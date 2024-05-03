//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::cell::Ref;

use super::{
    entities_trees::Trees,
    AuxiliaryIds,
    BrushMut,
    EntitiesManager,
    Innards,
    MovingMut,
    ThingMut
};
use crate::{
    map::{
        brush::Brush,
        editor::state::manager::quad_tree::QuadTreeIds,
        path::Moving,
        thing::ThingInstance
    },
    utils::{hull::Hull, identifiers::Id}
};

//=======================================================================//
// TYPES
//
//=======================================================================//

/// A wrapper that returns an iterator to certain [`Brush`]es of the map defined on creation.
#[must_use]
pub(in crate::map::editor::state) struct BrushesIter<'a>(&'a EntitiesManager, Ref<'a, QuadTreeIds>);

impl<'a> BrushesIter<'a>
{
    /// Returns a new [`BrushesIter`] that returns an iterator to the [`Brush`]es with [`Id`]s in
    /// `ids`.
    #[inline]
    pub(in crate::map::editor::state::manager) const fn new(
        manager: &'a EntitiesManager,
        ids: Ref<'a, QuadTreeIds>
    ) -> Self
    {
        Self(manager, ids)
    }

    /// Returns an iterator to the [`Brush`]es whose [`Id`] are contained in `self`.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &Brush> { self.1.ids().map(|id| self.0.brush(*id)) }
}

//=======================================================================//

/// A wrapper that returns an iterator to certain [`ThingInstance`]s of the map defined on creation.
#[must_use]
pub(in crate::map::editor::state) struct ThingsIter<'a>(&'a EntitiesManager, Ref<'a, QuadTreeIds>);

impl<'a> ThingsIter<'a>
{
    /// Returns a new [`ThingsIter`] that returns an iterator to the [`ThingInstance`]s with [`Id`]s
    /// in `ids`.
    #[inline]
    pub(in crate::map::editor::state::manager) const fn new(
        manager: &'a EntitiesManager,
        ids: Ref<'a, QuadTreeIds>
    ) -> Self
    {
        Self(manager, ids)
    }

    /// Returns an iterator to the [`ThingInstance`]s whose [`Id`] are contained in `self`.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &ThingInstance>
    {
        self.1.ids().map(|id| self.0.thing(*id))
    }
}

//=======================================================================//

/// A wrapper that returns an iterator to certain selected [`Brush`]es of the map defined on
/// creation.
#[must_use]
pub(in crate::map::editor::state) struct SelectedBrushesIter<'a>(
    &'a EntitiesManager,
    Ref<'a, QuadTreeIds>
);

impl<'a> SelectedBrushesIter<'a>
{
    /// Returns a new [`SelectedBrushesIter`] that returns an iterator to the selected [`Brush`]es
    /// among the ones with [`Id`]s in `ids`.
    #[inline]
    pub(in crate::map::editor::state::manager) const fn new(
        manager: &'a EntitiesManager,
        ids: Ref<'a, QuadTreeIds>
    ) -> Self
    {
        Self(manager, ids)
    }

    /// Returns an iterator to the selected [`Brush`]es among those whose [`Id`] are contained in
    /// `self`.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &Brush>
    {
        self.1
            .ids()
            .filter(|&id| self.0.is_selected(*id))
            .map(|id| self.0.brush(*id))
    }
}

//=======================================================================//

/// A wrapper that returns an iterator to the [`Id`]s of the entities within a certain range.
pub(in crate::map::editor::state) struct IdsInRange<'a>(Ref<'a, QuadTreeIds>);

impl<'a, 'b: 'a> IntoIterator for &'b IdsInRange<'a>
{
    type IntoIter = hashbrown::hash_map::Keys<'a, Id, Hull>;
    type Item = &'a Id;

    #[inline]
    fn into_iter(self) -> Self::IntoIter { self.iter() }
}

impl<'a> IdsInRange<'a>
{
    /// Returns a new [`IdsInRange`] from `ids`.
    #[inline]
    pub(in crate::map::editor::state::manager) const fn new(ids: Ref<'a, QuadTreeIds>) -> Self
    {
        Self(ids)
    }

    /// Returns an iterator to the [`Id`]s.
    #[inline]
    pub fn iter(&'a self) -> hashbrown::hash_map::Keys<'a, Id, Hull> { self.0.ids() }
}

//=======================================================================//

/// An iterator to all selected [`Brush`]es wrapped in [`BrushMut`].
#[must_use]
pub(in crate::map::editor::state::manager) struct SelectedBrushesMut<'a>
{
    /// The [`Id`]s iterator.
    iter:       hashbrown::hash_set::Iter<'a, Id>,
    /// The manager.
    manager:    &'a mut Innards,
    /// The [`QuadTree`]s.
    quad_trees: &'a mut Trees
}

impl<'a> Iterator for SelectedBrushesMut<'a>
{
    type Item = BrushMut<'a>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item>
    {
        unsafe {
            self.iter.next().map(|id| {
                BrushMut::new(
                    std::ptr::from_mut(self.manager).as_mut().unwrap(),
                    std::ptr::from_mut(self.quad_trees).as_mut().unwrap(),
                    *id
                )
            })
        }
    }
}

impl<'a> SelectedBrushesMut<'a>
{
    /// Returns a new [`SelectedBrushesMut`].
    #[inline]
    pub fn new(
        manager: &'a mut Innards,
        quad_trees: &'a mut Trees,
        selected_brushes: &'a AuxiliaryIds
    ) -> Self
    {
        Self {
            iter: selected_brushes.iter(),
            manager,
            quad_trees
        }
    }
}

//=======================================================================//

/// A wrapper to the selected [`ThingInstances`].
#[must_use]
pub(in crate::map::editor::state) struct SelectedThingsIter<'a>(
    &'a EntitiesManager,
    Ref<'a, QuadTreeIds>
);

impl<'a> SelectedThingsIter<'a>
{
    /// Returns a new [`SelectedThingsIter`].
    #[inline]
    pub(in crate::map::editor::state::manager) const fn new(
        manager: &'a EntitiesManager,
        ids: Ref<'a, QuadTreeIds>
    ) -> Self
    {
        Self(manager, ids)
    }

    /// Returns an iterator to the [`Id`]s.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &ThingInstance>
    {
        self.1
            .ids()
            .filter(|&id| self.0.is_selected(*id))
            .map(|id| self.0.thing(*id))
    }
}

//=======================================================================//

/// An iterator to all selected [`ThingInstance`]s wrapped in [`ThingMut`].
#[must_use]
pub(in crate::map::editor::state::manager) struct SelectedThingsMut<'a>
{
    /// The [`Id`]s iterator.
    iter:       hashbrown::hash_set::Iter<'a, Id>,
    /// The manager.
    manager:    &'a mut Innards,
    /// The [`QuadTree`]s.
    quad_trees: &'a mut Trees
}

impl<'a> Iterator for SelectedThingsMut<'a>
{
    type Item = ThingMut<'a>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item>
    {
        unsafe {
            self.iter.next().map(|id| {
                ThingMut::new(
                    std::ptr::from_mut(self.manager).as_mut().unwrap(),
                    std::ptr::from_mut(self.quad_trees).as_mut().unwrap(),
                    *id
                )
            })
        }
    }
}

impl<'a> SelectedThingsMut<'a>
{
    /// Returns a new [`SelectedThingsMut`].
    #[inline]
    pub fn new(
        manager: &'a mut Innards,
        quad_trees: &'a mut Trees,
        selected_brushes: &'a AuxiliaryIds
    ) -> Self
    {
        Self {
            iter: selected_brushes.iter(),
            manager,
            quad_trees
        }
    }
}

//=======================================================================//

/// A wrapper to the selected entities with a [`Path`].
#[must_use]
pub(in crate::map::editor::state) struct SelectedMovingsIter<'a>(
    &'a EntitiesManager,
    Ref<'a, QuadTreeIds>
);

impl<'a> SelectedMovingsIter<'a>
{
    /// Returns a new [`SelectedMovingsIter`].
    #[inline]
    pub(in crate::map::editor::state::manager) const fn new(
        manager: &'a EntitiesManager,
        ids: Ref<'a, QuadTreeIds>
    ) -> Self
    {
        Self(manager, ids)
    }

    /// Returns an iterator to the entities with a [`Path`] as [`Moving`] trait objects.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &dyn Moving>
    {
        self.1.ids().filter(|&id| self.0.is_selected(*id)).map(|id| {
            if self.0.innards.is_thing(*id)
            {
                self.0.thing(*id) as &dyn Moving
            }
            else
            {
                self.0.brush(*id) as &dyn Moving
            }
        })
    }
}

//=======================================================================//

/// A wrapper to the [`Id`]s of entities which implement the [`Moving`] trait.
#[must_use]
pub(in crate::map::editor::state) struct MovingsIter<'a>(&'a EntitiesManager, Ref<'a, QuadTreeIds>);

impl<'a> MovingsIter<'a>
{
    /// Returns a new [`MovingsIter`].
    #[inline]
    pub(in crate::map::editor::state::manager) const fn new(
        manager: &'a EntitiesManager,
        ids: Ref<'a, QuadTreeIds>
    ) -> Self
    {
        Self(manager, ids)
    }

    /// Returns an iterator to the entities with a [`Path`] as [`Moving`] trait objects.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &dyn Moving>
    {
        self.1.ids().map(|id| {
            if self.0.innards.is_thing(*id)
            {
                self.0.thing(*id) as &dyn Moving
            }
            else
            {
                self.0.brush(*id) as &dyn Moving
            }
        })
    }
}

//=======================================================================//

/// A iterator to the selected entities wrapped in [`MovingMut`].
#[must_use]
pub(in crate::map::editor::state::manager) struct SelectedMovingsMut<'a>
{
    /// The iterator of the [`Id`]s.
    iter:       hashbrown::hash_set::Iter<'a, Id>,
    /// The entities manager.
    manager:    &'a mut Innards,
    /// The [`QuadTree`]s.
    quad_trees: &'a mut Trees
}

impl<'a> Iterator for SelectedMovingsMut<'a>
{
    type Item = MovingMut<'a>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item>
    {
        unsafe {
            self.iter.next().map(|id| {
                std::ptr::from_mut(self.manager)
                    .as_mut()
                    .unwrap()
                    .moving_mut(std::ptr::from_mut(self.quad_trees).as_mut().unwrap(), *id)
            })
        }
    }
}

impl<'a> SelectedMovingsMut<'a>
{
    /// Returns a new [`SelectedMovingsMut`].
    #[inline]
    pub fn new(
        manager: &'a mut Innards,
        quad_trees: &'a mut Trees,
        selected_brushes: &'a AuxiliaryIds
    ) -> Self
    {
        Self {
            iter: selected_brushes.iter(),
            manager,
            quad_trees
        }
    }
}
