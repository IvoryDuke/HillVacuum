//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;

//=======================================================================//
// ENUMS
//
//=======================================================================//

pub(in crate::map::editor::state) enum Response
{
    None,
    PlusClicked,
    MinusClicked
}

//=======================================================================//
// TYPES
//
//=======================================================================//

#[must_use]
struct CoupledButtons(egui::Vec2);

impl CoupledButtons
{
    #[inline]
    pub const fn new(min_size: egui::Vec2) -> Self { Self(min_size) }

    #[inline]
    fn button(&self, text: &str) -> egui::Button
    {
        egui::Button::new(text).small().min_size(self.0)
    }

    #[inline]
    pub fn show(&self, ui: &mut egui::Ui, enabled: bool, strs: [&str; 2]) -> Response
    {
        let mut response = Response::None;

        ui.horizontal(|ui| {
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

#[must_use]
pub(in crate::map::editor::state) struct MinusPlusButtons(CoupledButtons);

impl MinusPlusButtons
{
    #[inline]
    pub const fn new(min_size: egui::Vec2) -> Self { Self(CoupledButtons::new(min_size)) }

    #[inline]
    pub fn show(&self, ui: &mut egui::Ui, enabled: bool) -> Response
    {
        self.0.show(ui, enabled, ["-", "+"])
    }
}

//=======================================================================//

#[must_use]
pub(in crate::map::editor::state) struct DownUpButtons(CoupledButtons);

impl DownUpButtons
{
    #[inline]
    pub const fn new(min_size: egui::Vec2) -> Self { Self(CoupledButtons::new(min_size)) }

    #[inline]
    pub fn show(&self, ui: &mut egui::Ui, enabled: bool) -> Response
    {
        self.0.show(ui, enabled, ["-", "+"])
    }
}
