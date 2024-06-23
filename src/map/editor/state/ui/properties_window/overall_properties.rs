//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::mem::Discriminant;

use bevy_egui::egui;
use hill_vacuum_shared::{match_or_panic, NextValue};

use crate::{
    map::{
        containers::hv_vec,
        editor::{
            state::{
                clipboard::Clipboard,
                editor_state::InputsPresses,
                ui::{checkbox::CheckBox, overall_value_field::OverallValueField}
            },
            Placeholder
        },
        indexed_map::IndexedMap,
        properties::{DefaultProperties, Properties, SetProperty, Value}
    },
    utils::overall_value::{OverallValue, OverallValueInterface, OverallValueToUi, UiOverallValue}
};

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The info concerning an overall property of an entity.
struct OverallProperty
{
    /// The discriminant of the [`Value`].
    d:     Discriminant<Value>,
    /// The overall [`Value`].
    value: OverallValue<Value>,
    /// The UI representation of the overall [`Value`].
    ui:    UiOverallValue<Value>
}

//=======================================================================//

/// The UI elements to edit the overall [`Properties`].
#[must_use]
pub(in crate::map) struct UiOverallProperties(IndexedMap<String, OverallProperty>);

impl Placeholder for UiOverallProperties
{
    #[inline]
    unsafe fn placeholder() -> Self { Self(IndexedMap::default()) }
}

impl From<&DefaultProperties> for UiOverallProperties
{
    #[inline]
    fn from(value: &DefaultProperties) -> Self { Self::new(value) }
}

impl UiOverallProperties
{
    /// Returns a new [`UiOverallProperties`].
    #[inline]
    pub fn new(values: &DefaultProperties) -> Self
    {
        let mut vec = hv_vec![capacity; values.len()];

        for (k, d_v) in values.iter()
        {
            let d = std::mem::discriminant(d_v);
            vec.push((k.clone(), (d, OverallValue::from(d_v.clone()))));
        }

        vec.sort_by(|(a, _), (b, _)| a.cmp(b));

        let mut values = hv_vec![capacity; vec.len()];
        let mut keys = hv_vec![capacity; vec.len()];

        for (k, (d, value)) in vec
        {
            keys.push(k);

            let ui = value.clone().ui();
            values.push(OverallProperty { d, value, ui });
        }

        let mut keys = keys.into_iter();
        Self(IndexedMap::new(values, |_| keys.next_value()))
    }

    /// The amount of overall properties.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize { self.0.len() }

    /// Overwrites all the overall properties.
    #[inline]
    pub fn total_overwrite<'a>(&mut self, mut iter: impl Iterator<Item = &'a Properties>)
    {
        {
            let properties = iter.next_value();
            assert!(properties.len() == self.len(), "Different lengths.");

            for (k, o) in self.0.iter_mut()
            {
                let b = properties.get(k);
                assert!(o.d == std::mem::discriminant(b), "Mismatching discriminants.");
                o.value = b.clone().into();
            }
        }

        let mut uniform = false;

        for properties in iter
        {
            assert!(properties.len() == self.len(), "Different lengths.");

            for (k, o) in self.0.iter_mut()
            {
                let b = properties.get(k);
                assert!(o.d == std::mem::discriminant(b), "Mismatching discriminants.");
                uniform |= !o.value.stack(b);
            }

            if !uniform
            {
                break;
            }
        }

        for o in self.0.values_mut()
        {
            o.ui = o.value.clone().ui();
        }
    }

    /// Overwrite the overall property with key `k`.
    #[inline]
    pub fn overwrite<'a>(&mut self, k: &str, mut iter: impl Iterator<Item = &'a Properties>)
    {
        {
            let properties = iter.next_value();
            assert!(properties.len() == self.len(), "Different lengths.");

            let o = self.0.get_mut(k).unwrap();
            let b = properties.get(k);
            assert!(o.d == std::mem::discriminant(b), "Mismatching discriminants.");
            o.value = b.clone().into();
        }

        _ = iter.any(|properties| self.0.get_mut(k).unwrap().value.stack(properties.get(k)));

        let o = self.0.get_mut(k).unwrap();
        o.ui = o.value.clone().ui();
    }

    /// Shows the [`Properties`] fields.
    #[inline]
    #[must_use]
    pub fn show<S: SetProperty>(
        &mut self,
        ui: &mut egui::Ui,
        value_setter: &mut S,
        clipboard: &mut Clipboard,
        inputs: &InputsPresses,
        default_properties: &DefaultProperties
    ) -> bool
    {
        assert!(default_properties.len() == self.0.len(), "Different lengths.");

        let mut focused = false;

        for (k, o) in self.0.iter_mut()
        {
            let d_v = default_properties.get(k);
            assert!(o.d == std::mem::discriminant(d_v), "Mismatching discriminants.");

            ui.label(k);

            if Value::BOOL_DISCRIMINANT == o.d
            {
                if let Some(value) =
                    CheckBox::show(ui, &o.value, |v| match_or_panic!(v, Value::Bool(value), *value))
                {
                    let value = Value::Bool(value);
                    value_setter.set_property(k, &value);
                    o.value = value.into();
                    o.ui = o.value.clone().ui();
                }
            }
            else
            {
                focused |= OverallValueField::show_always_enabled(
                    ui,
                    clipboard,
                    inputs,
                    &mut o.ui,
                    |new_value| {
                        let new_value = d_v.parse(&new_value)?;
                        value_setter.set_property(k, &new_value);
                        new_value.into()
                    }
                )
                .has_focus;
            }

            ui.end_row();
        }

        focused
    }
}
