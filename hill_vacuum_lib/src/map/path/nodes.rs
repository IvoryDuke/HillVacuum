//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::ops::{AddAssign, SubAssign};

use bevy::prelude::Vec2;
use serde::{Deserialize, Serialize};

use crate::{
    map::{selectable_vector::SelectableVector, HvVec},
    utils::math::AroundEqual
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The data concerning the travel of an entity from one [`Node`] to the next one.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Movement
{
    /// The maximum speed an entity is going to travel from this [`Node`] to the next one.
    /// It's a value higher than `min_speed`.
    max_speed:               f32,
    /// The minimum speed an entity is going to travel from this [`Node`] to the next one.
    /// It's a value higher than or equal to 0 and less than or equal to `max_speed`.
    min_speed:               f32,
    /// The percentage of the travel that the entity is going to take to go from the minimum
    /// speed to the maximum speed. It is a value between 0 and 1, and is never higher than (1
    /// - `decel_travel_percentage`).
    accel_travel_percentage: f32,
    /// The percentage of the travel that the entity is going to take to go from the maximum
    /// speed to the minimum speed. It is a value between 0 and 1, and is never higher than (1
    /// - `accel_travel_percentage`).
    decel_travel_percentage: f32,
    /// The time that has to pass before the entity should start moving.
    standby_time:            f32
}

impl Default for Movement
{
    #[inline]
    fn default() -> Self { Self::new(60f32, 0f32, 0f32, 0f32, 0f32) }
}

impl Movement
{
    /// Creates a new [`Movement`].
    /// # Panics
    /// Panics if at least one among the values is not acceptable.
    /// `min_speed` must be >= 0 and less than `max_speed`, `max_speed` must be > 0,
    /// `accel_travel_percentage` and `decel_travel_percentage` must be values between
    /// 0 and 1 and their sum must be <= 1.
    #[inline]
    pub(in crate::map) fn new(
        max_speed: f32,
        accel_travel_percentage: f32,
        decel_travel_percentage: f32,
        min_speed: f32,
        standby_time: f32
    ) -> Self
    {
        assert!(min_speed >= 0f32, "Min speed is a negative value.");
        assert!(max_speed > 0f32, "Max speed is not higher than 0.");
        assert!(min_speed <= max_speed, "Max speed is lower than min speed.");
        assert!(
            (0f32..=1f32).contains(&accel_travel_percentage),
            "Accel travel percentage is not within 0 and 1."
        );
        assert!(
            (0f32..=1f32).contains(&decel_travel_percentage),
            "Accel travel percentage is not within 0 and 1."
        );
        assert!(
            accel_travel_percentage + decel_travel_percentage <= 1f32,
            "Accel and decel percentages added are higher than 1."
        );
        assert!(standby_time >= 0f32, "Standby time is a negative value.");

        Self {
            max_speed,
            min_speed,
            accel_travel_percentage,
            decel_travel_percentage,
            standby_time
        }
    }

    /// Return the maximum travel speed.
    #[inline]
    #[must_use]
    pub const fn max_speed(&self) -> f32 { self.max_speed }

    /// Returns the minimum travel speed.
    #[inline]
    #[must_use]
    pub const fn min_speed(&self) -> f32 { self.min_speed }

    /// The time of the travel spent accelerating from the minimum to the maximum speed represented
    /// by a value between 0 and 100.
    #[inline]
    #[must_use]
    pub const fn accel_travel_percentage(&self) -> f32 { self.accel_travel_percentage }

    /// The time of the travel spent accelerating from the minimum to the maximum speed represented
    /// by a value between 0 and 1.
    #[inline]
    #[must_use]
    pub fn scaled_accel_travel_percentage(&self) -> f32 { self.accel_travel_percentage / 100f32 }

    /// The time of the travel spent decelerating from the maximum to the minimum speed represented
    /// by a value between 0 and 100.
    #[inline]
    #[must_use]
    pub const fn decel_travel_percentage(&self) -> f32 { self.decel_travel_percentage }

    /// The time of the travel spent decelerating from the maximum to the minimum speed represented
    /// by a value between 0 and 1.
    #[inline]
    #[must_use]
    pub fn scaled_decel_travel_percentage(&self) -> f32 { self.decel_travel_percentage / 100f32 }

    /// Returns the standby time, that is the time that has to pass before the entity should
    /// start moving to the next [`Node`].
    #[inline]
    #[must_use]
    pub const fn standby_time(&self) -> f32 { self.standby_time }

    /// Sets the maximum speed.
    #[inline]
    pub(in crate::map) fn set_max_speed(&mut self, value: f32) -> Option<Vec2>
    {
        if value.around_equal_narrow(&self.max_speed)
        {
            return None;
        }

        assert!(value > 0f32, "Max speed is not higher than 0.");

        let delta = value - std::mem::replace(&mut self.max_speed, value);
        let opposite = self.min_speed.min(value);
        Vec2::new(delta, opposite - std::mem::replace(&mut self.min_speed, opposite)).into()
    }

    /// Sets the minimum speed.
    #[inline]
    pub(in crate::map) fn set_min_speed(&mut self, value: f32) -> Option<Vec2>
    {
        if value.around_equal_narrow(&self.min_speed)
        {
            return None;
        }

        assert!(value >= 0f32, "Min speed is negative.");

        let delta = value - std::mem::replace(&mut self.min_speed, value);
        let opposite = self.max_speed.max(value);
        Vec2::new(delta, opposite - std::mem::replace(&mut self.max_speed, opposite)).into()
    }

    /// Sets the percentage of the travel to the next [`Node`] dedicated to going from the maximum
    /// to the minimum speed. If needed, adjusts the deceleration value to make sure that the
    /// sum of acceleration and deceleration is at most 1.
    /// # Panics
    /// Panics if `value` is not between 0 and 100.
    #[inline]
    #[must_use]
    pub(in crate::map) fn set_accel_travel_percentage(&mut self, value: f32) -> Option<Vec2>
    {
        if value.around_equal_narrow(&self.accel_travel_percentage)
        {
            return None;
        }

        assert!((0f32..=100f32).contains(&value), "Accel percentage is not within 0 and 100.");

        let delta_accel = value - std::mem::replace(&mut self.accel_travel_percentage, value);
        let decel = self.decel_travel_percentage.min(100f32 - value);
        Vec2::new(delta_accel, decel - std::mem::replace(&mut self.decel_travel_percentage, decel))
            .into()
    }

    /// Returns the percetage of the travel to the next [`Node`] dedicated to going from the minimum
    /// to the maximum speed. If needed, adjusts the acceleration value to make sure that the
    /// sum of acceleration and deceleration is at most 1.
    /// # Panics
    /// Panics if `value` is not between 0 and 100.
    #[inline]
    #[must_use]
    pub(in crate::map) fn set_decel_travel_percentage(&mut self, value: f32) -> Option<Vec2>
    {
        if value.around_equal_narrow(&self.decel_travel_percentage)
        {
            return None;
        }

        assert!((0f32..=100f32).contains(&value), "Decel percentage is not within 0 and 100.");

        let delta_decel = value - std::mem::replace(&mut self.decel_travel_percentage, value);
        let accel = self.accel_travel_percentage.min(100f32 - value);
        Vec2::new(delta_decel, accel - std::mem::replace(&mut self.accel_travel_percentage, accel))
            .into()
    }

    /// Sets the standby time, that is the time that has to pass before the entity should start
    /// moving to the next [`Node`].
    /// # Panics
    /// Panics if `value` is less than 0.
    #[inline]
    #[must_use]
    pub(in crate::map) fn set_standby_time(&mut self, value: f32) -> Option<f32>
    {
        assert!(value >= 0f32, "Standby time is negative.");

        if value.around_equal_narrow(&self.standby_time)
        {
            return None;
        }

        (value - std::mem::replace(&mut self.standby_time, value)).into()
    }

    /// The speed the entity should start moving. If there is no speed up it is the maximum
    /// speed, otherwise the minimum speed.
    #[inline]
    #[must_use]
    pub(in crate::map) fn start_speed(&self) -> f32
    {
        if self.accel_travel_percentage == 0f32
        {
            self.max_speed
        }
        else
        {
            self.min_speed
        }
    }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// A node of the travel [`Path`] of a moving entity.
/// The position of the Node is relative to the center of the entity.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Node
{
    /// The position in 2D space with respect to the center of the entity.
    pub selectable_vector: SelectableVector,
    /// The data concerning how the moving entity should travel to the next [`Node`].
    pub movement:          Movement
}

impl AddAssign<Vec2> for Node
{
    fn add_assign(&mut self, rhs: Vec2) { self.selectable_vector += rhs; }
}

impl SubAssign<Vec2> for Node
{
    fn sub_assign(&mut self, rhs: Vec2) { self.selectable_vector -= rhs; }
}

impl Node
{
    /// Creates a new [`Node`].
    #[inline]
    #[must_use]
    pub(in crate::map::path) fn new(vec: Vec2, selected: bool) -> Self
    {
        Self {
            selectable_vector: SelectableVector::with_selected(vec, selected),
            movement:          Movement::default()
        }
    }

    /// Creates a new [`Node`] from a non relative position.
    /// `center` is the center of the entity this [`Node`] is being assigned to.
    #[inline]
    #[must_use]
    pub(in crate::map::path) fn from_world_pos(pos: Vec2, selected: bool, center: Vec2) -> Self
    {
        Self::new(pos - center, selected)
    }

    /// The position of the node with respect to the center of the entity it is associated with.
    #[inline]
    #[must_use]
    pub const fn pos(&self) -> Vec2 { self.selectable_vector.vec }

    /// The position of the node.
    #[inline]
    #[must_use]
    pub fn world_pos(&self, center: Vec2) -> Vec2 { self.selectable_vector.vec + center }
}

//=======================================================================//

/// A [`Node`] of a [`Path`] expressed in world coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::map) struct NodeWorld(pub Vec2, pub bool);

impl NodeWorld
{
    /// Creates a new [`NodeWorld`]. `center` is the center of the entity `node` belongs to.
    #[inline]
    #[must_use]
    fn new(node: &Node, center: Vec2) -> Self
    {
        Self(node.selectable_vector.vec + center, node.selectable_vector.selected)
    }

    /// Creates a new [`NodeWorld`]. It is assumed that `pos` is a world coordinate.
    #[inline]
    #[must_use]
    const fn from_world_pos(pos: Vec2, selected: bool) -> Self { Self(pos, selected) }
}

//=======================================================================//

/// A [`Node`] of a [`Path`] expressed in world coordinates that can mutate the selection status of
/// the aforementioned [`Node`].
#[derive(Debug)]
pub(in crate::map) struct NodeWorldMut<'a>(pub Vec2, pub &'a mut bool);

impl<'a> NodeWorldMut<'a>
{
    /// Returns a new [`NodeWorldMut`]. `center` is the center of the entity `node` belongs to.
    #[inline]
    #[must_use]
    fn new(node: &'a mut Node, center: Vec2) -> Self
    {
        Self(node.selectable_vector.vec + center, &mut node.selectable_vector.selected)
    }
}

//=======================================================================//

/// The [`Node`]s of a [`Path`] expressed in world coordinates.
pub(in crate::map) struct NodesWorld<'a>
{
    /// The [`Node`]s.
    slice:  &'a HvVec<Node>,
    /// The center of the entity the [`Node`]s belong to.
    center: Vec2
}

impl<'a> NodesWorld<'a>
{
    /// Creates a new [`NodesWorld`].
    #[inline]
    #[must_use]
    pub const fn new(slice: &'a HvVec<Node>, center: Vec2) -> Self { Self { slice, center } }

    /// Returns the first [`NodeWorld`].
    #[inline]
    #[must_use]
    pub fn first(&self) -> NodeWorld { NodeWorld::new(&self.slice[0], self.center) }

    /// Returns the [`NodeWorld`] at index `index`.
    #[inline]
    #[must_use]
    pub fn nth(&self, index: usize) -> NodeWorld { NodeWorld::new(&self.slice[index], self.center) }
}

//=======================================================================//

/// The [`Node`]s of a [`Path`] expressed in world coordinates through [`NodeWorldMut`].
pub(in crate::map) struct NodesWorldMut<'a>
{
    /// The [`Node`]s.
    slice:  &'a mut HvVec<Node>,
    /// The center of the entity the [`Node`]s belong to.
    center: Vec2
}

impl<'a> NodesWorldMut<'a>
{
    /// Creates a new [`NodesWorldMut`]. `center` is the center of the entity the [`Node`]s
    /// belong to.
    #[inline]
    #[must_use]
    pub fn new(slice: &'a mut HvVec<Node>, center: Vec2) -> Self { Self { slice, center } }

    /// An iterator to the [`NodeWorldMut`]s.
    #[inline]
    pub fn iter_mut(&mut self) -> impl ExactSizeIterator<Item = NodeWorldMut>
    {
        self.slice.iter_mut().map(|node| NodeWorldMut::new(node, self.center))
    }
}

//=======================================================================//

/// An iterator used to represent a [`Path`] with the new [`Node`] that is being inserted.
/// The iterator also represents the [`Node`] candidate that overlaps the first [`Node`] of the
/// [`Path`] and that cannot be insertedin it without a second one.
#[derive(Clone)]
pub(in crate::map) struct NodesInsertionIter<'a>
{
    /// The [`Node`]s of the [`Path`] the new [`Node`] is being inserted into.
    slice:             &'a HvVec<Node>,
    /// The center of the entity the [`Path`] belongs to.
    center:            Vec2,
    /// The new [`Node`] being inserted expressed in world coordinates.
    new_node:          NodeWorld,
    /// The index where the new [`Node`] will be inserted.
    new_node_index:    usize,
    /// Whever the iterator has already returned the position of the new [`Node`] being inserted.
    new_node_returned: usize,
    /// The index of the next [`Node`] of `slice` to iterate.
    i:                 usize,
    /// The index of the previous returned [`Node`] of `slice`.
    j:                 usize
}

impl<'a> ExactSizeIterator for NodesInsertionIter<'a>
{
    #[inline]
    #[must_use]
    fn len(&self) -> usize { self.slice.len() - self.i + usize::from(self.new_node_returned != 0) }
}

impl<'a> Iterator for NodesInsertionIter<'a>
{
    type Item = [NodeWorld; 2];

    #[inline]
    #[must_use]
    fn next(&mut self) -> Option<Self::Item>
    {
        if self.len() == 0
        {
            return None;
        }

        if self.new_node_returned != 0
        {
            // The node being inserted is at the end of the path.
            if self.new_node_index == self.slice.len()
            {
                // Return the line from the new node to the first node of the path.
                if self.new_node_returned == 2
                {
                    self.new_node_returned -= 1;
                    self.j = self.i;
                    self.i += 1;

                    return Some([self.new_node, NodeWorld::new(&self.slice[0], self.center)]);
                }

                // At the end of the rope, draw the line from the last node to the one being
                // inserted.
                if self.i == self.slice.len()
                {
                    self.new_node_returned -= 1;

                    return Some([
                        NodeWorld::new(self.slice.last().unwrap(), self.center),
                        self.new_node
                    ]);
                }
            }
            // All other scenarios where the node being inserted is not at the end.
            else if self.i == self.new_node_index
            {
                self.new_node_returned -= 1;

                if self.new_node_returned == 1
                {
                    return Some([
                        NodeWorld::new(&self.slice[self.j], self.center),
                        self.new_node
                    ]);
                }

                let vxs = [
                    self.new_node,
                    NodeWorld::new(&self.slice[self.i], self.center)
                ];
                self.j = self.i;
                self.i += 1;

                return Some(vxs);
            }
        }

        // All cases that do not respond to the special scenarios.
        let value = Some([
            NodeWorld::new(&self.slice[self.j], self.center),
            NodeWorld::new(&self.slice[self.i], self.center)
        ]);

        self.j = self.i;
        self.i += 1;
        value
    }
}

impl<'a> NodesInsertionIter<'a>
{
    /// Creates a new [`NodesInsertionIter`].
    #[inline]
    #[must_use]
    pub fn new(slice: &'a HvVec<Node>, pos: Vec2, index: usize, center: Vec2) -> Self
    {
        Self {
            slice,
            center,
            new_node: NodeWorld::from_world_pos(pos, false),
            new_node_index: index,
            new_node_returned: 2,
            i: 0,
            j: slice.len() - 1
        }
    }
}
