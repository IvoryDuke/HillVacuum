//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::{
    input::{keyboard::KeyCode, ButtonInput},
    prelude::MouseButton
};
use glam::Vec2;
use hill_vacuum_shared::return_if_none;

use crate::{
    config::controls::{bind::Bind, BindsKeyCodes},
    HardcodedActions
};

//=======================================================================//
// MACROS
//
//=======================================================================//

/// A macro to generate the code of [`InputsPresses`].
macro_rules! input_presses {
    (
        $mouse_buttons:ident,
        $key_inputs:ident,
        $binds_inputs:ident,
        $(($name:ident, $input_type:ty, $key:expr, $source:ident $(, $binds:ident)?)),+
    ) => (
        /// A struct containing the states of all input presses required by the editor.
		pub(in crate::map::editor) struct InputsPresses
		{
			$(pub(in crate::map::editor::state) $name: $input_type,)+
            directional_keys_vector: Option<Vec2>,
            view_directional_keys_vector: Option<Vec2>
		}

        impl Default for InputsPresses
        {
            #[inline]
			fn default() -> Self
			{
				Self {
					$($name: <$input_type>::new($key),)+
                    directional_keys_vector: None,
                    view_directional_keys_vector: None
				}
			}
        }

		impl InputsPresses
		{
            /// Updates the state of the input presses.
			#[inline]
			pub(in crate::map::editor::state) fn update(
                &mut self,
                $key_inputs:    &ButtonInput<KeyCode>,
                $mouse_buttons: &ButtonInput<MouseButton>,
                config:         &mut crate::map::editor::Config,
                grid_size:      i16
            )
			{
                #[inline]
                #[must_use]
                pub fn directional_keys_vector(inputs: &InputsPresses, grid_size: i16) -> Option<Vec2>
                {
                    let mut dir = Vec2::ZERO;

                    if inputs.right.just_pressed()
                    {
                        dir.x += 1f32;
                    }

                    if inputs.left.just_pressed()
                    {
                        dir.x -= 1f32;
                    }

                    if inputs.up.just_pressed()
                    {
                        dir.y += 1f32;
                    }

                    if inputs.down.just_pressed()
                    {
                        dir.y -= 1f32;
                    }

                    (dir != Vec2::ZERO).then(|| dir * f32::from(grid_size))
                }

				$(self.$name.update($source $(, &config.$binds)?);)+

                let dir = directional_keys_vector(self, grid_size);

                if self.ctrl_pressed()
                {
                    self.directional_keys_vector = None;
                    self.view_directional_keys_vector = dir;
                }
                else
                {
                    self.view_directional_keys_vector = None;
                    self.directional_keys_vector = dir;
                }
			}

            /// Forcefully resets the input presses to not pressed.
            #[inline]
            pub(in crate::map::editor::state) fn clear(&mut self)
            {
                self.space.clear();
                self.back.clear();
                self.tab.clear();
                self.enter.clear();
                self.plus.clear();
                self.minus.clear();
                self.left_mouse.clear();
                self.right_mouse.clear();
                self.esc.clear();
                self.f4.clear();
                self.copy.clear();
                self.paste.clear();
                self.cut.clear();
                self.left.clear();
                self.right.clear();
                self.up.clear();
                self.down.clear();
            }
		}
	);
}

//=======================================================================//
// STRUCTS
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
// STRUCTS
//
//=======================================================================//

input_presses!(
    mouse_buttons,
    key_inputs,
    binds,
    (l_ctrl, InputStateHardCoded<KeyCode>, KeyCode::ControlLeft, key_inputs),
    (r_ctrl, InputStateHardCoded<KeyCode>, KeyCode::ControlRight, key_inputs),
    (l_shift, InputStateHardCoded<KeyCode>, KeyCode::ShiftLeft, key_inputs),
    (r_shift, InputStateHardCoded<KeyCode>, KeyCode::ShiftRight, key_inputs),
    (l_alt, InputStateHardCoded<KeyCode>, KeyCode::AltLeft, key_inputs),
    (r_alt, InputStateHardCoded<KeyCode>, KeyCode::AltRight, key_inputs),
    (space, InputStateHardCoded<KeyCode>, KeyCode::Space, key_inputs),
    (back, InputStateHardCoded<KeyCode>, KeyCode::Backspace, key_inputs),
    (tab, InputStateHardCoded<KeyCode>, KeyCode::Tab, key_inputs),
    (enter, InputStateHardCoded<KeyCode>, KeyCode::Enter, key_inputs),
    (plus, InputStateHardCoded<KeyCode>, KeyCode::NumpadAdd, key_inputs),
    (minus, InputStateHardCoded<KeyCode>, KeyCode::Minus, key_inputs),
    (left_mouse, InputStateHardCoded<MouseButton>, MouseButton::Left, mouse_buttons),
    (right_mouse, InputStateHardCoded<MouseButton>, MouseButton::Right, mouse_buttons),
    (esc, InputStateHardCoded<KeyCode>, KeyCode::Escape, key_inputs),
    (f4, InputStateHardCoded<KeyCode>, KeyCode::F4, key_inputs),
    (copy, InputStateHardCoded<KeyCode>, HardcodedActions::Copy.key(), key_inputs),
    (paste, InputStateHardCoded<KeyCode>, HardcodedActions::Paste.key(), key_inputs),
    (cut, InputStateHardCoded<KeyCode>, HardcodedActions::Cut.key(), key_inputs),
    (left, InputState, Bind::Left, key_inputs, binds),
    (right, InputState, Bind::Right, key_inputs, binds),
    (up, InputState, Bind::Up, key_inputs, binds),
    (down, InputState, Bind::Down, key_inputs, binds)
);

impl InputsPresses
{
    /// Whether shift is pressed.
    #[inline]
    #[must_use]
    pub const fn shift_pressed(&self) -> bool { self.l_shift.pressed() || self.r_shift.pressed() }

    /// Whether alt is pressed.
    #[inline]
    #[must_use]
    pub const fn alt_pressed(&self) -> bool { self.l_alt.pressed() || self.r_alt.pressed() }

    /// Whether ctrl is pressed.
    #[inline]
    #[must_use]
    pub const fn ctrl_pressed(&self) -> bool { self.l_ctrl.pressed() || self.r_ctrl.pressed() }

    /// Whether space is pressed.
    #[inline]
    #[must_use]
    pub const fn space_pressed(&self) -> bool { self.space.pressed() }

    /// Whether the copy key combo was just pressed.
    #[inline]
    #[must_use]
    pub const fn copy_just_pressed(&self) -> bool
    {
        self.ctrl_pressed() && self.copy.just_pressed()
    }

    /// Whether the paste key combo was just pressed.
    #[inline]
    #[must_use]
    pub const fn paste_just_pressed(&self) -> bool
    {
        self.ctrl_pressed() && self.paste.just_pressed()
    }

    /// Whether the cut key combo was just pressed.
    #[inline]
    #[must_use]
    pub const fn cut_just_pressed(&self) -> bool { self.ctrl_pressed() && self.cut.just_pressed() }

    #[inline]
    #[must_use]
    pub const fn directional_keys_delta(&self) -> Option<Vec2> { self.directional_keys_vector }

    #[inline]
    #[must_use]
    pub const fn directional_keys_view_delta(&self) -> Option<Vec2>
    {
        self.view_directional_keys_vector
    }
}

//=======================================================================//

/// The state of a hardcoded button.
pub(in crate::map::editor::state) struct InputStateHardCoded<T>
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
    fn new(key: T) -> Self
    {
        Self {
            button: key,
            state:  State::default()
        }
    }

    /// Whether the button is currently pressed.
    #[inline]
    #[must_use]
    pub const fn pressed(&self) -> bool
    {
        matches!(self.state, State::JustPressed | State::Pressed)
    }

    /// Whether the button has just been pressed.
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
pub(in crate::map::editor::state) struct InputState
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
    fn new(bind: Bind) -> Self
    {
        Self {
            bind,
            state: State::default()
        }
    }

    /// Whether the button has just been pressed.
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
