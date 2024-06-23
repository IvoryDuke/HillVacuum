//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::Vec2;
use hill_vacuum_shared::{continue_if_none, match_or_panic, return_if_no_match, NextValue};

use super::{
    points::{Intersections, Sides, Vertex, Vertexes},
    subnodes::Subnodes,
    QuadTree,
    QuadTreeIds,
    RemoveResult
};
use crate::{
    map::{editor::MAP_HALF_SIZE, MAP_SIZE},
    utils::{hull::Hull, identifiers::Id}
};

//=======================================================================//
// MACROS
//
//=======================================================================//

/// Inserts the hulls contained in `points` into `identifiers`.
macro_rules! insert_hulls {
    ($identifiers:ident, $($points:expr),+) => { $(
        for p in $points
        {
            for (id, hull) in p.hulls()
            {
                $identifiers.insert(*id, &hull);
            }
        }
    )+ };
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The content of a [`Node`].
#[must_use]
#[derive(Debug)]
enum Content
{
    /// Empty.
    None,
    /// Vertexes.
    Vertexes(Vertexes),
    /// Subnodes.
    Subnodes(Subnodes, Intersections)
}

impl Default for Content
{
    #[inline]
    fn default() -> Self { Self::None }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The area of a [`Node`].
#[derive(Clone, Copy, Debug)]
pub(in crate::map::editor::state::manager::quad_tree) struct Square
{
    /// The position of the top left corner.
    top_left: Vec2,
    /// The size of the side.
    size:     f32
}

impl Square
{
    /// Returns a new [`Square`].
    #[inline]
    #[must_use]
    pub const fn new(top_left: Vec2, size: f32) -> Self { Self { top_left, size } }

    /// Returns the positions of the top left corner.
    #[inline]
    #[must_use]
    pub const fn top_left(&self) -> Vec2 { self.top_left }

    /// Returns the length of the side of the [`quare`].
    #[inline]
    #[must_use]
    pub const fn size(&self) -> f32 { self.size }

    /// Returns the [`Hull`] describing the area covered by the [`Square`].
    #[inline]
    #[must_use]
    fn hull(&self) -> Hull
    {
        Hull::new(
            self.top_left.y,
            self.top_left.y - self.size,
            self.top_left.x,
            self.top_left.x + self.size
        )
    }

    /// Whever `self` contains `point` in its area.
    #[inline]
    #[must_use]
    fn contains_point(&self, point: Vec2) -> bool
    {
        (self.top_left.x..=self.top_left.x + self.size).contains(&point.x) &&
            (self.top_left.y - self.size..=self.top_left.y).contains(&point.y)
    }

    /// The [`SplitSegments`] cuttting the area of [`Square`] vertically and horizontally in half.
    #[inline]
    #[must_use]
    pub fn split_segments(&self) -> SplitSegments
    {
        let half_size = self.size / 2f32;
        let top_center = self.top_left + Vec2::new(half_size, 0f32);
        let left_center = self.top_left + Vec2::new(0f32, -half_size);

        SplitSegments {
            x_split: [top_center, top_center + Vec2::new(0f32, -self.size)],
            y_split: [left_center, left_center + Vec2::new(self.size, 0f32)]
        }
    }

    /// Whever the area covered by `self` intersects `hull`.
    #[inline]
    #[must_use]
    fn overlaps_hull(&self, hull: &Hull) -> bool { hull.overlaps(&self.hull()) }

    #[cfg(feature = "debug")]
    /// Whever the area of `self` is visible within `viewport`.
    #[inline]
    #[must_use]
    fn outline_visible(&self, viewport: &Hull) -> bool { self.hull().intersects(viewport) }

    #[cfg(feature = "debug")]
    /// Draws the outline of the area.
    #[inline]
    fn draw(&self, gizmos: &mut bevy::prelude::Gizmos)
    {
        super::draw_gizmo_hull(gizmos, &self.hull(), bevy::prelude::Color::BLUE);
    }
}

//=======================================================================//

/// The segments that cut in half both vertically and horizontally a square.
pub(in crate::map::editor::state::manager::quad_tree) struct SplitSegments
{
    /// The segments that cuts the height of the square.
    pub x_split: [Vec2; 2],
    /// The segments that cuts the width of the square.
    pub y_split: [Vec2; 2]
}

//=======================================================================//

/// A node of a [`QuadTree`].
#[derive(Debug)]
pub(in crate::map::editor::state::manager::quad_tree) struct Node
{
    /// The area covered.
    square:  Square,
    /// The contained data.
    content: Content
}

impl Node
{
    /// Returns a [`Node`] that covers the entire area of the map.
    #[inline]
    #[must_use]
    pub fn full_map() -> Self
    {
        Self {
            square:  Square::new(Vec2::new(-MAP_HALF_SIZE, MAP_HALF_SIZE), MAP_SIZE),
            content: Content::default()
        }
    }

    /// Returns a [`Node`] with area [`Square`].
    #[inline]
    #[must_use]
    pub fn new(square: &Square) -> Self
    {
        Self {
            square:  *square,
            content: Content::default()
        }
    }

    /// Returns the [`Square`] describing the area covered by `self`.
    #[inline]
    #[must_use]
    pub const fn square(&self) -> Square { self.square }

    /// Returns the [`SplitSegments`] of the area.
    #[inline]
    #[must_use]
    pub fn split_segments(&self) -> SplitSegments { self.square.split_segments() }

    /// Whever `pos` is contained in the covered area.
    #[inline]
    #[must_use]
    pub fn contains_point(&self, pos: Vec2) -> bool { self.square.contains_point(pos) }

    /// Clears the content of `self` and returns underlying [`Vertexes`], if any.
    #[inline]
    #[must_use]
    pub fn wipe(&mut self) -> Option<Vertexes>
    {
        let content = std::mem::take(&mut self.content);

        match content
        {
            Content::None => None,
            Content::Vertexes(vxs) => Some(vxs),
            Content::Subnodes(..) => panic!("Tried to wipe subnodes.")
        }
    }

    /// Returns a reference to the contained [`Subnodes`].
    #[inline]
    #[must_use]
    pub const fn subnodes(&self) -> &Subnodes
    {
        match_or_panic!(&self.content, Content::Subnodes(sts, _), sts)
    }

    /// Clears the content of `self`.
    #[inline]
    pub fn clear(&mut self)
    {
        assert!(
            match_or_panic!(&self.content, Content::Vertexes(vxs), vxs).is_empty(),
            "Content still contains vertexes."
        );
        self.content = Content::None;
    }

    /// Stores the ids of the entities of the [`Node`]s that contain `pos` in `identifiers`.
    /// Returns whever `pos` is contained in the area covered by `self`.
    #[inline]
    pub fn entities_at_pos(
        quad_tree: &QuadTree,
        identifiers: &mut QuadTreeIds,
        index: usize,
        pos: Vec2
    ) -> bool
    {
        let node = quad_tree.node(index);

        if !node.contains_point(pos)
        {
            return false;
        }

        match &node.content
        {
            Content::None => return true,
            Content::Vertexes(vxs) =>
            {
                insert_hulls!(identifiers, vxs);
                return true;
            },
            Content::Subnodes(subnodes, ints) =>
            {
                insert_hulls!(identifiers, ints);

                for node_index in subnodes.iter()
                {
                    if Self::entities_at_pos(quad_tree, identifiers, node_index, pos)
                    {
                        return true;
                    }
                }
            }
        };

        panic!("Failed entities search.");
    }

    /// Inserts `vertex` in the [`QuadTree`], splitting it into subnodes if necessary.
    #[inline]
    #[must_use]
    pub fn insert(quad_tree: &mut QuadTree, index: usize, mut vertex: Vertex) -> bool
    {
        if !quad_tree.node(index).contains_point(vertex.pos())
        {
            return false;
        }

        let mut subnodes = match &mut quad_tree.node_mut(index).content
        {
            Content::None =>
            {
                quad_tree.node_mut(index).content =
                    Content::Vertexes(Vertexes::from_vertex(vertex));
                return true;
            },
            Content::Vertexes(vertexes) =>
            {
                match vertexes.insert(vertex)
                {
                    Some(vx) => vertex = vx,
                    None => return true
                };

                // Create the subnodes
                let vertexes = std::mem::take(vertexes).into_iter();
                let subnodes = Subnodes::new(quad_tree, index);
                let subnodes_iter = subnodes.iter();
                let split_segments = quad_tree.node(index).square().split_segments();

                // Calculate the intersections of the vertexes.
                let mut intersections = quad_tree.intersections();

                'outer: for vx in vertexes
                {
                    vx.intersections(&mut intersections, &split_segments);

                    // Spread the vertexes.
                    for subnode_index in subnodes_iter.clone()
                    {
                        if quad_tree.node(subnode_index).contains_point(vx.pos())
                        {
                            assert!(
                                Self::insert(quad_tree, subnode_index, vx),
                                "Insertion failed."
                            );
                            continue 'outer;
                        }
                    }

                    panic!("Insertion failed.");
                }

                quad_tree.node_mut(index).content = Content::Subnodes(subnodes, intersections);
                subnodes_iter
            },
            Content::Subnodes(subnodes, _) => subnodes.iter()
        };

        // Insert the vertex without worrying about intersections.
        let insertion_index = subnodes
            .find(|subnode_index| quad_tree.node(*subnode_index).contains_point(vertex.pos()))
            .unwrap();

        assert!(Self::insert(quad_tree, insertion_index, vertex), "Insertion failed.");

        true
    }

    /// Inserts the intersections generated by `sides` with the [`SplitSegments`] of `quad_tree`.
    #[inline]
    pub fn insert_intersections(
        quad_tree: &mut QuadTree,
        index: usize,
        identifier: Id,
        sides: &Sides,
        hull: &Hull
    )
    {
        let node = quad_tree.node(index);

        if !hull.intersects(&node.square.hull())
        {
            return;
        }

        let split_segments = node.split_segments();
        let ints = return_if_no_match!(
            &mut quad_tree.node_mut(index).content,
            Content::Subnodes(_, ints),
            ints
        );

        if let Some(mut iter) = sides.intersections(&split_segments)
        {
            ints.insert(identifier, iter.next_value(), &sides.corner());
        }

        for node_index in quad_tree.node(index).subnodes()
        {
            Self::insert_intersections(quad_tree, node_index, identifier, sides, hull);
        }
    }

    /// Removes `pos` with associated [`Id`] from `quad_tree`.
    #[inline]
    #[must_use]
    pub fn remove(quad_tree: &mut QuadTree, index: usize, pos: Vec2, identifier: Id)
        -> RemoveResult
    {
        if !quad_tree.node(index).contains_point(pos)
        {
            return RemoveResult::None;
        }

        let node = quad_tree.node_mut(index);

        let mut subnodes = match &mut node.content
        {
            Content::None => return RemoveResult::None,
            Content::Vertexes(vertexes) => return vertexes.remove(pos, identifier),
            Content::Subnodes(subnodes, _) => subnodes.iter()
        };

        for node_index in subnodes.by_ref()
        {
            /// Tries to group the subtrees into one [`Node`].
            #[inline]
            fn try_group_subtrees(quad_tree: &mut QuadTree, index: usize) -> RemoveResult
            {
                let node = quad_tree.node(index);
                let mut vertexes_count = 0;

                for sub_node_index in node.subnodes()
                {
                    vertexes_count += match &quad_tree.node(sub_node_index).content
                    {
                        Content::None => continue,
                        Content::Vertexes(vertexes) => vertexes.len(),
                        Content::Subnodes(..) => return RemoveResult::VertexRemoved
                    };
                }

                if vertexes_count > 4
                {
                    return RemoveResult::VertexRemoved;
                }

                let mut vertexes = Vertexes::default();

                for sub_node_index in node.subnodes()
                {
                    for vx in continue_if_none!(quad_tree.remove_node(sub_node_index))
                    {
                        assert!(
                            vertexes.insert(vx).is_none(),
                            "Vertexes already contains the vertex."
                        );
                    }
                }

                let intersections = match_or_panic!(
                    &mut quad_tree.node_mut(index).content,
                    Content::Subnodes(_, ints),
                    std::mem::take(ints)
                );
                quad_tree.collect_intersections(intersections);
                quad_tree.node_mut(index).content = Content::Vertexes(vertexes);

                RemoveResult::SubnodesCollapsed
            }

            match Self::remove(quad_tree, node_index, pos, identifier)
            {
                RemoveResult::None => continue,
                RemoveResult::VertexJustRemoved(empty) =>
                {
                    if empty
                    {
                        quad_tree.node_mut(node_index).clear();
                    }

                    return try_group_subtrees(quad_tree, index);
                },
                RemoveResult::SubnodesCollapsed =>
                {
                    return try_group_subtrees(quad_tree, index);
                },
                RemoveResult::VertexRemoved | RemoveResult::IdJustRemoved =>
                {
                    return RemoveResult::VertexRemoved;
                }
            };
        }

        RemoveResult::None
    }

    /// Removes all the intersections with [`Id`] `identifier`.
    #[inline]
    pub fn remove_intersections(quad_tree: &mut QuadTree, index: usize, identifier: Id, hull: &Hull)
    {
        let node = quad_tree.node_mut(index);

        if !hull.intersects(&node.square.hull())
        {
            return;
        }

        return_if_no_match!(&mut node.content, Content::Subnodes(_, ints), ints).remove(identifier);

        for node_index in quad_tree.node(index).subnodes()
        {
            Self::remove_intersections(quad_tree, node_index, identifier, hull);
        }
    }

    /// Inserts in `identifiers` the ids contained in `self` if it intersects `range`.
    #[inline]
    pub fn intersect_range(
        quad_tree: &QuadTree,
        index: usize,
        identifiers: &mut QuadTreeIds,
        range: &Hull
    )
    {
        let node = quad_tree.node(index);

        if !node.square.overlaps_hull(range)
        {
            return;
        }

        match &node.content
        {
            Content::None => (),
            Content::Vertexes(vxs) => insert_hulls!(identifiers, vxs),
            Content::Subnodes(subnodes, ints) =>
            {
                insert_hulls!(identifiers, ints);

                for subnode_index in subnodes.iter()
                {
                    Self::intersect_range(quad_tree, subnode_index, identifiers, range);
                }
            }
        }
    }

    #[cfg(feature = "debug")]
    /// Draws the grid created by the [`QuadTree`].
    #[inline]
    pub fn draw_grid(
        quad_tree: &QuadTree,
        index: usize,
        viewport: &Hull,
        gizmos: &mut bevy::prelude::Gizmos,
        square_size: Vec2
    )
    {
        let node = quad_tree.node(index);

        if !node.square.overlaps_hull(viewport)
        {
            return;
        }

        // Only draw the outline if it's in sight.
        if node.square.outline_visible(viewport)
        {
            node.square.draw(gizmos);
        }

        let (subnodes, ints) =
            return_if_no_match!(&node.content, Content::Subnodes(subnodes, ints), (subnodes, ints));

        for pos in ints
            .iter()
            .filter_map(|int| viewport.contains_point(int.pos()).then_some(int.pos()))
        {
            gizmos.rect_2d(pos, 0f32, square_size, bevy::prelude::Color::GREEN);
        }

        for subnode_index in subnodes.iter()
        {
            Self::draw_grid(quad_tree, subnode_index, viewport, gizmos, square_size);
        }
    }
}
