//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{
    hash::Hash,
    ops::{Index, IndexMut, Range, RangeInclusive},
    slice::Chunks
};

#[cfg(feature = "arena_alloc")]
use blink_alloc::BlinkAlloc;
use hashbrown::{hash_map::DefaultHashBuilder, Equivalent, HashSet};
use serde::{Deserialize, Deserializer, Serialize};
#[cfg(not(feature = "arena_alloc"))]
use smallvec::SmallVec;

use super::AssertedInsertRemove;
use crate::utils::{
    identifiers::Id,
    iterators::{
        PairIterator,
        PairIteratorMut,
        SlicePairIter,
        SlicePairIterMut,
        SliceTripletIter,
        TripletIterator
    },
    misc::{NoneIfEmpty, ReplaceValues, TakeValue}
};

//=======================================================================//
// STATICS
//
//=======================================================================//

#[cfg(feature = "arena_alloc")]
static mut ALLOCATOR: BlinkAlloc = BlinkAlloc::with_chunk_size(32_768);

//=======================================================================//
// MACROS
//
//=======================================================================//

/// Creates a new [`HvVec`] based on the parameters.
macro_rules! hv_vec {
    [] => (
        crate::map::containers::HvVec::new()
    );

    [capacity; $n:expr] => (
        crate::map::HvVec::with_capacity($n)
    );

    [$x:expr; $n:literal] => ({
        let mut vec = crate::map::hv_vec![];

        for _ in 0..$n
        {
            vec.push($x);
        }

        vec
    });

    [$($x:expr),+] => ({
        let mut vec = crate::map::containers::hv_vec![];
        $(vec.push($x);)+
        vec
    });

    [collect; $x:expr] => ({
        let mut vec = crate::map::containers::hv_vec![];
        vec.extend($x);
        vec
    });
}

pub(in crate::map) use hv_vec;

//=======================================================================//

/// Creates a new [`HvHashMap`] based on the parameters.
macro_rules! hv_hash_map {
    [] => {{
        crate::map::containers::HvHashMap::new()
    }};

    [capacity; $n:expr] => {{
        crate::map::containers::HvHashMap::with_capacity($n)
    }};

    [$(($k:expr, $v:expr)),+] => ({
        let mut map = crate::map::containers::hv_hash_map![];
        $(map.insert($k, $v);)+
        map
    });

    [collect; $x:expr] => ({
        let mut map = crate::map::containers::hv_hash_map![];
        map.extend($x);
        map
    });
}

pub(in crate::map) use hv_hash_map;

//=======================================================================//

/// Creates a new [`HvHashSet`] based on the parameters.
macro_rules! hv_hash_set {
    [] => {
        crate::map::containers::HvHashSet::new()
    };

    [capacity; $n:expr] => (
        crate::map::containers::HvHashSet::with_capacity($n)
    );

    [$($v:expr),+] => ({
        let mut map = crate::map::containers::hv_hash_set![];
        $(map.insert($v);)+
        map
    });

    [collect; $x:expr] => ({
        let mut vec = crate::map::containers::hv_hash_set![];
        vec.extend($x);
        vec
    });
}

pub(in crate::map) use hv_hash_set;

//=======================================================================//

/// Creates a new [`HvBox`].
macro_rules! hv_box {
    ($x:expr) => {{
        #[cfg(feature = "arena_alloc")]
        let b = crate::map::containers::HvBox::new_in($x, crate::map::containers::blink_alloc());

        #[cfg(not(feature = "arena_alloc"))]
        let b = Box::new($x);

        b
    }};
}

pub(in crate::map) use hv_box;

//=======================================================================//
// TYPES
//
//=======================================================================//

#[cfg(feature = "arena_alloc")]
/// [`Vec`] wrapper.
#[must_use]
#[derive(Clone, Debug)]
pub struct HvVec<T>(Vec<T, &'static BlinkAlloc>);

#[cfg(not(feature = "arena_alloc"))]
/// [`SmallVec`] wrapper.
#[must_use]
#[derive(Clone, Debug)]
pub struct HvVec<T>(SmallVec<[T; 1]>);

impl<T> Default for HvVec<T>
{
    fn default() -> Self { Self::new() }
}

impl<T> std::ops::Deref for HvVec<T>
{
    type Target = [T];

    #[inline]
    fn deref(&self) -> &Self::Target { &self.0 }
}

impl<T> IntoIterator for HvVec<T>
{
    #[cfg(feature = "arena_alloc")]
    type IntoIter = std::vec::IntoIter<T, &'static BlinkAlloc>;
    #[cfg(not(feature = "arena_alloc"))]
    type IntoIter = smallvec::IntoIter<[T; 1]>;
    type Item = T;

    #[inline]
    fn into_iter(self) -> Self::IntoIter { self.0.into_iter() }
}

impl<'a, T> IntoIterator for &'a HvVec<T>
{
    type IntoIter = std::slice::Iter<'a, T>;
    type Item = &'a T;

    #[inline]
    fn into_iter(self) -> Self::IntoIter { self.iter() }
}

impl<'a, T> IntoIterator for &'a mut HvVec<T>
{
    type IntoIter = std::slice::IterMut<'a, T>;
    type Item = &'a mut T;

    #[inline]
    fn into_iter(self) -> Self::IntoIter { self.iter_mut() }
}

impl<T> Index<usize> for HvVec<T>
{
    type Output = T;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output { &self.0[index] }
}

impl<T> IndexMut<usize> for HvVec<T>
{
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output { &mut self.0[index] }
}

impl<T> Index<Range<usize>> for HvVec<T>
{
    type Output = [T];

    #[inline]
    fn index(&self, index: Range<usize>) -> &Self::Output { &self.0[index] }
}

impl<T> IndexMut<Range<usize>> for HvVec<T>
{
    #[inline]
    fn index_mut(&mut self, range: Range<usize>) -> &mut Self::Output { &mut self.0[range] }
}

impl<T> Index<RangeInclusive<usize>> for HvVec<T>
{
    type Output = [T];

    #[inline]
    fn index(&self, index: RangeInclusive<usize>) -> &Self::Output { &self.0[index] }
}

impl<T> IndexMut<RangeInclusive<usize>> for HvVec<T>
{
    #[inline]
    fn index_mut(&mut self, range: RangeInclusive<usize>) -> &mut Self::Output
    {
        &mut self.0[range]
    }
}

impl<T> Extend<T> for HvVec<T>
{
    #[inline]
    fn extend<A: IntoIterator<Item = T>>(&mut self, iter: A) { self.0.extend(iter); }
}

impl<T: Serialize> Serialize for HvVec<T>
{
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer
    {
        serializer.collect_seq(self)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for HvVec<T>
{
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<HvVec<T>, D::Error>
    where
        D: Deserializer<'de>
    {
        Vec::<T>::deserialize(deserializer).map(|vec| hv_vec![collect; vec])
    }
}

impl<T> NoneIfEmpty for HvVec<T>
{
    #[inline]
    #[must_use]
    fn none_if_empty(self) -> Option<Self>
    where
        Self: Sized
    {
        (!self.is_empty()).then_some(self)
    }
}

impl<T> ReplaceValues<T> for HvVec<T>
{
    #[inline]
    fn replace_values<I: IntoIterator<Item = T>>(&mut self, iter: I)
    {
        self.clear();
        self.extend(iter);
    }
}

impl<T> TakeValue for HvVec<T>
{
    #[inline]
    fn take_value(&mut self) -> Self { std::mem::replace(self, hv_vec![]) }
}

impl<'a, T: 'a> PairIterator<'a, &'a T, SlicePairIter<'a, T>> for HvVec<T>
{
    #[inline]
    #[must_use]
    fn pair_iter(&'a self) -> Option<SlicePairIter<'a, T>> { self.0.pair_iter() }
}

impl<'a, T: 'a> PairIteratorMut<'a, T, SlicePairIterMut<'a, T>> for HvVec<T>
{
    #[inline]
    #[must_use]
    fn pair_iter_mut(&'a mut self) -> Option<SlicePairIterMut<'a, T>> { self.0.pair_iter_mut() }
}

impl<'a, T: 'a> TripletIterator<'a, &'a T, SliceTripletIter<'a, T>> for HvVec<T>
{
    #[inline]
    #[must_use]
    fn triplet_iter(&'a self) -> Option<SliceTripletIter<'a, T>> { self.0.triplet_iter() }
}

impl<T> HvVec<T>
where
    T: PartialEq
{
    /// Returns `true` if the slice contains an element with the given value.
    #[inline]
    #[must_use]
    pub fn contains(&self, x: &T) -> bool { self.0.contains(x) }
}

impl<T> HvVec<T>
{
    //==============================================================
    // New

    /// Constructs an empty vector.
    #[inline]
    pub fn new() -> Self
    {
        #[cfg(feature = "arena_alloc")]
        let vec = Vec::new_in(blink_alloc());

        #[cfg(not(feature = "arena_alloc"))]
        let vec = SmallVec::new();

        Self(vec)
    }

    /// Constructs an empty vector with enough capacity pre-allocated to store at least `n`
    /// elements.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self
    {
        #[cfg(feature = "arena_alloc")]
        let vec = Vec::with_capacity_in(capacity, blink_alloc());

        #[cfg(not(feature = "arena_alloc"))]
        let vec = SmallVec::with_capacity(capacity);

        Self(vec)
    }

    //==============================================================
    // Info

    /// The number of elements stored in the vector.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize { self.0.len() }

    /// Returns `true` if the vector is empty.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool { self.0.is_empty() }

    /// Returns the last element of the slice, or `None` if it is empty.
    #[inline]
    #[must_use]
    pub fn last(&self) -> Option<&T> { self.0.last() }

    //==============================================================
    // Edit

    /// Returns a mutable reference to the last item in the slice, or `None` if it is empty.
    #[inline]
    #[must_use]
    pub fn last_mut(&mut self) -> Option<&mut T> { self.0.last_mut() }

    /// Append an item to the vector.
    #[inline]
    pub fn push(&mut self, value: T) { self.0.push(value); }

    /// Insert an element at position `index`, shifting all elements after it to the right.
    ///
    /// Panics if `index > len`.
    #[inline]
    pub fn insert(&mut self, index: usize, element: T) { self.0.insert(index, element); }

    /// Remove an item from the end of the vector and return it, or None if empty.
    #[inline]
    pub fn pop(&mut self) -> Option<T> { self.0.pop() }

    /// Remove and return the element at position `index`, shifting all elements after it to the
    /// left.
    ///
    /// Panics if `index` is out of bounds.
    #[inline]
    pub fn remove(&mut self, index: usize) -> T { self.0.remove(index) }

    /// Remove the element at position `index`, replacing it with the last element.
    ///
    /// This does not preserve ordering, but is O(1).
    ///
    /// Panics if `index` is out of bounds.
    #[inline]
    pub fn swap_remove(&mut self, index: usize) -> T { self.0.swap_remove(index) }

    /// Remove all elements from the vector.
    #[inline]
    pub fn clear(&mut self) { self.0.clear(); }

    /// Shorten the vector, keeping the first `len` elements and dropping the rest.
    ///
    /// If `len` is greater than or equal to the vector's current length, this has no
    /// effect.
    ///
    /// This does not re-allocate.  If you want the vector's capacity to shrink, call
    /// `shrink_to_fit` after truncating.
    #[inline]
    pub fn truncate(&mut self, len: usize) { self.0.truncate(len); }

    /// Sorts the slice, but might not preserve the order of equal elements.
    ///
    /// This sort is unstable (i.e., may reorder equal elements), in-place
    /// (i.e., does not allocate), and *O*(*n* \* log(*n*)) worst-case.
    #[inline]
    pub fn sort_unstable(&mut self)
    where
        T: Ord
    {
        self.0.sort_unstable();
    }

    /// Sorts the slice with a comparator function.
    ///
    /// This sort is stable (i.e., does not reorder equal elements) and *O*(*n* \* log(*n*))
    /// worst-case.
    #[inline]
    pub fn sort_by<F>(&mut self, compare: F)
    where
        F: FnMut(&T, &T) -> std::cmp::Ordering
    {
        self.0.sort_by(compare);
    }

    /// Reverses the order of elements in the slice, in place.
    #[inline]
    pub fn reverse(&mut self) { self.0.reverse(); }

    /// Retains only the elements specified by the predicate.
    ///
    /// This method is identical in behaviour to [`retain`]; it is included only
    /// to maintain api-compatability with `std::Vec`, where the methods are
    /// separate for historical reasons.
    #[inline]
    pub fn retain_mut<F>(&mut self, f: F)
    where
        F: FnMut(&mut T) -> bool
    {
        self.0.retain_mut(f);
    }

    #[cfg(feature = "arena_alloc")]
    /// Creates a draining iterator that removes the specified range in the vector
    /// and yields the removed items.
    ///
    /// Note 1: The element range is removed even if the iterator is only
    /// partially consumed or not consumed at all.
    ///
    /// Note 2: It is unspecified how many elements are removed from the vector
    /// if the `Drain` value is leaked.
    #[inline]
    pub fn drain<R>(&mut self, range: R) -> std::vec::Drain<T, &'static BlinkAlloc>
    where
        R: std::ops::RangeBounds<usize>
    {
        self.0.drain(range)
    }

    #[cfg(not(feature = "arena_alloc"))]
    /// Creates a draining iterator that removes the specified range in the vector
    /// and yields the removed items.
    ///
    /// Note 1: The element range is removed even if the iterator is only
    /// partially consumed or not consumed at all.
    ///
    /// Note 2: It is unspecified how many elements are removed from the vector
    /// if the `Drain` value is leaked.
    #[inline]
    pub fn drain<R>(&mut self, range: R) -> smallvec::Drain<[T; 1]>
    where
        R: std::ops::RangeBounds<usize>
    {
        self.0.drain(range)
    }

    /// Divides one mutable slice into two at an index.
    ///
    /// The first will contain all indices from `[0, mid)` (excluding
    /// the index `mid` itself) and the second will contain all
    /// indices from `[mid, len)` (excluding the index `len` itself).
    #[inline]
    pub fn split_at_mut(&mut self, mid: usize) -> (&mut [T], &mut [T]) { self.0.split_at_mut(mid) }

    //==============================================================
    // Iterators

    /// Returns an iterator over the slice.
    ///
    /// The iterator yields all items from start to end.
    #[inline]
    pub fn iter(&self) -> std::slice::Iter<T> { self.0.iter() }

    /// Returns an iterator that allows modifying each value.
    ///
    /// The iterator yields all items from start to end.
    #[inline]
    pub fn iter_mut(&mut self) -> std::slice::IterMut<T> { self.0.iter_mut() }

    /// Returns an iterator over `chunk_size` elements of the slice at a time, starting at the
    /// beginning of the slice.
    ///
    /// The chunks are slices and do not overlap. If `chunk_size` does not divide the length of the
    /// slice, then the last chunk will not have length `chunk_size`.
    #[inline]
    pub fn chunks(&self, chunk_size: usize) -> Chunks<T> { self.0.chunks(chunk_size) }
}

//=======================================================================//

#[cfg(feature = "arena_alloc")]
/// hashbrown [`HashMap`] alias.
#[derive(Debug, Clone)]
pub struct HvHashMap<K, V>(hashbrown::HashMap<K, V, DefaultHashBuilder, &'static BlinkAlloc>);

#[cfg(not(feature = "arena_alloc"))]
/// hashbrown [`HashMap`] alias.
#[derive(Debug, Clone)]
pub struct HvHashMap<K, V>(hashbrown::HashMap<K, V, DefaultHashBuilder>);

impl<K, V> Default for HvHashMap<K, V>
{
    #[inline]
    fn default() -> Self { hv_hash_map![] }
}

impl<K, V> IntoIterator for HvHashMap<K, V>
{
    #[cfg(feature = "arena_alloc")]
    type IntoIter = hashbrown::hash_map::IntoIter<K, V, &'static BlinkAlloc>;
    #[cfg(not(feature = "arena_alloc"))]
    type IntoIter = hashbrown::hash_map::IntoIter<K, V>;
    type Item = (K, V);

    #[inline]
    fn into_iter(self) -> Self::IntoIter { self.0.into_iter() }
}

impl<'a, K, V> IntoIterator for &'a HvHashMap<K, V>
{
    type IntoIter = hashbrown::hash_map::Iter<'a, K, V>;
    type Item = (&'a K, &'a V);

    #[inline]
    fn into_iter(self) -> Self::IntoIter { self.iter() }
}

impl<'a, K, V> IntoIterator for &'a mut HvHashMap<K, V>
{
    type IntoIter = hashbrown::hash_map::IterMut<'a, K, V>;
    type Item = (&'a K, &'a mut V);

    #[inline]
    fn into_iter(self) -> Self::IntoIter { self.iter_mut() }
}

impl<K: std::hash::Hash + std::cmp::Eq, V> Extend<(K, V)> for HvHashMap<K, V>
{
    #[inline]
    fn extend<A: IntoIterator<Item = (K, V)>>(&mut self, iter: A) { self.0.extend(iter); }
}

impl<K: std::hash::Hash + std::cmp::Eq + Serialize, V: Serialize> Serialize for HvHashMap<K, V>
{
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer
    {
        serializer.collect_seq(self)
    }
}

impl<'de, K, V> Deserialize<'de> for HvHashMap<K, V>
where
    K: std::hash::Hash + std::cmp::Eq + Deserialize<'de>,
    V: Deserialize<'de>
{
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<HvHashMap<K, V>, D::Error>
    where
        D: Deserializer<'de>
    {
        Vec::<(K, V)>::deserialize(deserializer).map(|vec| hv_hash_map![collect; vec])
    }
}

impl<'a, K: std::hash::Hash + std::cmp::Eq + Copy, V: Copy> ReplaceValues<(&'a K, &'a V)>
    for HvHashMap<K, V>
{
    #[inline]
    fn replace_values<I: IntoIterator<Item = (&'a K, &'a V)>>(&mut self, iter: I)
    {
        self.0.clear();
        self.0.extend(iter.into_iter().map(|(k, v)| (*k, *v)));
    }
}

impl<K: std::hash::Hash + std::cmp::Eq, V> TakeValue for HvHashMap<K, V>
{
    #[inline]
    #[must_use]
    fn take_value(&mut self) -> Self { std::mem::replace(self, hv_hash_map![]) }
}

impl<K, V> HvHashMap<K, V>
{
    /// Creates an empty `HashMap`.
    ///
    /// The hash map is initially created with a capacity of 0, so it will not allocate until it
    /// is first inserted into.
    #[inline]
    pub fn new() -> Self
    {
        #[cfg(feature = "arena_alloc")]
        let map = hashbrown::HashMap::new_in(blink_alloc());

        #[cfg(not(feature = "arena_alloc"))]
        let map = hashbrown::HashMap::new();

        Self(map)
    }

    /// Creates an empty `HashMap` with the specified capacity.
    ///
    /// The hash map will be able to hold at least `capacity` elements without
    /// reallocating. If `capacity` is 0, the hash map will not allocate.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self
    {
        #[cfg(feature = "arena_alloc")]
        let map = hashbrown::HashMap::with_capacity_in(capacity, blink_alloc());

        #[cfg(not(feature = "arena_alloc"))]
        let map = hashbrown::HashMap::with_capacity(capacity);

        Self(map)
    }

    /// Returns the number of elements in the map.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize { self.0.len() }

    /// Returns `true` if the map contains no elements.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool { self.0.is_empty() }

    /// Clears the map, removing all key-value pairs. Keeps the allocated memory
    /// for reuse.
    #[inline]
    pub fn clear(&mut self) { self.0.clear() }

    /// An iterator visiting all key-value pairs in arbitrary order.
    /// The iterator element type is `(&'a K, &'a V)`.
    #[inline]
    pub fn iter(&self) -> hashbrown::hash_map::Iter<'_, K, V> { self.0.iter() }

    /// An iterator visiting all key-value pairs in arbitrary order,
    /// with mutable references to the values.
    /// The iterator element type is `(&'a K, &'a mut V)`.
    #[inline]
    pub fn iter_mut(&mut self) -> hashbrown::hash_map::IterMut<'_, K, V> { self.0.iter_mut() }

    /// An iterator visiting all keys in arbitrary order.
    /// The iterator element type is `&'a K`.
    #[inline]
    pub fn keys(&self) -> hashbrown::hash_map::Keys<'_, K, V> { self.0.keys() }

    /// An iterator visiting all values in arbitrary order.
    /// The iterator element type is `&'a V`.
    #[inline]
    pub fn values(&self) -> hashbrown::hash_map::Values<'_, K, V> { self.0.values() }

    /// An iterator visiting all values mutably in arbitrary order.
    /// The iterator element type is `&'a mut V`.
    #[inline]
    pub fn values_mut(&mut self) -> hashbrown::hash_map::ValuesMut<'_, K, V> { self.0.values_mut() }
}

impl<K: std::hash::Hash + std::cmp::Eq, V> HvHashMap<K, V>
{
    /// Returns `true` if the map contains a value for the specified key.
    ///
    /// The key may be any borrowed form of the map's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    #[inline]
    pub fn contains_key<Q>(&self, k: &Q) -> bool
    where
        Q: ?Sized + Hash + Equivalent<K>
    {
        self.0.contains_key(k)
    }

    /// Inserts a key-value pair into the map.
    ///
    /// If the map did not have this key present, [`None`] is returned.
    ///
    /// If the map did have this key present, the value is updated, and the old
    /// value is returned. The key is not updated, though; this matters for
    /// types that can be `==` without being identical. See the [`std::collections`]
    /// [module-level documentation] for more.
    #[inline]
    pub fn insert(&mut self, k: K, v: V) -> Option<V> { self.0.insert(k, v) }

    /// Removes a key from the map, returning the value at the key if the key
    /// was previously in the map. Keeps the allocated memory for reuse.
    ///
    /// The key may be any borrowed form of the map's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    #[inline]
    pub fn remove<Q>(&mut self, k: &Q) -> Option<V>
    where
        Q: ?Sized + Hash + Equivalent<K>
    {
        self.0.remove(k)
    }

    /// Returns a reference to the value corresponding to the key.
    ///
    /// The key may be any borrowed form of the map's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    #[inline]
    pub fn get<Q: ?Sized + Hash + Equivalent<K>>(&self, k: &Q) -> Option<&V> { self.0.get(k) }

    /// Returns a mutable reference to the value corresponding to the key.
    ///
    /// The key may be any borrowed form of the map's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    #[inline]
    pub fn get_mut<Q>(&mut self, k: &Q) -> Option<&mut V>
    where
        Q: ?Sized + Hash + Equivalent<K>
    {
        self.0.get_mut(k)
    }

    /// Attempts to get mutable references to `N` values in the map at once.
    ///
    /// Returns an array of length `N` with the results of each query. For soundness, at most one
    /// mutable reference will be returned to any value. `None` will be returned if any of the
    /// keys are duplicates or missing.
    #[inline]
    pub fn get_many_mut<Q, const N: usize>(&mut self, ks: [&Q; N]) -> Option<[&'_ mut V; N]>
    where
        Q: ?Sized + Hash + Equivalent<K>
    {
        self.0.get_many_mut(ks)
    }

    /// Retains only the elements specified by the predicate.
    #[inline]
    pub fn retain<F>(&mut self, f: F)
    where
        F: FnMut(&K, &mut V) -> bool
    {
        self.0.retain(f);
    }
}

//=======================================================================//

#[cfg(feature = "arena_alloc")]
/// [`hashbrown::HashSet`] wrapper.
#[derive(Clone, Debug)]
pub struct HvHashSet<T>(hashbrown::HashSet<T, DefaultHashBuilder, &'static BlinkAlloc>);

#[cfg(not(feature = "arena_alloc"))]
/// [`hashbrown::HashSet`] wrapper.
#[derive(Clone, Debug)]
pub struct HvHashSet<T>(hashbrown::HashSet<T, DefaultHashBuilder>);

impl<T: Hash + Eq> Default for HvHashSet<T>
{
    fn default() -> Self { Self::new() }
}

impl<T: Eq + Hash> Extend<T> for HvHashSet<T>
{
    #[inline]
    fn extend<A: IntoIterator<Item = T>>(&mut self, iter: A) { self.0.extend(iter); }
}

impl<'a, T: 'a + Eq + Hash + Copy> Extend<&'a T> for HvHashSet<T>
{
    #[inline]
    fn extend<A: IntoIterator<Item = &'a T>>(&mut self, iter: A) { self.0.extend(iter); }
}

impl<T> IntoIterator for HvHashSet<T>
{
    #[cfg(feature = "arena_alloc")]
    type IntoIter = hashbrown::hash_set::IntoIter<T, &'static BlinkAlloc>;
    #[cfg(not(feature = "arena_alloc"))]
    type IntoIter = hashbrown::hash_set::IntoIter<T>;
    type Item = T;

    #[inline]
    fn into_iter(self) -> Self::IntoIter { self.0.into_iter() }
}

impl<'a, T> IntoIterator for &'a HvHashSet<T>
{
    type IntoIter = hashbrown::hash_set::Iter<'a, T>;
    type Item = &'a T;

    #[inline]
    fn into_iter(self) -> Self::IntoIter { self.iter() }
}

impl<T: Serialize> Serialize for HvHashSet<T>
{
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer
    {
        serializer.collect_seq(self)
    }
}

impl<'de, T: Deserialize<'de> + Eq + Hash> Deserialize<'de> for HvHashSet<T>
{
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>
    {
        Vec::<T>::deserialize(deserializer).map(|vec| hv_hash_set![collect; vec])
    }
}

impl<T: Eq + Hash> ReplaceValues<T> for HvHashSet<T>
{
    #[inline]
    fn replace_values<I: IntoIterator<Item = T>>(&mut self, iter: I)
    {
        self.clear();
        self.extend(iter);
    }
}

impl<'a, T: 'a + Eq + Hash + Copy> ReplaceValues<&'a T> for HvHashSet<T>
{
    #[inline]
    fn replace_values<I: IntoIterator<Item = &'a T>>(&mut self, iter: I)
    {
        self.clear();
        self.extend(iter);
    }
}

impl<T: Eq + Hash> NoneIfEmpty for HvHashSet<T>
{
    #[inline]
    #[must_use]
    fn none_if_empty(self) -> Option<Self> { (!self.is_empty()).then_some(self) }
}

impl<T: Hash + Eq> TakeValue for HvHashSet<T>
{
    #[inline]
    #[must_use]
    fn take_value(&mut self) -> Self { std::mem::replace(self, hv_hash_set![]) }
}

impl<T: Hash + Eq> AssertedInsertRemove<T, T, (), ()> for HvHashSet<T>
{
    #[inline]
    fn asserted_insert(&mut self, value: T)
    {
        assert!(self.0.insert(value), "Value is already present.");
    }

    #[inline]
    fn asserted_remove(&mut self, value: &T)
    {
        assert!(self.0.remove(value), "Value is not present.");
    }
}

impl<T: Hash + Equivalent<T> + Eq> HvHashSet<T>
{
    /// Returns `true` if the set contains a value.
    ///
    /// The value may be any borrowed form of the set's value type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the value type.
    #[inline]
    #[must_use]
    pub fn contains(&self, value: &T) -> bool { self.0.contains(value) }
}

impl<T: Hash + Eq> HvHashSet<T>
{
    //==============================================================
    // New

    /// Creates an empty `HashSet`.
    ///
    /// The hash set is initially created with a capacity of 0, so it will not allocate until it
    /// is first inserted into.
    #[inline]
    #[must_use]
    pub fn new() -> Self
    {
        #[cfg(feature = "arena_alloc")]
        let set = HashSet::new_in(blink_alloc());

        #[cfg(not(feature = "arena_alloc"))]
        let set = HashSet::new();

        Self(set)
    }

    /// Creates an empty `HashSet` with the specified capacity.
    ///
    /// The hash set will be able to hold at least `capacity` elements without
    /// reallocating. If `capacity` is 0, the hash set will not allocate.
    #[inline]
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self
    {
        #[cfg(feature = "arena_alloc")]
        let set = HashSet::with_capacity_in(capacity, blink_alloc());

        #[cfg(not(feature = "arena_alloc"))]
        let set = HashSet::with_capacity(capacity);

        Self(set)
    }

    //==============================================================
    // Edit

    /// Adds a value to the set.
    ///
    /// If the set did not have this value present, `true` is returned.
    ///
    /// If the set did have this value present, `false` is returned.
    #[inline]
    pub fn insert(&mut self, value: T) -> bool { self.0.insert(value) }

    /// Removes a value from the set. Returns whether the value was
    /// present in the set.
    ///
    /// The value may be any borrowed form of the set's value type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the value type.
    #[inline]
    pub fn remove(&mut self, value: &T) -> bool { self.0.remove(value) }
}

impl<T> HvHashSet<T>
{
    //==============================================================
    // Info

    /// Returns `true` if the set contains no elements.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool { self.0.is_empty() }

    /// Returns the number of elements in the set.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize { self.0.len() }

    //==============================================================
    // Edit

    /// Clears the set, removing all values.
    #[inline]
    pub fn clear(&mut self) { self.0.clear(); }

    /// Retains only the elements specified by the predicate.
    ///
    /// In other words, remove all elements `e` such that `f(&e)` returns `false`.
    #[inline]
    pub fn retain<F>(&mut self, f: F)
    where
        F: FnMut(&T) -> bool
    {
        self.0.retain(f);
    }

    //==============================================================
    // Iterators

    /// An iterator visiting all elements in arbitrary order.
    /// The iterator element type is `&'a T`.
    #[inline]
    pub fn iter(&self) -> hashbrown::hash_set::Iter<T> { self.0.iter() }
}

//=======================================================================//

/// Alias for a [`HvHashSet`] of [`Id`]s.
pub type Ids = HvHashSet<Id>;

//=======================================================================//

#[cfg(feature = "arena_alloc")]
/// [`Box`] alias.
pub(in crate::map) type HvBox<T> = Box<T, &'static BlinkAlloc>;
#[cfg(not(feature = "arena_alloc"))]
/// [`Box`] alias.
pub(in crate::map) type HvBox<T> = Box<T>;

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

#[cfg(feature = "arena_alloc")]
/// Returns a static reference to the arena allocator.
#[inline]
#[must_use]
pub(in crate::map) fn blink_alloc() -> &'static BlinkAlloc
{
    unsafe { &*core::ptr::addr_of!(ALLOCATOR) }
}
