mod overall_properties;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use hill_vacuum_shared::{match_or_panic, TEXTURE_HEIGHT_RANGE};

use self::overall_properties::UiOverallProperties;
use super::{window::Window, UiBundle, WindowCloser, WindowCloserInfo};
use crate::{
    config::controls::bind::Bind,
    map::{
        brush::Brush,
        drawer::drawing_resources::DrawingResources,
        editor::{
            state::{edits_history::EditsHistory, grid::Grid, manager::EntitiesManager},
            Placeholder
        },
        properties::{
            DefaultBrushProperties,
            DefaultProperties,
            DefaultThingProperties,
            SetProperty,
            ANGLE_LABEL,
            HEIGHT_LABEL
        },
        thing::{catalog::ThingsCatalog, ThingInstance}
    },
    utils::{
        identifiers::EntityId,
        misc::Toggle,
        overall_value::{OverallValue, OverallValueInterface, OverallValueToUi, UiOverallValue}
    },
    Value
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The properties to edit.
#[derive(Default)]
enum Target
{
    /// None.
    #[default]
    None,
    /// The properties of the brushes.
    Brushes,
    /// The properties of the [`ThingInstance`]s.
    Things
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The core of the editor.
struct Innards
{
    /// The properties to edit.
    target:                     Target,
    /// The overall brushes properties.
    overall_brushes_properties: UiOverallProperties,
    /// The overall draw height of the [`ThingInstance`]s.
    overall_things_draw_height: UiOverallValue<i8>,
    /// The overall angle of the [`ThingInstance`]s.
    overall_things_angle:       UiOverallValue<i16>,
    /// The overall [`ThingInstance`]s properties.
    overall_things_properties:  UiOverallProperties,
    /// The maximum amount of rows of the grid.
    max_rows:                   usize,
    /// The filler rows of the brushes grid.
    brushes_filler:             usize,
    /// The filler rows of the [`ThingInstance`]s grid.
    things_filler:              usize
}

impl Innards
{
    /// Shows the properties editor.
    #[inline]
    fn show(&mut self, ui: &mut egui::Ui, bundle: &mut UiBundle)
    {
        /// The height of the rows of the grid.
        const ROW_HEIGHT: f32 = 22f32;
        const COLUMNS: usize = 3;

        ui.horizontal(|ui| {
            ui.label("Entities");

            let any_brushes = bundle.manager.any_selected_brushes();
            let any_things = bundle.manager.any_selected_things();
            let brushes = ui.add_enabled(any_brushes, egui::Button::new("Brushes"));
            let things = ui.add_enabled(any_things, egui::Button::new("Things"));

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

        #[allow(clippy::cast_precision_loss)]
        egui::Grid::new("properties")
            .num_columns(COLUMNS)
            .spacing([0f32, 4f32])
            .striped(true)
            .min_col_width(ui.available_width() / COLUMNS as f32)
            .min_row_height(ROW_HEIGHT)
            .show(ui, |ui| {
                self.grid(ui, bundle);
            });
    }

    /// The grid of the properties.
    #[inline]
    fn grid(&mut self, ui: &mut egui::Ui, bundle: &mut UiBundle)
    {
        /// Fills the grid in case of few or unuven properties.
        #[inline]
        fn filler(ui: &mut egui::Ui, length: usize)
        {
            for _ in 0..length
            {
                ui.label("");
                ui.label("");
                ui.label("");
                ui.end_row();
            }
        }

        /// The struct that updates the properties of the brushes.
        #[allow(clippy::missing_docs_in_private_items)]
        struct BrushesPropertySetter<'a>
        {
            manager:       &'a mut EntitiesManager,
            edits_history: &'a mut EditsHistory
        }

        impl SetProperty for BrushesPropertySetter<'_>
        {
            #[inline]
            fn set_property(
                &mut self,
                drawing_resources: &DrawingResources,
                grid: &Grid,
                key: &str,
                value: &mut Value
            )
            {
                self.edits_history.property(
                    key,
                    self.manager.selected_brushes_mut(drawing_resources, grid).filter_map(
                        |mut brush| brush.set_property(key, value).map(|value| (brush.id(), value))
                    )
                );
            }
        }

        /// The struct that updates the properties of the [`ThingInstance`]s.
        #[allow(clippy::missing_docs_in_private_items)]
        struct ThingsPropertySetter<'a>
        {
            things_catalog: &'a ThingsCatalog,
            manager:        &'a mut EntitiesManager,
            edits_history:  &'a mut EditsHistory
        }

        impl SetProperty for ThingsPropertySetter<'_>
        {
            #[inline]
            fn set_property(&mut self, _: &DrawingResources, _: &Grid, key: &str, value: &mut Value)
            {
                macro_rules! set {
                    ($($ty:ident, $min:expr, $max:expr)?) => {{
                        $(*value = Value::$ty(match_or_panic!(value, Value::$ty(value), *value).clamp($min, $max));)?

                        self.edits_history.property(
                            key,
                            self.manager.selected_things_mut(self.things_catalog).filter_map(|mut thing| {
                                thing.set_property(key, value).map(|value| (thing.id(), value))
                            })
                        );
                    }};
                }

                match key
                {
                    ANGLE_LABEL => set!(I16, 0, 359),
                    HEIGHT_LABEL =>
                    {
                        set!(I8, *TEXTURE_HEIGHT_RANGE.start(), *TEXTURE_HEIGHT_RANGE.end());
                    },
                    _ => set!()
                };
            }
        }

        let UiBundle {
            drawing_resources,
            things_catalog,
            default_brush_properties,
            default_thing_properties,
            manager,
            edits_history,
            clipboard,
            inputs,
            grid,
            ..
        } = bundle;

        ui.label("Name");
        ui.label("Type");
        ui.label("Value");
        ui.end_row();

        match self.target
        {
            Target::None => filler(ui, self.max_rows),
            Target::Brushes =>
            {
                self.overall_brushes_properties.show(
                    ui,
                    drawing_resources,
                    *default_brush_properties,
                    clipboard,
                    inputs,
                    grid,
                    &mut BrushesPropertySetter {
                        manager,
                        edits_history
                    }
                );

                filler(ui, self.brushes_filler);
            },
            Target::Things =>
            {
                self.overall_things_properties.show(
                    ui,
                    drawing_resources,
                    *default_thing_properties,
                    clipboard,
                    inputs,
                    grid,
                    &mut ThingsPropertySetter {
                        things_catalog,
                        manager,
                        edits_history
                    }
                );

                filler(ui, self.things_filler);
            }
        }
    }
}

//=======================================================================//

/// The window to edit the properties of the brushes and [`ThingInstance`]s.
#[must_use]
pub(in crate::map::editor::state::ui) struct PropertiesWindow
{
    /// The window data.
    window:  Window,
    /// The core of the window.
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
        /// Calls the window close.
        #[inline]
        fn close(properties: &mut PropertiesWindow) { properties.window.close(); }

        self.window
            .layer_id()
            .map(|id| WindowCloser::Properties(id, close as fn(&mut Self)))
    }
}

impl Placeholder for PropertiesWindow
{
    #[inline]
    unsafe fn placeholder() -> Self
    {
        Self {
            window:  Window::default(),
            innards: Innards {
                target:                     Target::default(),
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
}

impl PropertiesWindow
{
    /// Returns a new [`PropertiesWindow`].
    #[inline]
    pub fn new(
        default_brush_properties: &DefaultBrushProperties,
        default_thing_properties: &DefaultThingProperties
    ) -> Self
    {
        let b_len = default_brush_properties.len();
        let t_len = default_thing_properties.len();
        let max_rows = b_len.max(t_len).max(10);

        Self {
            window:  Window::default(),
            innards: Innards {
                target: Target::default(),
                overall_brushes_properties: UiOverallProperties::new(default_brush_properties),
                overall_things_draw_height: UiOverallValue::none(),
                overall_things_angle: UiOverallValue::none(),
                overall_things_properties: UiOverallProperties::new(default_thing_properties),
                max_rows,
                brushes_filler: max_rows - b_len,
                things_filler: max_rows - t_len
            }
        }
    }

    /// Updates all the overall brushes properties.
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

    /// Update the overall brushes property with key `k`.
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

    /// Updates the [`ThingInstance`]s draw height and angle.
    #[inline]
    pub fn update_overall_things_info(&mut self, manager: &EntitiesManager)
    {
        if !manager.any_selected_things()
        {
            return;
        }

        let mut draw_height = OverallValue::None;
        let mut angle = OverallValue::None;
        _ = manager.selected_things().any(|thing| {
            let non_uni = draw_height.stack(&thing.draw_height());
            angle.stack(&thing.angle()) && non_uni
        });

        self.innards.overall_things_draw_height = draw_height.ui();
        self.innards.overall_things_angle = angle.ui();
    }

    /// Updates all the overall [`ThingInstance`]s properties.
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

    /// Update the overall [`ThingInstance`]s property with key `k`.
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

    /// Shows the properties window.
    #[inline]
    #[must_use]
    pub fn show(&mut self, egui_context: &egui::Context, bundle: &mut UiBundle) -> bool
    {
        if !self.window.check_open(
            !bundle.inputs.ctrl_pressed() &&
                Bind::PropertiesEditor.just_pressed(bundle.key_inputs, &bundle.config.binds)
        )
        {
            return false;
        }

        let any_sel_brushes = bundle.manager.any_selected_brushes();
        let any_sel_things = bundle.manager.any_selected_things();

        match self.innards.target
        {
            Target::Brushes if !any_sel_brushes => self.innards.target = Target::default(),
            Target::Things if !any_sel_things => self.innards.target = Target::default(),
            _ => ()
        };

        self.window
            .show(
                egui_context,
                egui::Window::new("Properties")
                    .vscroll(true)
                    .collapsible(true)
                    .resizable(true)
                    .default_height(280f32),
                |ui| {
                    self.innards.show(ui, bundle);
                }
            )
            .unwrap_or(false)
    }
}
