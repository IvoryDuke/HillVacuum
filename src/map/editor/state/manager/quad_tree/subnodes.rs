//=======================================================================//
// IMPORTS
//
//=======================================================================//

use arrayvec::ArrayVec;
use glam::Vec2;

use super::{node::Square, QuadTree};

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The cardinality of a subnode.
#[derive(Debug, Clone, Copy)]
pub(in crate::map::editor::state::manager::quad_tree) enum Cardinality
{
    /// North-West.
    NorthWest(usize),
    /// South-West.
    SouthWest(usize),
    /// South-East.
    SouthEast(usize),
    /// North-East.
    NorthEast(usize)
}

impl Cardinality
{
    /// Returns the [`Square`] describing the area covered by the subsquare of `square` with the
    /// `cardinality` of `self`.
    #[inline]
    #[must_use]
    fn subsquare(&self, square: &Square) -> Square
    {
        let top_left = square.top_left();
        let size = square.size() / 2f32;

        match self
        {
            Self::NorthWest(_) => Square::new(top_left, size),
            Self::SouthWest(_) => Square::new(top_left + Vec2::new(0f32, -size), size),
            Self::SouthEast(_) => Square::new(top_left + Vec2::new(size, -size), size),
            Self::NorthEast(_) => Square::new(top_left + Vec2::new(size, 0f32), size)
        }
    }

    #[inline]
    #[must_use]
    pub const fn index(&self) -> usize
    {
        let (Self::NorthEast(idx) |
        Self::NorthWest(idx) |
        Self::SouthEast(idx) |
        Self::SouthWest(idx)) = self;

        *idx
    }
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The indexes of the subnodes of a partitioned [`Node`].
#[derive(Debug)]
pub(in crate::map::editor::state::manager::quad_tree) struct Subnodes([Cardinality; 4]);

impl<'a> IntoIterator for &'a Subnodes
{
    type IntoIter = std::slice::Iter<'a, Cardinality>;
    type Item = &'a Cardinality;

    #[inline]
    fn into_iter(self) -> Self::IntoIter { self.0.iter() }
}

impl Subnodes
{
    /// Returns the [`Subnodes`] of the [`Node`] of `quad_tree` at `index`.
    #[inline]
    #[must_use]
    pub fn new(quad_tree: &mut QuadTree, index: usize) -> Self
    {
        let mut subnodes = Subnodes([
            Cardinality::NorthWest(0),
            Cardinality::SouthWest(0),
            Cardinality::SouthEast(0),
            Cardinality::NorthEast(0)
        ]);
        let square = quad_tree.node(index).square();

        for cardinality in &mut subnodes.0
        {
            let node_idx = quad_tree.insert_node(&cardinality.subsquare(&square));
            let (Cardinality::NorthEast(idx) |
            Cardinality::NorthWest(idx) |
            Cardinality::SouthEast(idx) |
            Cardinality::SouthWest(idx)) = cardinality;

            *idx = node_idx;
        }

        subnodes
    }

    #[inline]
    pub fn indexes(&self) -> impl Iterator<Item = usize> + Clone
    {
        self.0
            .iter()
            .map(Cardinality::index)
            .collect::<ArrayVec<_, 4>>()
            .into_inner()
            .unwrap()
            .into_iter()
    }
}
