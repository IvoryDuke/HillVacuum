//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::ops::Range;

use arrayvec::ArrayVec;

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The result of the iteration of [`FilteredSet`].
enum FilterResult
{
    /// The element was filtered out.
    Filtered,
    /// The element was not filtered.
    NotFiltered,
    /// There are no more filters.
    Empty
}

//=======================================================================//
// TRAITS
//
//=======================================================================//

/// A trait for iterators of sets to not have them return specific elements
/// which may or may not be part of them.
pub trait FilterSet
{
    /// Returns a set iterator where the elements of `filters` are filtered out.
    #[inline]
    fn filter_set<'a, T, const N: usize>(
        self,
        filters: impl Into<Filters<T, N>>
    ) -> FilteredSet<'a, Self, T, fn(&T) -> T, N>
    where
        Self: Sized + Iterator<Item = T>,
        T: PartialEq + Eq + Copy
    {
        /// Copies the value for comparison.
        #[inline]
        #[must_use]
        const fn predicate<T: PartialEq + Eq + Copy>(value: &T) -> T { *value }
        FilteredSet::new(self, filters, predicate)
    }

    /// Returns a set iterator where the elements of `filters` are filtered out based on a
    /// predicate.
    #[inline]
    fn filter_set_with_predicate<'a, T, P, const N: usize>(
        self,
        filters: impl Into<Filters<T, N>>,
        predicate: P
    ) -> FilteredSet<'a, Self, T, P, N>
    where
        Self: Sized + Iterator,
        T: PartialEq + Eq,
        P: Fn(&Self::Item) -> T
    {
        FilteredSet::new(self, filters, predicate)
    }
}

impl<T> FilterSet for T where T: Iterator {}

//=======================================================================//

/// A trait to create an iterator that returns the elements in pairs.
pub trait PairIterator<'a, T, I: Iterator<Item = [T; 2]>>
{
    /// Returns an iterator that returns the elements in pairs.
    fn pair_iter(&'a self) -> Option<I>;
}

impl<'a, T: 'a> PairIterator<'a, &'a T, SlicePairIter<'a, T>> for [T]
{
    /// Returns an iterator that returns the elements of a slice in pairs.
    /// Returns None if the elements are less than two.
    /// # Example
    /// ```rust
    /// let values = [0, 1, 2];
    /// let mut iter = values.pair_iter().unwrap();
    /// assert!(iter.next() == [2, 0]).into();
    /// assert!(iter.next() == [0, 1]).into();
    /// assert!(iter.next() == [1, 2]).into();
    /// assert!(iter.next() == None);
    /// ```
    #[inline]
    #[must_use]
    fn pair_iter(&'a self) -> Option<SlicePairIter<'a, T>> { SlicePairIter::new(self) }
}

//=======================================================================//

/// A trait to create an iterator that returns the elements in pairs.
pub trait PairIteratorMut<'a, T: 'a, I: Iterator<Item = [&'a mut T; 2]>>
{
    /// Returns an iterator that returns the elements in pairs.
    fn pair_iter_mut(&'a mut self) -> Option<I>;
}

impl<'a, T: 'a> PairIteratorMut<'a, T, SlicePairIterMut<'a, T>> for [T]
{
    /// Returns an iterator that returns the elements of a slice in pairs.
    /// Returns None if the elements are less than two.
    /// # Example
    /// ```rust
    /// let mut values = [0, 1, 2];
    /// let mut iter = values.pair_iter_mut().unwrap();
    /// assert!(iter.next() == [2, 0]).into();
    /// assert!(iter.next() == [0, 1]).into();
    /// assert!(iter.next() == [1, 2]).into();
    /// assert!(iter.next() == None);
    /// ```
    #[inline]
    #[must_use]
    fn pair_iter_mut(&'a mut self) -> Option<SlicePairIterMut<'a, T>>
    {
        SlicePairIterMut::new(self)
    }
}

//=======================================================================//

/// A trait to create an iterator that returns the elements in triplets.
pub trait TripletIterator<'a, T, I: Iterator<Item = [T; 3]>>
{
    /// Returns an iterator that returns the elements in tripets.
    fn triplet_iter(&'a self) -> Option<I>;
}

impl<'a, T: 'a> TripletIterator<'a, &'a T, SliceTripletIter<'a, T>> for [T]
{
    /// Returns an iterator that returns the elements of a slice in triplets.
    /// Returns None if the elements are less than three.
    /// # Example
    /// ```rust
    /// let values = [0, 1, 2];
    /// let mut iter = values.pair_iter().unwrap();
    /// assert!(iter.next() == [1, 2, 0]).into();
    /// assert!(iter.next() == [2, 0, 1]).into();
    /// assert!(iter.next() == [0, 1, 2]).into();
    /// assert!(iter.next() == None);
    /// ```
    #[inline]
    #[must_use]
    fn triplet_iter(&'a self) -> Option<SliceTripletIter<'a, T>> { SliceTripletIter::new(self) }
}

//=======================================================================//

/// A trait for iterators to have them return the indexes of the elements.
pub trait Enumeration<const N: usize>
{
    /// Returns the indexes of the iterated elements.
    #[must_use]
    fn enumeration(&self) -> [usize; N];
}

//=======================================================================//

/// A trait for iterators to not have them iterate a certain index.
pub trait SkipIndexIterator
{
    /// Returns an iterator that skips a certain index.
    /// Returns None if the index is not within bounds.
    #[inline]
    fn skip_index(self, index: usize) -> Option<SkipIndex<Self>>
    where
        Self: Sized + Iterator + ExactSizeIterator
    {
        SkipIndex::new(self, index)
    }
}

impl<T> SkipIndexIterator for T where T: Iterator + ExactSizeIterator {}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The elements to be skipped by [`FilteredSet`].
#[must_use]
pub struct Filters<T, const N: usize>(ArrayVec<T, N>);

impl<T> From<T> for Filters<T, 1>
{
    #[inline]
    fn from(value: T) -> Self { Self(ArrayVec::from([value])) }
}

impl<T, const N: usize> From<[T; N]> for Filters<T, N>
{
    #[inline]
    fn from(value: [T; N]) -> Self { Self(ArrayVec::from(value)) }
}

impl<T, const N: usize> From<ArrayVec<T, N>> for Filters<T, N>
{
    #[inline]
    fn from(value: ArrayVec<T, N>) -> Self { Self(value) }
}

impl<T, const N: usize> Filters<T, N>
where
    T: PartialEq + Eq
{
    /// Returns a value which represents whever `value` should be filtered.
    #[inline]
    #[must_use]
    fn filter(&mut self, value: &T) -> FilterResult
    {
        assert!(!self.0.is_empty(), "Tried to use empty Filters.");

        for i in 0..self.0.len()
        {
            if *value != self.0[i]
            {
                continue;
            }

            self.0.swap_remove(i);

            return if self.0.is_empty() { FilterResult::Empty } else { FilterResult::Filtered };
        }

        FilterResult::NotFiltered
    }
}

//=======================================================================//

/// An iterator that returns all elements except a subset that should be filtered.
pub struct FilteredSet<'a, I, T, P, const N: usize>
where
    I: Sized + Iterator
{
    /// The unfiltered elements.
    iter:      I,
    /// The elements to be filtered.
    filters:   Filters<T, N>,
    /// The predicate that allows to compare `iter`'s elements with the `filters`.
    predicate: P,
    /// The function which generates the returned value.
    factory:   fn(&mut Self) -> Option<I::Item>
}

impl<'a, I, T, P, const N: usize> FilteredSet<'a, I, T, P, N>
where
    I: Sized + Iterator,
    T: PartialEq + Eq,
    P: Fn(&I::Item) -> T
{
    /// Creates a new [`FilteredSet`].
    #[inline]
    #[must_use]
    pub fn new(iter: I, filters: impl Into<Filters<T, N>>, predicate: P) -> Self
    {
        Self {
            iter,
            filters: filters.into(),
            predicate,
            factory: Self::filtered_iteration
        }
    }

    /// Iterates over the elements and returns those that should not be filtered.
    /// If there are no more elements to be filtered `self.factory` is set to
    /// `Self::unfiltered_iteration`.
    #[inline]
    #[must_use]
    fn filtered_iteration(&mut self) -> Option<I::Item>
    {
        loop
        {
            match self.iter.next()
            {
                Some(v) =>
                {
                    match self.filters.filter(&(self.predicate)(&v))
                    {
                        FilterResult::Filtered => (),
                        FilterResult::NotFiltered => return Some(v),
                        FilterResult::Empty =>
                        {
                            self.factory = Self::unfiltered_iteration;
                            return self.iter.next();
                        }
                    };
                },
                None => return None
            };
        }
    }

    /// Returns the elements of the iterator.
    #[inline]
    #[must_use]
    fn unfiltered_iteration(&mut self) -> Option<I::Item> { self.iter.next() }
}

impl<'a, I, T, P, const N: usize> Iterator for FilteredSet<'a, I, T, P, N>
where
    I: Sized + Iterator,
    T: PartialEq + Eq + Clone,
    P: Fn(&I::Item) -> T
{
    type Item = I::Item;

    #[inline]
    #[must_use]
    fn next(&mut self) -> Option<Self::Item> { (self.factory)(self) }
}

//=======================================================================//

/// A slices iterator that returns the elements in pairs.
/// # Example
/// ```rust
/// let values = [0, 1, 2];
/// let mut iter = values.pair_iter().unwrap();
/// assert!(iter.next() == [&2, &0]).into();
/// assert!(iter.next() == [&0, &1]).into();
/// assert!(iter.next() == [&1, &2]).into();
/// assert!(iter.next() == None);
/// ```
#[derive(Clone)]
pub struct SlicePairIter<'a, T>
{
    /// The slice containing the elements.
    slice: &'a [T],
    /// The pairs of consecutive indexes.
    iter:  RangePairIter
}

impl<'a, T> SlicePairIter<'a, T>
{
    /// Creates a new [`PairIter`]. Return None if `slice` has less than two elements.
    #[inline]
    #[must_use]
    pub fn new(slice: &'a [T]) -> Option<Self>
    {
        (0..slice.len()).pair_iter().map(|iter| Self { slice, iter })
    }

    /// Returns an iterator that returns the slice indexes of the elements along with the pair of
    /// elements themselves.
    #[inline]
    #[must_use]
    pub const fn enumerate(self) -> Enumerate<Self, &'a T, 2> { Enumerate(self) }
}

impl<'a, T> Enumeration<2> for SlicePairIter<'a, T>
{
    /// Returns the indexes of the returned pair of elements.
    #[inline]
    fn enumeration(&self) -> [usize; 2] { [self.iter.j, self.iter.i] }
}

impl<'a, T> ExactSizeIterator for SlicePairIter<'a, T>
{
    #[inline]
    #[must_use]
    fn len(&self) -> usize { self.iter.len - self.iter.i }
}

impl<'a, T> Iterator for SlicePairIter<'a, T>
{
    type Item = [&'a T; 2];

    #[inline]
    #[must_use]
    fn next(&mut self) -> Option<Self::Item>
    {
        self.iter.next().map(|[j, i]| [&self.slice[j], &self.slice[i]])
    }
}

//=======================================================================//

/// An iterator returning the pairs of consecutive indexes in a given [`Range`].
/// 0 is paired with the last element of the [`Range`].
#[must_use]
#[derive(Clone)]
pub struct RangePairIter
{
    /// The lower index.
    i:   usize,
    /// The higher index.
    j:   usize,
    /// The amount of returned elements.
    len: usize
}

impl<'a> PairIterator<'a, usize, RangePairIter> for Range<usize>
{
    #[inline]
    fn pair_iter(&'a self) -> Option<RangePairIter> { RangePairIter::new(self) }
}

impl ExactSizeIterator for RangePairIter
{
    #[inline]
    #[must_use]
    fn len(&self) -> usize { self.len - self.i }
}

impl Iterator for RangePairIter
{
    type Item = [usize; 2];

    #[inline]
    fn next(&mut self) -> Option<Self::Item>
    {
        if self.i == self.len
        {
            return None;
        }

        let value = [self.j, self.i];
        self.j = self.i;
        self.i += 1;

        value.into()
    }
}

impl RangePairIter
{
    /// Returns a new [`RangePairIter`] if `range` contains 2 or more elements.
    #[inline]
    pub fn new(range: &Range<usize>) -> Option<Self>
    {
        let len = range.len();

        (len >= 2).then(|| {
            Self {
                i: 0,
                j: range.clone().last().unwrap(),
                len
            }
        })
    }
}

//=======================================================================//

/// A slices iterator that returns the elements in pairs.
/// # Example
/// ```rust
/// let mut values = [0, 1, 2];
/// let mut iter = values.pair_iter().unwrap();
/// assert!(iter.next() == [&mut 2, &mut 0]).into();
/// assert!(iter.next() == [&mut 0, &mut 1]).into();
/// assert!(iter.next() == [&mut 1, &mut 2]).into();
/// assert!(iter.next() == None);
/// ```
pub struct SlicePairIterMut<'a, T>
{
    /// The slice containing the elements.
    slice: &'a mut [T],
    /// The pairs of consecutive indexes.
    iter:  RangePairIter
}

impl<'a, T> SlicePairIterMut<'a, T>
{
    /// Creates a new [`PairIter`]. Return None if `slice` has less than two elements.
    #[inline]
    #[must_use]
    pub fn new(slice: &'a mut [T]) -> Option<Self>
    {
        (0..slice.len()).pair_iter().map(|iter| Self { slice, iter })
    }

    /// Returns an iterator that returns the slice indexes of the elements along with the pair of
    /// elements themselves.
    #[inline]
    #[must_use]
    pub fn enumerate(self) -> Enumerate<Self, &'a mut T, 2> { Enumerate(self) }
}

impl<'a, T> Enumeration<2> for SlicePairIterMut<'a, T>
{
    /// Returns the indexes of the returned pair of elements.
    #[inline]
    fn enumeration(&self) -> [usize; 2] { [self.iter.j, self.iter.i] }
}

impl<'a, T> ExactSizeIterator for SlicePairIterMut<'a, T>
{
    fn len(&self) -> usize { self.iter.len - self.iter.i }
}

impl<'a, T> Iterator for SlicePairIterMut<'a, T>
{
    type Item = [&'a mut T; 2];

    #[inline]
    #[must_use]
    fn next(&mut self) -> Option<Self::Item>
    {
        self.iter.next().map(|[j, i]| {
            let us_slice = self.slice.as_mut_ptr();
            unsafe { [&mut *us_slice.add(j), &mut *us_slice.add(i)] }
        })
    }
}

//=======================================================================//

/// A slices iterator that returns the elements in triplets.
/// Returns None if the elements are less than three.
/// # Example
/// ```rust
/// let values = [0, 1, 2];
/// let iter = values.pair_iter().unwrap();
/// assert!(iter.next() == [1, 2, 0]).into();
/// assert!(iter.next() == [2, 0, 1]).into();
/// assert!(iter.next() == [0, 1, 2]).into();
/// assert!(iter.next() == None);
/// ```
pub struct SliceTripletIter<'a, T>
{
    /// The slice containing the elements.
    slice: &'a [T],
    /// The triplets of consecutive indexes.
    iter:  RangeTripleIter
}

impl<'a, T> SliceTripletIter<'a, T>
{
    /// Returns a new [`TripletIter`]. Returns None if the length of `slice` is less than three.
    #[inline]
    #[must_use]
    pub fn new(slice: &'a [T]) -> Option<Self>
    {
        (0..slice.len()).triplet_iter().map(|iter| Self { slice, iter })
    }

    /// Returns an iterator that returns the slice indexes of the elements along with the pair of
    /// elements themselves.
    #[inline]
    #[must_use]
    pub const fn enumerate(self) -> Enumerate<Self, &'a T, 3> { Enumerate(self) }
}

impl<'a, T> Enumeration<3> for SliceTripletIter<'a, T>
{
    /// Returns the indexes of the returned triplet of elements.
    #[inline]
    fn enumeration(&self) -> [usize; 3] { [self.iter.i, self.iter.j, self.iter.k] }
}

impl<'a, T> ExactSizeIterator for SliceTripletIter<'a, T>
{
    #[inline]
    #[must_use]
    fn len(&self) -> usize { self.iter.len - self.iter.k }
}

impl<'a, T> Iterator for SliceTripletIter<'a, T>
{
    type Item = [&'a T; 3];

    #[inline]
    #[must_use]
    fn next(&mut self) -> Option<Self::Item>
    {
        self.iter
            .next()
            .map(|[i, j, k]| [&self.slice[i], &self.slice[j], &self.slice[k]])
    }
}

//=======================================================================//

/// An iterator returning the triplets of consecutive indexes in a given [`Range`].
/// 0 is grouped with the highest and second to highest elements, and 1 is grouped with 0 and the
/// highest element.
#[must_use]
pub struct RangeTripleIter
{
    /// Low.
    i:   usize,
    /// Middle
    j:   usize,
    /// High.
    k:   usize,
    /// The amount of triplets returned.
    len: usize
}

impl<'a> TripletIterator<'a, usize, RangeTripleIter> for Range<usize>
{
    #[inline]
    fn triplet_iter(&'a self) -> Option<RangeTripleIter> { RangeTripleIter::new(self) }
}

impl ExactSizeIterator for RangeTripleIter
{
    #[inline]
    #[must_use]
    fn len(&self) -> usize { self.len - self.i }
}

impl Iterator for RangeTripleIter
{
    type Item = [usize; 3];

    #[inline]
    fn next(&mut self) -> Option<Self::Item>
    {
        if self.k == self.len
        {
            return None;
        }

        let value = [self.i, self.j, self.k];
        self.i = self.j;
        self.j = self.k;
        self.k += 1;

        value.into()
    }
}

impl RangeTripleIter
{
    /// Returns a new [`RangeTripleIter`] if `range` has 3 or more elements.
    #[inline]
    pub fn new(range: &Range<usize>) -> Option<Self>
    {
        let len = range.len();

        if len < 3
        {
            return None;
        }

        Self {
            i: len - 2,
            j: len - 1,
            k: 0,
            len
        }
        .into()
    }
}

//=======================================================================//

/// A slice iterator that returns the indexes of the elements along with the elements themselves.
pub struct Enumerate<I, T, const N: usize>(I)
where
    I: Iterator<Item = [T; N]> + ExactSizeIterator + Enumeration<N>;

impl<I, T, const N: usize> ExactSizeIterator for Enumerate<I, T, N>
where
    I: Iterator<Item = [T; N]> + ExactSizeIterator + Enumeration<N>
{
    #[inline]
    #[must_use]
    fn len(&self) -> usize { self.0.len() }
}

impl<I, T, const N: usize> Iterator for Enumerate<I, T, N>
where
    I: Iterator<Item = [T; N]> + ExactSizeIterator + Enumeration<N>
{
    type Item = ([usize; N], [T; N]);

    #[inline]
    #[must_use]
    fn next(&mut self) -> Option<Self::Item>
    {
        let idxs = self.0.enumeration();
        self.0.next().map(|v| (idxs, v))
    }
}

//=======================================================================//

/// An iterator that skips a certain index.
pub struct SkipIndex<I>
where
    I: Sized + Iterator
{
    /// The values to iterate.
    iter:    I,
    /// The current iteration index.
    index:   usize,
    /// The index to be skipped.
    skip:    usize,
    /// The returned element generator.
    factory: fn(&mut Self) -> Option<I::Item>
}

impl<I> SkipIndex<I>
where
    I: Sized + Iterator + ExactSizeIterator
{
    /// Creates a new [`SkipIndex`].
    #[inline]
    #[must_use]
    fn new(iter: I, skip: usize) -> Option<Self>
    {
        (skip < iter.len()).then_some(Self {
            iter,
            index: 0,
            skip,
            factory: Self::filtered_iteration
        })
    }

    /// Returns the next element, skipping the one at the index that should be avoided.
    /// Sets the iterator `factory` to `Self::unfiltered_iteration` once the index has been skipped.
    #[inline]
    #[must_use]
    fn filtered_iteration(&mut self) -> Option<I::Item>
    {
        if self.index == self.skip
        {
            self.index += 1;
            self.iter.next();
            self.factory = Self::unfiltered_iteration;
        }

        self.index += 1;
        self.iter.next()
    }

    /// Returns the remaining elements.
    #[inline]
    #[must_use]
    fn unfiltered_iteration(&mut self) -> Option<I::Item> { self.iter.next() }
}

impl<I> Iterator for SkipIndex<I>
where
    I: Sized + Iterator
{
    type Item = I::Item;

    #[inline]
    #[must_use]
    fn next(&mut self) -> Option<Self::Item> { (self.factory)(self) }
}
