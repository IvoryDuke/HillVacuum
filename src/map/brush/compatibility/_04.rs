//=======================================================================//
// IMPORTS
//
//=======================================================================//

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{
    map::properties::Properties,
    utils::hull::Hull,
    Animation,
    Id,
    Mover,
    TextureInterface
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[must_use]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub(in crate::map) enum Sprite
{
    True(Hull),
    False
    {
        parallax_x: f32,
        parallax_y: f32,
        scroll_x:   f32,
        scroll_y:   f32
    }
}

impl Sprite
{
    crate::map::drawer::texture::sprite_values!(parallax_x, parallax_y, scroll_x, scroll_y);

    #[inline]
    #[must_use]
    pub const fn enabled(&self) -> bool { matches!(self, Self::True(_)) }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

crate::map::brush::impl_convex_polygon!(TextureSettings);
#[cfg(feature = "ui")]
crate::map::brush::impl_convex_polygon_ui!();

//=======================================================================//

#[must_use]
#[derive(Clone, Debug, Serialize, Deserialize)]
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

//=======================================================================//

crate::map::brush::compatibility::impl_brush!();
