//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::hash::Hash;

use serde::{Deserialize, Serialize};

use crate::{HvHashMap, HvVec};

//=======================================================================//
// STRUCTS
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
    /// The ordered vector of values
    vec: HvVec<T>,
    /// The keys with the indexes of the associated values contained in `vec`.
    map: HvHashMap<K, usize>
}

//=======================================================================//
// UI
//
//=======================================================================//

#[cfg(feature = "ui")]
pub(in crate::map) mod ui_only
{
    //=======================================================================//
    // IMPORTS
    //
    //=======================================================================//

    use std::{hash::Hash, ops::Index, slice::Chunks};

    use hashbrown::Equivalent;

    use super::IndexedMap;
    use crate::{utils::collections::hv_hash_map, HvHashMap, HvVec};

    //=======================================================================//
    // STRUCTS
    //
    //=======================================================================//

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

        /// Whether there are no elements.
        #[inline]
        #[must_use]
        pub fn is_empty(&self) -> bool { self.vec.is_empty() }

        #[inline]
        #[must_use]
        pub fn len(&self) -> usize { self.vec.len() }

        #[inline]
        #[must_use]
        pub fn contains<Q>(&self, k: &Q) -> bool
        where
            Q: ?Sized + Hash + Equivalent<K>
        {
            self.map.contains_key(k)
        }

        #[inline]
        pub fn decompose(self) -> (HvVec<T>, HvHashMap<K, usize>) { (self.vec, self.map) }

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

        //==============================================================
        // Iterators

        /// Returns an iterator returning references to the (key, value) pairs.
        #[inline]
        pub fn iter(&self) -> impl Iterator<Item = (&K, &T)>
        {
            self.map.iter().map(|(k, i)| (k, &self.vec[*i]))
        }

        /// Returns an iterator to the references of the contained values.
        #[inline]
        pub fn values(&self) -> std::slice::Iter<T> { self.vec.iter() }

        /// Returns an iterator returning the keys and the mutable references of the values
        /// associated to them.
        #[inline]
        pub fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut T)>
        {
            self.map.iter().map(|(k, i)| {
                (k, unsafe { std::ptr::addr_of_mut!(self.vec[*i]).as_mut().unwrap() })
            })
        }

        /// Returns an iterator to the mutable references of the contained values.
        #[inline]
        pub fn values_mut(&mut self) -> impl Iterator<Item = &mut T> { self.vec.iter_mut() }

        /// Returns a [`Chunks`] iterator with `chunk_size` to the contained values.
        #[inline]
        pub fn chunks(&self, chunk_size: usize) -> Chunks<T> { self.vec.chunks(chunk_size) }
    }
}
