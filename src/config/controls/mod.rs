pub mod bind;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::input::keyboard::KeyCode;
use configparser::ini::Ini;
use hill_vacuum_shared::{continue_if_none, return_if_none};

use self::bind::Bind;
use super::IniConfig;
use crate::utils::{iterators::SkipIndexIterator, misc::FromToStr};

//=======================================================================//
// STATICS
//
//=======================================================================//

/// The name of the sections of the config file containing the controls binds.
const INI_SECTION: &str = "EDITOR_CONTROLS";

//=======================================================================//
// TYPES
//
//=======================================================================//

/// `Keycode` values associated with the `Bind`s.
pub struct BindsKeyCodes([Option<KeyCode>; Bind::SIZE]);

impl Default for BindsKeyCodes
{
    #[inline]
    fn default() -> Self { Self([None; Bind::SIZE]) }
}

impl BindsKeyCodes
{
    /// Loads the control binds stored in `config`.
    #[inline]
    pub(in crate::config) fn load(&mut self, config: &Ini)
    {
        for bind in Bind::iter()
        {
            bind.set_from_config(config, self);
        }
    }

    /// Stores the `Keycode` values of the binds in `config`.
    #[inline]
    pub(in crate::config) fn save(&self, config: &mut IniConfig)
    {
        for bind in Bind::iter()
        {
            let value = match self.get(bind)
            {
                Some(key) => key.to_str().into(),
                None => String::new()
            };

            config.0.set(INI_SECTION, bind.config_file_key(), Some(value));
        }
    }

    /// Returns the `KeyCode` value associated with `bind`.
    #[inline]
    #[must_use]
    pub const fn get(&self, bind: Bind) -> Option<KeyCode> { self.0[bind as usize] }

    /// Sets the `KeyCode` associated with `bind`.
    #[inline]
    fn set(&mut self, bind: Bind, value: KeyCode) { self.0[bind as usize] = value.into(); }

    /// Sets the `KeyCode` associated with `bind`.
    /// If another `Bind` has `value` assigned to it, it is unbound.
    #[allow(clippy::missing_panics_doc)]
    #[inline]
    pub fn set_bind(&mut self, bind: Bind, value: KeyCode)
    {
        for key in self.0.iter_mut().skip_index(bind as usize).unwrap()
        {
            if *continue_if_none!(key) == value
            {
                *key = None;
                break;
            }
        }

        self.set(bind, value);
    }

    /// Sets the `KeyCode` associated with `bind`, unless there is already another `Bind` with
    /// associated `value`.
    #[allow(clippy::missing_panics_doc)]
    #[inline]
    pub fn set_bind_if_unique(&mut self, bind: Bind, value: KeyCode)
    {
        if self
            .0
            .iter_mut()
            .skip_index(bind as usize)
            .unwrap()
            .any(|key| *return_if_none!(key, false) == value)
        {
            return;
        }

        self.set(bind, value);
    }

    /// Removes the `KeyCode` associated with `bind`.
    #[inline]
    pub fn unbind(&mut self, bind: Bind) { self.0[bind as usize] = None; }

    #[inline]
    pub fn reset(&mut self)
    {
        for (key, default) in self.0.iter_mut().zip(Bind::iter())
        {
            *key = default.default_bind().into();
        }
    }
}
