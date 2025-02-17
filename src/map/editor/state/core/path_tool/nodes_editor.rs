//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use hill_vacuum_shared::return_if_none;

use crate::{
    map::{
        drawer::drawing_resources::DrawingResources,
        editor::state::{
            clipboard::Clipboard,
            edits_history::EditsHistory,
            grid::Grid,
            inputs_presses::InputsPresses,
            manager::EntitiesManager,
            ui::{overall_value_field::OverallValueField, UiBundle}
        },
        path::overall_values::{OverallMovement, UiOverallMovement},
        thing::catalog::ThingsCatalog
    },
    utils::{
        identifiers::EntityId,
        overall_value::{OverallValueInterface, OverallValueToUi, UiOverallValue}
    }
};

//=======================================================================//
// MACROS
//
//=======================================================================//

/// Generates code of the UI elements to edit a [`Node`] [`Movement`] value.
macro_rules! movement_values {
    ($(($value:ident, $label:literal, $clamp:expr, $interacting:literal $(, $opposite:ident)?)),+) => { paste::paste! { $(
        #[inline]
        fn [< set_ $value >](
            drawing_resources: &DrawingResources,
            things_catalog: &ThingsCatalog,
            manager: &mut EntitiesManager,
            edits_history: &mut EditsHistory,
            grid: &Grid,
            new_value: f32,
            overall: &mut OverallMovement
        ) -> f32
        {
            let new_value = ($clamp)(new_value);

            edits_history.[< path_nodes_ $value _cluster >](manager.selected_movings_mut(drawing_resources, things_catalog, grid).filter_map(|mut entity| {
                entity.[< set_selected_path_nodes_ $value >](new_value).map(|edit| {
                    _ = overall.merge(entity.overall_selected_path_nodes_movement());
                    (entity.id(), edit)
                })
            }));

            new_value
        }

        #[inline]
        fn $value(
            &mut self,
            ui: &mut egui::Ui,
            bundle: &mut UiBundle,
            simulation_active: bool
        )
        {
            let mut overall = OverallMovement::new();

            let interacting = Self::textedit(
                ui,
                &mut self.selected_nodes_movement.$value,
                bundle.clipboard,
                bundle.inputs,
                $label,
                simulation_active,
                |new_value| {
                    Self::[< set_ $value >](
                        bundle.drawing_resources,
                        bundle.things_catalog,
                        bundle.manager,
                        bundle.edits_history,
                        bundle.grid,
                        new_value,
                        &mut overall
                    ).into()
                }
            );

            $(
                if overall.is_some()
                {
                    self.selected_nodes_movement.$opposite = overall.$opposite.ui();
                }
            )?

            self.interacting[$interacting] = interacting;
        }
    )+}};
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The [`Node`]s editor.
#[derive(Default)]
pub(in crate::map::editor::state::core) struct NodesEditor
{
    /// The overall [`Node`]s movement parameters.
    selected_nodes_movement: UiOverallMovement,
    /// The index of the UI element being interacted with.
    interacting:             [bool; 5]
}

impl NodesEditor
{
    movement_values!(
        (standby_time, "Standby", zero_clamp, 0),
        (max_speed, "Max speed", one_clamp, 1, min_speed),
        (min_speed, "Min speed", zero_clamp, 2, max_speed),
        (
            accel_travel_percentage,
            "Accel (%)",
            travel_percentage_clamp,
            3,
            decel_travel_percentage
        ),
        (
            decel_travel_percentage,
            "Decel (%)",
            travel_percentage_clamp,
            4,
            accel_travel_percentage
        )
    );

    /// Whether an UI element is being interacted with.
    #[inline]
    #[must_use]
    pub fn interacting(&self) -> bool { self.interacting.iter().any(|b| *b) }

    /// The textedit widget. Returns its [`egui::Response`].
    #[inline]
    fn textedit<F: FnOnce(f32) -> Option<f32>>(
        ui: &mut egui::Ui,
        value: &mut UiOverallValue<f32>,
        clipboard: &mut Clipboard,
        inputs: &InputsPresses,
        label: &str,
        simulation_active: bool,
        f: F
    ) -> bool
    {
        ui.label(label);
        let interacting =
            OverallValueField::show(ui, clipboard, inputs, value, !simulation_active, f);
        ui.end_row();
        interacting
    }

    /// Shows the UI elements.
    #[inline]
    pub fn show(&mut self, ui: &mut egui::Ui, bundle: &mut UiBundle, simulation_active: bool)
    {
        self.interacting = [false; 5];
        ui.label(egui::RichText::new("PLATFORM TOOL"));

        egui::Grid::new("nodes_editor")
            .num_columns(2)
            .spacing([10f32, 4f32])
            .striped(true)
            .show(ui, |ui| {
                self.standby_time(ui, bundle, simulation_active);
                self.max_speed(ui, bundle, simulation_active);
                self.min_speed(ui, bundle, simulation_active);
                self.accel_travel_percentage(ui, bundle, simulation_active);
                self.decel_travel_percentage(ui, bundle, simulation_active);
            });
    }

    /// Updates the overall [`Node`]s info.
    #[inline]
    pub fn update_overall_node(&mut self, manager: &EntitiesManager)
    {
        let mut overall = OverallMovement::new();

        for moving in manager.selected_moving()
        {
            if overall.merge(moving.overall_selected_path_nodes_movement())
            {
                break;
            }
        }

        self.selected_nodes_movement = overall.ui();
    }

    /// Forces the start of a movement simulation, updating the WIP value being edited if possible
    /// to parse.
    #[inline]
    pub fn force_simulation(&mut self, bundle: &mut UiBundle)
    {
        #[allow(clippy::missing_docs_in_private_items)]
        type ValueSetPair<'a> = (
            &'a mut UiOverallValue<f32>,
            fn(
                &DrawingResources,
                &ThingsCatalog,
                &mut EntitiesManager,
                &mut EditsHistory,
                &Grid,
                f32,
                &mut OverallMovement
            ) -> f32
        );

        let set_array: [ValueSetPair; 5] = [
            (&mut self.selected_nodes_movement.standby_time, Self::set_standby_time),
            (&mut self.selected_nodes_movement.max_speed, Self::set_max_speed),
            (&mut self.selected_nodes_movement.min_speed, Self::set_min_speed),
            (
                &mut self.selected_nodes_movement.accel_travel_percentage,
                Self::set_accel_travel_percentage
            ),
            (
                &mut self.selected_nodes_movement.decel_travel_percentage,
                Self::set_decel_travel_percentage
            )
        ];

        let (i, (value, func)) =
            return_if_none!(self.interacting.iter_mut().zip(set_array).find(|(i, _)| **i));

        value.update(false, true, |value| {
            func(
                bundle.drawing_resources,
                bundle.things_catalog,
                bundle.manager,
                bundle.edits_history,
                bundle.grid,
                value,
                &mut OverallMovement::new()
            )
            .into()
        });
        *i = false;
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Returns the maximum value between `speed` and 1.
#[inline]
#[must_use]
fn one_clamp(speed: f32) -> f32 { speed.max(1f32) }

//=======================================================================//

/// Returns the maximum value between `speed` and 0.
#[inline]
#[must_use]
fn zero_clamp(speed: f32) -> f32 { speed.max(0f32) }

//=======================================================================//

/// Clamps `value` to the 0..100 range.
#[inline]
#[must_use]
fn travel_percentage_clamp(value: f32) -> f32 { value.clamp(0f32, 100f32) }
