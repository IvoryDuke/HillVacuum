//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::Transform;
use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{Animation, Group, HvHashMap, HvVec, Id, TextureInterface, Value};

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[must_use]
#[derive(Serialize, Deserialize)]
pub(in crate::map) enum Sprite
{
    True,
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
    pub const fn enabled(&self) -> bool { matches!(self, Self::True) }
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

#[must_use]
#[derive(Serialize, Deserialize)]
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
    fn offset_y(&self) -> f32 { -self.offset_y }

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

    #[inline]
    fn draw_offset(&self) -> Vec2 { unreachable!() }

    #[inline]
    fn draw_offset_with_parallax_and_scroll(&self, _: &Transform, _: f32, _: Vec2, _: bool)
        -> Vec2
    {
        unreachable!()
    }
}

//=======================================================================//

#[must_use]
#[derive(Serialize, Deserialize)]
pub struct BrushViewer
{
    /// The [`Id`].
    pub id:         Id,
    /// The vertexes.
    pub vertexes:   HvVec<Vec2>,
    /// The texture.
    pub texture:    Option<TextureSettings>,
    /// The [`Mover`].
    pub group:      Group,
    /// Whether collision against the polygonal shape is enabled.
    pub collision:  bool,
    /// The associated properties.
    pub properties: HvHashMap<String, Value>
}
