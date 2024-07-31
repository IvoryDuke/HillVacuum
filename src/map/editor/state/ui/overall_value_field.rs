//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{marker::PhantomData, str::FromStr};

use bevy_egui::egui;
use hill_vacuum_shared::return_if_none;

use super::{
    minus_plus_buttons::MinusPlusButtons,
    singleline_textedit,
    ActuallyLostFocus,
    Interacting
};
use crate::{
    map::editor::state::{clipboard::Clipboard, editor_state::InputsPresses},
    utils::{misc::ReplaceValues, overall_value::UiOverallValue}
};

//=======================================================================//
// TYPES
//
//=======================================================================//

/// A field where to edit an overall value and update the value itself in the entities.
#[must_use]
pub(in crate::map) struct OverallValueField<T: ToString + FromStr>(PhantomData<T>);

impl<T: ToString + FromStr + PartialEq> OverallValueField<T>
{
    /// Shows the [`OverallValueField`] enabled depending on the `enabled` parameter.
    #[inline]
    pub fn show<F: FnOnce(T) -> Option<T>>(
        ui: &mut egui::Ui,
        clipboard: &mut Clipboard,
        inputs: &InputsPresses,
        value: &mut UiOverallValue<T>,
        enabled: bool,
        f: F
    ) -> bool
    {
        if value.is_none() || !enabled
        {
            ui.add_enabled(false, singleline_textedit(value.buffer_mut(), f32::INFINITY));
            return false;
        }

        Self::show_always_enabled(ui, clipboard, inputs, value, f)
    }

    /// Always shows the [`OverallValueField`] enabled.
    #[inline]
    pub fn show_always_enabled<F: FnOnce(T) -> Option<T>>(
        ui: &mut egui::Ui,
        clipboard: &mut Clipboard,
        inputs: &InputsPresses,
        value: &mut UiOverallValue<T>,
        f: F
    ) -> bool
    {
        let response =
            clipboard.copy_paste_text_editor(inputs, ui, value.buffer_mut(), f32::INFINITY);
        value.update(response.gained_focus(), response.actually_lost_focus(), f);
        response.interacting()
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
    ) -> bool
    {
        let mut interacting = false;

        strip.cell(|ui| {
            interacting = OverallValueField::show(ui, clipboard, inputs, value, true, |value| {
                f(clamp(value, step))
            });
        });

        if value.is_none()
        {
            strip.cell(|ui| {
                self.minus_plus.show(ui, false);
            });

            return false;
        }

        strip.cell(|ui| {
            let step = return_if_none!(self.minus_plus.show(ui, true).step(step));

            if let Some(v) = value.uniform_value()
            {
                let v = clamp(*v + step, step);
                value.buffer_mut().replace_values(v.to_string().chars());
            }

            value.update(false, true, f);
        });

        interacting
    }
}
