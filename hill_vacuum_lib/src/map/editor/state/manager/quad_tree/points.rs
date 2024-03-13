//=======================================================================//
// IMPORTS
//
//=======================================================================//

use arrayvec::ArrayVec;
use bevy::prelude::Vec2;
#[cfg(feature = "arena_alloc")]
use blink_alloc::BlinkAlloc;
use shared::{continue_if_none, NextValue};

use super::{node::SplitSegments, RemoveResult};
use crate::{
    map::{hv_hash_map, hv_vec, AssertedInsertRemove, HvHashMap, HvVec},
    utils::{
        hull::Hull,
        identifiers::Id,
        iterators::SkipIndexIterator,
        math::{lines_and_segments::segments_intersection, AroundEqual}
    }
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[derive(PartialEq, Clone, Copy, Debug)]
pub(in crate::map::editor::state::manager::quad_tree) enum Corner
{
    TopRight(Vec2, f32, f32),
    TopLeft(Vec2, f32, f32),
    BottomLeft(Vec2, f32, f32),
    BottomRight(Vec2, f32, f32)
}

impl Corner
{
    #[inline]
    #[must_use]
    pub fn from_hull(hull: &Hull, corner: crate::utils::hull::Corner) -> Self
    {
        use crate::utils::hull::Corner;

        let (width, height) = hull.dimensions();
        let pos = hull.corner_vertex(corner);

        match corner
        {
            Corner::TopRight => Self::TopRight(pos, -width, -height),
            Corner::TopLeft => Self::TopLeft(pos, width, -height),
            Corner::BottomLeft => Self::BottomLeft(pos, width, height),
            Corner::BottomRight => Self::BottomRight(pos, -width, height)
        }
    }

    #[inline]
    #[must_use]
    pub const fn pos(&self) -> Vec2
    {
        let (Self::TopRight(p, ..) |
        Self::BottomLeft(p, ..) |
        Self::TopLeft(p, ..) |
        Self::BottomRight(p, ..)) = self;

        *p
    }

    #[inline]
    #[must_use]
    pub fn hull(&self) -> Hull
    {
        match self
        {
            Self::TopRight(p, x, y) => Hull::new(p.y, p.y + *y, p.x + *x, p.x),
            Self::TopLeft(p, x, y) => Hull::new(p.y, p.y + *y, p.x, p.x + x),
            Self::BottomLeft(p, x, y) => Hull::new(p.y + *y, p.y, p.x, p.x + *x),
            Self::BottomRight(p, x, y) => Hull::new(p.y + *y, p.y, p.x + *x, p.x)
        }
    }

    #[inline]
    pub fn sides(&self) -> Sides
    {
        let (Self::TopRight(p, x, y) |
        Self::BottomLeft(p, x, y) |
        Self::TopLeft(p, x, y) |
        Self::BottomRight(p, x, y)) = self;

        Sides {
            x:      [*p, *p + Vec2::new(0f32, *y)],
            y:      [*p, *p + Vec2::new(*x, 0f32)],
            corner: *self
        }
    }

    #[inline]
    pub fn intersections(
        &self,
        split_segments: &SplitSegments
    ) -> Option<impl Iterator<Item = Vec2>>
    {
        self.sides().intersections(split_segments)
    }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

#[must_use]
pub(in crate::map::editor::state::manager::quad_tree) struct Sides
{
    x:      [Vec2; 2],
    y:      [Vec2; 2],
    corner: Corner
}

impl Sides
{
    #[inline]
    pub fn from_hull(hull: &Hull) -> impl Iterator<Item = Sides> + '_
    {
        hull.corners()
            .map(|(corner, _)| (Corner::from_hull(hull, corner).sides()))
            .skip_index(1)
            .unwrap()
            .take(2)
    }

    #[inline]
    #[must_use]
    pub const fn corner(&self) -> Corner { self.corner }

    #[inline]
    pub fn intersections(
        &self,
        split_segments: &SplitSegments
    ) -> Option<impl Iterator<Item = Vec2>>
    {
        let mut intersections = [None; 2];
        let mut len = 0;

        for (side, segment) in [&self.x, &self.y]
            .into_iter()
            .zip([&split_segments.y_split, &split_segments.x_split])
        {
            intersections[len] = continue_if_none!(segments_intersection(side, segment)).0.into();
            len += 1;
        }

        if len == 0
        {
            return None;
        }

        intersections.into_iter().flatten().into()
    }
}

//=======================================================================//

#[must_use]
#[derive(Debug)]
pub(in crate::map::editor::state::manager::quad_tree) struct Vertex
{
    pos:     Vec2,
    corners: HvHashMap<Id, Corner>
}

impl Vertex
{
    #[inline]
    pub fn new(corners: HvHashMap<Id, Corner>) -> Self
    {
        let mut iter = corners.iter();
        let pos = iter.next_value().1.pos();

        for (_, corner) in iter
        {
            assert!(
                corner.pos().around_equal_narrow(&pos),
                "Corners don't have the same position."
            );
        }

        Self { pos, corners }
    }

    #[inline]
    #[must_use]
    pub const fn pos(&self) -> Vec2 { self.pos }

    #[inline]
    pub fn hulls(&self) -> impl Iterator<Item = (&Id, Hull)>
    {
        self.corners.iter().map(|(id, corner)| (id, corner.hull()))
    }

    #[inline]
    pub fn entities_ids(&self) -> VertexIds<'_> { VertexIds(self.corners.iter()) }

    #[inline]
    pub fn intersections(&self, intersections: &mut Intersections, split_segments: &SplitSegments)
    {
        for (id, corner) in &self.corners
        {
            intersections.push(
                *id,
                continue_if_none!(corner.intersections(split_segments)).next_value(),
                corner
            );
        }
    }

    #[inline]
    fn insert_corner(&mut self, element: &(Id, Corner))
    {
        assert!(
            element.1.pos().around_equal_narrow(&self.pos),
            "The new corner does not have the same the same position."
        );
        self.corners.asserted_insert(*element);
    }

    #[inline]
    #[must_use]
    fn remove_entity_id(&mut self, identifier: Id) -> bool
    {
        if self.corners.len() == 1
        {
            assert!(
                *self.corners.iter().next_value().0 == identifier,
                "Last stored id does not match the one requested to remove."
            );
            return true;
        }

        self.corners.asserted_remove(&identifier);
        false
    }
}

//=======================================================================//

pub(in crate::map::editor::state::manager::quad_tree) struct VertexIds<'a>(
    hashbrown::hash_map::Iter<'a, Id, Corner>
);

impl<'a> Iterator for VertexIds<'a>
{
    type Item = &'a Id;

    #[inline]
    #[must_use]
    fn next(&mut self) -> Option<Self::Item> { self.0.next().map(|(id, _)| id) }
}

//=======================================================================//

#[must_use]
#[derive(Default, Debug)]
pub(in crate::map::editor::state::manager::quad_tree) struct Vertexes(ArrayVec<Vertex, 4>);

impl IntoIterator for Vertexes
{
    type IntoIter = arrayvec::IntoIter<Vertex, 4>;
    type Item = Vertex;

    #[inline]
    fn into_iter(self) -> Self::IntoIter { self.0.into_iter() }
}

impl<'a> IntoIterator for &'a Vertexes
{
    type IntoIter = std::slice::Iter<'a, Vertex>;
    type Item = &'a Vertex;

    #[inline]
    fn into_iter(self) -> Self::IntoIter { self.iter() }
}

impl Vertexes
{
    #[inline]
    pub fn from_vertex(vertex: Vertex) -> Self
    {
        let mut array = ArrayVec::new();
        array.push(vertex);

        Self(array)
    }

    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize { self.0.len() }

    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool { self.0.is_empty() }

    #[inline]
    pub fn iter(&self) -> std::slice::Iter<Vertex> { self.0.iter() }

    #[inline]
    #[must_use]
    pub fn insert(&mut self, vertex: Vertex) -> Option<Vertex>
    {
        for vx in &mut self.0
        {
            if !vx.pos.around_equal_narrow(&vertex.pos)
            {
                continue;
            }

            for info in vertex.corners
            {
                vx.insert_corner(&info);
            }

            return None;
        }

        if self.0.len() == 4
        {
            return vertex.into();
        }

        self.0.push(vertex);
        None
    }

    #[inline]
    #[must_use]
    pub fn remove(&mut self, pos: Vec2, identifier: Id) -> RemoveResult
    {
        assert!(!self.is_empty(), "Vertexes is already empty.");

        let index = self.0.iter().position(|vx| vx.pos.around_equal_narrow(&pos)).unwrap();

        if self.0[index].remove_entity_id(identifier)
        {
            _ = self.0.swap_remove(index);
            return RemoveResult::VertexJustRemoved(self.is_empty());
        }

        RemoveResult::IdJustRemoved
    }
}

//=======================================================================//

pub(in crate::map::editor::state::manager::quad_tree) struct VertexesIds<'a>
{
    vertexes: &'a Vertexes,
    iter:     VertexIds<'a>,
    left:     usize
}

impl<'a> Iterator for VertexesIds<'a>
{
    type Item = &'a Id;

    fn next(&mut self) -> Option<Self::Item>
    {
        let mut value = self.iter.next();

        if value.is_none()
        {
            self.left += 1;

            if self.left == self.vertexes.len()
            {
                return None;
            }

            self.iter = self.vertexes.0[self.left].entities_ids();
            value = self.iter.next();
        }

        value
    }
}

//=======================================================================//

#[derive(Clone, Debug)]
pub(in crate::map::editor::state::manager::quad_tree) struct Intersection
{
    pos:     Vec2,
    corners: HvHashMap<Id, Corner>
}

impl Intersection
{
    #[inline]
    #[must_use]
    pub fn new(pos: Vec2, corners: HvHashMap<Id, Corner>) -> Self
    {
        assert!(!corners.is_empty(), "No corners associated to the intersection.");
        Self { pos, corners }
    }

    #[inline]
    #[must_use]
    pub const fn pos(&self) -> Vec2 { self.pos }

    #[inline]
    pub fn hulls(&self) -> impl Iterator<Item = (&Id, Hull)>
    {
        self.corners.iter().map(|(id, corner)| (id, corner.hull()))
    }

    #[inline]
    #[must_use]
    pub fn contains_id(&self, identifier: Id) -> bool { self.corners.contains_key(&identifier) }

    #[inline]
    pub fn insert_corner(&mut self, identifier: Id, corner: &Corner)
    {
        if !self.contains_id(identifier)
        {
            self.corners.insert(identifier, *corner);
        }
    }

    #[inline]
    #[must_use]
    pub fn remove_entity_id(&mut self, identifier: Id) -> Option<bool>
    {
        if self.corners.len() == 1
        {
            return (*self.corners.iter().next_value().0 == identifier).into();
        }

        if self.corners.remove(&identifier).is_some()
        {
            return false.into();
        }

        None
    }
}

//=======================================================================//

#[must_use]
#[derive(Debug)]
pub(in crate::map::editor::state::manager::quad_tree) struct Intersections(HvVec<Intersection>);

impl Default for Intersections
{
    #[inline]
    fn default() -> Self { Self(hv_vec![]) }
}

impl IntoIterator for Intersections
{
    #[cfg(feature = "arena_alloc")]
    type IntoIter = std::vec::IntoIter<Intersection, &'static BlinkAlloc>;
    #[cfg(not(feature = "arena_alloc"))]
    type IntoIter = smallvec::IntoIter<[Intersection; 1]>;
    type Item = Intersection;

    #[inline]
    fn into_iter(self) -> Self::IntoIter { self.0.into_iter() }
}

impl<'a> IntoIterator for &'a Intersections
{
    type IntoIter = std::slice::Iter<'a, Intersection>;
    type Item = &'a Intersection;

    #[inline]
    fn into_iter(self) -> Self::IntoIter { self.0.iter() }
}

impl Intersections
{
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &Intersection> { self.0.iter() }

    #[inline]
    #[must_use]
    pub fn contains_id(&self, identifier: Id) -> bool
    {
        self.iter().any(|int| int.contains_id(identifier))
    }

    #[inline]
    pub fn push(&mut self, identifier: Id, pos: Vec2, corner: &Corner)
    {
        if self.contains_id(identifier)
        {
            return;
        }

        match self.0.iter_mut().find(|int| int.pos().around_equal_narrow(&pos))
        {
            Some(int) => int.insert_corner(identifier, corner),
            None =>
            {
                self.0
                    .push(Intersection::new(pos, hv_hash_map![(identifier, *corner)]));
            }
        };
    }

    #[inline]
    pub fn remove(&mut self, identifier: Id)
    {
        for (i, int) in self.0.iter_mut().enumerate()
        {
            if continue_if_none!(int.remove_entity_id(identifier))
            {
                self.0.remove(i);
                return;
            }
        }
    }

    #[inline]
    pub fn clear(&mut self) { self.0.clear() }
}
