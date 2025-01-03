//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use hill_vacuum_shared::{match_or_panic, NextValue};

use crate::{
    map::{
        drawer::drawing_resources::DrawingResources,
        editor::{
            state::{
                clipboard::Clipboard,
                grid::Grid,
                inputs_presses::InputsPresses,
                ui::{checkbox::CheckBox, overall_value_field::OverallValueField}
            },
            Placeholder
        },
        properties::{DefaultProperties, Properties, SetProperty}
    },
    utils::{
        collections::IndexMap,
        overall_value::{OverallValue, OverallValueInterface, OverallValueToUi, UiOverallValue}
    },
    Value
};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The info concerning an overall property of an entity.
struct OverallProperty
{
    /// The discriminant of the [`Value`].
    tag:   u8,
    /// The overall [`Value`].
    value: OverallValue<Value>,
    /// The UI representation of the overall [`Value`].
    ui:    UiOverallValue<Value>
}

//=======================================================================//

/// The UI elements to edit the overall [`Properties`].
#[must_use]
pub(in crate::map) struct UiOverallProperties(IndexMap<String, OverallProperty>);

impl Placeholder for UiOverallProperties
{
    #[inline]
    unsafe fn placeholder() -> Self { Self(IndexMap::default()) }
}

impl UiOverallProperties
{
    /// Returns a new [`UiOverallProperties`].
    #[inline]
    pub fn new<D: DefaultProperties>(values: &D) -> Self
    {
        Self(
            values
                .iter()
                .map(|(k, d_v)| {
                    let value = OverallValue::from(d_v.clone());
                    let ui = value.clone().ui();

                    (k.to_string(), OverallProperty {
                        tag: d_v.tag(),
                        value,
                        ui
                    })
                })
                .collect()
        )
    }

    /// The amount of overall properties.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize { self.0.len() }

    /// Overwrites all the overall properties.
    #[inline]
    pub fn total_overwrite<'a, P: Properties + 'a>(&mut self, mut iter: impl Iterator<Item = &'a P>)
    {
        {
            let properties = iter.next_value();
            assert!(properties.len() == self.len(), "Different lengths.");

            for (k, o) in &mut self.0
            {
                let b = properties.get(k);
                assert!(o.tag == b.tag(), "Mismatching discriminants.");
                o.value = b.clone().into();
            }
        }

        let mut uniform = false;

        for properties in iter
        {
            assert!(properties.len() == self.len(), "Different lengths.");

            for (k, o) in &mut self.0
            {
                let b = properties.get(k);
                assert!(o.tag == b.tag(), "Mismatching discriminants.");
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
    pub fn overwrite<'a, P: Properties + 'a>(
        &mut self,
        k: &str,
        mut iter: impl Iterator<Item = &'a P>
    )
    {
        {
            let properties = iter.next_value();
            assert!(properties.len() == self.len(), "Different lengths.");

            let o = self.0.get_mut(k).unwrap();
            let b = properties.get(k);
            assert!(o.tag == b.tag(), "Mismatching discriminants.");
            o.value = b.clone().into();
        }

        _ = iter.any(|properties| self.0.get_mut(k).unwrap().value.stack(properties.get(k)));

        let o = self.0.get_mut(k).unwrap();
        o.ui = o.value.clone().ui();
    }

    /// Shows the [`Properties`] fields.
    #[inline]
    pub fn show<D: DefaultProperties, S: SetProperty>(
        &mut self,
        ui: &mut egui::Ui,
        drawing_resources: &DrawingResources,
        default_properties: &D,
        clipboard: &mut Clipboard,
        inputs: &InputsPresses,
        grid: &Grid,
        value_setter: &mut S
    )
    {
        assert!(default_properties.len() == self.0.len(), "Different lengths.");

        for (k, o) in &mut self.0
        {
            let d_v = default_properties.get(k);
            assert!(o.tag == d_v.tag(), "Mismatching discriminants.");

            ui.label(k);
            ui.label(d_v.type_str());

            if Value::BOOL_TAG == o.tag
            {
                if let Some(value) =
                    CheckBox::show(ui, &o.value, |v| match_or_panic!(v, Value::Bool(value), *value))
                {
                    let mut value = Value::Bool(value);
                    value_setter.set_property(drawing_resources, grid, k, &mut value);
                    o.value = value.into();
                    o.ui = o.value.clone().ui();
                }
            }
            else
            {
                OverallValueField::show_always_enabled(
                    ui,
                    clipboard,
                    inputs,
                    &mut o.ui,
                    |new_value| {
                        let mut new_value = d_v.parse(&new_value)?;
                        value_setter.set_property(drawing_resources, grid, k, &mut new_value);
                        new_value.into()
                    }
                );
            }

            ui.end_row();
        }
    }
}
