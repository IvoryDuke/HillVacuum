//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::Vec2;
use proc_macros::EnumIter;

use super::{node::Square, QuadTree};

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[derive(EnumIter, Clone, Copy)]
enum Cardinality
{
    NorthWest,
    SouthWest,
    SouthEast,
    NorthEast
}

impl Cardinality
{
    #[inline]
    #[must_use]
    fn subsquare(self, square: &Square) -> Square
    {
        let top_left = square.top_left();
        let size = square.size() / 2f32;

        match self
        {
            Self::NorthWest => Square::new(top_left, size),
            Self::SouthWest => Square::new(top_left + Vec2::new(0f32, -size), size),
            Self::SouthEast => Square::new(top_left + Vec2::new(size, -size), size),
            Self::NorthEast => Square::new(top_left + Vec2::new(size, 0f32), size)
        }
    }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

#[derive(Debug)]
pub(in crate::map::editor::state::manager::quad_tree) struct Subnodes([usize; 4]);

impl<'a> IntoIterator for &'a Subnodes
{
    type IntoIter = std::array::IntoIter<usize, 4>;
    type Item = usize;

    #[inline]
    fn into_iter(self) -> Self::IntoIter { self.0.into_iter() }
}

impl Subnodes
{
    #[inline]
    #[must_use]
    pub fn new(quad_tree: &mut QuadTree, index: usize) -> Self
    {
        let mut subnodes = Subnodes([0; 4]);
        let square = quad_tree.node(index).square();

        for (node_index, cardinality) in subnodes.0.iter_mut().zip(Cardinality::iter())
        {
            *node_index = quad_tree.insert_node(&cardinality.subsquare(&square));
        }

        subnodes
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = usize> + Clone { self.0.into_iter() }
}
