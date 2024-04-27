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
/// A [`Vec`] wrapper.
#[must_use]
#[derive(Clone, Debug)]
pub struct HvVec<T>(Vec<T, &'static BlinkAlloc>);

#[cfg(not(feature = "arena_alloc"))]
/// A [`Vec`] wrapper.
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
    #[inline]
    #[must_use]
    pub fn contains(&self, x: &T) -> bool { self.0.contains(x) }
}

impl<T> HvVec<T>
{
    //==============================================================
    // New

    #[inline]
    pub fn new() -> Self
    {
        #[cfg(feature = "arena_alloc")]
        let vec = Vec::new_in(blink_alloc());

        #[cfg(not(feature = "arena_alloc"))]
        let vec = SmallVec::new();

        Self(vec)
    }

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

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize { self.0.len() }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool { self.0.is_empty() }

    #[inline]
    #[must_use]
    pub fn last(&self) -> Option<&T> { self.0.last() }

    //==============================================================
    // Edit

    #[inline]
    #[must_use]
    pub fn last_mut(&mut self) -> Option<&mut T> { self.0.last_mut() }

    #[inline]
    pub fn push(&mut self, value: T) { self.0.push(value); }

    #[inline]
    pub fn insert(&mut self, index: usize, element: T) { self.0.insert(index, element); }

    #[inline]
    pub fn pop(&mut self) -> Option<T> { self.0.pop() }

    #[inline]
    pub fn remove(&mut self, index: usize) -> T { self.0.remove(index) }

    #[inline]
    pub fn swap_remove(&mut self, index: usize) -> T { self.0.swap_remove(index) }

    #[inline]
    pub fn clear(&mut self) { self.0.clear(); }

    #[inline]
    pub fn truncate(&mut self, len: usize) { self.0.truncate(len); }

    #[inline]
    pub fn sort_unstable(&mut self)
    where
        T: Ord
    {
        self.0.sort_unstable();
    }

    #[inline]
    pub fn sort_by<F>(&mut self, compare: F)
    where
        F: FnMut(&T, &T) -> std::cmp::Ordering
    {
        self.0.sort_by(compare);
    }

    #[inline]
    pub fn reverse(&mut self) { self.0.reverse(); }

    #[inline]
    pub fn retain_mut<F>(&mut self, f: F)
    where
        F: FnMut(&mut T) -> bool
    {
        self.0.retain_mut(f);
    }

    #[cfg(feature = "arena_alloc")]
    #[inline]
    pub fn drain<R>(&mut self, range: R) -> std::vec::Drain<T, &'static BlinkAlloc>
    where
        R: std::ops::RangeBounds<usize>
    {
        self.0.drain(range)
    }

    #[cfg(not(feature = "arena_alloc"))]
    #[inline]
    pub fn drain<R>(&mut self, range: R) -> smallvec::Drain<[T; 1]>
    where
        R: std::ops::RangeBounds<usize>
    {
        self.0.drain(range)
    }

    #[inline]
    pub fn split_at_mut(&mut self, mid: usize) -> (&mut [T], &mut [T]) { self.0.split_at_mut(mid) }

    //==============================================================
    // Iterators

    #[inline]
    pub fn iter(&self) -> std::slice::Iter<T> { self.0.iter() }

    #[inline]
    pub fn iter_mut(&mut self) -> std::slice::IterMut<T> { self.0.iter_mut() }

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
    #[inline]
    pub fn new() -> Self
    {
        #[cfg(feature = "arena_alloc")]
        let map = hashbrown::HashMap::new_in(blink_alloc());

        #[cfg(not(feature = "arena_alloc"))]
        let map = hashbrown::HashMap::new();

        Self(map)
    }

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self
    {
        #[cfg(feature = "arena_alloc")]
        let map = hashbrown::HashMap::with_capacity_in(capacity, blink_alloc());

        #[cfg(not(feature = "arena_alloc"))]
        let map = hashbrown::HashMap::with_capacity(capacity);

        Self(map)
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize { self.0.len() }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool { self.0.is_empty() }

    #[inline]
    pub fn clear(&mut self) { self.0.clear() }

    #[inline]
    pub fn iter(&self) -> hashbrown::hash_map::Iter<'_, K, V> { self.0.iter() }

    #[inline]
    pub fn iter_mut(&mut self) -> hashbrown::hash_map::IterMut<'_, K, V> { self.0.iter_mut() }

    #[inline]
    pub fn keys(&self) -> hashbrown::hash_map::Keys<'_, K, V> { self.0.keys() }

    #[inline]
    pub fn values(&self) -> hashbrown::hash_map::Values<'_, K, V> { self.0.values() }

    #[inline]
    pub fn values_mut(&mut self) -> hashbrown::hash_map::ValuesMut<'_, K, V> { self.0.values_mut() }
}

impl<K: std::hash::Hash + std::cmp::Eq, V> HvHashMap<K, V>
{
    #[inline]
    pub fn contains_key<Q>(&self, k: &Q) -> bool
    where
        Q: ?Sized + Hash + Equivalent<K>
    {
        self.0.contains_key(k)
    }

    #[inline]
    pub fn insert(&mut self, k: K, v: V) -> Option<V> { self.0.insert(k, v) }

    #[inline]
    pub fn remove<Q>(&mut self, k: &Q) -> Option<V>
    where
        Q: ?Sized + Hash + Equivalent<K>
    {
        self.0.remove(k)
    }

    #[inline]
    pub fn get<Q: ?Sized + Hash + Equivalent<K>>(&self, k: &Q) -> Option<&V> { self.0.get(k) }

    #[inline]
    pub fn get_mut<Q>(&mut self, k: &Q) -> Option<&mut V>
    where
        Q: ?Sized + Hash + Equivalent<K>
    {
        self.0.get_mut(k)
    }

    #[inline]
    pub fn get_many_mut<Q, const N: usize>(&mut self, ks: [&Q; N]) -> Option<[&'_ mut V; N]>
    where
        Q: ?Sized + Hash + Equivalent<K>
    {
        self.0.get_many_mut(ks)
    }

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
/// hashbrown [`HashSet`] alias.
#[derive(Clone, Debug)]
pub struct HvHashSet<T>(hashbrown::HashSet<T, DefaultHashBuilder, &'static BlinkAlloc>);

#[cfg(not(feature = "arena_alloc"))]
/// hashbrown [`HashSet`] alias.
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
    #[inline]
    #[must_use]
    pub fn contains(&self, value: &T) -> bool { self.0.contains(value) }
}

impl<T: Hash + Eq> HvHashSet<T>
{
    //==============================================================
    // New

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

    #[inline]
    pub fn insert(&mut self, value: T) -> bool { self.0.insert(value) }

    #[inline]
    pub fn remove(&mut self, value: &T) -> bool { self.0.remove(value) }
}

impl<T> HvHashSet<T>
{
    //==============================================================
    // Info

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool { self.0.is_empty() }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize { self.0.len() }

    //==============================================================
    // Edit

    #[inline]
    pub fn clear(&mut self) { self.0.clear(); }

    #[inline]
    pub fn retain<F>(&mut self, f: F)
    where
        F: FnMut(&T) -> bool
    {
        self.0.retain(f);
    }

    //==============================================================
    // Iterators

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
