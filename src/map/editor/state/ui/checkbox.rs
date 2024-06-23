//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::str::FromStr;

use bevy_egui::egui;

use crate::utils::overall_value::OverallValue;

//=======================================================================//
// TYPES
//
//=======================================================================//

/// A checkbox that returns whever the value was changed.
pub(in crate::map::editor) struct CheckBox;

impl CheckBox
{
    /// Shows a checkbox and returns whever it was toggled after being clicked.
    /// `f` determines how the boolean on/off is generated from `value`.
    #[inline]
    #[must_use]
    pub fn show<T, F>(ui: &mut egui::Ui, value: &OverallValue<T>, extractor: F) -> Option<bool>
    where
        T: Clone + ToString + FromStr + PartialEq,
        F: Fn(&T) -> bool
    {
        let checked = match value
        {
            OverallValue::None | OverallValue::NonUniform => false,
            OverallValue::Uniform(value) => extractor(value)
        };
        let mut new_checked = checked;

        (ui.add(egui::Checkbox::without_text(&mut new_checked)).clicked() && checked != new_checked)
            .then_some(new_checked)
    }
}
