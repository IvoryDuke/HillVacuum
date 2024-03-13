//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{marker::PhantomData, str::FromStr};

use bevy_egui::egui;

use super::{minus_plus_buttons::MinusPlusButtons, ActuallyLostFocus, Interacting};
use crate::{
    map::editor::state::{clipboard::Clipboard, editor_state::InputsPresses},
    utils::{misc::ReplaceValues, overall_value::UiOverallValue}
};

//=======================================================================//
// TYPES
//
//=======================================================================//

#[must_use]
#[derive(Default, Clone, Copy)]
pub(in crate::map::editor::state) struct Response
{
    pub has_focus:     bool,
    pub interacting:   bool,
    pub value_changed: bool
}

impl std::ops::BitOrAssign for Response
{
    #[inline]
    fn bitor_assign(&mut self, rhs: Self)
    {
        self.has_focus |= rhs.has_focus;
        self.interacting |= rhs.interacting;
        self.value_changed |= rhs.value_changed;
    }
}

//=======================================================================//

#[must_use]
pub(in crate::map::editor::state) struct OverallValueField<T: ToString + FromStr>(PhantomData<T>);

impl<T: ToString + FromStr + PartialEq> OverallValueField<T>
{
    #[inline]
    fn singleline_textedit(buffer: &mut String) -> egui::TextEdit
    {
        egui::TextEdit::singleline(buffer).desired_width(f32::INFINITY)
    }

    #[inline]
    pub fn show<F: FnMut(T) -> Option<T>>(
        ui: &mut egui::Ui,
        clipboard: &mut Clipboard,
        inputs: &InputsPresses,
        value: &mut UiOverallValue<T>,
        enabled: bool,
        f: F
    ) -> Response
    {
        if value.is_none() || !enabled
        {
            ui.add_enabled(false, Self::singleline_textedit(value.buffer_mut()));
            return Response::default();
        }

        Self::show_always_enabled(ui, clipboard, inputs, value, f)
    }

    #[inline]
    pub fn show_always_enabled<F: FnMut(T) -> Option<T>>(
        ui: &mut egui::Ui,
        clipboard: &mut Clipboard,
        inputs: &InputsPresses,
        value: &mut UiOverallValue<T>,
        f: F
    ) -> Response
    {
        let output = Self::singleline_textedit(value.buffer_mut()).show(ui);
        let response = clipboard.copy_paste_text_editor(inputs, ui, value.buffer_mut(), output);
        let has_focus = response.has_focus();
        let lost_focus = response.actually_lost_focus();

        Response {
            has_focus,
            interacting: response.interacting(),
            value_changed: value.update(response.gained_focus(), lost_focus, f)
        }
    }
}

//=======================================================================//

#[must_use]
pub(in crate::map::editor::state) struct MinusPlusOverallValueField<T>
where
    T: ToString + FromStr + std::ops::Add<Output = T> + std::ops::Neg<Output = T> + Clone + Copy
{
    minus_plus: MinusPlusButtons,
    data:       PhantomData<T>
}

impl<T> MinusPlusOverallValueField<T>
where
    T: ToString
        + FromStr
        + std::ops::Add<Output = T>
        + std::ops::Neg<Output = T>
        + Clone
        + Copy
        + PartialEq
{
    #[inline]
    pub const fn new(min_size: egui::Vec2) -> Self
    {
        Self {
            minus_plus: MinusPlusButtons::new(min_size),
            data:       PhantomData
        }
    }

    #[inline]
    pub fn show<C: Fn(T, T) -> T, F: FnMut(T) -> Option<T>>(
        &mut self,
        strip: &mut egui_extras::Strip,
        clipboard: &mut Clipboard,
        inputs: &InputsPresses,
        value: &mut UiOverallValue<T>,
        step: T,
        clamp: C,
        mut f: F
    ) -> Response
    {
        let mut response = Response::default();

        strip.cell(|ui| {
            response = OverallValueField::show(ui, clipboard, inputs, value, true, |value| {
                f(clamp(value, step))
            });
        });

        if value.is_none()
        {
            strip.cell(|ui| {
                self.minus_plus.show(ui, false);
            });

            return Response::default();
        }

        strip.cell(|ui| {
            use crate::map::editor::state::ui::minus_plus_buttons::Response;

            let step = match self.minus_plus.show(ui, true)
            {
                Response::None => return,
                Response::PlusClicked => step,
                Response::MinusClicked => -step
            };

            if let Some(v) = value.uniform_value()
            {
                let v = clamp(*v + step, step);
                value.buffer_mut().replace_values(v.to_string().chars());
            }

            value.update(false, true, f);
        });

        response
    }
}
