#[cfg(feature = "ui")]
pub(in crate::map) mod overall_values;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use glam::UVec2;
use serde::{Deserialize, Serialize};

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The duration of the frames of an [`Atlas`] animation.
#[must_use]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Timing
{
    /// Same duration for all frames.
    Uniform(f32),
    /// Different time for all frames.
    PerFrame(Vec<f32>)
}

impl Timing
{
    /// Returns the duration of the frame at `index`.
    #[inline]
    #[must_use]
    pub fn time(&self, index: usize) -> f32
    {
        match self
        {
            Timing::Uniform(time) => *time,
            Timing::PerFrame(vec) => vec[index]
        }
    }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The animation of a texture.
#[allow(clippy::missing_docs_in_private_items)]
#[must_use]
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub enum Animation
{
    /// None.
    #[default]
    None,
    /// A list of frames.
    List(List),
    /// A texture partitioning.
    Atlas(Atlas)
}

//=======================================================================//

/// The partitioning of a texture into sub-textures.
#[must_use]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Atlas
{
    /// The columns.
    x:      u32,
    /// The rows.
    y:      u32,
    /// The amount of frames.
    len:    usize,
    /// The time the frames must be drawn.
    timing: Timing
}

impl Atlas
{
    /// The amount of frames.
    #[allow(clippy::len_without_is_empty)]
    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize { self.len }

    /// The maximum possible amount of frames.
    #[inline]
    #[must_use]
    pub const fn max_len(&self) -> usize { (self.x * self.y) as usize }

    /// The size of the area of the texture to draw.
    #[inline]
    #[must_use]
    pub const fn size(&self, texture_size: UVec2) -> UVec2
    {
        UVec2::new(texture_size.x / self.x, texture_size.y / self.y)
    }

    /// The amounts of rows of the atlas.
    #[inline]
    #[must_use]
    pub const fn x_partition(&self) -> u32 { self.x }

    /// The amount of columns of the atlas.
    #[inline]
    #[must_use]
    pub const fn y_partition(&self) -> u32 { self.y }

    /// Returns a reference to the [`Timing`] of the atlas.
    #[inline]
    pub const fn timing(&self) -> &Timing { &self.timing }
}

//=======================================================================//

/// A list of textures and the amount of time they should be drawn on screen.
#[must_use]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct List(Vec<(String, f32)>);

impl List
{
    /// Returns the amount of frames in the animation.
    #[allow(clippy::len_without_is_empty)]
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize { self.0.len() }

    /// Returns a reference to the frame at `index`.
    #[inline]
    #[must_use]
    pub fn frame(&self, index: usize) -> &(String, f32) { &self.0[index] }

    /// Returns a reference to the frames of the animation.
    #[inline]
    #[must_use]
    pub const fn frames(&self) -> &Vec<(String, f32)> { &self.0 }
}

//=======================================================================//
// UI
//
//=======================================================================//

#[cfg(feature = "ui")]
pub(in crate::map) mod ui_mod
{
    //=======================================================================//
    // IMPORTS
    //
    //=======================================================================//

    use std::cmp::Ordering;

    use hill_vacuum_shared::{match_or_panic, return_if_no_match};

    use crate::{
        map::drawer::{
            drawers::Uv,
            drawing_resources::{DrawingResources, TextureMaterials}
        },
        utils::{identifiers::EntityId, math::AroundEqual, misc::next},
        Animation,
        Atlas,
        Id,
        List,
        Timing
    };

    //=======================================================================//
    // TRAITS
    //
    //=======================================================================//

    /// A trait to move up and down frames of a texture animation.
    pub(in crate::map) trait MoveUpDown
    {
        /// Moves the frame at `index` to the previous index.
        fn move_up(&mut self, index: usize);

        /// Moves the frame at `index` to the next index.
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

    impl Timing
    {
        /// Returns a new [`Timing`] with uniform frame time.
        #[inline]
        const fn new() -> Self { Self::Uniform(f32::INFINITY) }
    }

    //=======================================================================//

    /// A struct that updates that animation of a texture.
    #[must_use]
    #[derive(Debug)]
    pub(in crate::map) enum Animator
    {
        /// Animates a [`List`] animation.
        List(ListAnimator),
        /// Animates an [`Atlas`] animation.
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
        /// Returns a new [`Animator`].
        #[inline]
        pub fn new(animation: &Animation, identifier: Id) -> Option<Self>
        {
            if animation.len() < 2
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

    impl Animation
    {
        /// Returns a [`List`] animation with a single texture.
        #[inline]
        pub(in crate::map) fn list_animation(texture: &str) -> Self
        {
            Self::List(List::new(texture))
        }

        /// Returns a default [`Atlas`] animation.
        #[inline]
        pub(in crate::map) const fn atlas_animation() -> Self { Self::Atlas(Atlas::new()) }

        /// Whether there is no animation.
        #[inline]
        #[must_use]
        pub const fn is_none(&self) -> bool { matches!(self, Self::None) }

        /// Returns the amount of frames.
        #[allow(clippy::len_without_is_empty)]
        #[inline]
        #[must_use]
        pub fn len(&self) -> usize
        {
            match self
            {
                Animation::None => 0,
                Animation::List(anim) => anim.len(),
                Animation::Atlas(anim) => anim.len()
            }
        }

        /// Returns a reference to the [`List`] animation.
        #[inline]
        pub(in crate::map) const fn get_list_animation(&self) -> &List
        {
            match_or_panic!(self, Self::List(anim), anim)
        }

        /// Returns a mutable reference to the [`List`] animation.
        #[inline]
        pub(in crate::map) fn get_list_animation_mut(&mut self) -> &mut List
        {
            match_or_panic!(self, Self::List(anim), anim)
        }

        /// Returns a reference to the [`Atlas`] animation.
        #[inline]
        pub(in crate::map) const fn get_atlas_animation(&self) -> &Atlas
        {
            match_or_panic!(self, Self::Atlas(anim), anim)
        }

        /// Returns a mutable reference to the [`Atlas`] animation.
        #[inline]
        pub(in crate::map) fn get_atlas_animation_mut(&mut self) -> &mut Atlas
        {
            match_or_panic!(self, Self::Atlas(anim), anim)
        }
    }

    //=======================================================================//

    impl MoveUpDown for Atlas
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

    impl Atlas
    {
        /// Returns a new [`Atlas`].
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

        /// Whether the [`Timing`] is uniform.
        #[inline]
        #[must_use]
        pub(in crate::map) const fn is_uniform(&self) -> bool
        {
            matches!(self.timing, Timing::Uniform(_))
        }

        /// Resizes the amount of frames to `new_len`.
        #[inline]
        fn resize_frames(&mut self, new_len: usize)
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

        /// Sets the amount of rows in which the texture is partitioned. Returns the previous vale
        /// if different.
        #[inline]
        #[must_use]
        pub(in crate::map) fn set_x_partition(&mut self, x: u32) -> Option<u32>
        {
            if self.x == x
            {
                return None;
            }

            let prev = std::mem::replace(&mut self.x, x);
            self.resize_frames((self.x * self.y) as usize);
            prev.into()
        }

        /// Sets the amount of columns in which the texture is partitioned. Returns the previous
        /// vale if different.
        #[inline]
        #[must_use]
        pub(in crate::map) fn set_y_partition(&mut self, y: u32) -> Option<u32>
        {
            if self.y == y
            {
                return None;
            }

            let prev = std::mem::replace(&mut self.y, y);
            self.resize_frames((self.x * self.y) as usize);
            prev.into()
        }

        /// Sets the amount of frames that should be drawn.
        #[inline]
        #[must_use]
        pub(in crate::map) fn set_len(&mut self, len: usize) -> Option<usize>
        {
            if self.len == len
            {
                return None;
            }

            let prev = self.len;
            self.resize_frames(len);
            prev.into()
        }

        /// Sets the [`Timing`] to `timing`. Returns the previous value.
        #[inline]
        pub(in crate::map) fn set_timing(&mut self, timing: Timing) -> Timing
        {
            std::mem::replace(&mut self.timing, timing)
        }

        /// Sets the [`Timing`] to [`Timing::Uniform`]. Returns the previous value if it was a
        /// [`Timing::PerFrame`].
        #[inline]
        #[must_use]
        pub(in crate::map) fn set_uniform(&mut self) -> Option<Timing>
        {
            let new = Timing::Uniform(
                return_if_no_match!(&mut self.timing, Timing::PerFrame(vec), vec, None)[0]
            );
            std::mem::replace(&mut self.timing, new).into()
        }

        /// Sets the time of a [`Timing::Uniform`]. Returns the previous value if different.
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

        /// Sets the [`Timing`] to [`Timing::PerFrame`]. Returns the previous value if it was a
        /// [`Timing::Uniform`].
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

        /// Sets the time of the frame at `index`. Returns the preious value if different.
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

    impl MoveUpDown for List
    {
        #[inline]
        fn move_up(&mut self, index: usize) { self.0.move_up(index); }

        #[inline]
        fn move_down(&mut self, index: usize) { self.0.move_down(index); }
    }

    impl List
    {
        /// Returns a new [`List`].
        #[inline]
        pub(in crate::map) fn new(texture: &str) -> Self
        {
            Self(vec![(texture.to_owned(), f32::INFINITY)])
        }

        /// Pushes a new frame.
        #[inline]
        pub(in crate::map) fn push(&mut self, texture: &str)
        {
            self.0.push((texture.to_owned(), self.0.last().unwrap().1));
        }

        /// Inserts a new frame at `index`.
        #[inline]
        pub(in crate::map) fn insert(&mut self, index: usize, texture: &str, time: f32)
        {
            self.0.insert(index, (texture.to_owned(), time));
        }

        /// Removes the last frame.
        #[inline]
        pub(in crate::map) fn pop(&mut self) { _ = self.0.pop().unwrap(); }

        /// Removes the frame at `index`.
        #[inline]
        pub(in crate::map) fn remove(&mut self, index: usize) { self.0.remove(index); }

        /// Sets the texture of the frame at `index`. Returns the previous value if different.
        #[inline]
        #[must_use]
        pub(in crate::map) fn set_texture(&mut self, index: usize, texture: &str)
            -> Option<String>
        {
            if index == self.len()
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

        /// Sets the time of the frame at `index`. Returns the previous value if different.
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

    /// The updater of a list [`Animation`].
    #[must_use]
    #[derive(Debug)]
    pub(in crate::map) struct ListAnimator
    {
        /// The [`Id`] of the entity whose texture must be animated.
        id:           Id,
        /// The index of the texture to draw.
        index:        usize,
        /// The time the current frame is drawn on screen.
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
        /// Returns a new [`ListAnimator`].
        #[inline]
        pub fn new(animation: &List, identifier: Id) -> Self
        {
            Self {
                id:           identifier,
                index:        0,
                current_time: animation.0[0].1
            }
        }

        /// Updates the texture to draw.
        #[inline]
        pub fn update(&mut self, animation: &List, mut delta_time: f32)
        {
            loop
            {
                self.current_time -= delta_time;

                if self.current_time > 0f32
                {
                    break;
                }

                self.index = next(self.index, animation.len());
                delta_time =
                    std::mem::replace(&mut self.current_time, animation.0[self.index].1).abs();
            }
        }

        /// Returns a reference to the [`TextureMaterials`] of the animationtexture to draw.
        #[inline]
        pub(in crate::map::drawer) fn texture<'a>(
            &self,
            drawing_resources: &'a DrawingResources,
            animation: &List
        ) -> &'a TextureMaterials
        {
            drawing_resources.texture_materials(&animation.0[self.index].0)
        }
    }

    //=======================================================================//

    /// The updater of an atlas [`Animation`].
    #[must_use]
    #[derive(Debug)]
    pub(in crate::map) struct AtlasAnimator
    {
        /// The [`Id`] of the entity whose texture must be animated.
        id:           Id,
        /// The frame of the atlas to draw.
        index:        usize,
        /// The row of the atlas where the frame to draw is.
        row:          f32,
        /// The column of the atlas where the frame to draw is.
        column:       f32,
        /// The width of the atlas frame in UV coordinated.
        x_uv_section: f32,
        /// The height of the atlas frame in UV coordinated.
        y_uv_section: f32,
        /// The time the current frame is drawn on screen.
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
        /// Returns a new [`AtlasAnimator`].
        #[allow(clippy::cast_precision_loss)]
        #[inline]
        pub fn new(animation: &Atlas, identifier: Id) -> Self
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

        /// Updates the portion of the texture to draw.
        #[allow(clippy::float_cmp)]
        #[allow(clippy::cast_precision_loss)]
        #[inline]
        pub fn update(&mut self, animation: &Atlas, mut delta_time: f32)
        {
            loop
            {
                self.current_time -= delta_time;

                if self.current_time > 0f32
                {
                    break;
                }

                self.index = next(self.index, animation.len());

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
                    std::mem::replace(&mut self.current_time, animation.timing.time(self.index))
                        .abs();
            }
        }

        /// The UV coordinates of the top left point of the portion of the texture to draw.
        #[inline]
        pub fn pivot(&self) -> Uv
        {
            [
                self.x_uv_section * self.column,
                self.y_uv_section * self.row
            ]
        }
    }
}

#[cfg(feature = "ui")]
pub(in crate::map) use ui_mod::*;
