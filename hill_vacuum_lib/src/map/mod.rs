pub mod brush;
mod camera;
pub mod drawer;
mod editor;
mod ordered_map;
pub mod thing;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{
    cmp::Ordering,
    fs::File,
    hash::Hash,
    io::BufReader,
    ops::{Index, IndexMut, Range, RangeBounds, RangeInclusive},
    path::PathBuf,
    slice::Chunks
};

use bevy::{
    input::mouse::MouseWheel,
    prelude::*,
    render::{camera::RenderTarget, render_resource::Extent3d},
    sprite::Mesh2dHandle,
    window::{PrimaryWindow, WindowCloseRequested, WindowMode},
    winit::WinitSettings
};
use bevy_egui::{egui, EguiContext, EguiContextQuery, EguiContexts, EguiPlugin, EguiUserTextures};
#[cfg(feature = "arena_alloc")]
use blink_alloc::BlinkAlloc;
use hashbrown::{hash_map::DefaultHashBuilder, Equivalent, HashSet};
use serde::{Deserialize, Deserializer, Serialize};
use shared::{return_if_err, return_if_none, NextValue};
#[cfg(not(feature = "arena_alloc"))]
use smallvec::SmallVec;

use self::{
    brush::{Brush, BrushViewer},
    camera::init_camera_transform,
    drawer::{
        color::Color,
        drawing_resources::DrawingResources,
        texture_loader::{TextureLoader, TextureLoadingProgress}
    },
    editor::{
        state::clipboard::{Prop, PropCameras, PropCamerasMut},
        Editor
    },
    thing::ThingInstance
};
use crate::{
    config::Config,
    map::editor::state::clipboard::{PaintToolPropCamera, PropCamera},
    utils::{
        hull::{EntityHull, Hull},
        identifiers::Id,
        iterators::{
            PairIterator,
            PairIteratorMut,
            SlicePairIter,
            SlicePairIterMut,
            SliceTripletIter,
            TripletIterator
        },
        misc::{NoneIfEmpty, ReplaceValues, TakeValue, Toggle}
    },
    Animation,
    EditorState,
    HardcodedThings,
    TextureInterface,
    PROP_CAMERAS_AMOUNT,
    PROP_CAMERAS_ROWS
};

//=======================================================================//
// STATICS
//
//=======================================================================//

#[cfg(feature = "arena_alloc")]
static mut ALLOCATOR: BlinkAlloc = BlinkAlloc::with_chunk_size(32_768);

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

/// The size of half of the map square.
const MAP_HALF_SIZE: f32 = 16384f32;
/// The size of the map square.
const MAP_SIZE: f32 = MAP_HALF_SIZE * 2f32;
/// The range of the map dimensions.
const MAP_RANGE: RangeInclusive<f32> = -MAP_HALF_SIZE..=MAP_HALF_SIZE;
/// The [`Hull`] representing the map's area.
const MAP_RECT: Hull = unsafe {
    std::mem::transmute::<_, Hull>([MAP_HALF_SIZE, -MAP_HALF_SIZE, -MAP_HALF_SIZE, MAP_HALF_SIZE])
};

/// The general offset of the tooltips.
const TOOLTIP_OFFSET: egui::Vec2 = egui::Vec2::new(0f32, -12.5);
/// Cyan in [`egui::Color32`] format.
const EGUI_CYAN: egui::Color32 = egui::Color32::from_rgb(0, 255, 255);

//=======================================================================//
// MACROS
//
//=======================================================================//

/// Creates a new [`HvVec`] based on the parameters.
macro_rules! hv_vec {
    [] => (
        crate::map::HvVec::new()
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
        let mut vec = crate::map::hv_vec![];
        $(vec.push($x);)+
        vec
    });

    [collect; $x:expr] => ({
        let mut vec = crate::map::hv_vec![];
        vec.extend($x);
        vec
    });
}

use hv_vec;

//=======================================================================//

/// Creates a new [`HvHashMap`] based on the parameters.
macro_rules! hv_hash_map {
    [] => {{
        #[cfg(feature = "arena_alloc")]
        let map = crate::map::HvHashMap::new_in(crate::map::blink_alloc());

        #[cfg(not(feature = "arena_alloc"))]
        let map = hashbrown::HashMap::new();

        map
    }};

    [capacity; $n:expr] => {{
        #[cfg(feature = "arena_alloc")]
        let map = crate::map::HvHashMap::with_capacity_in($n, crate::map::blink_alloc());

        #[cfg(not(feature = "arena_alloc"))]
        let map = hashbrown::HashMap::with_capacity($n);

        map
    }};

    [$(($k:expr, $v:expr)),+] => ({
        let mut map = crate::map::hv_hash_map![];
        $(map.insert($k, $v);)+
        map
    });

    [collect; $x:expr] => ({
        let mut vec = crate::map::hv_hash_map![];
        vec.extend($x);
        vec
    });
}

use hv_hash_map;

//=======================================================================//

/// Creates a new [`HvHashSet`] based on the parameters.
macro_rules! hv_hash_set {
    [] => {
        crate::map::HvHashSet::new()
    };

    [capacity; $n:expr] => (
        crate::map::HvHashSet::with_capacity($n)
    );

    [$($v:expr),+] => ({
        let mut map = crate::map::hv_hash_set![];
        $(map.insert($v);)+
        map
    });

    [collect; $x:expr] => ({
        let mut vec = crate::map::hv_hash_set![];
        vec.extend($x);
        vec
    });
}

use hv_hash_set;

//=======================================================================//

/// Creates a new [`HvBox`].
macro_rules! hv_box {
    ($x:expr) => {{
        #[cfg(feature = "arena_alloc")]
        let b = crate::map::HvBox::new_in($x, crate::map::blink_alloc());

        #[cfg(not(feature = "arena_alloc"))]
        let b = Box::new($x);

        b
    }};
}

use hv_box;

//=======================================================================//
// TRAIT
//
//=======================================================================//

/// A trait to determine wherever an entity fits within the map's bounds.
pub trait OutOfBounds
{
    /// Whever the entity fits within the map bounds.
    #[must_use]
    fn out_of_bounds(&self) -> bool;
}

impl OutOfBounds for Hull
{
    #[inline]
    fn out_of_bounds(&self) -> bool
    {
        self.top() > MAP_RECT.top() ||
            self.bottom() < MAP_RECT.bottom() ||
            self.left() < MAP_RECT.left() ||
            self.right() > MAP_RECT.right()
    }
}

impl OutOfBounds for Vec2
{
    #[inline]
    fn out_of_bounds(&self) -> bool { !MAP_RECT.contains_point(*self) }
}

impl OutOfBounds for f32
{
    #[inline]
    fn out_of_bounds(&self) -> bool { self.abs() > MAP_HALF_SIZE }
}

impl<T: EntityHull> OutOfBounds for T
{
    fn out_of_bounds(&self) -> bool { self.hull().out_of_bounds() }
}

//=======================================================================//

/// A trait for collections that allows to insert and remove a value but causes the application to
/// panic if the insert or remove was unsuccesful.
trait AssertedInsertRemove<T, U, V, X>
{
    /// Insert `value` in the collection. Panics if the collection already contains `value`.
    fn asserted_insert(&mut self, value: T) -> V;

    /// Remove `value` from the collection. Panics if the collection does not contain `value`.
    fn asserted_remove(&mut self, value: &U) -> X;
}

impl<K, V> AssertedInsertRemove<(K, V), K, (), V> for HvHashMap<K, V>
where
    K: Eq + std::hash::Hash
{
    /// Inserts `value`, a (key, element) pair. Panics if the collection already contains the key.
    #[inline]
    fn asserted_insert(&mut self, value: (K, V))
    {
        assert!(self.insert(value.0, value.1).is_none(), "Key is a already present.");
    }

    /// Remove the element associated with the key `value`. Panics if the collection does not
    /// contain `value`. Returns the removed element.
    #[inline]
    fn asserted_remove(&mut self, value: &K) -> V { self.remove(value).unwrap() }
}

//=======================================================================//

impl Toggle for WindowMode
{
    /// Switches the [`WindowMode`] from windowed to borderless fullscreen, and viceversa.
    #[inline]
    fn toggle(&mut self)
    {
        *self = match self
        {
            WindowMode::Windowed => WindowMode::BorderlessFullscreen,
            WindowMode::BorderlessFullscreen => WindowMode::Windowed,
            _ => unreachable!()
        };
    }
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The two execution steps of the running application.
#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
enum EditorSet
{
    /// Update entities.
    Update,
    /// Draw visible entities.
    Draw
}

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
        F: FnMut(&T, &T) -> Ordering
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
        R: RangeBounds<usize>
    {
        self.0.drain(range)
    }

    #[cfg(not(feature = "arena_alloc"))]
    #[inline]
    pub fn drain<R>(&mut self, range: R) -> smallvec::Drain<[T; 1]>
    where
        R: RangeBounds<usize>
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
type HvHashMap<K, V> = hashbrown::HashMap<K, V, DefaultHashBuilder, &'static BlinkAlloc>;
#[cfg(not(feature = "arena_alloc"))]
/// hashbrown [`HashMap`] alias.
type HvHashMap<K, V> = hashbrown::HashMap<K, V, DefaultHashBuilder>;

impl<'a, K: std::hash::Hash + std::cmp::Eq + Copy, V: Copy> ReplaceValues<(&'a K, &'a V)>
    for HvHashMap<K, V>
{
    #[inline]
    fn replace_values<I: IntoIterator<Item = (&'a K, &'a V)>>(&mut self, iter: I)
    {
        self.clear();
        self.extend(iter.into_iter().map(|(k, v)| (*k, *v)));
    }
}

impl<K: std::hash::Hash + std::cmp::Eq, V> TakeValue for HvHashMap<K, V>
{
    #[inline]
    #[must_use]
    fn take_value(&mut self) -> Self { std::mem::replace(self, hv_hash_map![]) }
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
        assert!(self.0.remove(value), "Value was not present.");
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
type HvBox<T> = Box<T, &'static BlinkAlloc>;
#[cfg(not(feature = "arena_alloc"))]
/// [`Box`] alias.
type HvBox<T> = Box<T>;

//=======================================================================//

type MainCameraQuery<'world, 'state, 'a> = Query<
    'world,
    'state,
    &'a Transform,
    (With<Camera>, Without<PropCamera>, Without<PaintToolPropCamera>)
>;

//=======================================================================//

type MainCameraQueryMut<'world, 'state, 'a> = Query<
    'world,
    'state,
    &'a mut Transform,
    (With<Camera>, Without<PropCamera>, Without<PaintToolPropCamera>)
>;

//=======================================================================//

type PaintToolCameraQuery<'world, 'state, 'a> =
    Query<'world, 'state, &'a Transform, (With<PaintToolPropCamera>, Without<PropCamera>)>;

//=======================================================================//

type PaintToolCameraQueryMut<'world, 'state, 'a> = Query<
    'world,
    'state,
    (&'a mut Camera, &'a mut Transform),
    (With<PaintToolPropCamera>, Without<PropCamera>)
>;

//=======================================================================//

/// The plugin that builds the map editor.
#[allow(clippy::module_name_repetitions)]
pub struct MapEditorPlugin;

impl Plugin for MapEditorPlugin
{
    #[inline]
    fn build(&self, app: &mut App)
    {
        app
            // UI
            .add_plugins(EguiPlugin)
            // Init resources
            .insert_non_send_resource(Editor::placeholder())
            .insert_state(TextureLoadingProgress::default())
            .insert_resource(ClearColor(Color::Clear.bevy_color()))
            .insert_resource(WinitSettings::default())
            .init_resource::<TextureLoader>()
            // Setup
            .add_systems(PostStartup, initialize)
            // Texture loading
            .add_systems(
                Update,
                (load_textures, texture_loading_ui).chain().run_if(not(in_state(TextureLoadingProgress::Complete)))
            )
            .add_systems(
                OnEnter(TextureLoadingProgress::Complete),
                (store_loaded_textures, apply_state_transition::<EditorState>).chain()
            )
            // Handle brush creation and editing
            .add_systems(
                Update,
                (
                    update_state,
                    update_active_tool,
                    apply_state_transition::<EditorState>
                )
                .chain()
                .in_set(EditorSet::Update)
                .run_if(in_state(EditorState::Run))
            )
            .add_systems(
                Update,
                draw
                    .in_set(EditorSet::Draw)
                    .after(EditorSet::Update)
                    .run_if(in_state(EditorState::Run))
            )
            // Shutdowm
            .add_systems(OnEnter(EditorState::ShutDown), cleanup);
    }
}

//=======================================================================//

/// The header of the saved map file.
#[derive(Clone, Copy, Serialize, Deserialize)]
struct MapHeader
{
    /// The amount of brushes.
    pub brushes:    usize,
    /// The amount of things.
    pub things:     usize,
    /// The amount of animations.
    pub animations: usize,
    /// The amount of props.
    pub props:      usize
}

//=======================================================================//

/// The struct used to read a map file and generate the brushes and things to be used to generate
/// another file format.
#[must_use]
pub struct Exporter(pub HvVec<BrushViewer>, pub HvVec<ThingInstance>);

impl Exporter
{
    /// Returns a new [`Exporter`] generated from the requested `path`, unless there was an error.
    /// # Errors
    /// Returns an error if there was an issue reading the requested file.
    #[allow(clippy::missing_panics_doc)]
    #[inline]
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, &'static str>
    {
        let file = match File::open(Into::<PathBuf>::into(path))
        {
            Ok(file) => file,
            Err(_) => return Err("Could not open the file")
        };

        let mut file = BufReader::new(file);

        let header = match ciborium::from_reader::<MapHeader, _>(&mut file)
        {
            Ok(header) => header,
            Err(_) => return Err("Error reading file header")
        };

        let animations = match DrawingResources::file_animations(header.animations, &mut file)
        {
            Ok(animations) => animations,
            Err(_) => return Err("Error reading default animations")
        };

        let mut brushes = hv_vec![];

        for _ in 0..header.brushes
        {
            let brush = match ciborium::from_reader::<Brush, _>(&mut file)
            {
                Ok(brush) => brush,
                Err(_) => return Err("Error reading Brush")
            };

            brushes.push(BrushViewer::new(brush));
        }

        if !animations.is_empty()
        {
            // Replaces the empty animations of a brush with the texture's default one.
            let mut textured_anim_none = brushes
                .iter()
                .enumerate()
                .filter_map(|(i, brush)| {
                    matches!(return_if_none!(&brush.texture, None).animation(), Animation::None)
                        .then_some(i)
                })
                .collect::<Vec<_>>();

            for animation in animations
            {
                let mut i = 0;

                while i < textured_anim_none.len()
                {
                    let brush = &mut brushes[textured_anim_none[i]];

                    if brush.texture.as_ref().unwrap().name() == animation.texture
                    {
                        brush.set_texture_animation(animation.animation.clone());
                        textured_anim_none.swap_remove(i);
                        continue;
                    }

                    i += 1;
                }
            }
        }

        let mut things = hv_vec![];

        for _ in 0..header.things
        {
            let thing = match ciborium::from_reader::<ThingInstance, _>(&mut file)
            {
                Ok(thing) => thing,
                Err(_) => return Err("Error reading ThingInstance")
            };

            things.push(thing);
        }

        Ok(Self(brushes, things))
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Initializes the editor.
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::cast_precision_loss)]
#[inline]
pub(in crate::map) fn initialize(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut egui_contexts: Query<EguiContextQuery>
)
{
    macro_rules! camera {
        ($marker:ident) => {
            #[must_use]
            #[inline]
            fn prop_camera(images: &mut Assets<Image>, pos: Vec2) -> (Camera2dBundle, $marker)
            {
                (
                    Camera2dBundle {
                        camera: Camera {
                            is_active: false,
                            target: RenderTarget::Image(images.add(Prop::image(Extent3d {
                                width:                 1,
                                height:                1,
                                depth_or_array_layers: 1
                            }))),
                            ..Default::default()
                        },
                        transform: Transform::from_translation(pos.extend(0f32)),
                        ..Default::default()
                    },
                    $marker
                )
            }
        };
    }

    let mut context = egui_contexts.iter_mut().next_value();

    // Cameras.
    commands.spawn(Camera2dBundle {
        transform: init_camera_transform(),
        ..Default::default()
    });

    let mut prop_cameras_amount = 0;
    let mut y = 0f32;

    for i in 0..PROP_CAMERAS_ROWS
    {
        camera!(PropCamera);

        let plus_one = i + 1;
        let start = MAP_SIZE * (plus_one as f32);
        y = -start;

        for _ in 0..=(plus_one * 2)
        {
            commands.spawn(prop_camera(&mut images, Vec2::new(-start, y)));
            commands.spawn(prop_camera(&mut images, Vec2::new(start, y)));

            y += MAP_SIZE;
            prop_cameras_amount += 2;
        }

        let mut x = -start + MAP_SIZE;

        for _ in 0..=(i * 2)
        {
            commands.spawn(prop_camera(&mut images, Vec2::new(x, start)));
            commands.spawn(prop_camera(&mut images, Vec2::new(x, -start)));

            x += MAP_SIZE;
            prop_cameras_amount += 2;
        }
    }

    assert!(prop_cameras_amount == PROP_CAMERAS_AMOUNT);

    camera!(PaintToolPropCamera);
    commands.spawn(prop_camera(&mut images, Vec2::new(0f32, y + MAP_SIZE)));

    // Extract necessary values.
    let ctx = context.ctx.get_mut();

    // Do a fake frame thing to allow the labels initialization.
    ctx.begin_frame(egui::RawInput::default());
    let full_output = ctx.end_frame();

    let egui::FullOutput {
        platform_output,
        textures_delta,
        ..
    } = full_output;
    context.render_output.textures_delta.append(textures_delta);
    context.egui_output.platform_output = platform_output.clone();

    // Set looks.
    let mut style = (*ctx.style()).clone();
    for font in style.text_styles.values_mut()
    {
        font.size += 2f32;
    }
    ctx.set_style(style);

    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = egui::Color32::WHITE.into();
    ctx.set_visuals(visuals);
}

//=======================================================================//

/// Stores the loaded textures in the [`Editor`].
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::too_many_arguments)]
#[inline]
fn store_loaded_textures(
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    mut prop_cameras: PropCamerasMut,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut user_textures: ResMut<EguiUserTextures>,
    mut editor: NonSendMut<Editor>,
    config: Res<Config>,
    mut texture_loader: ResMut<TextureLoader>,
    hardcoded_things: Option<Res<HardcodedThings>>,
    state: Res<State<EditorState>>,
    mut next_state: ResMut<NextState<EditorState>>
)
{
    if *state.get() == EditorState::SplashScreen
    {
        *editor = Editor::new(
            window.single_mut().as_mut(),
            &mut prop_cameras,
            &asset_server,
            &mut images,
            &mut meshes,
            &mut materials,
            &mut user_textures,
            &config,
            &mut texture_loader,
            hardcoded_things
        );

        next_state.set(EditorState::Run);
        return;
    }

    editor.reload_textures(&mut materials, texture_loader.loaded_textures());
}

//=======================================================================//

/// Updates the editor state.
#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::too_many_arguments)]
#[inline]
fn update_state(
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
    mut camera: MainCameraQueryMut,
    mut prop_cameras: PropCamerasMut,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut mouse_wheel: EventReader<MouseWheel>,
    mut key_inputs: ResMut<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut close_events: EventReader<WindowCloseRequested>,
    mut egui_contexts: Query<
        (&'static mut EguiContext, Option<&'static PrimaryWindow>),
        With<Window>
    >,
    mut user_textures: ResMut<EguiUserTextures>,
    mut editor: NonSendMut<Editor>,
    mut config: ResMut<Config>,
    mut next_editor_state: ResMut<NextState<EditorState>>,
    mut next_tex_load: ResMut<NextState<TextureLoadingProgress>>
)
{
    let mut window = return_if_err!(window.get_single_mut());
    let egui_context = egui_contexts
        .iter_mut()
        .find_map(|(ctx, pw)| pw.map(|_| ctx))
        .unwrap()
        .into_inner()
        .get_mut();
    let mut camera = camera.single_mut();

    if close_events.read().next().is_some() &&
        editor.quit(
            &mut window,
            &mut images,
            &mut camera,
            &mut prop_cameras,
            &time,
            egui_context,
            &mut user_textures,
            &mouse_buttons,
            &mut key_inputs,
            &mut config,
            &mut next_editor_state,
            &mut next_tex_load
        )
    {
        return;
    }

    editor.update(
        &mut window,
        &mut images,
        &mut camera,
        &mut prop_cameras,
        &time,
        egui_context,
        &mut user_textures,
        &mouse_buttons,
        &mut mouse_wheel,
        &mut key_inputs,
        &mut config,
        &mut next_editor_state,
        &mut next_tex_load
    );
}

//=======================================================================//

/// Updates the active tool.
#[allow(clippy::needless_pass_by_value)]
#[inline]
fn update_active_tool(
    window: Query<&Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
    mut camera: MainCameraQueryMut,
    mut prop_cameras: PropCamerasMut,
    mut paint_tool_camera: PaintToolCameraQueryMut,
    time: Res<Time>,
    mut user_textures: ResMut<EguiUserTextures>,
    mut editor: NonSendMut<Editor>
)
{
    let mut paint_tool_camera = paint_tool_camera.single_mut();

    editor.update_active_tool(
        return_if_err!(window.get_single()),
        &mut images,
        camera.get_single_mut().unwrap().as_mut(),
        &mut prop_cameras,
        (paint_tool_camera.0.as_mut(), paint_tool_camera.1.as_mut()),
        &time,
        &mut user_textures
    );
}

//=======================================================================//

/// Draws the visible portion of the map.
#[allow(clippy::needless_pass_by_value)]
#[inline]
fn draw(
    mut commands: Commands,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: MainCameraQuery,
    prop_cameras: PropCameras,
    paint_tool_camera: PaintToolCameraQuery,
    mut meshes: ResMut<Assets<Mesh>>,
    time: Res<Time>,
    mut egui_context: EguiContexts,
    meshes_query: Query<Entity, With<Mesh2dHandle>>,
    mut editor: NonSendMut<Editor>,
    #[cfg(feature = "debug")] mut gizmos: bevy::gizmos::gizmos::Gizmos
)
{
    editor.draw(
        &mut commands,
        return_if_err!(window.get_single()),
        camera.single(),
        &prop_cameras,
        paint_tool_camera.single(),
        &time,
        &mut meshes,
        egui_context.ctx_mut(),
        &meshes_query,
        #[cfg(feature = "debug")]
        &mut gizmos
    );
}

//=======================================================================//

/// Shutdown cleanup.
#[allow(clippy::needless_pass_by_value)]
#[inline]
fn cleanup(mut meshes: ResMut<Assets<Mesh>>, editor: NonSend<Editor>)
{
    editor.cleanup(&mut meshes);
}

//=======================================================================//

/// Loads the textures from the assets files.
#[allow(clippy::needless_pass_by_value)]
#[inline]
fn load_textures(
    mut images: ResMut<Assets<Image>>,
    mut user_textures: ResMut<EguiUserTextures>,
    mut texture_loader: ResMut<TextureLoader>,
    mut load_state: ResMut<NextState<TextureLoadingProgress>>
)
{
    texture_loader.load(&mut images, &mut user_textures, &mut load_state);
}

//=======================================================================//

/// The UI of the texture loading process.
#[allow(clippy::needless_pass_by_value)]
#[inline]
fn texture_loading_ui(
    window: Query<&Window, With<PrimaryWindow>>,
    mut egui_context: EguiContexts,
    texture_loader: Res<TextureLoader>
)
{
    texture_loader.ui(window.single(), egui_context.ctx_mut());
}

//=======================================================================//

#[cfg(feature = "arena_alloc")]
/// Returns a static reference to the arena allocator.
#[inline]
#[must_use]
fn blink_alloc() -> &'static BlinkAlloc { unsafe { &*core::ptr::addr_of!(crate::map::ALLOCATOR) } }
