//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{hash::Hash, ops::Index, slice::Chunks};

use hashbrown::Equivalent;
use serde::{Deserialize, Serialize};

use super::{containers::hv_hash_map, HvHashMap, HvVec};

//=======================================================================//
// TYPES
//
//=======================================================================//

/// A hashmap that can also be indexed.
#[must_use]
#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Clone, Serialize, Deserialize)]
pub(in crate::map) struct IndexedMap<K, T>
where
    K: Hash + Eq
{
    vec: HvVec<T>,
    map: HvHashMap<K, usize>
}

impl<K, T> Default for IndexedMap<K, T>
where
    K: Hash + Eq
{
    #[inline]
    fn default() -> Self
    {
        Self {
            vec: HvVec::default(),
            map: HvHashMap::default()
        }
    }
}

impl<K, T> Index<usize> for IndexedMap<K, T>
where
    K: Hash + Eq
{
    type Output = T;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output { &self.vec[index] }
}

impl<K, T> IndexedMap<K, T>
where
    K: Hash + Eq
{
    //==============================================================
    // New

    /// Creates a new [`IndexedMap`] from a vector of elements.
    #[inline]
    pub fn new<F: FnMut(&T) -> K>(vec: HvVec<T>, mut f: F) -> Self
    {
        let map = hv_hash_map![collect; vec.iter().enumerate().map(|(i, item)| (f(item), i))];
        Self { vec, map }
    }

    //==============================================================
    // Info

    /// Whever there are no elements.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool { self.vec.is_empty() }

    /// Whever there are no elements.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize { self.vec.len() }

    //==============================================================
    // Values

    /// Returns a reference to the element associated to the key `k`.
    #[inline]
    #[must_use]
    pub fn get<Q>(&self, k: &Q) -> Option<&T>
    where
        Q: ?Sized + Hash + Equivalent<K>
    {
        self.map.get(k).map(|idx| &self.vec[*idx])
    }

    /// Returns a mutable reference to the element associated to the key `k`.
    #[inline]
    #[must_use]
    pub fn get_mut<Q>(&mut self, k: &Q) -> Option<&mut T>
    where
        Q: ?Sized + Hash + Equivalent<K>
    {
        self.map.get_mut(k).map(|idx| &mut self.vec[*idx])
    }

    /// Returns the index of the element associated to the key `k`.
    #[inline]
    #[must_use]
    pub fn index<Q>(&self, k: &Q) -> Option<usize>
    where
        Q: ?Sized + Hash + Equivalent<K>
    {
        self.map.get(k).copied()
    }

    //==============================================================
    // Iterators

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (&K, &T)>
    {
        self.map.iter().map(|(k, i)| (k, &self.vec[*i]))
    }

    #[inline]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut T)>
    {
        self.map.iter().map(|(k, i)| {
            (k, &mut unsafe { std::ptr::addr_of_mut!(self.vec).as_mut().unwrap() }[*i])
        })
    }

    #[inline]
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut T> { self.vec.iter_mut() }

    /// Returns a [`Chunks`] iterator to the element of the map with `chunk_size`.
    #[inline]
    pub fn chunks(&self, chunk_size: usize) -> Chunks<T> { self.vec.chunks(chunk_size) }
}
