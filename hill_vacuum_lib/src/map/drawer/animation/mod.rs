pub(in crate::map) mod overall_values;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::cmp::Ordering;

use bevy::prelude::UVec2;
use serde::{Deserialize, Serialize};
use shared::{match_or_panic, return_if_no_match};

use super::{
    drawing_resources::{DrawingResources, TextureMaterials},
    Uv
};
use crate::utils::{
    identifiers::{EntityId, Id},
    math::AroundEqual,
    misc::next
};

//=======================================================================//
// TRAITS
//
//=======================================================================//

pub(in crate::map) trait MoveUpDown
{
    fn move_up(&mut self, index: usize);

    fn move_down(&mut self, index: usize);
}

impl<T> MoveUpDown for Vec<T>
{
    #[inline]
    fn move_up(&mut self, index: usize)
    {
        if index == self.len() - 1
        {
            return;
        }

        self.swap(index, index + 1);
    }

    #[inline]
    fn move_down(&mut self, index: usize)
    {
        if index == 0
        {
            return;
        }

        self.swap(index, index - 1);
    }
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[must_use]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(in crate::map) enum Timing
{
    Uniform(f32),
    PerFrame(Vec<f32>)
}

impl Timing
{
    #[inline]
    const fn new() -> Self { Self::Uniform(f32::INFINITY) }

    #[inline]
    #[must_use]
    fn time(&self, index: usize) -> f32
    {
        match self
        {
            Timing::Uniform(time) => *time,
            Timing::PerFrame(vec) => vec[index]
        }
    }
}

//=======================================================================//

#[must_use]
#[derive(Debug)]
pub(in crate::map) enum Animator
{
    List(ListAnimator),
    Atlas(AtlasAnimator)
}

impl EntityId for Animator
{
    #[inline]
    fn id(&self) -> Id
    {
        match self
        {
            Animator::List(a) => a.id(),
            Animator::Atlas(a) => a.id()
        }
    }

    #[inline]
    fn id_as_ref(&self) -> &Id
    {
        match self
        {
            Animator::List(a) => a.id_as_ref(),
            Animator::Atlas(a) => a.id_as_ref()
        }
    }
}

impl Animator
{
    #[inline]
    pub fn new(animation: &Animation, identifier: Id) -> Option<Self>
    {
        if animation.frames() < 2
        {
            return None;
        }

        match animation
        {
            Animation::None => unreachable!(),
            Animation::List(anim) => Self::List(ListAnimator::new(anim, identifier)),
            Animation::Atlas(anim) => Self::Atlas(AtlasAnimator::new(anim, identifier))
        }
        .into()
    }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

#[must_use]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub enum Animation
{
    #[default]
    None,
    List(ListAnimation),
    Atlas(AtlasAnimation)
}

impl Animation
{
    #[inline]
    pub fn list_animation(texture: &str) -> Self { Self::List(ListAnimation::new(texture)) }

    #[inline]
    pub const fn atlas_animation() -> Self { Self::Atlas(AtlasAnimation::new()) }

    #[inline]
    #[must_use]
    pub fn is_none(&self) -> bool { matches!(self, Self::None) }

    #[inline]
    #[must_use]
    pub fn frames(&self) -> usize
    {
        match self
        {
            Animation::None => 0,
            Animation::List(anim) => anim.frames(),
            Animation::Atlas(anim) => anim.frames()
        }
    }

    #[inline]
    pub const fn get_list_animation(&self) -> &ListAnimation
    {
        match_or_panic!(self, Self::List(anim), anim)
    }

    #[inline]
    pub fn get_list_animation_mut(&mut self) -> &mut ListAnimation
    {
        match_or_panic!(self, Self::List(anim), anim)
    }

    #[inline]
    pub const fn get_atlas_animation(&self) -> &AtlasAnimation
    {
        match_or_panic!(self, Self::Atlas(anim), anim)
    }

    #[inline]
    pub fn get_atlas_animation_mut(&mut self) -> &mut AtlasAnimation
    {
        match_or_panic!(self, Self::Atlas(anim), anim)
    }
}

//=======================================================================//

#[must_use]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtlasAnimation
{
    x:      u32,
    y:      u32,
    len:    usize,
    timing: Timing
}

impl MoveUpDown for AtlasAnimation
{
    #[inline]
    fn move_up(&mut self, index: usize)
    {
        match_or_panic!(&mut self.timing, Timing::PerFrame(vec), vec).move_up(index);
    }

    #[inline]
    fn move_down(&mut self, index: usize)
    {
        match_or_panic!(&mut self.timing, Timing::PerFrame(vec), vec).move_down(index);
    }
}

impl AtlasAnimation
{
    #[inline]
    pub(in crate::map) const fn new() -> Self
    {
        Self {
            x:      1,
            y:      1,
            len:    1,
            timing: Timing::new()
        }
    }

    #[inline]
    #[must_use]
    pub const fn frames(&self) -> usize { self.len }

    #[inline]
    #[must_use]
    pub const fn max_len(&self) -> usize { (self.x * self.y) as usize }

    #[inline]
    #[must_use]
    pub const fn size(&self, texture_size: UVec2) -> UVec2
    {
        UVec2::new(texture_size.x / self.x, texture_size.y / self.y)
    }

    #[inline]
    #[must_use]
    pub const fn x_partition(&self) -> u32 { self.x }

    #[inline]
    #[must_use]
    pub const fn y_partition(&self) -> u32 { self.y }

    #[inline]
    #[must_use]
    pub(in crate::map) const fn is_uniform(&self) -> bool
    {
        matches!(self.timing, Timing::Uniform(_))
    }

    #[inline]
    fn fill_per_frame_timing(&mut self, new_len: usize)
    {
        let vec = match &mut self.timing
        {
            Timing::Uniform(_) =>
            {
                self.len = new_len;
                return;
            },
            Timing::PerFrame(vec) => vec
        };

        match new_len.cmp(&self.len)
        {
            Ordering::Less => vec.truncate(new_len),
            Ordering::Equal => (),
            Ordering::Greater =>
            {
                let last_duration = *vec.last().unwrap();
                vec.extend(std::iter::repeat(last_duration).take(new_len - self.len));
            }
        };

        self.len = new_len;
    }

    #[inline]
    #[must_use]
    pub(in crate::map) fn set_x_partition(&mut self, x: u32) -> Option<u32>
    {
        if self.x == x
        {
            return None;
        }

        let prev = std::mem::replace(&mut self.x, x);
        self.fill_per_frame_timing((self.x * self.y) as usize);
        prev.into()
    }

    #[inline]
    #[must_use]
    pub(in crate::map) fn set_y_partition(&mut self, y: u32) -> Option<u32>
    {
        if self.y == y
        {
            return None;
        }

        let prev = std::mem::replace(&mut self.y, y);
        self.fill_per_frame_timing((self.x * self.y) as usize);
        prev.into()
    }

    #[inline]
    #[must_use]
    pub(in crate::map) fn set_len(&mut self, len: usize) -> Option<usize>
    {
        if self.len == len
        {
            return None;
        }

        let prev = self.len;
        self.fill_per_frame_timing(len);
        prev.into()
    }

    #[inline]
    pub(in crate::map) fn set_timing(&mut self, timing: Timing) -> Timing
    {
        std::mem::replace(&mut self.timing, timing)
    }

    #[inline]
    #[must_use]
    pub(in crate::map) fn set_uniform(&mut self) -> Option<Timing>
    {
        let new = Timing::Uniform(
            return_if_no_match!(&mut self.timing, Timing::PerFrame(vec), vec, None)[0]
        );
        std::mem::replace(&mut self.timing, new).into()
    }

    #[inline]
    #[must_use]
    pub(in crate::map) fn set_uniform_time(&mut self, value: f32) -> Option<f32>
    {
        assert!(value > 0f32, "Time is not higher than 0.");
        let time = match_or_panic!(&mut self.timing, Timing::Uniform(value), value);

        if value.around_equal_narrow(time)
        {
            return None;
        }

        std::mem::replace(time, value).into()
    }

    #[inline]
    #[must_use]
    pub(in crate::map) fn set_per_frame(&mut self) -> Option<Timing>
    {
        let new = Timing::PerFrame(return_if_no_match!(
            &mut self.timing,
            Timing::Uniform(duration),
            vec![*duration; self.len],
            None
        ));

        std::mem::replace(&mut self.timing, new).into()
    }

    #[inline]
    #[must_use]
    pub(in crate::map) fn set_frame_time(&mut self, index: usize, value: f32) -> Option<f32>
    {
        assert!(value > 0f32, "Time is not higher than 0.");
        let time = &mut match_or_panic!(&mut self.timing, Timing::PerFrame(vec), vec)[index];

        if value.around_equal_narrow(time)
        {
            return None;
        }

        std::mem::replace(time, value).into()
    }
}

//=======================================================================//

#[must_use]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAnimation(Vec<(String, f32)>);

impl MoveUpDown for ListAnimation
{
    #[inline]
    fn move_up(&mut self, index: usize) { self.0.move_up(index); }

    #[inline]
    fn move_down(&mut self, index: usize) { self.0.move_down(index); }
}

impl ListAnimation
{
    #[inline]
    pub(in crate::map) fn new(texture: &str) -> Self
    {
        Self(vec![(texture.to_owned(), f32::INFINITY)])
    }

    #[inline]
    #[must_use]
    pub fn frames(&self) -> usize { self.0.len() }

    #[inline]
    #[must_use]
    pub fn frame(&self, index: usize) -> &(String, f32) { &self.0[index] }

    #[inline]
    pub(in crate::map) fn push(&mut self, texture: &str)
    {
        self.0.push((texture.to_owned(), self.0.last().unwrap().1));
    }

    #[inline]
    pub(in crate::map) fn insert(&mut self, index: usize, texture: &str, time: f32)
    {
        self.0.insert(index, (texture.to_owned(), time));
    }

    #[inline]
    pub(in crate::map) fn pop(&mut self) { _ = self.0.pop().unwrap(); }

    #[inline]
    pub(in crate::map) fn remove(&mut self, index: usize) { self.0.remove(index); }

    #[inline]
    #[must_use]
    pub(in crate::map) fn set_texture(&mut self, index: usize, texture: &str) -> Option<String>
    {
        if index == self.frames()
        {
            self.push(texture);
            return None;
        }

        let prev = &mut self.0[index].0;

        if *prev == texture
        {
            return None;
        }

        std::mem::replace(prev, texture.to_owned()).into()
    }

    #[inline]
    #[must_use]
    pub(in crate::map) fn set_time(&mut self, index: usize, value: f32) -> Option<f32>
    {
        let prev = &mut self.0[index].1;

        if value.around_equal_narrow(prev)
        {
            return None;
        }

        std::mem::replace(prev, value).into()
    }
}

//=======================================================================//

#[must_use]
#[derive(Debug)]
pub(in crate::map) struct ListAnimator
{
    id:           Id,
    index:        usize,
    current_time: f32
}

impl EntityId for ListAnimator
{
    #[inline]
    fn id(&self) -> Id { self.id }

    #[inline]
    fn id_as_ref(&self) -> &Id { &self.id }
}

impl ListAnimator
{
    #[inline]
    pub fn new(animation: &ListAnimation, identifier: Id) -> Self
    {
        Self {
            id:           identifier,
            index:        0,
            current_time: animation.0[0].1
        }
    }

    #[inline]
    pub fn update(&mut self, animation: &ListAnimation, mut delta_time: f32)
    {
        loop
        {
            self.current_time -= delta_time;

            if self.current_time > 0f32
            {
                break;
            }

            self.index = next(self.index, animation.frames());
            delta_time = std::mem::replace(&mut self.current_time, animation.0[self.index].1).abs();
        }
    }

    #[inline]
    pub(in crate::map::drawer) fn texture<'a>(
        &self,
        drawing_resources: &'a DrawingResources,
        animation: &ListAnimation
    ) -> &'a TextureMaterials
    {
        drawing_resources.texture_materials(&animation.0[self.index].0)
    }
}

//=======================================================================//

#[must_use]
#[derive(Debug)]
pub(in crate::map) struct AtlasAnimator
{
    id:           Id,
    index:        usize,
    row:          f32,
    column:       f32,
    x_uv_section: f32,
    y_uv_section: f32,
    current_time: f32
}

impl EntityId for AtlasAnimator
{
    #[inline]
    fn id(&self) -> Id { self.id }

    #[inline]
    fn id_as_ref(&self) -> &Id { &self.id }
}

impl AtlasAnimator
{
    #[allow(clippy::cast_precision_loss)]
    #[inline]
    pub fn new(animation: &AtlasAnimation, identifier: Id) -> Self
    {
        Self {
            id:           identifier,
            index:        0,
            row:          0f32,
            column:       0f32,
            x_uv_section: 1f32 / animation.x as f32,
            y_uv_section: 1f32 / animation.y as f32,
            current_time: animation.timing.time(0)
        }
    }

    #[allow(clippy::float_cmp)]
    #[allow(clippy::cast_precision_loss)]
    #[inline]
    pub fn update(&mut self, animation: &AtlasAnimation, mut delta_time: f32)
    {
        loop
        {
            self.current_time -= delta_time;

            if self.current_time > 0f32
            {
                break;
            }

            self.index = next(self.index, animation.frames());

            if self.index == 0
            {
                self.column = 0f32;
                self.row = 0f32;
            }
            else if self.column == (animation.x - 1) as f32
            {
                self.column = 0f32;
                self.row += 1f32;
            }
            else
            {
                self.column += 1f32;
            }

            delta_time =
                std::mem::replace(&mut self.current_time, animation.timing.time(self.index)).abs();
        }
    }

    #[inline]
    pub fn pivot(&self) -> Uv
    {
        [
            self.x_uv_section * self.column,
            self.y_uv_section * self.row
        ]
    }
}
