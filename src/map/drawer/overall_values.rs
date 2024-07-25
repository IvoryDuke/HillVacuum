//=======================================================================//
// IMPORTS
//
//=======================================================================//

use super::animation::overall_values::{OverallAnimation, UiOverallAnimation};
use crate::{
    utils::overall_value::{OverallValue, OverallValueInterface, OverallValueToUi, UiOverallValue},
    Sprite,
    TextureInterface,
    TextureSettings
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The overall sprite value of the selected brushes' textures.
#[derive(Debug, Default)]
pub(in crate::map) enum OverallSprite
{
    /// Nothing selected.
    #[default]
    None,
    /// Non uniform.
    NonUniform,
    /// True.
    True,
    /// False.
    False
    {
        /// The overall horizontal parallax.
        parallax_x: OverallValue<f32>,
        /// The overall vertical parallax.
        parallax_y: OverallValue<f32>,
        /// The overall horizontal scroll.
        scroll_x:   OverallValue<f32>,
        /// The overall vertical scroll.
        scroll_y:   OverallValue<f32>
    }
}

impl From<&Sprite> for OverallSprite
{
    #[inline]
    fn from(value: &Sprite) -> Self
    {
        match value
        {
            Sprite::True { .. } => Self::True,
            Sprite::False {
                parallax_x,
                parallax_y,
                scroll_x,
                scroll_y
            } =>
            {
                Self::False {
                    parallax_x: (*parallax_x).into(),
                    parallax_y: (*parallax_y).into(),
                    scroll_x:   (*scroll_x).into(),
                    scroll_y:   (*scroll_y).into()
                }
            },
        }
    }
}

impl OverallValueInterface<Sprite> for OverallSprite
{
    #[inline]
    fn is_not_uniform(&self) -> bool { matches!(self, Self::NonUniform) }

    #[inline]
    fn stack(&mut self, value: &Sprite) -> bool
    {
        match (&mut *self, value)
        {
            (Self::None, Sprite::True { .. }) => *self = Self::True,
            (
                Self::None,
                Sprite::False {
                    parallax_x,
                    parallax_y,
                    scroll_x,
                    scroll_y
                }
            ) =>
            {
                *self = Self::False {
                    parallax_x: (*parallax_x).into(),
                    parallax_y: (*parallax_y).into(),
                    scroll_x:   (*scroll_x).into(),
                    scroll_y:   (*scroll_y).into()
                };
            },
            (Self::True, Sprite::False { .. }) | (Self::False { .. }, Sprite::True { .. }) =>
            {
                *self = Self::NonUniform;
            },
            _ => ()
        };

        self.is_not_uniform()
    }

    #[allow(clippy::similar_names)]
    #[inline]
    fn merge(&mut self, other: Self) -> bool
    {
        if let Self::None = self
        {
            *self = other;
            return self.is_not_uniform();
        }

        match (&mut *self, other)
        {
            (Self::None, _) => unreachable!(),
            (_, Self::NonUniform) |
            (Self::True, Self::False { .. }) |
            (Self::False { .. }, Self::True) => *self = Self::NonUniform,
            (
                Self::False {
                    parallax_x: parallax_x_0,
                    parallax_y: parallax_y_0,
                    scroll_x: scroll_x_0,
                    scroll_y: scroll_y_0
                },
                Self::False {
                    parallax_x: parallax_x_1,
                    parallax_y: parallax_y_1,
                    scroll_x: scroll_x_1,
                    scroll_y: scroll_y_1
                }
            ) =>
            {
                _ = parallax_x_0.merge(parallax_x_1);
                _ = parallax_y_0.merge(parallax_y_1);
                _ = scroll_x_0.merge(scroll_x_1);
                _ = scroll_y_0.merge(scroll_y_1);
            },
            _ => ()
        };

        self.is_not_uniform()
    }
}

//=======================================================================//

/// The overall settings of the textures of the selected brushes.
#[allow(clippy::missing_docs_in_private_items)]
#[derive(Debug, Default)]
pub(in crate::map) struct OverallTextureSettings
{
    name:      OverallValue<String>,
    scale_x:   OverallValue<f32>,
    scale_y:   OverallValue<f32>,
    offset_x:  OverallValue<f32>,
    offset_y:  OverallValue<f32>,
    angle:     OverallValue<f32>,
    height:    OverallValue<i8>,
    sprite:    OverallSprite,
    animation: OverallAnimation
}

impl From<Option<&TextureSettings>> for OverallTextureSettings
{
    #[inline]
    #[must_use]
    fn from(value: Option<&TextureSettings>) -> Self
    {
        match value
        {
            Some(value) =>
            {
                Self {
                    name:      value.name().to_string().into(),
                    scale_x:   value.scale_x().into(),
                    scale_y:   value.scale_y().into(),
                    offset_x:  value.offset_x().into(),
                    offset_y:  value.offset_y().into(),
                    height:    value.height().into(),
                    angle:     value.angle().into(),
                    sprite:    value.sprite_struct().into(),
                    animation: value.animation().into()
                }
            },
            None =>
            {
                Self {
                    name:      OverallValue::None,
                    scale_x:   OverallValue::None,
                    scale_y:   OverallValue::None,
                    offset_x:  OverallValue::None,
                    offset_y:  OverallValue::None,
                    height:    OverallValue::None,
                    angle:     OverallValue::None,
                    sprite:    OverallSprite::None,
                    animation: OverallAnimation::NoSelection
                }
            },
        }
    }
}

impl OverallValueInterface<Option<&TextureSettings>> for OverallTextureSettings
{
    #[allow(clippy::ref_option_ref)]
    #[inline]
    fn stack(&mut self, value: &Option<&TextureSettings>) -> bool { self.merge((*value).into()) }

    #[inline]
    fn merge(&mut self, other: Self) -> bool
    {
        let mut uniform = false;

        match (&self.name, &other.name)
        {
            (OverallValue::None, OverallValue::Uniform(_)) |
            (OverallValue::Uniform(_), OverallValue::None) =>
            {
                self.name = OverallValue::NonUniform;
            },
            _ => uniform |= self.name.merge(other.name)
        };

        for (v_0, v_1) in [
            (&mut self.scale_x, &other.scale_x),
            (&mut self.scale_y, &other.scale_y),
            (&mut self.offset_x, &other.offset_x),
            (&mut self.offset_y, &other.offset_y),
            (&mut self.angle, &other.angle)
        ]
        {
            uniform |= !v_0.merge(*v_1);
        }

        uniform |= !self.height.merge(other.height) |
            !self.sprite.merge(other.sprite) |
            !self.animation.merge(other.animation);

        !uniform
    }

    #[inline]
    fn is_not_uniform(&self) -> bool
    {
        self.name.is_not_uniform() &&
            self.scale_x.is_not_uniform() &&
            self.scale_y.is_not_uniform() &&
            self.offset_x.is_not_uniform() &&
            self.offset_y.is_not_uniform() &&
            self.height.is_not_uniform() &&
            self.angle.is_not_uniform() &&
            self.sprite.is_not_uniform() &&
            self.animation.is_not_uniform()
    }
}

impl OverallTextureSettings
{
    /// Returns an empty [`OverallTextureSettings`].
    #[inline]
    #[must_use]
    pub fn none() -> Self { Self::from(None::<&TextureSettings>) }
}

//=======================================================================//

/// A UI representation of the overall settings of the textures of the selected brushes.
#[allow(clippy::missing_docs_in_private_items)]
#[derive(Default)]
pub(in crate::map) struct UiOverallTextureSettings
{
    pub name:       UiOverallValue<String>,
    pub scale_x:    UiOverallValue<f32>,
    pub scale_y:    UiOverallValue<f32>,
    pub offset_x:   UiOverallValue<f32>,
    pub offset_y:   UiOverallValue<f32>,
    pub scroll_x:   Option<UiOverallValue<f32>>,
    pub scroll_y:   Option<UiOverallValue<f32>>,
    pub height:     UiOverallValue<i8>,
    pub angle:      UiOverallValue<f32>,
    pub sprite:     OverallValue<bool>,
    pub parallax_x: Option<UiOverallValue<f32>>,
    pub parallax_y: Option<UiOverallValue<f32>>,
    pub animation:  UiOverallAnimation
}

impl From<OverallTextureSettings> for UiOverallTextureSettings
{
    #[inline]
    #[must_use]
    fn from(value: OverallTextureSettings) -> Self
    {
        let (sprite, parallax_x, parallax_y, scroll_x, scroll_y) = match value.sprite
        {
            OverallSprite::None => (OverallValue::None, None, None, None, None),
            OverallSprite::NonUniform => (OverallValue::NonUniform, None, None, None, None),
            OverallSprite::True => (true.into(), None, None, None, None),
            OverallSprite::False {
                parallax_x,
                parallax_y,
                scroll_x,
                scroll_y
            } =>
            {
                (
                    false.into(),
                    Some(parallax_x.into()),
                    Some(parallax_y.into()),
                    Some(scroll_x.into()),
                    Some(scroll_y.into())
                )
            },
        };

        Self {
            name: value.name.ui(),
            scale_x: value.scale_x.ui(),
            scale_y: value.scale_y.ui(),
            offset_x: value.offset_x.ui(),
            offset_y: value.offset_y.ui(),
            scroll_x,
            scroll_y,
            height: value.height.ui(),
            angle: value.angle.ui(),
            sprite,
            parallax_x,
            parallax_y,
            animation: value.animation.ui()
        }
    }
}
