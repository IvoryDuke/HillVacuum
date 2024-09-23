pub(in crate::map) mod _03;
pub(in crate::map) mod _04;
pub(in crate::map) mod _06;
pub(in crate::map) mod _061;
pub(in crate::map) mod _07;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use serde::{Deserialize, Serialize};

use crate::{map::path::Path, utils::collections::Ids, Id};

//=======================================================================//
// MACROS
//
//=======================================================================//

macro_rules! impl_brush {
    () => {
        #[derive(Serialize, Deserialize)]
        pub(in crate::map::brush) struct BrushData
        {
            pub polygon:    ConvexPolygon,
            pub mover:      crate::map::brush::compatibility::Mover,
            pub properties: crate::map::properties::Properties
        }

        //=======================================================================//

        #[must_use]
        #[derive(Serialize, Deserialize)]
        pub(in crate::map) struct Brush
        {
            pub(in crate::map::brush) id:   crate::Id,
            pub(in crate::map::brush) data: BrushData
        }
    };
}

use impl_brush;

//=======================================================================//

macro_rules! tex_settings_061_07 {
    () => {
        #[must_use]
        #[derive(serde::Serialize, serde::Deserialize)]
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
            animation: crate::Animation
        }

        impl crate::TextureInterface for TextureSettings
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
            fn parallax_y(&self) -> f32 { -self.sprite.parallax_y() }

            #[inline]
            fn height(&self) -> i8 { self.height }

            #[inline]
            fn height_f32(&self) -> f32 { f32::from(self.height) }

            #[inline]
            fn angle(&self) -> f32 { self.angle }

            #[inline]
            fn sprite(&self) -> bool { self.sprite.enabled() }

            #[inline]
            fn animation(&self) -> &crate::Animation { &self.animation }

            #[inline]
            fn draw_offset(&self) -> Vec2 { unreachable!() }

            #[inline]
            fn draw_offset_with_parallax_and_scroll(
                &self,
                _: &bevy::prelude::Transform,
                _: f32,
                _: Vec2,
                _: bool
            ) -> Vec2
            {
                unreachable!()
            }
        }
    };
}

use tex_settings_061_07;

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[must_use]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(in crate::map) enum Mover
{
    #[default]
    None,
    Anchors(Ids),
    Motor(Motor),
    Anchored(Id)
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

#[must_use]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::map) struct Motor
{
    /// The [`Path`].
    pub path:             Path,
    /// The [`Id`]s of the attached [`Brush`]es.
    pub anchored_brushes: Ids
}
