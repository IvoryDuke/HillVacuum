mod node;
mod points;
mod subnodes;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::Vec2;
use hashbrown::hash_map::Iter;

use self::{
    node::{Node, Square},
    points::{Corner, Intersections, Sides, Vertex, Vertexes}
};
use crate::{
    map::{containers::hv_hash_map, hv_vec, HvHashMap, HvVec},
    utils::{
        hull::{EntityHull, Hull},
        identifiers::{EntityId, Id},
        math::AroundEqual,
        misc::bumped_vertex_highlight_square
    }
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The outcome of an [`Id`] removal.
#[derive(Debug)]
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
#[derive(Debug)]
struct MaybeNode(Option<Node>);

impl Default for MaybeNode
{
    #[inline]
    #[must_use]
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
    fn wipe(&mut self) -> Option<Vertexes> { std::mem::take(&mut self.0).unwrap().wipe() }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// A quad tree that stores bidimensional entities. It stores the vertexes of the non-rotated
/// rectangle encompassing the entity and the intersections of its sides with the segments
/// partitioning the space into nodes.
#[derive(Debug)]
pub(in crate::map::editor::state::manager) struct QuadTree
{
    /// The nodes of the tree.
    nodes:                 HvVec<MaybeNode>,
    /// The nodes that are currently unused.
    vacant_spots:          HvVec<usize>,
    /// The recycled [`Intersections`].
    recycle_intersections: HvVec<Intersections>
}

impl QuadTree
{
    /// Returns a new [`QuadTree`].
    #[inline]
    #[must_use]
    pub fn new() -> Self
    {
        let mut vec = hv_vec![capacity; 256];
        vec.push(MaybeNode(Node::full_map().into()));

        Self {
            nodes:                 vec,
            vacant_spots:          hv_vec![],
            recycle_intersections: hv_vec![capacity; 32]
        }
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
        let pos_hull = bumped_vertex_highlight_square(camera_scale) + pos;

        for pos in pos_hull.vertexes()
        {
            assert!(Node::entities_at_pos(self, entities, 0, pos), "Entities research failed.");
        }

        entities.retain(|_, hull| pos_hull.vertexes().any(|vx| hull.contains_point(vx)));
    }

    /// Inserts `entity`.
    #[inline]
    pub fn insert_entity<T>(&mut self, entity: &T)
    where
        T: EntityHull + EntityId
    {
        self.insert_hull(entity.id(), &entity.hull());
    }

    /// Inserts a ([`Id`], [`Hull`]) pair.
    #[inline]
    pub fn insert_hull(&mut self, identifier: Id, hull: &Hull)
    {
        for corner in hull.corners().map(|(corner, _)| Corner::from_hull(hull, corner))
        {
            assert!(
                Node::insert(self, 0, Vertex::new(hv_hash_map![(identifier, corner)])),
                "Hull corner insertion failed."
            );
        }

        for sides in Sides::from_hull(hull)
        {
            Node::insert_intersections(self, 0, identifier, &sides, hull);
        }
    }

    /// Replaces the [`Hull`] associated to `identifier`.
    #[inline]
    pub fn replace_hull(
        &mut self,
        identifier: Id,
        current_hull: &Hull,
        previous_hull: &Hull
    ) -> bool
    {
        if previous_hull.around_equal_narrow(current_hull)
        {
            return false;
        }

        self.remove_hull(identifier, previous_hull);
        self.insert_hull(identifier, current_hull);
        true
    }

    /// Removes `entity`.
    #[inline]
    pub fn remove_entity<T>(&mut self, entity: &T)
    where
        T: EntityHull + EntityId
    {
        self.remove_hull(entity.id(), &entity.hull());
    }

    /// Removes the ([`Id`], [`Hull`]) pair.
    #[inline]
    pub fn remove_hull(&mut self, identifier: Id, hull: &Hull)
    {
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

        Node::remove_intersections(self, 0, identifier, hull);
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

    #[cfg(feature = "debug")]
    /// Draws the outlines of the [`Hull`]s.
    #[inline]
    pub fn draw(&self, gizmos: &mut bevy::prelude::Gizmos, viewport: &Hull, camera_scale: f32)
    {
        let square_side = vertex_highlight_side_length(camera_scale);

        Node::draw_grid(self, 0, viewport, gizmos, Vec2::new(square_side, square_side));

        let mut entities = QuadTreeIds::new();
        self.entities_intersect_range(&mut entities, viewport);

        for hull in entities.hulls()
        {
            draw_gizmo_hull(gizmos, hull, bevy::prelude::Color::RED);
        }
    }
}

//=======================================================================//

/// A struct to store the [`Id`]s collected from a [`QuadTree`].
#[derive(Debug)]
pub(in crate::map::editor::state::manager) struct QuadTreeIds(HvHashMap<Id, Hull>);

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
    pub fn new() -> Self { Self(hv_hash_map![]) }

    /// Returns an iterator to the ([`Id`], [`Hull`]) pairs.
    #[inline]
    pub fn iter(&self) -> Iter<'_, Id, Hull> { self.0.iter() }

    /// Returns an iterator to the stored [`Id`]s.
    #[inline]
    pub fn ids(&self) -> hashbrown::hash_map::Keys<'_, Id, Hull> { self.0.keys() }

    #[cfg(feature = "debug")]
    /// Returns an iterator to the stored [`Hull`]s.
    #[inline]
    fn hulls(&self) -> impl Iterator<Item = &Hull> { self.0.values() }

    /// Whever it contains a value for the specified key.
    #[inline]
    #[must_use]
    pub fn contains(&self, identifier: Id) -> bool { self.0.contains_key(&identifier) }

    /// Inserts a ([`Id`], [`Hull`]) pair.
    #[inline]
    pub fn insert(&mut self, identifier: Id, hull: &Hull) { self.0.insert(identifier, *hull); }

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

//=======================================================================//
// FUNCTONS
//
//=======================================================================//

#[cfg(feature = "debug")]
#[inline]
fn draw_gizmo_hull(gizmos: &mut bevy::prelude::Gizmos, hull: &Hull, color: bevy::prelude::Color)
{
    for [start, end] in hull.sides_segments()
    {
        gizmos.line_2d(start, end, color);
    }
}
