//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use shared::return_if_none;

use crate::{
    map::{
        editor::state::{
            clipboard::Clipboard,
            editor_state::InputsPresses,
            edits_history::EditsHistory,
            manager::EntitiesManager,
            ui::overall_value_field::{OverallValueField, Response}
        },
        path::overall_values::{OverallMovement, UiOverallMovement}
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

macro_rules! movement_values {
    ($(($value:ident, $label:literal, $clamp:expr, $interacting:literal $(, $opposite:ident)?)),+) => { paste::paste! { $(
        #[inline]
        fn [< set_ $value >](
            manager: &mut EntitiesManager,
            edits_history: &mut EditsHistory,
            new_value: f32,
            overall: &mut OverallMovement
        ) -> f32
        {
            let new_value = ($clamp)(new_value);

            edits_history.[< path_nodes_ $value _cluster >](manager.selected_movings_mut().filter_map(|mut entity| {
                entity.[< set_selected_path_nodes_ $value >](new_value).map(|edit| {
                    _ = overall.merge(entity.overall_selected_path_nodes_movement());
                    (entity.id(), edit)
                })
            }));

            new_value
        }


        #[inline]
        #[must_use]
        fn $value(
            &mut self,
            manager: &mut EntitiesManager,
            edits_history: &mut EditsHistory,
            clipboard: &mut Clipboard,
            inputs: &InputsPresses,
            ui: &mut egui::Ui,
            simulation_active: bool,
        ) -> bool
        {
            let mut overall = OverallMovement::new();

            let response = Self::textedit(
                ui,
                &mut self.selected_nodes_movement.$value,
                clipboard,
                inputs,
                $label,
                simulation_active,
                |new_value| {
                    Self::[< set_ $value >](manager, edits_history, new_value, &mut overall).into()
                }
            );

            $(
                if overall.is_some()
                {
                    self.selected_nodes_movement.$opposite = overall.$opposite.ui();
                }
            )?

            self.interacting[$interacting] = response.interacting;
            response.has_focus
        }
    )+}};
}

//=======================================================================//
// TYPES
//
//=======================================================================//

#[derive(Debug, Default)]
pub(in crate::map::editor::state::core) struct NodesEditor
{
    selected_nodes_movement: UiOverallMovement,
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

    #[inline]
    #[must_use]
    pub fn interacting(&self) -> bool { self.interacting.iter().any(|b| *b) }

    #[inline]
    fn textedit<F: FnMut(f32) -> Option<f32>>(
        ui: &mut egui::Ui,
        value: &mut UiOverallValue<f32>,
        clipboard: &mut Clipboard,
        inputs: &InputsPresses,
        label: &str,
        simulation_active: bool,
        f: F
    ) -> Response
    {
        ui.label(label);
        let response = OverallValueField::show(ui, clipboard, inputs, value, !simulation_active, f);
        ui.end_row();
        response
    }

    #[inline]
    #[must_use]
    pub fn update(
        &mut self,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        clipboard: &mut Clipboard,
        inputs: &InputsPresses,
        ui: &mut egui::Ui,
        simulation_active: bool
    ) -> bool
    {
        self.interacting = [false; 5];
        ui.label(egui::RichText::new("PLATFORM TOOL"));

        egui::Grid::new("nodes_editor")
            .num_columns(2)
            .spacing([10f32, 4f32])
            .striped(true)
            .show(ui, |ui| {
                let mut focused = self.standby_time(
                    manager,
                    edits_history,
                    clipboard,
                    inputs,
                    ui,
                    simulation_active
                );
                focused |= self.max_speed(
                    manager,
                    edits_history,
                    clipboard,
                    inputs,
                    ui,
                    simulation_active
                );
                focused |= self.min_speed(
                    manager,
                    edits_history,
                    clipboard,
                    inputs,
                    ui,
                    simulation_active
                );
                focused |= self.accel_travel_percentage(
                    manager,
                    edits_history,
                    clipboard,
                    inputs,
                    ui,
                    simulation_active
                );
                focused |= self.decel_travel_percentage(
                    manager,
                    edits_history,
                    clipboard,
                    inputs,
                    ui,
                    simulation_active
                );
                focused
            })
            .inner
    }

    #[inline]
    pub fn update_overall_node(&mut self, manager: &EntitiesManager)
    {
        let mut overall = OverallMovement::new();

        for brush in manager.selected_moving()
        {
            if overall.merge(brush.overall_selected_path_nodes_movement())
            {
                break;
            }
        }

        self.selected_nodes_movement = overall.ui();
    }

    #[inline]
    pub fn force_simulation(
        &mut self,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory
    )
    {
        type ValueSetPair<'a> = (
            &'a mut UiOverallValue<f32>,
            fn(&mut EntitiesManager, &mut EditsHistory, f32, &mut OverallMovement) -> f32
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
            func(manager, edits_history, value, &mut OverallMovement::new()).into()
        });
        *i = false;
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

#[inline]
#[must_use]
fn one_clamp(speed: f32) -> f32 { speed.max(1f32) }

//=======================================================================//

#[inline]
#[must_use]
fn zero_clamp(speed: f32) -> f32 { speed.max(0f32) }

//=======================================================================//

#[inline]
#[must_use]
fn travel_percentage_clamp(value: f32) -> f32 { value.clamp(0f32, 100f32) }
