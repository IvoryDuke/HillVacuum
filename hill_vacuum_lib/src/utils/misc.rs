//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::{prelude::Vec2, window::Window};

use super::hull::Hull;

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

/// The length of the sides of the vertex highlights.
pub const VX_HGL_SIDE: f32 = 5f32;
/// The squared length of the sides of the vertex highlights.
pub const VX_HGL_SIDE_SQUARED: f32 = VX_HGL_SIDE * VX_HGL_SIDE;

//=======================================================================//
// TRAITS
//
//=======================================================================//

/// A trait for collections that consumes them and returns None if they are empty.
pub trait NoneIfEmpty
{
    /// Returns None is `self` is empty, otherwise returns `Some(self)`.
    #[must_use]
    fn none_if_empty(self) -> Option<Self>
    where
        Self: Sized;
}

impl<T> NoneIfEmpty for Vec<T>
{
    #[inline]
    fn none_if_empty(self) -> Option<Self> { (!self.is_empty()).then_some(self) }
}

//=======================================================================//

/// A trait for collections that replaces all the value with new ones.
/// This is helpful to preserve the accumulated capacity.
pub trait ReplaceValues<T>
{
    /// Replaces all the contained values with the ones returned by `iter`.
    fn replace_values<I: IntoIterator<Item = T>>(&mut self, iter: I);
}

impl<T> ReplaceValues<T> for Vec<T>
{
    #[inline]
    fn replace_values<I: IntoIterator<Item = T>>(&mut self, iter: I)
    {
        self.clear();
        self.extend(iter);
    }
}

impl<'a, T: 'a + Copy> ReplaceValues<&'a T> for Vec<T>
{
    #[inline]
    fn replace_values<I: IntoIterator<Item = &'a T>>(&mut self, iter: I)
    {
        self.clear();
        self.extend(iter);
    }
}

impl ReplaceValues<char> for String
{
    #[inline]
    fn replace_values<I: IntoIterator<Item = char>>(&mut self, iter: I)
    {
        self.clear();
        self.extend(iter);
    }
}

//=======================================================================//

/// A trait to replace values of variables with their default and return the original.
/// Equivalent to using `std::mem::take(value)`.
pub trait TakeValue
{
    /// Replaces `self` with its default and returns its value.
    #[must_use]
    fn take_value(&mut self) -> Self;
}

//=======================================================================//

/// A trait for objects representing cameras.
pub trait Camera
{
    /// The camera position.
    #[must_use]
    fn pos(&self) -> Vec2;

    /// The camera scale.
    #[must_use]
    fn scale(&self) -> f32;

    /// Returns a [`Hull`] representing the camera's viewport.
    #[must_use]
    fn viewport_ui_constricted(&self, window: &Window) -> Hull;

    /// Returns a [`Hull`] describing the viewport of `window`.
    #[must_use]
    fn viewport(&self, window: &Window) -> Hull;

    /// Sets the position of `self`.
    fn set_pos(&mut self, pos: Vec2);

    /// Moves the position of `self` by `delta`.
    fn translate(&mut self, delta: Vec2);

    /// Changes the scale of the the camera.
    #[must_use]
    fn change_scale(&mut self, units: f32) -> f32;

    /// Zooms in/out by `units`.
    fn zoom(&mut self, units: f32);

    /// Zooms in.
    #[inline]
    fn zoom_in(&mut self) { self.zoom(1f32); }

    /// Zooms out.
    #[inline]
    fn zoom_out(&mut self) { self.zoom(-1f32); }

    /// Zooms `self` on a certain position by `units` amount.
    #[inline]
    fn zoom_on_ui_pos(&mut self, window: &Window, pos: Vec2, units: f32)
    where
        Self: Sized
    {
        let pre_scale_pos = to_world_coordinates(pos, window, self);
        _ = self.change_scale(units);
        self.translate(pre_scale_pos - to_world_coordinates(pos, window, self));
    }

    /// Like `scale_viewport_to_hull`, but also accounts for the UI on screen space.
    fn scale_viewport_ui_constricted_to_hull(&mut self, window: &Window, hull: &Hull, padding: f32);

    /// Returns the UI dimensions of the window divided by half and scaled to represent its world
    /// dimensions.
    #[inline]
    #[must_use]
    fn scaled_window_half_sizes(&self, window: &Window) -> (f32, f32)
    {
        ((window.width() / 2f32) * self.scale(), (window.height() / 2f32) * self.scale())
    }
}

//=======================================================================//

/// A trait to create an object from a static `str` and to get a static `str` representing the
/// object.
pub trait FromToStr
where
    Self: Sized
{
    /// Creates an instance of `Self` from `value`.
    /// Returns None if it's not possible.
    #[must_use]
    fn from_str(value: &str) -> Option<Self>;

    /// Returns a static `str` representing `self`.
    #[must_use]
    fn to_str(self) -> &'static str;
}

/// Implements [`FromToStr`] for [`bevy::prelude::KeyCode`].
macro_rules! keycode_from_to_str {
    ($(($str:expr, $kc:ident)),+) => (
        impl FromToStr for bevy::prelude::KeyCode
        {
            #[inline]
            fn from_str(value: &str) -> Option<Self>
            {
                match value
                {
                    $($str => Some(bevy::prelude::KeyCode::$kc),)+
                    _ => None
                }
            }

            #[inline]
            fn to_str(self) -> &'static str
            {
                match self
                {
                    $(bevy::prelude::KeyCode::$kc => $str,)+
                    _ => ""
                }
            }
        }
	);
}

keycode_from_to_str!(
    ("1", Digit1),
    ("2", Digit2),
    ("3", Digit3),
    ("4", Digit4),
    ("5", Digit5),
    ("6", Digit6),
    ("7", Digit7),
    ("8", Digit8),
    ("9", Digit9),
    ("0", Digit0),
    ("A", KeyA),
    ("B", KeyB),
    ("C", KeyC),
    ("D", KeyD),
    ("E", KeyE),
    ("F", KeyF),
    ("G", KeyG),
    ("H", KeyH),
    ("I", KeyI),
    ("J", KeyJ),
    ("K", KeyK),
    ("L", KeyL),
    ("M", KeyM),
    ("N", KeyN),
    ("O", KeyO),
    ("P", KeyP),
    ("Q", KeyQ),
    ("R", KeyR),
    ("S", KeyS),
    ("T", KeyT),
    ("U", KeyU),
    ("V", KeyV),
    ("W", KeyW),
    ("X", KeyX),
    ("Y", KeyY),
    ("Z", KeyZ),
    ("Escape", Escape),
    ("F1", F1),
    ("F2", F2),
    ("F3", F3),
    ("F4", F4),
    ("F5", F5),
    ("F6", F6),
    ("F7", F7),
    ("F8", F8),
    ("F9", F9),
    ("F10", F10),
    ("F11", F11),
    ("F12", F12),
    ("F13", F13),
    ("F14", F14),
    ("F15", F15),
    ("F16", F16),
    ("F17", F17),
    ("F18", F18),
    ("F19", F19),
    ("F20", F20),
    ("F21", F21),
    ("F22", F22),
    ("F23", F23),
    ("F24", F24),
    ("Insert", Insert),
    ("Home", Home),
    ("Delete", Delete),
    ("End", End),
    ("PageDown", PageDown),
    ("PageUp", PageUp),
    ("Left", ArrowLeft),
    ("Up", ArrowUp),
    ("Right", ArrowRight),
    ("Down", ArrowDown),
    ("Back", Backspace),
    ("Enter", Enter),
    ("Space", Space),
    ("Numpad0", Numpad0),
    ("Numpad1", Numpad1),
    ("Numpad2", Numpad2),
    ("Numpad3", Numpad3),
    ("Numpad4", Numpad4),
    ("Numpad5", Numpad5),
    ("Numpad6", Numpad6),
    ("Numpad7", Numpad7),
    ("Numpad8", Numpad8),
    ("Numpad9", Numpad9),
    ("+", NumpadAdd),
    ("'", Quote),
    ("\\", Backslash),
    ("CapsLock", CapsLock),
    (",", Comma),
    ("Convert", Convert),
    ("NumpadDecimal", NumpadDecimal),
    ("Numpad /", NumpadDivide),
    ("=", Equal),
    ("`", Backquote),
    ("LAlt", AltLeft),
    ("[", BracketLeft),
    ("LCtrl", ControlLeft),
    ("LShift", ShiftLeft),
    ("LSuper", SuperLeft),
    ("-", Minus),
    ("Numpad *", NumpadMultiply),
    ("Numpad ,", NumpadComma),
    ("NumpadEnter", NumpadEnter),
    ("Numpad =", NumpadEqual),
    (".", Period),
    ("RAlt", AltRight),
    ("]", BracketRight),
    ("RCtrl", ControlRight),
    ("RShift", ShiftRight),
    ("RWin", SuperRight),
    (";", Semicolon),
    ("/", Slash),
    ("Numpad -", NumpadSubtract),
    ("Tab", Tab)
);

//=======================================================================//

/// A trait to implement value toggle for an object.
pub trait Toggle
{
    /// Toggles 'self'.
    fn toggle(&mut self);
}

impl Toggle for bool
{
    #[inline]
    fn toggle(&mut self) { *self = !*self; }
}

impl Toggle for f32
{
    #[inline]
    fn toggle(&mut self) { *self = -*self; }
}

//=======================================================================//

/// A trait to determine whever a point is inside the UI rectangle highlight of a point.
pub trait PointInsideUiHighlight
{
    /// Whever `p` is inside the area of `self` while accounting for `camera_scale`.
    #[must_use]
    fn is_point_inside_ui_highlight(&self, p: Vec2, camera_scale: f32) -> bool;
}

impl PointInsideUiHighlight for Vec2
{
    #[inline]
    fn is_point_inside_ui_highlight(&self, p: Vec2, camera_scale: f32) -> bool
    {
        let half_side_length = bumped_vertex_highlight_side_length(camera_scale) / 2f32;
        f32::abs(self.x - p.x) <= half_side_length && f32::abs(self.y - p.y) <= half_side_length
    }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// An on/off switch that pulses at a certain interval.
#[must_use]
#[derive(Clone, Copy)]
pub struct Blinker
{
    /// The leftover amount of time it must pulse.
    time:     f32,
    /// Whever the pulse is on or off.
    onoff:    bool,
    /// The duration of the pulsation.
    interval: f32
}

impl Blinker
{
    /// Returns a new [`Blinker`].
    #[inline]
    pub const fn new(interval: f32) -> Self
    {
        Self {
            time: interval,
            onoff: true,
            interval
        }
    }

    /// Whever the [`Blinker`] is in the on state.
    #[inline]
    #[must_use]
    pub const fn on(&self) -> bool { self.onoff }

    /// Updates the state of the [`Blinker`].
    #[inline]
    pub fn update(&mut self, delta_time: f32) -> bool
    {
        self.time -= delta_time;

        if self.time <= 0f32
        {
            let delta_time = self.time.abs();
            self.time = self.interval;
            self.onoff.toggle();

            self.update(delta_time);
        }

        self.onoff
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Returns the world position of 'p'.
#[inline]
#[must_use]
pub fn to_world_coordinates<T: Camera>(p: Vec2, window: &Window, camera: &T) -> Vec2
{
    let p =
        p * camera.scale() - (Vec2::new(window.width(), window.height()) * camera.scale()) / 2f32;
    let camera_pos = camera.pos();
    Vec2::new(p.x + camera_pos.x, -(p.y - camera_pos.y))
}

//=======================================================================//

/// Returns the scaled length of the vertex highlight side.
#[inline]
#[must_use]
pub fn vertex_highlight_side_length(camera_scale: f32) -> f32 { camera_scale * VX_HGL_SIDE }

/// Returns a slightly increased length of the vertex highlight side.
#[inline]
#[must_use]
pub fn bumped_vertex_highlight_side_length(camera_scale: f32) -> f32
{
    vertex_highlight_side_length(camera_scale) * 4f32
}

/// Returns a [`Hull`] describing a square with side `side_length` with center at the origin.
#[inline]
#[must_use]
fn square(side_length: f32) -> Hull
{
    Hull::new(side_length, -side_length, -side_length, side_length)
}

/// Returns a [`Hull`] representing a vertex highlight with center at the origin.
#[inline]
#[must_use]
pub fn vertex_highlight_square(camera_scale: f32) -> Hull
{
    square(vertex_highlight_side_length(camera_scale))
}

/// Returns a [`Hull`] representing a slightly buffed vertex highlight with center at the origin.
#[inline]
#[must_use]
pub fn bumped_vertex_highlight_square(camera_scale: f32) -> Hull
{
    square(bumped_vertex_highlight_side_length(camera_scale))
}

//=======================================================================//

/// Returns the number after to `n` in the residue class described by `div`.
#[inline]
#[must_use]
pub fn next(n: usize, div: usize) -> usize { next_n_steps(n, 1, div) }

/// Returns the number `s` steps ahead of `n` in the residue class described by `div`.
#[inline]
#[must_use]
pub fn next_n_steps(n: usize, s: usize, div: usize) -> usize
{
    assert!(n < div, "n {n} is higher or equal than div {div}.");

    let n = n + s;

    if n >= div
    {
        n - div
    }
    else
    {
        n
    }
}

/// Returns the element of `l` at the index after `n`.
#[inline]
#[must_use]
pub fn next_element<T>(n: usize, l: &[T]) -> &T { &l[next(n, l.len())] }

//=======================================================================//

/// Returns the number before `n` in the residue class described by `div`.
#[inline]
#[must_use]
pub fn prev(n: usize, div: usize) -> usize { prev_n_steps(n, 1, div) }

/// Returns the number `s` steps before `n` in the residue class described by `div`.
#[inline]
#[must_use]
pub fn prev_n_steps(n: usize, s: usize, div: usize) -> usize
{
    assert!(n < div, "n {n} is higher or equal to div {div}.");

    if n < s
    {
        n + div - s
    }
    else
    {
        n - s
    }
}

/// Returns the element of `l` at the index before `n`.
#[inline]
#[must_use]
pub fn prev_element<T>(n: usize, l: &[T]) -> &T { &l[prev(n, l.len())] }

/// Returns the element of `l` at the index `s` steps before before `n`.
#[inline]
#[must_use]
pub fn prev_element_n_steps<T>(n: usize, s: usize, l: &[T]) -> &T
{
    &l[prev_n_steps(n, s, l.len())]
}
