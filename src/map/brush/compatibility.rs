//=======================================================================//
// IMPORTS
//
//=======================================================================//

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{map::drawer::texture::sprite_values, Animation, Group, HvHashMap, HvVec, Id, Value};

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// Whether the texture should be rendered as a sprite.
#[must_use]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub(in crate::map) enum Sprite
{
    /// Yes.
    True,
    /// No.
    False
    {
        /// The horizontal parallax of the texture.
        parallax_x: f32,
        /// The vertical parallax of the texture.
        parallax_y: f32,
        /// The horizontal scrolling of the texture.
        scroll_x:   f32,
        /// The vertical scrolling of the texture.
        scroll_y:   f32
    }
}

impl Sprite
{
    sprite_values!(parallax_x, parallax_y, scroll_x, scroll_y);

    /// Whether `self` has value [`Sprite::True`].
    #[inline]
    #[must_use]
    pub const fn enabled(&self) -> bool { matches!(self, Self::True { .. }) }
}

//=======================================================================//
// STRUCTS
//
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
    /// The group of brushes this brush belong to.
    pub group:      Group,
    /// Whether collision against the polygonal shape is enabled.
    pub collision:  bool,
    /// The associated properties.
    pub properties: HvHashMap<String, Value>
}

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

impl TextureSettings
{
    #[inline]
    pub fn name(&self) -> &str { &self.texture }

    #[inline]
    pub fn offset_x(&self) -> f32 { self.offset_x }

    #[inline]
    pub fn offset_y(&self) -> f32 { self.offset_y }

    #[inline]
    pub fn scale_x(&self) -> f32 { self.scale_x }

    #[inline]
    #[must_use]
    pub fn scale_y(&self) -> f32 { self.scale_y }

    #[inline]
    pub fn scroll_x(&self) -> f32 { self.sprite.scroll_x() }

    #[inline]
    pub fn scroll_y(&self) -> f32 { self.sprite.scroll_y() }

    #[inline]
    pub fn parallax_x(&self) -> f32 { self.sprite.parallax_x() }

    #[inline]
    pub fn parallax_y(&self) -> f32 { self.sprite.parallax_y() }

    #[inline]
    pub fn height(&self) -> i8 { self.height }

    #[inline]
    pub fn angle(&self) -> f32 { self.angle }

    #[inline]
    pub fn sprite(&self) -> bool { self.sprite.enabled() }

    #[inline]
    pub fn animation(&self) -> &Animation { &self.animation }
}
