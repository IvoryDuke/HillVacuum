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

/// The response of the UI element.
#[must_use]
#[derive(Default, Clone, Copy)]
pub(in crate::map::editor::state) struct Response
{
    /// Whever the UI element has focus.
    pub has_focus:     bool,
    /// Whever the UI element is being interacted with.
    pub interacting:   bool,
    /// Whever the value was changed.
    pub value_changed: bool
}

impl std::ops::BitOr for Response
{
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self::Output
    {
        Self {
            has_focus:     self.has_focus | rhs.has_focus,
            interacting:   self.interacting | rhs.interacting,
            value_changed: self.value_changed | rhs.value_changed
        }
    }
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

/// A field where to edit an overall value and update the value itself in the entities.
#[must_use]
pub(in crate::map) struct OverallValueField<T: ToString + FromStr>(PhantomData<T>);

impl<T: ToString + FromStr + PartialEq> OverallValueField<T>
{
    /// The text editor of the value.
    #[inline]
    fn singleline_textedit(buffer: &mut String) -> egui::TextEdit
    {
        egui::TextEdit::singleline(buffer).desired_width(f32::INFINITY)
    }

    /// Shows the [`OverallValueField`] enabled depending on the `enabled` parameter.
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

    /// Always shows the [`OverallValueField`] enabled.
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
        let lost_focus = response.actually_lost_focus();

        Response {
            has_focus:     response.has_focus() || lost_focus,
            interacting:   response.interacting(),
            value_changed: value.update(response.gained_focus(), lost_focus, f)
        }
    }
}

//=======================================================================//

/// An [`OverallValueField`] combined with a minus and plus buttons.
#[must_use]
pub(in crate::map::editor::state) struct MinusPlusOverallValueField<T>
where
    T: ToString + FromStr + std::ops::Add<Output = T> + std::ops::Neg<Output = T> + Clone + Copy
{
    /// The minus and plus buttons.
    minus_plus: MinusPlusButtons,
    /// Phantom data.
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
    /// Returns a new [`MinusPlusOverallValueField`].
    #[inline]
    pub const fn new(min_size: egui::Vec2) -> Self
    {
        Self {
            minus_plus: MinusPlusButtons::new(min_size),
            data:       PhantomData
        }
    }

    /// Shows the [`OverallValueField`] and the [`MinusPlusButtons`].
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
