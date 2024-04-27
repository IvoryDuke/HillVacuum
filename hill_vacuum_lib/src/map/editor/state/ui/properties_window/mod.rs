mod overall_properties;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use shared::{continue_if_none, TEXTURE_HEIGHT_RANGE};

use self::overall_properties::UiOverallProperties;
use super::{window::Window, WindowCloser, WindowCloserInfo};
use crate::{
    config::controls::bind::Bind,
    map::{
        brush::Brush,
        editor::{
            state::{
                clipboard::Clipboard,
                editor_state::InputsPresses,
                edits_history::EditsHistory,
                manager::EntitiesManager,
                ui::{checkbox::CheckBox, overall_value_field::OverallValueField}
            },
            AllDefaultProperties,
            StateUpdateBundle
        },
        properties::{DefaultProperties, SetProperty, Value},
        thing::{ThingInstance, ThingInterface}
    },
    utils::{
        identifiers::EntityId,
        misc::Toggle,
        overall_value::{OverallValue, OverallValueInterface, OverallValueToUi, UiOverallValue}
    }
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[derive(Default)]
enum Target
{
    #[default]
    None,
    Brushes,
    Things
}

//=======================================================================//
// TYPES
//
//=======================================================================//

struct Innards
{
    target:                     Target,
    overall_brushes_collision:  OverallValue<bool>,
    overall_brushes_properties: UiOverallProperties,
    overall_things_draw_height: UiOverallValue<i8>,
    overall_things_angle:       UiOverallValue<f32>,
    overall_things_properties:  UiOverallProperties,
    max_rows:                   usize,
    brushes_filler:             usize,
    things_filler:              usize
}

impl Innards
{
    #[inline]
    fn show(
        &mut self,
        ui: &mut egui::Ui,
        brushes_default_properties: &DefaultProperties,
        things_default_properties: &DefaultProperties,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        clipboard: &mut Clipboard,
        inputs: &InputsPresses
    ) -> bool
    {
        const COLUMN_WIDTH: f32 = 122f32;

        ui.horizontal(|ui| {
            ui.label("Entities");

            let any_brushes = manager.any_selected_brushes();
            let any_things = manager.any_selected_things();
            let brushes =
                ui.add_enabled(manager.any_selected_brushes(), egui::Button::new("Brushes"));
            let things = ui.add_enabled(manager.any_selected_things(), egui::Button::new("Things"));

            if brushes.clicked()
            {
                self.target = Target::Brushes;
            }
            else if things.clicked()
            {
                self.target = Target::Things;
            }

            match self.target
            {
                Target::None =>
                {
                    if any_brushes
                    {
                        self.target = Target::Brushes;
                    }
                    else if any_things
                    {
                        self.target = Target::Things;
                    }
                },
                Target::Brushes => _ = brushes.highlight(),
                Target::Things => _ = things.highlight()
            };
        });

        egui::Grid::new("properties")
            .num_columns(2)
            .spacing([0f32, 4f32])
            .striped(true)
            .min_col_width(COLUMN_WIDTH)
            .max_col_width(COLUMN_WIDTH)
            .min_row_height(22f32)
            .show(ui, |ui| {
                self.grid(
                    ui,
                    brushes_default_properties,
                    things_default_properties,
                    manager,
                    edits_history,
                    clipboard,
                    inputs
                )
            })
            .inner
    }

    #[inline]
    #[must_use]
    fn grid(
        &mut self,
        ui: &mut egui::Ui,
        brushes_default_properties: &DefaultProperties,
        things_default_properties: &DefaultProperties,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        clipboard: &mut Clipboard,
        inputs: &InputsPresses
    ) -> bool
    {
        macro_rules! set_property {
            ($self:ident, $key:ident, $value:ident, $entities:ident) => {
                $self.edits_history.property(
                    $key,
                    $self.manager.$entities().filter_map(|mut entity| {
                        entity.set_property($key, $value).map(|value| (entity.id(), value))
                    })
                );
            };
        }

        #[inline]
        fn filler(ui: &mut egui::Ui, length: usize)
        {
            for _ in 0..length
            {
                ui.label("");
                ui.label("");
                ui.end_row();
            }
        }

        struct BrushesPropertySetter<'a>
        {
            manager:       &'a mut EntitiesManager,
            edits_history: &'a mut EditsHistory
        }

        impl<'a> SetProperty for BrushesPropertySetter<'a>
        {
            #[inline]
            fn set_property(&mut self, key: &str, value: &Value)
            {
                set_property!(self, key, value, selected_brushes_mut);
            }
        }

        struct ThingsPropertySetter<'a>
        {
            manager:       &'a mut EntitiesManager,
            edits_history: &'a mut EditsHistory
        }

        impl<'a> SetProperty for ThingsPropertySetter<'a>
        {
            #[inline]
            fn set_property(&mut self, key: &str, value: &Value)
            {
                set_property!(self, key, value, selected_things_mut);
            }
        }

        match self.target
        {
            Target::None =>
            {
                filler(ui, self.max_rows);
                false
            },
            Target::Brushes =>
            {
                ui.label("Collision");

                if let Some(value) = CheckBox::show(ui, &self.overall_brushes_collision, |v| *v)
                {
                    for mut brush in manager.selected_brushes_mut()
                    {
                        edits_history
                            .collision(brush.id(), continue_if_none!(brush.set_collision(value)));
                    }

                    self.overall_brushes_collision = value.into();
                }

                ui.end_row();

                let focused = self.overall_brushes_properties.show(
                    ui,
                    &mut BrushesPropertySetter {
                        manager,
                        edits_history
                    },
                    clipboard,
                    inputs,
                    brushes_default_properties
                );

                filler(ui, self.brushes_filler);

                focused
            },
            Target::Things =>
            {
                macro_rules! angle_height {
                    ($label:literal, $value:ident, $min:expr, $max:expr) => {{
                        paste::paste! {
                            ui.label($label);

                            let focused = OverallValueField::show_always_enabled(
                                ui,
                                clipboard,
                                inputs,
                                &mut self.[< overall_things_ $value >],
                                |value| {
                                    let value = value.clamp($min, $max);

                                    edits_history.[< thing_ $value _cluster >](
                                        manager.selected_things_mut().filter_map(
                                            |mut thing| {
                                                thing
                                                    .[< set_ $value >](value)
                                                    .map(|value| (thing.id(), value))
                                            }
                                        )
                                    );

                                    value.into()
                                }
                            )
                            .has_focus;

                            ui.end_row();

                            focused
                        }
                    }};
                }

                let focused = angle_height!(
                    "Draw height",
                    draw_height,
                    *TEXTURE_HEIGHT_RANGE.start(),
                    *TEXTURE_HEIGHT_RANGE.end()
                ) | angle_height!("Angle", angle, 0f32, 359f32) |
                    self.overall_things_properties.show(
                        ui,
                        &mut ThingsPropertySetter {
                            manager,
                            edits_history
                        },
                        clipboard,
                        inputs,
                        things_default_properties
                    );

                filler(ui, self.things_filler);

                focused
            }
        }
    }
}

//=======================================================================//

#[must_use]
pub(in crate::map::editor::state::ui) struct PropertiesWindow
{
    window:  Window,
    innards: Innards
}

impl Toggle for PropertiesWindow
{
    #[inline]
    fn toggle(&mut self) { self.window.toggle(); }
}

impl WindowCloserInfo for PropertiesWindow
{
    #[inline]
    fn window_closer(&self) -> Option<WindowCloser>
    {
        #[inline]
        fn close(properties: &mut PropertiesWindow) { properties.window.close(); }

        self.window
            .layer_id()
            .map(|id| WindowCloser::Properties(id, close as fn(&mut Self)))
    }
}

impl PropertiesWindow
{
    #[inline]
    pub fn new(
        brushes_default_properties: &DefaultProperties,
        things_default_properties: &DefaultProperties
    ) -> Self
    {
        let b_len = brushes_default_properties.len() + 1;
        let t_len = things_default_properties.len() + 2;
        let max_rows = b_len.max(t_len).max(10);

        Self {
            window:  Window::default(),
            innards: Innards {
                target: Target::default(),
                overall_brushes_collision: true.into(),
                overall_brushes_properties: UiOverallProperties::from(brushes_default_properties),
                overall_things_draw_height: UiOverallValue::none(),
                overall_things_angle: UiOverallValue::none(),
                overall_things_properties: UiOverallProperties::from(things_default_properties),
                max_rows,
                brushes_filler: max_rows - b_len,
                things_filler: max_rows - t_len
            }
        }
    }

    #[inline]
    pub unsafe fn placeholder() -> Self
    {
        Self {
            window:  Window::default(),
            innards: Innards {
                target:                     Target::default(),
                overall_brushes_collision:  true.into(),
                overall_brushes_properties: UiOverallProperties::placeholder(),
                overall_things_draw_height: UiOverallValue::none(),
                overall_things_angle:       UiOverallValue::none(),
                overall_things_properties:  UiOverallProperties::placeholder(),
                max_rows:                   0,
                brushes_filler:             0,
                things_filler:              0
            }
        }
    }

    #[inline]
    pub fn update_overall_brushes_collision(&mut self, manager: &EntitiesManager)
    {
        if !manager.any_selected_brushes()
        {
            return;
        }

        self.innards.overall_brushes_collision = OverallValue::None;

        for brush in manager.selected_brushes()
        {
            if self.innards.overall_brushes_collision.stack(&brush.collision())
            {
                return;
            }
        }
    }

    #[inline]
    pub fn update_overall_total_brush_properties(&mut self, manager: &EntitiesManager)
    {
        if !manager.any_selected_brushes()
        {
            return;
        }

        self.innards
            .overall_brushes_properties
            .total_overwrite(manager.selected_brushes().map(Brush::properties_as_ref));
    }

    #[inline]
    pub fn update_overall_brushes_property(&mut self, manager: &EntitiesManager, k: &str)
    {
        if !manager.any_selected_brushes()
        {
            return;
        }

        self.innards
            .overall_brushes_properties
            .overwrite(k, manager.selected_brushes().map(Brush::properties_as_ref));
    }

    #[inline]
    pub fn update_overall_things_info(&mut self, manager: &EntitiesManager)
    {
        if !manager.any_selected_things()
        {
            return;
        }

        let mut draw_height = OverallValue::None;
        let mut angle = OverallValue::None;

        for thing in manager.selected_things()
        {
            let non_uni = draw_height.stack(&thing.draw_height());

            if angle.stack(&thing.angle()) && non_uni
            {
                break;
            }
        }

        self.innards.overall_things_draw_height = draw_height.ui();
        self.innards.overall_things_angle = angle.ui();
    }

    #[inline]
    pub fn update_overall_total_things_properties(&mut self, manager: &EntitiesManager)
    {
        if !manager.any_selected_things()
        {
            return;
        }

        self.innards
            .overall_things_properties
            .total_overwrite(manager.selected_things().map(ThingInstance::properties));
    }

    #[inline]
    pub fn update_overall_things_property(&mut self, manager: &EntitiesManager, k: &str)
    {
        if !manager.any_selected_things()
        {
            return;
        }

        self.innards
            .overall_things_properties
            .overwrite(k, manager.selected_things().map(ThingInstance::properties));
    }

    #[inline]
    #[must_use]
    pub fn show(
        &mut self,
        bundle: &mut StateUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        clipboard: &mut Clipboard,
        inputs: &InputsPresses
    ) -> bool
    {
        if !inputs.ctrl_pressed() &&
            Bind::PropertiesEditor.just_pressed(bundle.key_inputs, &bundle.config.binds)
        {
            self.window.open();
        }

        let any_sel_brushes = manager.any_selected_brushes();
        let any_sel_things = manager.any_selected_things();

        match self.innards.target
        {
            Target::Brushes if !any_sel_brushes => self.innards.target = Target::default(),
            Target::Things if !any_sel_things => self.innards.target = Target::default(),
            _ => ()
        };

        if !self.window.is_open()
        {
            return false;
        }

        let StateUpdateBundle {
            egui_context,
            default_properties:
                AllDefaultProperties {
                    map_brushes: map_brushes_default_properties,
                    map_things: map_things_default_properties,
                    ..
                },
            ..
        } = bundle;

        let Self { window, innards } = self;

        window
            .show(
                egui_context,
                egui::Window::new("Properties")
                    .vscroll(true)
                    .collapsible(true)
                    .resizable(true),
                |ui| {
                    innards.show(
                        ui,
                        map_brushes_default_properties,
                        map_things_default_properties,
                        manager,
                        edits_history,
                        clipboard,
                        inputs
                    )
                }
            )
            .unwrap()
    }
}
