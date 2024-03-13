//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::{ButtonInput, KeyCode};
use configparser::ini::Ini;
use proc_macros::{bind_enum, EnumIter, EnumSize};
use shared::return_if_none;

use super::{BindsKeyCodes, INI_SECTION};
use crate::utils::misc::FromToStr;

//=======================================================================//
// ENUMS
//
//=======================================================================//

bind_enum!(
    Left,
    Right,
    Up,
    Down,
    ToggleGrid,
    ToggleTooltips,
    IncreaseGridSize,
    DecreaseGridSize,
    ShiftGrid,
    ToggleCursorSnap,
    ToggleCollision,
    TextureEditor
);

impl Bind
{
    /// The default `KeyCode`s associated with the [`Bind`] values. It is stored in the default
    /// config file generated on first launch, or any subsequent time when the program is
    /// launched and a config file cannot be found.
    #[inline]
    #[must_use]
    pub(in crate::config::controls) const fn default_bind(self) -> &'static str
    {
        match self
        {
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Up => "Up",
            Self::Down => "Down",
            Self::ToggleGrid => ".",
            Self::ToggleTooltips => "\'",
            Self::IncreaseGridSize => "[",
            Self::DecreaseGridSize => "]",
            Self::ShiftGrid => "/",
            Self::ToggleCursorSnap => "\\",
            Self::ToggleCollision => "=",
            Self::Square => "Q",
            Self::Triangle => "T",
            Self::Circle => "R",
            Self::FreeDraw => "D",
            Self::Entity => "E",
            Self::Vertex => "V",
            Self::Side => "S",
            Self::Path => "L",
            Self::Clip => "C",
            Self::Shatter => "H",
            Self::Merge => "M",
            Self::Hollow => "W",
            Self::Intersection => "I",
            Self::Subtract => "U",
            Self::Scale => "A",
            Self::Shear => "J",
            Self::Zoom => "Z",
            Self::Snap => "N",
            Self::Rotate => "K",
            Self::Flip => "F",
            Self::TextureEditor => "X",
            Self::Paint => "P",
            Self::Thing => "G"
        }
    }

    /// `KeyCode` associated with [`Bind`].
    #[inline]
    #[must_use]
    pub const fn keycode(self, binds: &BindsKeyCodes) -> Option<KeyCode> { binds.get(self) }

    /// Returns a `str` representing this [`Bind`]'s associated `Keycode`.
    #[inline]
    #[must_use]
    pub fn keycode_str(self, binds: &BindsKeyCodes) -> &'static str
    {
        match binds.get(self)
        {
            Some(key) => key.to_str(),
            None => ""
        }
    }

    /// Whever the `KeyCode` associated with this has just been pressed.
    #[inline]
    #[must_use]
    pub fn just_pressed(self, key_inputs: &ButtonInput<KeyCode>, binds: &BindsKeyCodes) -> bool
    {
        key_inputs.just_pressed(return_if_none!(binds.get(self), false))
    }

    /// Returns true if the alternative function of the bind has just been pressed.
    #[inline]
    #[must_use]
    pub fn alt_just_pressed(self, key_inputs: &ButtonInput<KeyCode>, binds: &BindsKeyCodes)
        -> bool
    {
        assert!(
            matches!(self, Self::TextureEditor | Self::Snap | Self::Zoom),
            "Bind {self:?} has no alternative function."
        );

        if !(key_inputs.pressed(KeyCode::AltLeft) || key_inputs.pressed(KeyCode::AltRight)) ||
            key_inputs.pressed(KeyCode::ControlLeft) ||
            key_inputs.pressed(KeyCode::ControlRight)
        {
            return false;
        }

        self.just_pressed(key_inputs, binds)
    }

    /// Returns true whever `value` can be assigned to a `Bind`.
    #[inline]
    #[must_use]
    fn is_keycode_legal(value: KeyCode) -> bool
    {
        const MODIFIERS: [KeyCode; 6] = [
            KeyCode::ControlLeft,
            KeyCode::ControlRight,
            KeyCode::AltLeft,
            KeyCode::AltRight,
            KeyCode::ShiftLeft,
            KeyCode::ShiftRight
        ];

        const HARDCODED_BINDS: [KeyCode; 6] = [
            KeyCode::Escape,
            KeyCode::Space,
            KeyCode::Tab,
            KeyCode::Backspace,
            KeyCode::Enter,
            KeyCode::Backquote
        ];

        MODIFIERS.into_iter().chain(HARDCODED_BINDS).all(|key| key != value)
    }

    /// Loads the `KeyCode` associated with [`Bind`] from the `config` data, but only if `value`
    /// is acceptable. If `value` is not acceptable no `KeyCode` is assigned to this,
    /// nor is it assigned if another `Bind` holds such a value.
    #[inline]
    pub(in crate::config::controls) fn set_from_config(
        self,
        config: &Ini,
        binds: &mut BindsKeyCodes
    )
    {
        let value = return_if_none!(KeyCode::from_str(&return_if_none!(
            config.get(INI_SECTION, self.config_file_key())
        )));

        if !Self::is_keycode_legal(value)
        {
            return;
        }

        binds.set_bind_if_unique(self, value);
    }

    /// Sets the `KeyCode` associated with this, but only if `value` is acceptable.
    /// Returns false if `value` could not be assigned.
    /// If another `Bind` has `value` assigned it is unbound.
    #[allow(clippy::missing_panics_doc)]
    #[inline]
    #[must_use]
    pub fn set_bind(self, value: KeyCode, binds: &mut BindsKeyCodes) -> bool
    {
        if !Self::is_keycode_legal(value)
        {
            return false;
        }

        binds.set_bind(self, value);
        true
    }

    /// Removes the `KeyCode` associated with this.
    #[inline]
    pub fn unbind(self, binds: &mut BindsKeyCodes) { binds.unbind(self); }
}
