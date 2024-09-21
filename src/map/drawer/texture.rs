//=======================================================================//
// IMPORTS
//
//=======================================================================//

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::Animation;

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
                Sprite::True { .. }=> 0f32,
                Sprite::False { $value, .. } => *$value
            }
        }
    )+};

    ($(($t:ty, $value:ident)),+) => { paste::paste! {$(
        #[inline]
        #[must_use]
        pub fn [< rotation_ $value >](&self) -> $t
        {
            match self
            {
                Sprite::True { .. }=> <$t>::default(),
                Sprite::False { rotation_auxiliary, .. } => rotation_auxiliary.$value
            }
        }
    )+}};
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

    /// Returns the horizontal offset necessary to draw the texture as displayed in the editor.
    /// It may be different from the value returned by `TextureInterface::offset_x`.
    #[must_use]
    fn draw_offset_x(&self) -> f32;

    /// Returns the vertical offset necessary to draw the texture as displayed in the editor.
    /// It may be different from the value returned by `TextureInterface::offset_y`.
    #[must_use]
    fn draw_offset_y(&self) -> f32;

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

    /// The horizontal scroll value based on the elapsed time.
    #[must_use]
    fn draw_scroll_x(&self, elapsed_time: f32) -> f32;

    /// The vertical scroll value based on the elapsed time.
    #[must_use]
    fn draw_scroll_y(&self, elapsed_time: f32) -> f32;

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
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub(in crate::map::drawer) enum Sprite
{
    /// Yes.
    True,
    /// No.
    False
    {
        /// The horizontal parallax of the texture.
        parallax_x:         f32,
        /// The vertical parallax of the texture.
        parallax_y:         f32,
        /// The horizontal scrolling of the texture.
        scroll_x:           f32,
        /// The vertical scrolling of the texture.
        scroll_y:           f32,
        rotation_auxiliary: RotationAuxiliary
    }
}

impl Default for Sprite
{
    #[inline]
    fn default() -> Self
    {
        Self::False {
            parallax_x:         0f32,
            parallax_y:         0f32,
            scroll_x:           0f32,
            scroll_y:           0f32,
            rotation_auxiliary: RotationAuxiliary::default()
        }
    }
}

impl Sprite
{
    sprite_values!(parallax_x, parallax_y, scroll_x, scroll_y);

    sprite_values!((f32, offset_x), (f32, offset_y));

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
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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
    fn draw_offset_x(&self) -> f32 { self.offset_x + self.sprite.rotation_offset_x() }

    #[inline]
    fn draw_offset_y(&self) -> f32 { self.offset_y - self.sprite.rotation_offset_y() }

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
    fn draw_scroll_x(&self, elapsed_time: f32) -> f32 { self.scroll_x() * elapsed_time }

    #[inline]
    fn draw_scroll_y(&self, elapsed_time: f32) -> f32 { self.scroll_y() * elapsed_time }

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
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub(in crate::map::drawer) struct RotationAuxiliary
{
    offset_x:   f32,
    offset_y:   f32,
    pivot_mod:  Vec2,
    last_pivot: Option<Vec2>
}

//=======================================================================//

/// The animation associated to a texture.
#[must_use]
#[derive(Debug, Serialize, Deserialize)]
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

    use arrayvec::ArrayVec;
    use bevy::{
        asset::{Assets, Handle},
        render::texture::{Image, ImageSampler, ImageSamplerDescriptor}
    };
    use glam::{UVec2, Vec2};
    use hill_vacuum_shared::{
        match_or_panic,
        return_if_no_match,
        return_if_none,
        TEXTURE_HEIGHT_RANGE
    };

    use super::{RotationAuxiliary, Sprite};
    use crate::{
        map::{
            brush::convex_polygon::ScaleInfo,
            drawer::{animation::MoveUpDown, drawing_resources::DrawingResources},
            editor::Placeholder,
            AssertNormalizedDegreesAngle,
            OutOfBounds,
            Translate
        },
        utils::{
            hull::{EntityHull, Flip, Hull},
            math::{
                points::{rotate_point, rotate_point_around_origin},
                AroundEqual
            }
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
            pub(in crate::map) fn [< check_offset_ $xy >](&mut self, drawing_resources: &DrawingResources, value: f32, center: Vec2) -> bool
            {
                if !self.sprite.enabled() || value.around_equal_narrow(&self.[< offset_ $xy >])
                {
                    return true;
                }

                let prev = std::mem::replace(&mut self.[< offset_ $xy >], value);
                let result = self.check_sprite_vxs(drawing_resources, center);
                self.[< offset_ $xy >] = prev;

                result.is_ok()
            }

            #[inline]
            #[must_use]
            pub(in crate::map) fn [< set_offset_ $xy >](&mut self, value: f32) -> Option<f32>
            {
                if value.around_equal_narrow(&self.[< offset_ $xy >])
                {
                    return None;
                }

                let prev = std::mem::replace(&mut self.[< offset_ $xy >], value);
                prev.into()
            }

            #[inline]
            pub(in crate::map) fn [< check_scale_ $xy >](&mut self, drawing_resources: &DrawingResources, value: f32, center: Vec2) -> bool
            {
                assert!(value != 0f32, "Scale is 0.");

                if !self.sprite.enabled() || value.around_equal_narrow(&self.[< scale_ $xy >])
                {
                    return true;
                }

                let prev = std::mem::replace(&mut self.[< scale_ $xy >], value);
                let result = self.check_sprite_vxs(drawing_resources, center);
                self.[< scale_ $xy >] = prev;

                result.is_ok()
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

                let prev = std::mem::replace(&mut self.[< scale_ $xy >], value);
                prev.into()
            }

            #[inline]
            pub(in crate::map) fn [< check_ $xy _flip >](&mut self, drawing_resources: &DrawingResources, mirror: f32, old_center: Vec2, new_center: Vec2) -> bool
            {
                let sprite_center = return_if_none!(self.sprite_hull(drawing_resources, old_center), true).center();
                let [< new_offset_ $xy >] = mirror - sprite_center.$xy - new_center.$xy;
                let [< prev_offset_ $xy >] = std::mem::replace(&mut self.[< offset_ $xy >], [< new_offset_ $xy >]);
                let result = self.check_sprite_vxs(drawing_resources, new_center);

                self.[< offset_ $xy >] = [< prev_offset_ $xy >];

                result.is_ok()
            }

            #[inline]
            pub(in crate::map) fn [< $xy _flip >](&mut self, drawing_resources: &DrawingResources, mirror: f32, old_center: Vec2, new_center: Vec2)
            {
                self.[< scale_ $xy >] = -self.[< scale_ $xy >];
                let sprite_center = return_if_none!(self.sprite_hull(drawing_resources, old_center)).center();
                self.[< offset_ $xy >] = mirror - sprite_center.$xy - new_center.$xy;
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
                value: u32,
                center: Vec2
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

                let result = self.check_sprite_vxs(drawing_resources, center).is_ok();
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

                std::mem::replace($value, value).into()
            }
        )+}};
    }

    //=======================================================================//

    macro_rules! from_compat {
        ($($v:ident),+) => { $(
            impl From<crate::map::brush::compatibility::$v::TextureSettings> for TextureSettings
            {
                #[inline]
                fn from(value: crate::map::brush::compatibility::$v::TextureSettings) -> Self
                {
                    let sprite = if value.sprite()
                    {
                        Sprite::True
                    }
                    else
                    {
                        Sprite::False {
                            parallax_x: value.parallax_x(),
                            parallax_y: value.parallax_y(),
                            scroll_x:   value.scroll_x(),
                            scroll_y:   value.scroll_y(),
                            rotation_auxiliary: crate::map::drawer::texture::RotationAuxiliary::default()
                        }
                    };

                    Self {
                        texture: value.name().to_string(),
                        scale_x: value.scale_x(),
                        scale_y: value.scale_y(),
                        offset_x: value.offset_x(),
                        offset_y: value.offset_y(),
                        angle: value.angle(),
                        height: value.height(),
                        sprite,
                        animation: value.animation().clone()
                    }
                }
            }
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

        /// Returns the size the texture must be drawn.
        #[must_use]
        fn draw_size(&self, drawing_resources: &DrawingResources) -> UVec2;

        #[must_use]
        fn sprite_hull(&self, drawing_resources: &DrawingResources, center: Vec2) -> Option<Hull>;

        #[must_use]
        fn sprite_vxs(
            &self,
            drawing_resources: &DrawingResources,
            center: Vec2
        ) -> Option<[Vec2; 4]>;
    }

    //=======================================================================//
    // ENUMS
    //
    //=======================================================================//

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
                parallax_x:         0f32,
                parallax_y:         0f32,
                scroll_x:           0f32,
                scroll_y:           0f32,
                rotation_auxiliary: RotationAuxiliary::default()
            }
        }
    }

    impl Sprite
    {
        set_sprite_values!(parallax_x, parallax_y, scroll_x, scroll_y);

        #[inline]
        fn rotation_offset(&mut self, texture_angle: f32, rotation_angle: f32, pivot: Vec2)
            -> Vec2
        {
            match_or_panic!(
                self,
                Self::False {
                    rotation_auxiliary,
                    ..
                },
                rotation_auxiliary
            )
            .rotation_offset(texture_angle, rotation_angle, pivot)
        }
    }

    //=======================================================================//
    // STRUCTS
    //
    //=======================================================================//

    /// The outcome of a valid texture rotation.
    #[derive(Debug, Clone, Copy)]
    pub(in crate::map) struct TextureRotation
    {
        /// The new offset.
        offset: Vec2,
        /// The new angle.
        angle:  f32
    }

    //=======================================================================//

    /// The outcome of a valid texture scale.
    #[derive(Debug)]
    pub(in crate::map) struct TextureScale
    {
        /// The new horizontal offset.
        offset_x: f32,
        /// The new vertical offset.
        offset_y: f32,
        /// The new horizontal scale.
        scale_x:  f32,
        /// The new vertical scale.
        scale_y:  f32
    }

    //=======================================================================//

    /// The outcome of a valid texture scale.
    #[derive(Debug)]
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
    // STRUCTS
    //
    //=======================================================================//

    impl RotationAuxiliary
    {
        #[inline]
        #[must_use]
        fn rotation_offset(&mut self, texture_angle: f32, rotation_angle: f32, pivot: Vec2)
            -> Vec2
        {
            let pivot = match self.last_pivot
            {
                Some(last_pivot) =>
                {
                    if !self.last_pivot.unwrap().around_equal_narrow(&pivot)
                    {
                        let delta = pivot - last_pivot;
                        self.pivot_mod += -delta + rotate_point_around_origin(delta, texture_angle);
                        self.last_pivot = pivot.into();
                    }

                    pivot + self.pivot_mod
                },
                None =>
                {
                    self.pivot_mod = Vec2::ZERO;
                    self.last_pivot = pivot.into();
                    pivot
                }
            };

            rotate_point(
                Vec2::new(self.offset_x, self.offset_y),
                pivot.with_y(-pivot.y),
                rotation_angle
            )
        }
    }

    //=======================================================================//

    /// A texture which can be rendered on screen and its metadata.
    #[allow(clippy::missing_docs_in_private_items)]
    #[must_use]
    #[derive(Debug)]
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

    impl EntityHull for Texture
    {
        #[inline]
        fn hull(&self) -> Hull { self.hull }
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
                hull:      Hull::new(1f32, 0f32, 0f32, 1f32),
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
        #[must_use]
        fn create_hull(size: UVec2) -> Hull
        {
            let half_width = (size.x / 2) as f32;
            let half_height = (size.y / 2) as f32;
            Hull::new(half_height, -half_height, -half_width, half_width)
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

        /// Whether the texture was edited.
        #[inline]
        #[must_use]
        pub const fn dirty(&self) -> bool { self.dirty }

        /// Clears the texture dirty flag.
        #[inline]
        pub fn clear_dirty_flag(&mut self) { self.dirty = false; }
    }

    //=======================================================================//

    from_compat!(_03, _04, _07);

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
        fn sprite_hull(&self, drawing_resources: &DrawingResources, center: Vec2) -> Option<Hull>
        {
            self.sprite_vxs(drawing_resources, center)
                .map(|vxs| Hull::from_points(vxs.into_iter()).unwrap())
        }

        #[inline]
        fn sprite_vxs(
            &self,
            drawing_resources: &DrawingResources,
            center: Vec2
        ) -> Option<[Vec2; 4]>
        {
            let mut vxs = self.sprite_vxs_at_origin(drawing_resources);

            if let Some(vxs) = &mut vxs
            {
                vxs.translate(center);
            }

            vxs
        }

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
        fn draw_size(&self, drawing_resources: &DrawingResources) -> UVec2
        {
            let size = drawing_resources.texture_or_error(&self.texture).size;

            if !self.sprite.enabled()
            {
                return size;
            }

            return_if_no_match!(
                self.overall_animation(drawing_resources),
                Animation::Atlas(anim),
                anim,
                size
            )
            .size(size)
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

        /// Checks whether the move is valid.
        #[inline]
        pub(in crate::map) fn check_move(
            &self,
            drawing_resources: &DrawingResources,
            delta: Vec2,
            center: Vec2
        ) -> bool
        {
            !(return_if_none!(self.sprite_hull(drawing_resources, center), true) + delta)
                .out_of_bounds()
        }

        /// Checks whether the scale is valid. Returns a [`TextureScale`] describing the outcome if
        /// it is.
        #[inline]
        pub(in crate::map) fn check_scale(
            &mut self,
            drawing_resources: &DrawingResources,
            info: &ScaleInfo,
            old_center: Vec2,
            new_center: Vec2
        ) -> Option<TextureScale>
        {
            let scale_x = self.scale_x * info.width_multi();
            let scale_y = self.scale_y * info.height_multi();
            let sprite_hull = return_if_none!(
                self.sprite_hull(drawing_resources, old_center),
                TextureScale {
                    offset_x: self.offset_x,
                    offset_y: self.offset_y,
                    scale_x,
                    scale_y
                }
                .into()
            );

            let new_offset = info.scaled_point(sprite_hull.center()) - new_center;
            let prev_offset_x = std::mem::replace(&mut self.offset_x, new_offset.x);
            let prev_offset_y = std::mem::replace(&mut self.offset_y, new_offset.y);
            let prev_scale_x = std::mem::replace(&mut self.scale_x, scale_x);
            let prev_scale_y = std::mem::replace(&mut self.scale_y, scale_y);

            let result = self.check_sprite_vxs(drawing_resources, new_center);

            self.offset_x = prev_offset_x;
            self.offset_y = prev_offset_y;
            self.scale_x = prev_scale_x;
            self.scale_y = prev_scale_y;

            match result
            {
                Ok(_) =>
                {
                    TextureScale {
                        offset_x: new_offset.x,
                        offset_y: new_offset.y,
                        scale_x,
                        scale_y
                    }
                    .into()
                },
                Err(()) => None
            }
        }

        #[inline]
        pub(in crate::map) fn scale(&mut self, value: &TextureScale)
        {
            self.scale_x = value.scale_x;
            self.scale_y = value.scale_y;
            self.offset_x = value.offset_x;
            self.offset_y = value.offset_y;
        }

        /// Checks whether the scale and flipping of the texture is valid. Returns a
        /// [`TextureScale`] describing the outcome if it is.
        #[inline]
        pub(in crate::map) fn check_flip_scale(
            &mut self,
            drawing_resources: &DrawingResources,
            info: &ScaleInfo,
            flip_queue: &ArrayVec<Flip, 2>,
            old_center: Vec2,
            new_center: Vec2
        ) -> Option<TextureScale>
        {
            let mut scale_x = self.scale_x * info.width_multi();
            let mut scale_y = self.scale_y * info.height_multi();

            for flip in flip_queue
            {
                match flip
                {
                    Flip::Above(_) | Flip::Below(_) => scale_y = -scale_y,
                    Flip::Left(_) | Flip::Right(_) => scale_x = -scale_x
                }
            }

            let sprite_hull = return_if_none!(
                self.sprite_hull(drawing_resources, old_center),
                TextureScale {
                    offset_x: self.offset_x,
                    offset_y: self.offset_y,
                    scale_x,
                    scale_y
                }
                .into()
            );
            let mut sprite_center = sprite_hull.center();

            for flip in flip_queue
            {
                match flip
                {
                    Flip::Above(mirror) | Flip::Below(mirror) =>
                    {
                        sprite_center.y = mirror - sprite_center.y;
                    },
                    Flip::Left(mirror) | Flip::Right(mirror) =>
                    {
                        sprite_center.x = mirror - sprite_center.x;
                    }
                }
            }

            let new_offset = info.scaled_point(sprite_center) - new_center;
            let prev_offset_x = std::mem::replace(&mut self.offset_x, new_offset.x);
            let prev_offset_y = std::mem::replace(&mut self.offset_y, new_offset.y);
            let prev_scale_x = std::mem::replace(&mut self.scale_x, scale_x);
            let prev_scale_y = std::mem::replace(&mut self.scale_y, scale_y);

            let result = self.check_sprite_vxs(drawing_resources, new_center);

            self.offset_x = prev_offset_x;
            self.offset_y = prev_offset_y;
            self.scale_x = prev_scale_x;
            self.scale_y = prev_scale_y;

            match result
            {
                Ok(_) =>
                {
                    TextureScale {
                        offset_x: new_offset.x,
                        offset_y: new_offset.y,
                        scale_x,
                        scale_y
                    }
                    .into()
                },
                Err(()) => None
            }
        }

        /// Whether the texture change is valid.
        #[inline]
        pub(in crate::map) fn check_texture_change(
            &mut self,
            drawing_resources: &DrawingResources,
            texture: &str,
            center: Vec2
        ) -> bool
        {
            if !self.sprite.enabled()
            {
                return true;
            }

            let prev = std::mem::replace(&mut self.texture, texture.to_owned());
            let result = self.check_sprite_vxs(drawing_resources, center).is_ok();
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
        pub(in crate::map) fn move_offset(&mut self, value: Vec2)
        {
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

            std::mem::replace(&mut self.height, value).into()
        }

        /// Whether the new angle is valid.
        #[inline]
        pub(in crate::map) fn check_angle(
            &mut self,
            drawing_resources: &DrawingResources,
            value: f32,
            center: Vec2
        ) -> bool
        {
            value.assert_normalized_degrees_angle();

            if !self.sprite.enabled() || value.around_equal_narrow(&self.angle)
            {
                return true;
            }

            let prev = std::mem::replace(&mut self.angle, value);
            let result = self.check_sprite_vxs(drawing_resources, center);
            self.angle = prev;

            result.is_ok()
        }

        /// Checks whether the rotation is valid. If valid returns a [`TextureRotation`] describing
        /// the outcome, if valid.
        #[inline]
        pub(in crate::map) fn check_rotation(
            &mut self,
            drawing_resources: &DrawingResources,
            pivot: Vec2,
            angle: f32,
            old_center: Vec2,
            new_center: Vec2
        ) -> Option<TextureRotation>
        {
            angle.assert_normalized_degrees_angle();

            let end_angle = (self.angle - angle).rem_euclid(360f32);
            let angle = angle.to_radians();

            let sprite_center = match self.sprite_hull(drawing_resources, old_center)
            {
                Some(hull) => hull.center(),
                None =>
                {
                    return TextureRotation {
                        offset: self.sprite.rotation_offset(self.angle.to_radians(), angle, pivot),
                        angle:  end_angle
                    }
                    .into();
                }
            };

            let new_offset = rotate_point(sprite_center, pivot, angle) - new_center;
            let prev_offset_x = std::mem::replace(&mut self.offset_x, new_offset.x);
            let prev_offset_y = std::mem::replace(&mut self.offset_y, new_offset.y);
            let prev_angle = std::mem::replace(&mut self.angle, end_angle);

            let result = self.check_sprite_vxs(drawing_resources, new_center);

            self.offset_x = prev_offset_x;
            self.offset_y = prev_offset_y;
            self.angle = prev_angle;

            match result
            {
                Ok(_) =>
                {
                    TextureRotation {
                        offset: new_offset,
                        angle:  end_angle
                    }
                    .into()
                },
                Err(()) => None
            }
        }

        #[inline]
        pub(in crate::map) fn rotate(&mut self, payload: &mut TextureRotation)
        {
            std::mem::swap(&mut self.angle, &mut payload.angle);

            match &mut self.sprite
            {
                Sprite::True =>
                {
                    std::mem::swap(&mut self.offset_x, &mut payload.offset.x);
                    std::mem::swap(&mut self.offset_y, &mut payload.offset.y);
                },
                Sprite::False {
                    rotation_auxiliary:
                        RotationAuxiliary {
                            offset_x, offset_y, ..
                        },
                    ..
                } =>
                {
                    std::mem::swap(offset_x, &mut payload.offset.x);
                    std::mem::swap(offset_y, &mut payload.offset.y);
                }
            };
        }

        /// Sets the angle, returns the previous value if different.
        #[inline]
        #[must_use]
        pub(in crate::map) fn set_angle(&mut self, value: f32) -> Option<f32>
        {
            value.assert_normalized_degrees_angle();

            if value.around_equal_narrow(&self.angle)
            {
                return None;
            }

            let prev = std::mem::replace(&mut self.angle, value);
            prev.into()
        }

        /// Whether the sprite value change is valid.
        #[inline]
        #[must_use]
        pub(in crate::map) fn check_sprite(
            &mut self,
            drawing_resources: &DrawingResources,
            value: bool,
            center: Vec2
        ) -> bool
        {
            if !value || self.sprite.enabled()
            {
                return true;
            }

            let prev_offset_x = std::mem::replace(&mut self.offset_x, 0f32);
            let prev_offset_y = std::mem::replace(&mut self.offset_y, 0f32);
            let new = Sprite::from(true);

            let prev_sprite = std::mem::replace(&mut self.sprite, new);
            let result = self.check_sprite_vxs(drawing_resources, center).is_ok();

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

            let prev = std::mem::replace(&mut self.sprite, Sprite::from(value));
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
            std::mem::swap(&mut self.sprite, &mut value.sprite);
            std::mem::swap(&mut self.offset_x, &mut value.offset_x);
            std::mem::swap(&mut self.offset_y, &mut value.offset_y);
        }

        /// Checks whether the texture is within bounds.
        #[inline]
        #[must_use]
        pub(in crate::map) fn check_within_bounds(
            &self,
            drawing_resources: &DrawingResources,
            center: Vec2
        ) -> bool
        {
            self.check_sprite_vxs(drawing_resources, center).is_ok()
        }

        /// Checks whether changing animation makes the sprite, if any, go out of bounds.
        #[inline]
        #[must_use]
        pub(in crate::map) fn check_animation_change(
            &mut self,
            drawing_resources: &DrawingResources,
            animation: &Animation,
            center: Vec2
        ) -> bool
        {
            if !self.sprite.enabled()
            {
                return true;
            }

            let prev = std::mem::replace(&mut self.animation, animation.clone());
            let result = self.check_sprite_vxs(drawing_resources, center).is_ok();
            self.animation = prev;
            result
        }

        /// Sets the [`Animation`].
        #[inline]
        pub(in crate::map) fn set_animation(&mut self, animation: Animation) -> Animation
        {
            std::mem::replace(&mut self.animation, animation)
        }

        /// Sets the [`Animation`] to list, using `texture` for the first frame, and returns the
        /// previous one.
        #[inline]
        pub(in crate::map) fn set_list_animation(&mut self, texture: &str) -> Animation
        {
            std::mem::replace(&mut self.animation, Animation::list_animation(texture))
        }

        /// Sets the [`Animation`] to list and returns the previous one.
        #[inline]
        pub(in crate::map) fn generate_list_animation(&mut self) -> Animation
        {
            std::mem::replace(&mut self.animation, Animation::list_animation(&self.texture))
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

        /// Returns the new sprite vertexes if the texture is being rendered as a sprite.
        #[inline]
        fn sprite_vxs_at_origin(&self, drawing_resources: &DrawingResources) -> Option<[Vec2; 4]>
        {
            if !self.sprite.enabled()
            {
                return None;
            }

            let size = self.draw_size(drawing_resources).as_vec2() *
                Vec2::new(self.scale_x.abs(), self.scale_y.abs()) /
                2f32;
            let mut rect = Hull::new(size.y, -size.y, -size.x, size.x).rectangle();
            let angle = -self.angle.to_radians();

            if angle != 0f32
            {
                for vx in &mut rect
                {
                    *vx = rotate_point_around_origin(*vx, angle);
                }
            }

            let offset = Vec2::new(self.offset_x, self.offset_y);

            for vx in &mut rect
            {
                *vx += offset;
            }

            rect.into()
        }

        /// Checks whether the sprite, if any, fits within the map.
        #[inline]
        fn check_sprite_vxs(
            &self,
            drawing_resources: &DrawingResources,
            center: Vec2
        ) -> Result<Option<[Vec2; 4]>, ()>
        {
            match self.sprite_vxs_at_origin(drawing_resources)
            {
                Some(rect) =>
                {
                    if rect.iter().any(|vx| (*vx + center).out_of_bounds())
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
