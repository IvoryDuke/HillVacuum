mod node;
mod points;
mod subnodes;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use glam::Vec2;
use hashbrown::hash_map::Iter;
use hill_vacuum_shared::return_if_none;

use self::{
    node::{Node, Square},
    points::{Corner, Intersections, Sides, Vertex, Vertexes}
};
use crate::{
    map::MAP_SIZE,
    utils::{
        collections::{hash_map, HashMap},
        hull::Hull,
        identifiers::{EntityId, Id},
        math::AroundEqual,
        misc::{bumped_vertex_highlight_square, AssertedInsertRemove, TakeValue}
    }
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[derive(Clone, Copy)]
pub(in crate::map::editor::state::manager) enum InsertResult
{
    Inserted,
    Replaced,
    Unchanged
}

impl InsertResult
{
    #[inline]
    #[must_use]
    pub const fn inserted(self) -> bool { matches!(self, Self::Inserted) }
}

//=======================================================================//

/// The outcome of an [`Id`] removal.
enum RemoveResult
{
    /// Nothing.
    None,
    /// The [`Id`] was removed.
    IdJustRemoved,
    /// The [`Id`] was removed as well as the [`Vertexes`]. Contains `true` if the [`Node`] is now
    /// empty.
    VertexJustRemoved(bool),
    /// The subnodes were agglomerated in a single [`Node`].
    SubnodesCollapsed,
    /// A [`Vertex`] was removed.
    VertexRemoved
}

//=======================================================================//

/// A struct that may or may not contain a [`Node`].
struct MaybeNode(Option<Node>);

impl Default for MaybeNode
{
    #[inline]
    fn default() -> Self { Self(None) }
}

impl MaybeNode
{
    /// Returns a reference to the wrapped [`Node`], if any.
    #[inline]
    #[must_use]
    const fn as_ref(&self) -> Option<&Node> { self.0.as_ref() }

    /// Returns a mutable reference to the wrapped [`Node`], if any.
    #[inline]
    #[must_use]
    fn as_mut(&mut self) -> Option<&mut Node> { self.0.as_mut() }

    /// Sets the internal value to [`None`].
    #[inline]
    #[must_use]
    fn wipe(&mut self) -> Option<Vertexes> { self.0.take_value().unwrap().wipe() }
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// A quad tree that stores bidimensional entities. It stores the vertexes of the non-rotated
/// rectangle encompassing the entity and the intersections of its sides with the segments
/// partitioning the space into nodes.
pub(in crate::map::editor::state::manager) struct QuadTree
{
    size:                  f32,
    entities:              HashMap<Id, Hull>,
    /// The nodes of the tree.
    nodes:                 Vec<MaybeNode>,
    /// The nodes that are currently unused.
    vacant_spots:          Vec<usize>,
    /// The recycled [`Intersections`].
    recycle_intersections: Vec<Intersections>
}

impl QuadTree
{
    /// Returns a new [`QuadTree`].
    #[inline]
    #[must_use]
    pub fn new() -> Self { Self::with_size(MAP_SIZE) }

    #[inline]
    #[must_use]
    pub fn with_size(size: f32) -> Self
    {
        let mut vec = Vec::with_capacity(256);
        Self::start_nodes(&mut vec, size);

        Self {
            size,
            entities: hash_map![],
            nodes: vec,
            vacant_spots: Vec::new(),
            recycle_intersections: Vec::with_capacity(32)
        }
    }

    #[inline]
    fn start_nodes(vec: &mut Vec<MaybeNode>, size: f32)
    {
        vec.clear();
        vec.push(MaybeNode(Node::from_size(size).into()));
    }

    /// Returns a reference to the [`Node`] at `index`.
    #[inline]
    #[must_use]
    fn node(&self, index: usize) -> &Node { self.nodes[index].as_ref().unwrap() }

    /// Returns a mutable reference to the [`Node`] at `index`.
    #[inline]
    #[must_use]
    fn node_mut(&mut self, index: usize) -> &mut Node { self.nodes[index].as_mut().unwrap() }

    /// Stores the ids of the entities at pos in `entities`.
    #[inline]
    pub fn entities_at_pos(&self, entities: &mut QuadTreeIds, pos: Vec2)
    {
        Node::entities_at_pos(self, entities, 0, pos);
        entities.retain(|_, hull| hull.contains_point(pos));
    }

    /// Stores the ids of entities near `pos` in `entities`.
    #[inline]
    pub fn entities_near_pos(&self, entities: &mut QuadTreeIds, pos: Vec2, camera_scale: f32)
    {
        let hull = bumped_vertex_highlight_square(camera_scale) + pos;

        for pos in hull.corners().map(|(_, pos)| pos)
        {
            assert!(Node::entities_at_pos(self, entities, 0, pos), "Entities research failed.");
        }

        entities.retain(|_, h| hull.intersects(h));
    }

    /// Inserts `entity`.
    #[inline]
    #[must_use]
    pub fn insert_entity<T, F>(&mut self, entity: &T, f: F) -> InsertResult
    where
        T: EntityId + ?Sized,
        F: Fn(&T) -> Hull
    {
        let id = entity.id();
        let hull = &f(entity);

        match self.entities.get(&id).copied()
        {
            Some(prev_hull) =>
            {
                if prev_hull.around_equal_narrow(hull)
                {
                    return InsertResult::Unchanged;
                }

                self.remove_hull(id);
                self.insert_hull(id, hull);
                InsertResult::Replaced
            },
            None =>
            {
                self.insert_hull(id, hull);
                InsertResult::Inserted
            }
        }
    }

    /// Inserts a ([`Id`], [`Hull`]) pair.
    #[inline]
    fn insert_hull(&mut self, identifier: Id, hull: &Hull)
    {
        for corner in hull.corners().map(|(corner, _)| Corner::from_hull(hull, corner))
        {
            assert!(
                Node::insert(self, 0, Vertex::new(hash_map![(identifier, corner)])),
                "Hull corner insertion failed."
            );
        }

        for sides in Sides::from_hull(hull)
        {
            Node::insert_intersections(self, 0, identifier, &sides, hull);
        }

        self.entities.asserted_insert((identifier, *hull));
    }

    /// Removes `entity`.
    #[inline]
    #[must_use]
    pub fn remove_entity<T>(&mut self, entity: &T) -> bool
    where
        T: EntityId + ?Sized
    {
        self.remove_hull(entity.id())
    }

    /// Removes the ([`Id`], [`Hull`]) pair.
    #[inline]
    fn remove_hull(&mut self, identifier: Id) -> bool
    {
        let hull = return_if_none!(self.entities.remove(&identifier), false);

        for pos in hull.vertexes()
        {
            match Node::remove(self, 0, pos, identifier)
            {
                RemoveResult::None => panic!("Hull was not in the quad tree."),
                RemoveResult::IdJustRemoved |
                RemoveResult::VertexRemoved |
                RemoveResult::SubnodesCollapsed => (),
                RemoveResult::VertexJustRemoved(empty) =>
                {
                    if empty
                    {
                        self.node_mut(0).clear();
                    }
                }
            };
        }

        Node::remove_intersections(self, 0, identifier, &hull);
        true
    }

    /// Inserts a new [`Node`] in the tree with size defined by `square`.
    /// Returns the index where it has been stored.
    #[inline]
    #[must_use]
    fn insert_node(&mut self, square: &Square) -> usize
    {
        let node = MaybeNode(Node::new(square).into());

        if !self.vacant_spots.is_empty()
        {
            let index = self.vacant_spots.pop().unwrap();
            self.nodes[index] = node;
            return index;
        }

        let index = self.nodes.len();
        self.nodes.push(node);
        index
    }

    /// Clears the [`Node`] at `index` and removes it from the tree.
    /// Returns the [`Vertexes`] it contained, if any.
    #[inline]
    #[must_use]
    fn remove_node(&mut self, index: usize) -> Option<Vertexes>
    {
        self.vacant_spots.push(index);
        self.nodes[index].wipe()
    }

    /// Stores an [`Intersections`] to be recycled.
    #[inline]
    pub(in crate::map::editor::state::manager::quad_tree) fn collect_intersections(
        &mut self,
        mut intersections: Intersections
    )
    {
        intersections.clear();
        self.recycle_intersections.push(intersections);
    }

    /// Returns a new [`Intersections`].
    #[inline]
    pub(in crate::map::editor::state::manager::quad_tree) fn intersections(
        &mut self
    ) -> Intersections
    {
        self.recycle_intersections.pop().unwrap_or_default()
    }

    /// Stores in `entities` the ids of the [`Hull`]s that are fully contained in `range`.
    #[inline]
    pub fn entities_in_range(&self, entities: &mut QuadTreeIds, range: &Hull)
    {
        Node::intersect_range(self, 0, entities, range);
        entities.retain(|_, hull| range.contains_hull(hull));
    }

    /// Stores in `entities` the ids of the [`Hull`]s that intersect `range`.
    #[inline]
    pub fn entities_intersect_range(&self, entities: &mut QuadTreeIds, range: &Hull)
    {
        Node::intersect_range(self, 0, entities, range);
        entities.retain(|_, hull| range.overlaps(hull));
    }

    #[inline]
    pub fn clear(&mut self)
    {
        self.entities.clear();
        Self::start_nodes(&mut self.nodes, self.size);
        self.vacant_spots.clear();
        self.recycle_intersections.clear();
    }
}

//=======================================================================//

/// A struct to store the [`Id`]s collected from a [`QuadTree`].
pub(in crate::map::editor::state::manager) struct QuadTreeIds(HashMap<Id, Hull>);

impl<'a> IntoIterator for &'a QuadTreeIds
{
    type IntoIter = hashbrown::hash_map::Iter<'a, Id, Hull>;
    type Item = (&'a Id, &'a Hull);

    #[inline]
    fn into_iter(self) -> Self::IntoIter { self.iter() }
}

impl QuadTreeIds
{
    /// Returns a new [`QuadTreeIds`].
    #[inline]
    #[must_use]
    pub fn new() -> Self { Self(hash_map![]) }

    /// Returns an iterator to the ([`Id`], [`Hull`]) pairs.
    #[inline]
    pub fn iter(&self) -> Iter<'_, Id, Hull> { self.0.iter() }

    #[inline]
    pub fn ids(&self) -> hashbrown::hash_map::Keys<'_, Id, Hull> { self.0.keys() }

    /// Whether it contains a value for the specified key.
    #[inline]
    #[must_use]
    pub fn contains(&self, identifier: Id) -> bool { self.0.contains_key(&identifier) }

    /// Inserts a ([`Id`], [`Hull`]) pair.
    #[inline]
    fn insert(&mut self, identifier: Id, hull: &Hull) { self.0.insert(identifier, *hull); }

    /// Retains only the elements specified by the predicate.
    #[inline]
    fn retain<F>(&mut self, f: F)
    where
        F: FnMut(&Id, &mut Hull) -> bool
    {
        self.0.retain(f);
    }

    /// Clears the stored elements.
    #[inline]
    pub fn clear(&mut self) { self.0.clear() }
}
