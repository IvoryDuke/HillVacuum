//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The result of the interaction with a [`MinusPlusButtons`] or [`DownUpButtons`].
pub(in crate::map::editor::state) enum Response
{
    /// No clicks.
    None,
    /// "Plus" clicked.
    PlusClicked,
    /// "Minus" clicked.
    MinusClicked
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// A couple of buttons with same minimum size.
#[must_use]
struct CoupledButtons(egui::Vec2);

impl CoupledButtons
{
    /// Returns a new [`CoupledButtons`] with the specified minimum size.
    #[inline]
    pub const fn new(min_size: egui::Vec2) -> Self { Self(min_size) }

    /// Shows a button with the stored size.
    #[inline]
    fn button(&self, text: &str) -> egui::Button
    {
        egui::Button::new(text).small().min_size(self.0)
    }

    /// Shows a couple of buttons side by side.
    #[inline]
    pub fn show(&self, ui: &mut egui::Ui, enabled: bool, strs: [&str; 2]) -> Response
    {
        let mut response = Response::None;

        ui.horizontal(|ui| {
            /// Shows the button with `str` string and sets the response to `clicked`.
            macro_rules! buttons {
                ($(($str:expr, $clicked:ident)),+) => { $(
                    if ui.add_enabled(enabled, self.button($str)).clicked()
                    {
                        response = Response::$clicked;
                    }
                )+}
            }

            buttons!((strs[0], MinusClicked), (strs[1], PlusClicked));
        });

        response
    }
}

//=======================================================================//

/// Minus and plus UI buttons.
#[must_use]
pub(in crate::map::editor::state) struct MinusPlusButtons(CoupledButtons);

impl MinusPlusButtons
{
    /// Returns a new [`MinusPlusButtons`].
    #[inline]
    pub const fn new(min_size: egui::Vec2) -> Self { Self(CoupledButtons::new(min_size)) }

    /// Shows the minus and plus buttons.
    #[inline]
    pub fn show(&self, ui: &mut egui::Ui, enabled: bool) -> Response
    {
        self.0.show(ui, enabled, ["-", "+"])
    }
}

//=======================================================================//

/// Down and up UI buttons.
#[must_use]
pub(in crate::map::editor::state) struct DownUpButtons(CoupledButtons);

impl DownUpButtons
{
    /// Returns a new [`DownUpButtons`].
    #[inline]
    pub const fn new(min_size: egui::Vec2) -> Self { Self(CoupledButtons::new(min_size)) }

    /// Shows the down and up buttons.
    #[inline]
    pub fn show(&self, ui: &mut egui::Ui, enabled: bool) -> Response
    {
        self.0.show(ui, enabled, ["-", "+"])
    }
}
