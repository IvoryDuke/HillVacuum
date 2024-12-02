//=======================================================================//
// IMPORTS
//
//=======================================================================//

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{utils::math::points::rotate_point_around_origin, Animation};

//=======================================================================//
// MACROS
//
//=======================================================================//

macro_rules! sprite_values {
    ($($value:ident),+) => {$(
        #[inline]
        #[must_use]
        pub const fn $value(&self) -> f32
        {
            match self
            {
                Sprite::True { .. } => 0f32,
                Sprite::False { $value, .. } => *$value
            }
        }
    )+};

    ($(($func:ident, $t:ty, $value:ident)),+) => {$(
        #[inline]
        #[must_use]
        fn $func(&self) -> $t
        {
            match self
            {
                Sprite::True { .. } => <$t>::default(),
                Sprite::False { offset_auxiliary, .. } =>
                {
                    offset_auxiliary.scale.$value + offset_auxiliary.rotation.$value
                }
            }
        }
    )+};
}

pub(in crate::map) use sprite_values;

//=======================================================================//
// TRAITS
//
//=======================================================================//

/// A trait to return information about a texture.
pub trait TextureInterface
{
    /// Returns the name of the texture.
    fn name(&self) -> &str;

    /// Returns the horizontal offset of the texture as set by the user.
    #[must_use]
    fn offset_x(&self) -> f32;

    /// Returns the vertical offset of the texture as set by the user.
    #[must_use]
    fn offset_y(&self) -> f32;

    /// Returns the offset necessary to draw the texture as displayed in the editor.
    /// It may be different from the value returned by `TextureInterface::offset_x` and
    /// `TextureInterface::offset_y`.
    #[must_use]
    fn draw_offset(&self) -> Vec2;

    /// Returns the offset necessary to draw the texture as displayed in the editor also accounting
    /// for scroll and parallax.  
    /// !! If the texture is rendered as a sprite, scroll and parallax are both always 0.
    #[must_use]
    fn draw_offset_with_parallax_and_scroll(&self, camera_pos: Vec2, elapsed_time: f32) -> Vec2;

    /// Returns the horizontal scale of the texture.
    #[must_use]
    fn scale_x(&self) -> f32;

    /// Returns the vertical scale of the texture.
    #[must_use]
    fn scale_y(&self) -> f32;

    /// The horizontal scrolling.
    #[must_use]
    fn scroll_x(&self) -> f32;

    /// The vertical scrolling.
    #[must_use]
    fn scroll_y(&self) -> f32;

    /// The horizontal parallax.
    #[must_use]
    fn parallax_x(&self) -> f32;

    /// The vertical parallax.
    #[must_use]
    fn parallax_y(&self) -> f32;

    /// The angle.
    #[must_use]
    fn angle(&self) -> f32;

    /// The draw height.
    #[must_use]
    fn height(&self) -> i8;

    /// The draw height as [`f32`].
    #[must_use]
    fn height_f32(&self) -> f32;

    /// Whether the texture should be rendered like a sprite
    #[must_use]
    fn sprite(&self) -> bool;

    /// Returns a reference to the [`Animation`].
    fn animation(&self) -> &Animation;
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// Whether the texture should be rendered as a sprite.
#[must_use]
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq)]
pub(in crate::map::drawer) enum Sprite
{
    /// Yes.
    True,
    /// No.
    False
    {
        /// The horizontal parallax of the texture.
        parallax_x:       f32,
        /// The vertical parallax of the texture.
        parallax_y:       f32,
        /// The horizontal scrolling of the texture.
        scroll_x:         f32,
        /// The vertical scrolling of the texture.
        scroll_y:         f32,
        offset_auxiliary: OffsetAuxiliary
    }
}

impl Sprite
{
    sprite_values!(parallax_x, parallax_y, scroll_x, scroll_y);

    sprite_values!((auxiliary_offset_x, f32, offset_x), (auxiliary_offset_y, f32, offset_y));

    /// Whether `self` has value [`Sprite::True`].
    #[inline]
    #[must_use]
    pub const fn enabled(&self) -> bool { matches!(self, Self::True { .. }) }
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The information relative to which texture should be drawn and how.
#[allow(clippy::missing_docs_in_private_items)]
#[allow(clippy::unsafe_derive_deserialize)]
#[must_use]
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct TextureSettings
{
    texture:   String,
    scale_x:   f32,
    scale_y:   f32,
    offset_x:  f32,
    offset_y:  f32,
    angle:     f32,
    height:    i8,
    sprite:    Sprite,
    animation: Animation
}

impl TextureInterface for TextureSettings
{
    #[inline]
    fn name(&self) -> &str { &self.texture }

    #[inline]
    fn offset_x(&self) -> f32 { self.offset_x }

    #[inline]
    fn offset_y(&self) -> f32 { self.offset_y }

    #[inline]
    fn draw_offset(&self) -> Vec2
    {
        Vec2::new(
            self.offset_x + self.sprite.auxiliary_offset_x(),
            self.offset_y - self.sprite.auxiliary_offset_y()
        )
    }

    #[inline]
    fn draw_offset_with_parallax_and_scroll(&self, camera_pos: Vec2, elapsed_time: f32) -> Vec2
    {
        #[allow(clippy::match_bool)]
        let p_s = camera_pos * Vec2::new(self.parallax_x(), self.parallax_y()) +
            Vec2::new(self.scroll_x(), self.scroll_y()) * elapsed_time;

        if self.angle == 0f32
        {
            return self.draw_offset() + p_s;
        }

        self.draw_offset() + rotate_point_around_origin(p_s, self.angle.to_radians())
    }

    #[inline]
    fn scale_x(&self) -> f32 { self.scale_x }

    #[inline]
    #[must_use]
    fn scale_y(&self) -> f32 { self.scale_y }

    #[inline]
    fn scroll_x(&self) -> f32 { self.sprite.scroll_x() }

    #[inline]
    fn scroll_y(&self) -> f32 { self.sprite.scroll_y() }

    #[inline]
    fn parallax_x(&self) -> f32 { self.sprite.parallax_x() }

    #[inline]
    fn parallax_y(&self) -> f32 { self.sprite.parallax_y() }

    #[inline]
    fn height(&self) -> i8 { self.height }

    #[inline]
    fn height_f32(&self) -> f32 { f32::from(self.height) }

    #[inline]
    fn angle(&self) -> f32 { self.angle }

    #[inline]
    fn sprite(&self) -> bool { self.sprite.enabled() }

    #[inline]
    fn animation(&self) -> &Animation { &self.animation }
}

impl TextureSettings
{
    /// Sets the [`Animation`] without checking the map bounds.
    #[inline]
    pub(in crate::map) unsafe fn unsafe_set_animation(&mut self, animation: Animation)
    {
        self.animation = animation;
    }
}

//=======================================================================//

#[must_use]
#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub(in crate::map::drawer) struct OffsetAuxiliary
{
    scale:    ScaleOffset,
    rotation: RotationOffset
}

//=======================================================================//

#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
struct ScaleOffset
{
    offset_x: f32,
    offset_y: f32
}

//=======================================================================//

#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
struct RotationOffset
{
    offset_x:   f32,
    offset_y:   f32,
    pivot_mod:  Vec2,
    last_pivot: Option<Vec2>
}

//=======================================================================//

/// The animation associated to a texture.
#[must_use]
#[derive(Serialize, Deserialize)]
pub(in crate::map) struct DefaultAnimation
{
    /// The texture.
    pub texture:   String,
    /// The animation.
    pub animation: Animation
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

    use bevy::{
        asset::{Assets, Handle},
        image::{Image, ImageSampler, ImageSamplerDescriptor}
    };
    use glam::{UVec2, Vec2};
    use hill_vacuum_shared::{match_or_panic, return_if_none, TEXTURE_HEIGHT_RANGE};

    use super::{OffsetAuxiliary, RotationOffset, Sprite};
    use crate::{
        map::{
            brush::convex_polygon::ScaleInfo,
            drawer::{
                animation::{Animator, MoveUpDown},
                drawing_resources::DrawingResources,
                TextureSize
            },
            editor::{state::grid::Grid, Placeholder},
            OutOfBounds
        },
        utils::{
            hull::Hull,
            math::{
                points::{rotate_point, rotate_point_around_origin},
                AroundEqual
            },
            misc::{AssertNormalizedDegreesAngle, ReplaceValue, SwapValue, TakeValue, Translate}
        },
        Animation,
        TextureInterface,
        TextureSettings,
        Timing
    };

    //=======================================================================//
    // MACROS
    //
    //=======================================================================//

    /// Generates the code for functions relative to x/y parameters.
    macro_rules! xy {
        ($($xy:ident),+) => { paste::paste! { $(
            #[inline]
            #[must_use]
            pub(in crate::map) fn [< check_offset_ $xy >](
                &mut self,
                drawing_resources: &DrawingResources,
                grid: &Grid,
                value: f32,
                brush_center: Vec2
            ) -> bool
            {
                if !self.sprite.enabled() || value.around_equal_narrow(&self.[< offset_ $xy >])
                {
                    return true;
                }

                let prev = self.[< offset_ $xy >].replace_value(value);
                let result = self.check_sprite_vxs(drawing_resources, grid, brush_center).is_ok();
                self.[< offset_ $xy >] = prev;
                result
            }

            #[inline]
            #[must_use]
            pub(in crate::map) fn [< set_offset_ $xy >](&mut self, value: f32) -> Option<f32>
            {
                if value.around_equal_narrow(&self.[< offset_ $xy >])
                {
                    return None;
                }

                let prev = self.[< offset_ $xy >].replace_value(value);
                prev.into()
            }

            #[inline]
            pub(in crate::map) fn [< check_scale_ $xy >](
                &mut self,
                drawing_resources: &DrawingResources,
                grid: &Grid,
                value: f32,
                brush_center: Vec2
            ) -> bool
            {
                assert!(value != 0f32, "Scale is 0.");

                if !self.sprite.enabled() || value.around_equal_narrow(&self.[< scale_ $xy >])
                {
                    return true;
                }

                let prev = self.[< scale_ $xy >].replace_value(value);
                let result = self.check_sprite_vxs(drawing_resources, grid, brush_center).is_ok();
                self.[< scale_ $xy >] = prev;
                result
            }

            #[inline]
            #[must_use]
            pub(in crate::map) fn [< set_scale_ $xy >](&mut self, value: f32) -> Option<f32>
            {
                assert!(value != 0f32, "Scale is 0.");

                if value.around_equal_narrow(&self.[< scale_ $xy >])
                {
                    return None;
                }

                let prev = self.[< scale_ $xy >].replace_value(value);
                prev.into()
            }

            #[inline]
            pub(in crate::map) fn [< check_ $xy _flip >](
                &mut self,
                drawing_resources: &DrawingResources,
                grid: &Grid,
                mirror: f32,
                old_brush_center: Vec2,
                new_center: Vec2
            ) -> bool
            {
                let sprite_pivot = return_if_none!(self.sprite_pivot(old_brush_center), true);
                let [< new_offset_ $xy >] = mirror - sprite_pivot.$xy - new_center.$xy;
                let [< prev_offset_ $xy >] = self.[< offset_ $xy >].replace_value([< new_offset_ $xy >]);

                let result = self.check_sprite_vxs(drawing_resources, grid, new_center).is_ok();
                self.[< offset_ $xy >] = [< prev_offset_ $xy >];
                result
            }

            #[inline]
            pub(in crate::map) fn [< $xy _flip >](
                &mut self,
                mirror: f32,
                old_brush_center: Vec2,
                new_center: Vec2
            )
            {
                self.[< scale_ $xy >] = -self.[< scale_ $xy >];
                let sprite_pivot = return_if_none!(self.sprite_pivot(old_brush_center));
                self.[< offset_ $xy >] = mirror - sprite_pivot.$xy - new_center.$xy;
            }

            #[inline]
            #[must_use]
            pub(in crate::map) fn [< set_parallax_ $xy >](&mut self, value: f32) -> Option<f32>
            {
                let prev = self.[< parallax_ $xy >]();

                if value.around_equal_narrow(&prev)
                {
                    return None;
                }

                self.sprite.[< set_parallax_ $xy >](value);
                prev.into()
            }

            #[inline]
            #[must_use]
            pub(in crate::map) fn [< set_scroll_ $xy >](&mut self, value: f32) -> Option<f32>
            {
                let prev = self.[< scroll_ $xy >]();

                if value.around_equal_narrow(&prev)
                {
                    return None;
                }

                self.sprite.[< set_scroll_ $xy >](value);
                prev.into()
            }

            #[inline]
            #[must_use]
            pub(in crate::map) fn [< check_atlas_animation_ $xy _partition >](
                &mut self,
                drawing_resources: &DrawingResources,
                grid: &Grid,
                value: u32,
                brush_center: Vec2
            ) -> bool
            {
                let prev = {
                    let atlas = self.animation.get_atlas_animation_mut();
                    let prev = atlas.[< $xy _partition >]();

                    if prev <= value
                    {
                        return true;
                    }

                    _ = atlas.[< set _$xy _partition >](value);
                    prev
                };

                let result = self.check_sprite_vxs(drawing_resources, grid, brush_center).is_ok();
                _ = self.animation
                    .get_atlas_animation_mut()
                    .[< set _$xy _partition >](prev);
                result
            }

            #[inline]
            #[must_use]
            pub(in crate::map) fn [< set_atlas_animation_ $xy _partition >](
                &mut self,
                value: u32,
            ) -> Option<u32>
            {
                self.animation.get_atlas_animation_mut().[< set _$xy _partition >](value)
            }
        )+}};
    }

    //=======================================================================//

    macro_rules! set_sprite_values {
        ($($value:ident),+) => { paste::paste!{ $(
            #[inline]
            fn [< set_ $value >](&mut self, value: f32) -> Option<f32>
            {
                let $value = match_or_panic!(self, Self::False { $value, .. }, $value);

                if value.around_equal_narrow($value)
                {
                    return None;
                }

                $value.replace_value(value).into()
            }
        )+}};
    }

    //=======================================================================//

    macro_rules! swap {
        ($self:ident, $source:ident, ($($value:ident),+)) => { $(
            $self.$value.swap_value(&mut $source.$value);
        )+};

        ($self:ident, ($($value:ident),+)) => { $(
            $self.$value.swap_value($value);
        )+};
    }

    //=======================================================================//
    // TRAITS
    //
    //=======================================================================//

    /// A trait for texture to return additional information.
    pub(in crate::map) trait TextureInterfaceExtra
    {
        /// Returns the associated animation.
        fn overall_animation<'a>(
            &'a self,
            drawing_resources: &'a DrawingResources
        ) -> &'a Animation;

        #[must_use]
        fn sprite_hull<T: TextureSize>(
            &self,
            resources: &T,
            grid: &Grid,
            brush_center: Vec2
        ) -> Option<Hull>;

        #[must_use]
        fn sprite_vxs(
            &self,
            drawing_resources: &DrawingResources,
            grid: &Grid,
            brush_center: Vec2
        ) -> Option<[Vec2; 4]>;

        #[must_use]
        fn animated_sprite_vxs(
            &self,
            drawing_resources: &DrawingResources,
            grid: &Grid,
            animator: Option<&Animator>,
            brush_center: Vec2
        ) -> Option<[Vec2; 4]>;

        #[must_use]
        fn sprite_pivot(&self, brush_center: Vec2) -> Option<Vec2>;
    }

    //=======================================================================//
    // ENUMS
    //
    //=======================================================================//

    impl Default for Sprite
    {
        #[inline]
        fn default() -> Self
        {
            Self::False {
                parallax_x:       0f32,
                parallax_y:       0f32,
                scroll_x:         0f32,
                scroll_y:         0f32,
                offset_auxiliary: OffsetAuxiliary::default()
            }
        }
    }

    impl From<bool> for Sprite
    {
        #[inline]
        fn from(value: bool) -> Self
        {
            if value
            {
                return Sprite::True;
            }

            Sprite::False {
                parallax_x:       0f32,
                parallax_y:       0f32,
                scroll_x:         0f32,
                scroll_y:         0f32,
                offset_auxiliary: OffsetAuxiliary::default()
            }
        }
    }

    impl Sprite
    {
        set_sprite_values!(parallax_x, parallax_y, scroll_x, scroll_y);

        #[inline]
        fn rotation_data(
            &self,
            texture_angle: f32,
            rotation_angle: f32,
            pivot: Vec2
        ) -> RotationOffset
        {
            match_or_panic!(
                self,
                Self::False {
                    offset_auxiliary,
                    ..
                },
                offset_auxiliary
            )
            .rotation_data(texture_angle, rotation_angle, pivot)
        }
    }

    //=======================================================================//

    #[derive(Clone, Copy)]
    enum RotationAuxiliary
    {
        Texture(RotationOffset),
        Sprite(f32, f32)
    }

    // //=======================================================================//

    #[derive(Clone, Copy)]
    enum ScaleOffset
    {
        Texture(f32, f32),
        Sprite(f32, f32)
    }

    //=======================================================================//
    // STRUCTS
    //
    //=======================================================================//

    /// The outcome of a valid texture rotation.
    #[must_use]
    #[derive(Clone, Copy)]
    pub(in crate::map) struct TextureRotation
    {
        auxiliary: RotationAuxiliary,
        angle:     f32
    }

    //=======================================================================//

    /// The outcome of a valid texture scale.
    #[must_use]
    pub(in crate::map) struct TextureScale
    {
        offset:  ScaleOffset,
        scale_x: f32,
        scale_y: f32
    }

    //=======================================================================//

    /// The outcome of a valid texture scale.
    #[must_use]
    pub(in crate::map) struct TextureSpriteSet
    {
        sprite:   Sprite,
        offset_x: f32,
        offset_y: f32
    }

    impl TextureSpriteSet
    {
        #[inline]
        #[must_use]
        pub const fn enabled(&self) -> bool { self.sprite.enabled() }
    }

    //=======================================================================//

    #[must_use]
    pub(in crate::map) struct TextureReset
    {
        scale_x:  f32,
        scale_y:  f32,
        offset_x: f32,
        offset_y: f32,
        angle:    f32,
        sprite:   Sprite
    }

    //=======================================================================//
    // STRUCTS
    //
    //=======================================================================//

    impl OffsetAuxiliary
    {
        #[inline]
        fn rotation_data(
            &self,
            texture_angle: f32,
            rotation_angle: f32,
            pivot: Vec2
        ) -> RotationOffset
        {
            let mut copy = self.rotation;

            let pivot = match copy.last_pivot
            {
                Some(last_pivot) =>
                {
                    if !copy.last_pivot.unwrap().around_equal_narrow(&pivot)
                    {
                        let delta = pivot - last_pivot;
                        copy.pivot_mod += -delta + rotate_point_around_origin(delta, texture_angle);
                        copy.last_pivot = pivot.into();
                    }

                    pivot + copy.pivot_mod
                },
                None =>
                {
                    copy.pivot_mod = Vec2::ZERO;
                    copy.last_pivot = pivot.into();
                    pivot
                }
            };

            [copy.offset_x, copy.offset_y] = rotate_point(
                Vec2::new(copy.offset_x, copy.offset_y),
                pivot.with_y(-pivot.y),
                rotation_angle
            )
            .to_array();

            copy
        }
    }

    //=======================================================================//

    /// A texture which can be rendered on screen and its metadata.
    #[allow(clippy::missing_docs_in_private_items)]
    #[must_use]
    pub(in crate::map) struct Texture
    {
        name:      String,
        size:      UVec2,
        label:     String,
        size_str:  String,
        repeat:    Handle<Image>,
        clamp:     Handle<Image>,
        animation: Animation,
        hull:      Hull,
        dirty:     bool
    }

    impl Clone for Texture
    {
        #[inline]
        fn clone(&self) -> Self
        {
            Self {
                name:      self.name.clone(),
                size:      self.size,
                label:     self.label.clone(),
                size_str:  self.size_str.clone(),
                repeat:    self.repeat.clone_weak(),
                clamp:     self.clamp.clone_weak(),
                animation: self.animation.clone(),
                dirty:     false,
                hull:      self.hull
            }
        }
    }

    impl Placeholder for Texture
    {
        #[inline]
        unsafe fn placeholder() -> Self
        {
            Self {
                name:      String::new(),
                size:      UVec2::new(1, 1),
                label:     String::new(),
                size_str:  String::new(),
                hull:      Hull::new(1f32, 0f32, 0f32, 1f32).unwrap(),
                repeat:    Handle::default(),
                clamp:     Handle::default(),
                animation: Animation::default(),
                dirty:     false
            }
        }
    }

    impl Texture
    {
        /// Returns a [`String`] that features both `name` and `size`.
        #[inline]
        #[must_use]
        fn format_label(name: &str, size: UVec2) -> String
        {
            format!("{} {}", name, Self::format_size(size))
        }

        /// Returns a [`String`] containing the formatted `size`.
        #[inline]
        #[must_use]
        fn format_size(size: UVec2) -> String { format!("({}x{})", size.x, size.y) }

        /// Returns the [`Hull`] describing `size`.
        #[allow(clippy::cast_precision_loss)]
        #[inline]
        fn create_hull(size: UVec2) -> Hull
        {
            let half_width = (size.x / 2) as f32;
            let half_height = (size.y / 2) as f32;
            Hull::new(half_height, -half_height, -half_width, half_width).unwrap()
        }

        /// Returns a new [`Texture`].
        #[inline]
        pub fn new(name: impl Into<String>, image: Image, images: &mut Assets<Image>) -> Self
        {
            let name = Into::<String>::into(name);
            let size = image.size();
            let size_str = Self::format_size(size);
            let label = Self::format_label(&name, size);

            let mut clamp = image.clone();
            clamp.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::default());

            Self {
                name,
                size,
                label,
                size_str,
                repeat: images.add(image),
                clamp: images.add(clamp),
                animation: Animation::None,
                hull: Self::create_hull(size),
                dirty: false
            }
        }

        /// Returns a new [`Texture`] from the arguments.
        #[inline]
        pub fn from_parts(
            name: impl Into<String>,
            size: UVec2,
            handle: Handle<Image>,
            clamp: Handle<Image>
        ) -> Self
        {
            let name = Into::<String>::into(name);
            let label = Self::format_label(&name, size);
            let size_str = Self::format_size(size);

            Self {
                name,
                size,
                label,
                size_str,
                repeat: handle,
                clamp,
                animation: Animation::None,
                hull: Self::create_hull(size),
                dirty: false
            }
        }

        /// The name of the texture.
        #[inline]
        #[must_use]
        pub fn name(&self) -> &str { &self.name }

        /// The UI label of the texture.
        #[inline]
        #[must_use]
        pub fn label(&self) -> &str { &self.label }

        /// The size of the texture.
        #[inline]
        #[must_use]
        pub const fn size(&self) -> UVec2 { self.size }

        /// A stringal representation of the size of the texture.
        #[inline]
        #[must_use]
        pub fn size_str(&self) -> &str { &self.size_str }

        /// The [`Handle<Image>`] of the texture.
        #[inline]
        #[must_use]
        pub fn repeat_handle(&self) -> Handle<Image> { self.repeat.clone_weak() }

        /// The [`Handle<Image>`] of the clamped texture.
        #[inline]
        #[must_use]
        pub fn clamp_handle(&self) -> Handle<Image> { self.clamp.clone_weak() }

        /// Returns a reference to the texture's [`Animation`].
        #[inline]
        pub const fn animation(&self) -> &Animation { &self.animation }

        /// Returns a mutable reference to the texture [`Animation`].
        #[inline]
        pub fn animation_mut(&mut self) -> &mut Animation { &mut self.animation }

        /// Returns a mutable reference to the texture [`Animation`] and marks it as changed.
        #[inline]
        pub fn animation_mut_set_dirty(&mut self) -> &mut Animation
        {
            self.dirty = true;
            &mut self.animation
        }

        #[inline]
        pub const fn hull(&self) -> Hull { self.hull }

        /// Whether the texture was edited.
        #[inline]
        #[must_use]
        pub const fn dirty(&self) -> bool { self.dirty }

        /// Clears the texture dirty flag.
        #[inline]
        pub fn clear_dirty_flag(&mut self) { self.dirty = false; }
    }

    //=======================================================================//

    impl From<&Texture> for TextureSettings
    {
        #[inline]
        fn from(value: &Texture) -> Self
        {
            Self {
                texture:   value.name.clone(),
                scale_x:   1f32,
                scale_y:   1f32,
                offset_x:  0f32,
                offset_y:  0f32,
                angle:     0f32,
                height:    0,
                sprite:    Sprite::default(),
                animation: Animation::None
            }
        }
    }

    impl TextureInterfaceExtra for TextureSettings
    {
        #[inline]
        fn overall_animation<'a>(&'a self, drawing_resources: &'a DrawingResources)
            -> &'a Animation
        {
            if !self.animation.is_none()
            {
                return &self.animation;
            }

            &drawing_resources.texture_or_error(&self.texture).animation
        }

        #[inline]
        fn sprite_hull<T: TextureSize>(
            &self,
            resources: &T,
            grid: &Grid,
            brush_center: Vec2
        ) -> Option<Hull>
        {
            self.texture_sprite_vxs(resources, grid, &self.texture, brush_center)
                .map(Hull::from_points)
        }

        #[inline]
        fn sprite_vxs(
            &self,
            drawing_resources: &DrawingResources,
            grid: &Grid,
            brush_center: Vec2
        ) -> Option<[Vec2; 4]>
        {
            self.texture_sprite_vxs(drawing_resources, grid, &self.texture, brush_center)
        }

        #[inline]
        fn animated_sprite_vxs(
            &self,
            drawing_resources: &DrawingResources,
            grid: &Grid,
            animator: Option<&Animator>,
            brush_center: Vec2
        ) -> Option<[Vec2; 4]>
        {
            match animator
            {
                Some(animator) =>
                {
                    let texture = match animator
                    {
                        Animator::List(list_animator) =>
                        {
                            list_animator
                                .texture(
                                    drawing_resources,
                                    self.overall_animation(drawing_resources).get_list_animation()
                                )
                                .texture()
                                .name()
                        },
                        Animator::Atlas(_) => &self.texture
                    };

                    self.texture_sprite_vxs(drawing_resources, grid, texture, brush_center)
                },
                None => self.sprite_vxs(drawing_resources, grid, brush_center)
            }
        }

        #[inline]
        fn sprite_pivot(&self, brush_center: Vec2) -> Option<Vec2>
        {
            self.sprite.enabled().then(|| brush_center + self.draw_offset())
        }
    }

    impl TextureSettings
    {
        xy!(x, y);

        #[inline]
        pub(in crate::map::drawer) const fn sprite_struct(&self) -> &Sprite { &self.sprite }

        /// Returns the maximum possible frames of the atlas animation.
        #[inline]
        #[must_use]
        pub(in crate::map) const fn atlas_animation_max_len(&self) -> usize
        {
            self.animation.get_atlas_animation().max_len()
        }

        /// Returns a reference to the list animation frame at `index`.
        #[inline]
        #[must_use]
        pub(in crate::map) fn list_animation_frame(&self, index: usize) -> &(String, f32)
        {
            self.animation.get_list_animation().frame(index)
        }

        #[inline]
        #[must_use]
        pub(in crate::map) fn check_move(
            &self,
            drawing_resources: &DrawingResources,
            grid: &Grid,
            new_center: Vec2
        ) -> bool
        {
            self.sprite_vxs(drawing_resources, grid, new_center)
                .map_or(false, |hull| {
                    !hull.into_iter().any(|vx| grid.point_projection(vx).out_of_bounds())
                })
        }

        /// Checks whether the scale and flipping of the texture is valid. Returns a
        /// [`TextureScale`] describing the outcome if it is.
        #[inline]
        pub(in crate::map) fn check_scale(
            &mut self,
            drawing_resources: &DrawingResources,
            grid: &Grid,
            info: &ScaleInfo,
            old_brush_center: Vec2,
            new_center: Vec2
        ) -> Option<TextureScale>
        {
            let (scale_x, scale_y) = info.texture_scale(self.scale_x, self.scale_y);
            let sprite_pivot = match self.sprite_pivot(old_brush_center)
            {
                Some(pivot) => pivot,
                None =>
                {
                    let offset = Vec2::new(
                        self.sprite.auxiliary_offset_x(),
                        self.sprite.auxiliary_offset_y()
                    ) + Vec2::new(self.offset_x, -self.offset_y);
                    let offset = offset.with_x(-offset.x);
                    let offset = info.scaled_point(offset) - offset;

                    return TextureScale {
                        offset: ScaleOffset::Texture(-offset.x, offset.y),
                        scale_x,
                        scale_y
                    }
                    .into();
                }
            };

            let new_offset = info.scaled_point(sprite_pivot) - new_center;
            let prev_offset_x = self.offset_x.replace_value(new_offset.x);
            let prev_offset_y = self.offset_y.replace_value(new_offset.y);
            let prev_scale_x = self.scale_x.replace_value(scale_x);
            let prev_scale_y = self.scale_y.replace_value(scale_y);

            let result = self.check_sprite_vxs(drawing_resources, grid, new_center);

            self.offset_x = prev_offset_x;
            self.offset_y = prev_offset_y;
            self.scale_x = prev_scale_x;
            self.scale_y = prev_scale_y;

            match result
            {
                Ok(_) =>
                {
                    TextureScale {
                        offset: ScaleOffset::Sprite(new_offset.x, new_offset.y),
                        scale_x,
                        scale_y
                    }
                    .into()
                },
                Err(()) => None
            }
        }

        #[inline]
        pub(in crate::map) fn scale(&mut self, value: &mut TextureScale)
        {
            swap!(self, value, (scale_x, scale_y));

            match (&mut self.sprite, &mut value.offset)
            {
                (Sprite::True, ScaleOffset::Sprite(offset_x, offset_y)) =>
                {
                    swap!(self, (offset_x, offset_y));
                },
                (
                    Sprite::False {
                        offset_auxiliary, ..
                    },
                    ScaleOffset::Texture(x, y)
                ) =>
                {
                    offset_auxiliary.scale.offset_x += *x;
                    offset_auxiliary.scale.offset_y += *y;

                    *x = -*x;
                    *y = -*y;
                },
                _ => unreachable!()
            }
        }

        /// Whether the texture change is valid.
        #[inline]
        pub(in crate::map) fn check_texture_change(
            &mut self,
            drawing_resources: &DrawingResources,
            grid: &Grid,
            texture: &str,
            brush_center: Vec2
        ) -> bool
        {
            if !self.sprite.enabled()
            {
                return true;
            }

            let prev = self.texture.replace_value(texture.to_owned());
            let result = self.check_sprite_vxs(drawing_resources, grid, brush_center).is_ok();
            self.texture = prev;
            result
        }

        /// Sets the texture, returns the previous value if different.
        #[inline]
        pub(in crate::map) fn set_texture(
            &mut self,
            drawing_resources: &DrawingResources,
            texture: &str
        ) -> Option<String>
        {
            if self.texture == texture
            {
                return None;
            }

            let prev = std::mem::replace(
                &mut self.texture,
                drawing_resources.texture_or_error(texture).name.clone()
            );
            prev.into()
        }

        /// Moves the offset.
        #[inline]
        pub(in crate::map) fn move_offset(&mut self, mut value: Vec2)
        {
            if !self.sprite()
            {
                value = rotate_point_around_origin(-value, self.angle.to_radians());
            }

            self.offset_x += value.x;
            self.offset_y += value.y;
        }

        /// Sets the draw height, returns the previous value if different.
        #[inline]
        #[must_use]
        pub(in crate::map) fn set_height(&mut self, value: i8) -> Option<i8>
        {
            assert!(TEXTURE_HEIGHT_RANGE.contains(&value), "Invalid height value.");

            if value == self.height
            {
                return None;
            }

            self.height.replace_value(value).into()
        }

        /// Whether the new angle is valid.
        #[inline]
        pub(in crate::map) fn check_angle(
            &mut self,
            drawing_resources: &DrawingResources,
            grid: &Grid,
            value: f32,
            brush_center: Vec2
        ) -> bool
        {
            value.assert_normalized_degrees_angle();

            if !self.sprite.enabled() || value.around_equal_narrow(&self.angle)
            {
                return true;
            }

            let prev = self.angle.replace_value(value);
            let result = self.check_sprite_vxs(drawing_resources, grid, brush_center).is_ok();
            self.angle = prev;
            result
        }

        /// Checks whether the rotation is valid. If valid returns a [`TextureRotation`] describing
        /// the outcome, if valid.
        #[inline]
        pub(in crate::map) fn check_rotation(
            &mut self,
            drawing_resources: &DrawingResources,
            grid: &Grid,
            pivot: Vec2,
            angle: f32,
            old_brush_center: Vec2,
            new_center: Vec2
        ) -> Option<TextureRotation>
        {
            angle.assert_normalized_degrees_angle();

            let end_angle = (self.angle - angle).rem_euclid(360f32);
            let angle = angle.to_radians();
            let sprite_pivot = match self.sprite_pivot(old_brush_center)
            {
                Some(pivot) => pivot,
                None =>
                {
                    return TextureRotation {
                        auxiliary: RotationAuxiliary::Texture(self.sprite.rotation_data(
                            self.angle.to_radians(),
                            angle,
                            pivot
                        )),
                        angle:     end_angle
                    }
                    .into();
                }
            };

            let [x, y] = (rotate_point(sprite_pivot, pivot, angle) - new_center).to_array();
            let prev_offset_x = self.offset_x.replace_value(x);
            let prev_offset_y = self.offset_y.replace_value(y);
            let prev_angle = self.angle.replace_value(end_angle);

            let result = self.check_sprite_vxs(drawing_resources, grid, new_center);

            self.offset_x = prev_offset_x;
            self.offset_y = prev_offset_y;
            self.angle = prev_angle;

            match result
            {
                Ok(_) =>
                {
                    TextureRotation {
                        auxiliary: RotationAuxiliary::Sprite(x, y),
                        angle:     end_angle
                    }
                    .into()
                },
                Err(()) => None
            }
        }

        #[inline]
        pub(in crate::map) fn rotate(&mut self, payload: &mut TextureRotation)
        {
            swap!(self, payload, (angle));

            match (&mut self.sprite, &mut payload.auxiliary)
            {
                (Sprite::True, RotationAuxiliary::Sprite(offset_x, offset_y)) =>
                {
                    swap!(self, (offset_x, offset_y));
                },
                (
                    Sprite::False {
                        offset_auxiliary, ..
                    },
                    RotationAuxiliary::Texture(auxiliary)
                ) => offset_auxiliary.rotation.swap_value(auxiliary),
                _ => unreachable!()
            };
        }

        /// Sets the angle, returns the previous value if different.
        #[inline]
        #[must_use]
        pub(in crate::map) fn set_angle(
            &mut self,
            drawing_resources: &DrawingResources,
            grid: &Grid,
            value: f32,
            brush_center: Vec2
        ) -> Option<TextureRotation>
        {
            value.assert_normalized_degrees_angle();

            if value.around_equal_narrow(&self.angle)
            {
                return None;
            }

            if self.sprite.enabled()
            {
                self.angle = value;

                return TextureRotation {
                    auxiliary: RotationAuxiliary::Sprite(self.offset_x, self.offset_y),
                    angle:     value
                }
                .into();
            }

            let pivot = match_or_panic!(
                &self.sprite,
                Sprite::False {
                    offset_auxiliary,
                    ..
                },
                offset_auxiliary
            )
            .rotation
            .last_pivot
            .unwrap_or_default();

            let mut rotation = self
                .check_rotation(
                    drawing_resources,
                    grid,
                    pivot,
                    (self.angle - value).rem_euclid(360f32),
                    brush_center,
                    brush_center
                )
                .unwrap();
            self.rotate(&mut rotation);

            rotation.into()
        }

        /// Whether the sprite value change is valid.
        #[inline]
        #[must_use]
        pub(in crate::map) fn check_sprite(
            &mut self,
            drawing_resources: &DrawingResources,
            grid: &Grid,
            value: bool,
            brush_center: Vec2
        ) -> bool
        {
            if !value || self.sprite.enabled()
            {
                return true;
            }

            let prev_offset_x = self.offset_x.replace_value(0f32);
            let prev_offset_y = self.offset_y.replace_value(0f32);
            let new = Sprite::from(true);

            let prev_sprite = self.sprite.replace_value(new);
            let result = self.check_sprite_vxs(drawing_resources, grid, brush_center).is_ok();

            self.offset_x = prev_offset_x;
            self.offset_y = prev_offset_y;
            self.sprite = prev_sprite;

            result
        }

        /// Sets the texture sprite setting to `value`. If sprite is enabled the offsets are set to
        /// zero. Returns the previous [`Sprite`] and offset values if different.
        #[inline]
        #[must_use]
        pub(in crate::map) fn set_sprite(&mut self, value: bool) -> Option<TextureSpriteSet>
        {
            if value == self.sprite.enabled()
            {
                return None;
            }

            if value
            {
                self.offset_x = 0f32;
                self.offset_y = 0f32;
            }

            let prev = self.sprite.replace_value(Sprite::from(value));
            let offset_x = self.offset_x;
            let offset_y = self.offset_y;

            TextureSpriteSet {
                sprite: prev,
                offset_x,
                offset_y
            }
            .into()
        }

        #[inline]
        pub(in crate::map) fn undo_redo_sprite(&mut self, value: &mut TextureSpriteSet)
        {
            self.sprite.swap_value(&mut value.sprite);
            self.offset_x.swap_value(&mut value.offset_x);
            self.offset_y.swap_value(&mut value.offset_y);
        }

        /// Checks whether the texture is within bounds.
        #[inline]
        #[must_use]
        pub(in crate::map) fn check_within_bounds(
            &self,
            drawing_resources: &DrawingResources,
            grid: &Grid,
            brush_center: Vec2
        ) -> bool
        {
            self.check_sprite_vxs(drawing_resources, grid, brush_center).is_ok()
        }

        /// Checks whether changing animation makes the sprite, if any, go out of bounds.
        #[inline]
        #[must_use]
        pub(in crate::map) fn check_animation_change(
            &mut self,
            drawing_resources: &DrawingResources,
            grid: &Grid,
            animation: &Animation,
            brush_center: Vec2
        ) -> bool
        {
            if !self.sprite.enabled()
            {
                return true;
            }

            let prev = self.animation.replace_value(animation.clone());
            let result = self.check_sprite_vxs(drawing_resources, grid, brush_center).is_ok();
            self.animation = prev;
            result
        }

        /// Sets the [`Animation`].
        #[inline]
        pub(in crate::map) fn set_animation(&mut self, animation: Animation) -> Animation
        {
            self.animation.replace_value(animation)
        }

        /// Sets the [`Animation`] to list, using `texture` for the first frame, and returns the
        /// previous one.
        #[inline]
        pub(in crate::map) fn set_list_animation(&mut self, texture: &str) -> Animation
        {
            self.animation.replace_value(Animation::list_animation(texture))
        }

        /// Sets the [`Animation`] to list and returns the previous one.
        #[inline]
        pub(in crate::map) fn generate_list_animation(&mut self) -> Animation
        {
            self.animation.replace_value(Animation::list_animation(&self.texture))
        }

        /// Sets the amount of frames of the atlas animation. Returns the previous value, if
        /// different,
        #[inline]
        #[must_use]
        pub(in crate::map) fn set_atlas_animation_len(&mut self, len: usize) -> Option<usize>
        {
            self.animation.get_atlas_animation_mut().set_len(len)
        }

        /// Sets the [`Timing`] of the atlas animation and returns the previous one.
        #[inline]
        pub(in crate::map) fn set_atlas_animation_timing(&mut self, timing: Timing) -> Timing
        {
            self.animation.get_atlas_animation_mut().set_timing(timing)
        }

        /// Sets the [`Timing`] of the atlas animation to uniform and returns the previous value if
        /// different.
        #[inline]
        pub(in crate::map) fn set_atlas_animation_uniform_timing(&mut self) -> Option<Timing>
        {
            self.animation.get_atlas_animation_mut().set_uniform()
        }

        /// Sets the [`Timing`] of the atlas animation to per frame and returns the previous value
        /// if different.
        #[inline]
        pub(in crate::map) fn set_atlas_animation_per_frame_timing(&mut self) -> Option<Timing>
        {
            self.animation.get_atlas_animation_mut().set_per_frame()
        }

        /// Sets the uniform frame time of the atlas animation and returns the previous value if
        /// different.
        #[inline]
        #[must_use]
        pub(in crate::map) fn set_atlas_animation_uniform_time(&mut self, value: f32)
            -> Option<f32>
        {
            self.animation.get_atlas_animation_mut().set_uniform_time(value)
        }

        /// Sets the frame time of atlas animation frame at `index` and returns the previous value
        /// if different.
        #[inline]
        #[must_use]
        pub(in crate::map) fn set_atlas_animation_frame_time(
            &mut self,
            index: usize,
            value: f32
        ) -> Option<f32>
        {
            self.animation.get_atlas_animation_mut().set_frame_time(index, value)
        }

        /// Moves up the frame time of the atlas animation at `index`.
        #[inline]
        pub(in crate::map) fn move_up_atlas_animation_frame_time(&mut self, index: usize)
        {
            self.animation.get_atlas_animation_mut().move_up(index);
        }

        /// Moves down the frame time of the atlas animation at `index`.
        #[inline]
        pub(in crate::map) fn move_down_atlas_animation_frame_time(&mut self, index: usize)
        {
            self.animation.get_atlas_animation_mut().move_down(index);
        }

        /// Sets the texture of the list animation frame at `index`, and returns the previous value
        /// if different.
        #[inline]
        #[must_use]
        pub(in crate::map) fn set_list_animation_texture(
            &mut self,
            index: usize,
            texture: &str
        ) -> Option<String>
        {
            self.animation.get_list_animation_mut().set_texture(index, texture)
        }

        /// Sets the time of the list animation frame at `index` and returns the previous value if
        /// different.
        #[inline]
        #[must_use]
        pub(in crate::map) fn set_list_animation_time(
            &mut self,
            index: usize,
            time: f32
        ) -> Option<f32>
        {
            self.animation.get_list_animation_mut().set_time(index, time)
        }

        /// Moves up the frame of the list animation at `index`.
        #[inline]
        pub(in crate::map) fn move_up_list_animation_frame(&mut self, index: usize)
        {
            self.animation.get_list_animation_mut().move_up(index);
        }

        /// Moves down the frame of the list animation at `index`.
        #[inline]
        pub(in crate::map) fn move_down_list_animation_frame(&mut self, index: usize)
        {
            self.animation.get_list_animation_mut().move_down(index);
        }

        /// Inserts a new list animation frame at `index`.
        #[inline]
        pub(in crate::map) fn insert_list_animation_frame(
            &mut self,
            index: usize,
            texture: &str,
            time: f32
        )
        {
            self.animation.get_list_animation_mut().insert(index, texture, time);
        }

        /// Removes the last frame of the list animation.
        #[inline]
        pub(in crate::map) fn pop_list_animation_frame(&mut self)
        {
            self.animation.get_list_animation_mut().pop();
        }

        /// Removes the frame at `index` from the list animation.
        #[inline]
        pub(in crate::map) fn remove_list_animation_frame(&mut self, index: usize)
        {
            self.animation.get_list_animation_mut().remove(index);
        }

        /// Pushes a new frame onto the list animation.
        #[inline]
        pub(in crate::map) fn push_list_animation_frame(&mut self, texture: &str)
        {
            self.animation.get_list_animation_mut().push(texture);
        }

        #[inline]
        pub(in crate::map) fn reset(&mut self) -> TextureReset
        {
            TextureReset {
                scale_x:  self.scale_x.replace_value(1f32),
                scale_y:  self.scale_y.replace_value(1f32),
                offset_x: self.offset_x.take_value(),
                offset_y: self.offset_y.take_value(),
                angle:    self.angle.take_value(),
                sprite:   self.sprite.take_value()
            }
        }

        #[inline]
        pub(in crate::map) fn undo_redo_reset(&mut self, value: &mut TextureReset)
        {
            swap!(self, value, (scale_x, scale_y, offset_x, offset_y, angle, sprite));
        }

        #[inline]
        fn texture_sprite_vxs_at_origin_no_offset<T: TextureSize>(
            &self,
            resources: &T,
            grid: &Grid,
            texture: &str
        ) -> Option<[Vec2; 4]>
        {
            if !self.sprite.enabled()
            {
                return None;
            }

            let size = resources.texture_size(texture, self).as_vec2() *
                Vec2::new(self.scale_x.abs(), self.scale_y.abs()) /
                2f32;
            let mut rect = Hull::new(size.y, -size.y, -size.x, size.x).unwrap().rectangle();
            let angle = -self.angle.to_radians();

            if angle != 0f32
            {
                for vx in &mut rect
                {
                    *vx = rotate_point_around_origin(*vx, angle);
                }
            }

            if grid.isometric()
            {
                rect.translate(Vec2::new(
                    0f32,
                    Hull::from_opposite_vertexes(rect[0], rect[2]).half_height()
                ));
            }

            rect.into()
        }

        #[inline]
        fn texture_sprite_vxs_at_origin<T: TextureSize>(
            &self,
            resources: &T,
            grid: &Grid,
            texture: &str
        ) -> Option<[Vec2; 4]>
        {
            self.texture_sprite_vxs_at_origin_no_offset(resources, grid, texture)
                .map(|mut rect| {
                    let offset = self.draw_offset();
                    rect.translate(grid.transform_point(offset));
                    rect
                })
        }

        #[inline]
        fn texture_sprite_vxs<T: TextureSize>(
            &self,
            resources: &T,
            grid: &Grid,
            texture: &str,
            brush_center: Vec2
        ) -> Option<[Vec2; 4]>
        {
            let mut vxs = self.texture_sprite_vxs_at_origin(resources, grid, texture);

            if let Some(vxs) = &mut vxs
            {
                vxs.translate(grid.transform_point(brush_center));
            }

            vxs
        }

        #[inline]
        #[must_use]
        pub(in crate::map) fn sprite_pivot(&self, brush_center: Vec2) -> Option<Vec2>
        {
            self.sprite.enabled().then(|| brush_center + self.draw_offset())
        }

        /// Checks whether the sprite, if any, fits within the map.
        #[inline]
        fn check_sprite_vxs(
            &self,
            drawing_resources: &DrawingResources,
            grid: &Grid,
            brush_center: Vec2
        ) -> Result<Option<[Vec2; 4]>, ()>
        {
            match self.texture_sprite_vxs(drawing_resources, grid, &self.texture, brush_center)
            {
                Some(rect) =>
                {
                    if rect.iter().any(OutOfBounds::out_of_bounds)
                    {
                        Result::Err(())
                    }
                    else
                    {
                        Result::Ok(rect.into())
                    }
                },
                None => Result::Ok(None)
            }
        }
    }
}

#[cfg(feature = "ui")]
pub(in crate::map) use ui_mod::*;
