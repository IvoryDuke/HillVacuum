//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::{ButtonInput, KeyCode};
use hill_vacuum_shared::return_if_none;

use crate::config::controls::{bind::Bind, BindsKeyCodes};

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The state of the button of an I/O device.
#[derive(Default)]
enum State
{
    /// Not pressed.
    #[default]
    NotPressed,
    /// Pressed.
    Pressed,
    /// Just pressed.
    JustPressed
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The state of a hardcoded button.
pub struct InputStateHardCoded<T>
{
    /// The button.
    button: T,
    /// Its press [`State`].
    state:  State
}

impl<T> InputStateHardCoded<T>
where
    T: Copy + Eq + std::hash::Hash + std::marker::Send + std::marker::Sync
{
    /// Returns a new [`InputStateHardCoded`].
    #[inline]
    #[must_use]
    pub fn new(key: T) -> Self
    {
        Self {
            button: key,
            state:  State::default()
        }
    }

    /// Whever the button is currently pressed.
    #[inline]
    #[must_use]
    pub const fn pressed(&self) -> bool
    {
        matches!(self.state, State::JustPressed | State::Pressed)
    }

    /// Whever the button has just been pressed.
    #[inline]
    #[must_use]
    pub const fn just_pressed(&self) -> bool { matches!(self.state, State::JustPressed) }

    /// Updates the state of the button.
    #[inline]
    pub fn update(&mut self, source: &ButtonInput<T>)
    {
        if source.just_pressed(self.button)
        {
            self.state = State::JustPressed;
        }
        else if source.pressed(self.button)
        {
            self.state = State::Pressed;
        }
        else
        {
            self.state = State::NotPressed;
        }
    }

    /// Forcefully sets the press state of the button to not pressed.
    #[inline]
    pub fn clear(&mut self) { self.state = State::default(); }
}

//=======================================================================//

/// The state of the button associated to a [`Bind`].
pub struct InputState
{
    /// The [`Bind`].
    bind:  Bind,
    /// The associated button press state.
    state: State
}

impl InputState
{
    /// Returns a new [`InputState`].
    #[inline]
    #[must_use]
    pub fn new(bind: Bind) -> Self
    {
        Self {
            bind,
            state: State::default()
        }
    }

    /// Whever the button has just been pressed.
    #[inline]
    #[must_use]
    pub const fn just_pressed(&self) -> bool { matches!(self.state, State::JustPressed) }

    /// Updates the state of the button associated to the bind.
    #[inline]
    pub fn update(&mut self, source: &ButtonInput<KeyCode>, binds: &BindsKeyCodes)
    {
        let key = return_if_none!(binds.get(self.bind));

        if source.just_pressed(key)
        {
            self.state = State::JustPressed;
        }
        else if source.pressed(key)
        {
            self.state = State::Pressed;
        }
        else
        {
            self.state = State::NotPressed;
        }
    }

    /// Forcefully sets the press state of the button to not pressed.
    #[inline]
    pub fn clear(&mut self) { self.state = State::default(); }
}
