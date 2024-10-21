#![allow(dead_code)]

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use glam::Vec2;
use serde::{Deserialize, Deserializer};

use crate::{
    map::selectable_vector::SelectableVector,
    utils::{
        collections::{hv_hash_map, hv_vec},
        math::HashVec2
    },
    HvHashMap,
    HvVec,
    Movement
};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

#[must_use]
#[derive(Deserialize)]
struct Hull
{
    top:    f32,
    bottom: f32,
    left:   f32,
    right:  f32
}

impl Hull
{
    #[inline]
    fn new(top: f32, bottom: f32, left: f32, right: f32) -> Option<Self>
    {
        if bottom > top || left > right
        {
            return None;
        }

        Self {
            top,
            bottom,
            left,
            right
        }
        .into()
    }

    #[inline]
    fn from_points(points: impl IntoIterator<Item = Vec2>) -> Self
    {
        let (mut top, mut bottom, mut left, mut right) = (f32::MIN, f32::MAX, f32::MAX, f32::MIN);

        for vx in points
        {
            if vx.y > top
            {
                top = vx.y;
            }

            if vx.y < bottom
            {
                bottom = vx.y;
            }

            if vx.x < left
            {
                left = vx.x;
            }

            if vx.x > right
            {
                right = vx.x;
            }
        }

        Hull::new(top, bottom, left, right).unwrap()
    }
}

//=======================================================================//

#[must_use]
#[derive(Deserialize)]
pub(in crate::map) struct Node
{
    pub selectable_vector: SelectableVector,
    pub movement:          Movement
}

//=======================================================================//

#[must_use]
pub(in crate::map) struct Path
{
    nodes:   HvVec<Node>,
    hull:    Hull,
    buckets: Buckets
}

impl<'de> Deserialize<'de> for Path
{
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Path, D::Error>
    where
        D: Deserializer<'de>
    {
        HvVec::deserialize(deserializer).map(|nodes| {
            let hull = Path::nodes_hull(nodes.iter());
            let mut buckets = Buckets::new();

            for (i, node) in nodes.iter().enumerate()
            {
                buckets.insert(i, node.selectable_vector.vec);
            }

            Self {
                nodes,
                hull,
                buckets
            }
        })
    }
}

impl Path
{
    #[inline]
    fn nodes_hull<'a, I: ExactSizeIterator<Item = &'a Node>>(nodes: I) -> Hull
    {
        Hull::from_points(nodes.map(|node| node.selectable_vector.vec))
    }
}

//=======================================================================//

struct Buckets(HvHashMap<HashVec2, HvVec<usize>>);

impl Buckets
{
    #[inline]
    pub fn new() -> Self { Self(hv_hash_map![]) }

    #[inline]
    pub fn insert(&mut self, index: usize, pos: Vec2)
    {
        let key = HashVec2(pos);

        for bucket in self.0.values_mut()
        {
            for idx in bucket.iter_mut().filter(|idx| **idx >= index)
            {
                *idx += 1;
            }
        }

        match self.0.get_mut(&key)
        {
            Some(idxs) =>
            {
                assert!(!idxs.contains(&index), "Bucket already contains index {index}");
                idxs.push(index);
                idxs.sort_unstable();
            },
            None => _ = self.0.insert(key, hv_vec![index])
        };
    }
}
