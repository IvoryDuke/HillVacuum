//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::hash::BuildHasherDefault;

use ahash::AHasher;

use super::misc::AssertedInsertRemove;

//=======================================================================//
// MACROS
//
//=======================================================================//

macro_rules! hash_map {
        [] => {{
            crate::utils::collections::HashMap::with_hasher(std::hash::BuildHasherDefault::<ahash::AHasher>::default())
        }};

        [$(($k:expr, $v:expr)),+] => {{
            let mut map = crate::utils::collections::hash_map![];
            $(map.insert($k, $v);)+
            map
        }};

        [capacity; $n:expr] => {{
            crate::utils::collections::HashMap::with_capacity_and_hasher($n, std::hash::BuildHasherDefault::<ahash::AHasher>::default())
        }};
    }

pub(crate) use hash_map;

//=======================================================================//
// TYPES
//
//=======================================================================//

pub(crate) type HashMap<K, V> = hashbrown::HashMap<K, V, BuildHasherDefault<AHasher>>;

impl<K, V> AssertedInsertRemove<(K, V), K, (), V> for HashMap<K, V>
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

pub(crate) type HashSet<V> = hashbrown::HashSet<V, BuildHasherDefault<AHasher>>;

impl<T: std::hash::Hash + Eq> AssertedInsertRemove<T, T, (), ()> for HashSet<T>
{
    #[inline]
    fn asserted_insert(&mut self, value: T)
    {
        assert!(self.insert(value), "Value is already present.");
    }

    #[inline]
    fn asserted_remove(&mut self, value: &T)
    {
        assert!(self.remove(value), "Value is not present.");
    }
}

//=======================================================================//

pub(crate) type Ids = HashSet<crate::Id>;

//=======================================================================//
// UI
//
//=======================================================================//

#[cfg(feature = "ui")]
pub(crate) mod ui_mod
{
    //=======================================================================//
    // IMPORTS
    //
    //=======================================================================//

    use std::hash::BuildHasherDefault;

    use ahash::AHasher;

    use super::HashSet;
    use crate::utils::misc::{NoneIfEmpty, ReplaceValues};

    //=======================================================================//
    // MACROS
    //
    //=======================================================================//

    macro_rules! hash_set {
        [] => {{
            crate::utils::collections::HashSet::with_hasher(std::hash::BuildHasherDefault::<ahash::AHasher>::default())
        }};

        [$($v:expr),+] => {{
            let mut map = crate::utils::collections::hash_set![];
            $(map.insert($v);)+
            map
        }};

        [capacity; $n:expr] => {{
            crate::utils::collections::HashSet::with_capacity_and_hasher($n, std::hash::BuildHasherDefault::<ahash::AHasher>::default())
        }};
    }

    pub(crate) use hash_set;

    //=======================================================================//

    macro_rules! index_map {
        [] => {{
            crate::utils::collections::IndexMap::with_hasher(std::hash::BuildHasherDefault::<ahash::AHasher>::default())
        }};

        [$($v:expr),+] => {{
            let mut map = crate::utils::collections::index_map![];
            $(map.insert($v);)+
            map
        }};

        [capacity; $n:expr] => {{
            crate::utils::collections::IndexMap::with_capacity_and_hasher($n, std::hash::BuildHasherDefault::<ahash::AHasher>::default())
        }};
    }

    pub(crate) use index_map;

    //=======================================================================//
    // TYPES
    //
    //=======================================================================//

    pub(crate) type HvVec<T> = smallvec::SmallVec<[T; 1]>;

    impl<T> NoneIfEmpty for HvVec<T>
    {
        #[inline]
        fn none_if_empty(self) -> Option<Self>
        where
            Self: Sized
        {
            (self.is_empty()).then_some(self)
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

    //=======================================================================//

    impl<T: Eq + std::hash::Hash> ReplaceValues<T> for HashSet<T>
    {
        #[inline]
        fn replace_values<I: IntoIterator<Item = T>>(&mut self, iter: I)
        {
            self.clear();
            self.extend(iter);
        }
    }

    impl<'a, T: 'a + Eq + std::hash::Hash + Copy> ReplaceValues<&'a T> for HashSet<T>
    {
        #[inline]
        fn replace_values<I: IntoIterator<Item = &'a T>>(&mut self, iter: I)
        {
            self.clear();
            self.extend(iter);
        }
    }

    //=======================================================================//

    pub(crate) type IndexMap<K, V> = indexmap::IndexMap<K, V, BuildHasherDefault<AHasher>>;
}

#[cfg(feature = "ui")]
pub(crate) use ui_mod::*;
