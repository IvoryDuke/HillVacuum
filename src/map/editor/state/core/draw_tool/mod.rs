pub(in crate::map::editor::state) mod cursor_polygon;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::Vec2;
use bevy_egui::egui::{self, Color32};
use hill_vacuum_shared::match_or_panic;

use self::cursor_polygon::{
    CircleCursorPolygon,
    DrawCursorPolygon,
    FreeDrawCursorPolygon,
    FreeDrawStatus,
    SquareCursorPolygon,
    TriangleCursorPolygon
};
use super::{
    tool::{DisableSubtool, EnabledTool, OngoingMultiframeChange, Tool},
    ActiveTool
};
use crate::{
    map::{
        containers::{hv_hash_set, Ids},
        drawer::color::Color,
        editor::{
            cursor_pos::Cursor,
            state::{
                editor_state::{InputsPresses, ToolsSettings},
                edits_history::EditsHistory,
                manager::EntitiesManager
            },
            DrawBundle,
            ToolUpdateBundle
        },
        AssertedInsertRemove
    },
    utils::{
        identifiers::{EntityId, Id},
        misc::TakeValue
    }
};

//=======================================================================//
// MACROS
//
//=======================================================================//

/// Generates the functions that return the [`ActiveTool`]s relative to `shape`.
macro_rules! draw_tools {
    ($(($name:ident, $shape:ident $(, $cursor:ident $(, $settings:ident)?)?)),+) => { $(
        #[inline]
        pub fn $name(current_tool: &mut ActiveTool $(, $cursor: &Cursor $(, $settings: &ToolsSettings)?)?) -> ActiveTool
        {
            paste::paste! { let shape = Shape::$shape([<$shape CursorPolygon>]::new($($cursor $(, $settings)?)?)); }

            if let ActiveTool::Draw(DrawTool {
                drawn_brushes,
                ..
            }) = current_tool
            {
                return ActiveTool::Draw(DrawTool {
                    drawn_brushes: drawn_brushes.take_value(),
                    shape
                });
            }

            DrawTool::shape_tool(shape)
        }
    )+};
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The shape to draw with [`DrawTool`].
#[derive(Debug)]
enum Shape
{
    /// A square,
    Square(SquareCursorPolygon),
    /// A triangle.
    Triangle(TriangleCursorPolygon),
    /// A "circle".
    Circle(CircleCursorPolygon),
    /// A polygon.
    FreeDraw(FreeDrawCursorPolygon)
}

//=======================================================================//

/// The draw tool.
#[derive(Debug)]
pub(in crate::map::editor::state::core) struct DrawTool
{
    /// The [`Id`]s of the drawn [`Brush`]es.
    drawn_brushes: Ids,
    /// The shape being drawn.
    shape:         Shape
}

impl Default for DrawTool
{
    #[must_use]
    #[inline]
    fn default() -> Self
    {
        Self {
            drawn_brushes: hv_hash_set![],
            shape:         Shape::Square(SquareCursorPolygon::default())
        }
    }
}

impl EnabledTool for DrawTool
{
    type Item = Tool;

    #[inline]
    fn is_tool_enabled(&self, tool: Self::Item) -> bool
    {
        tool == match &self.shape
        {
            Shape::Square(_) => Tool::Square,
            Shape::Triangle(_) => Tool::Triangle,
            Shape::Circle(_) => Tool::Circle,
            Shape::FreeDraw(_) => Tool::FreeDraw
        }
    }
}

impl DisableSubtool for DrawTool
{
    #[inline]
    fn disable_subtool(&mut self)
    {
        if let Shape::FreeDraw(cb) = &mut self.shape
        {
            cb.disable_subtool();
        }
    }
}

impl OngoingMultiframeChange for DrawTool
{
    #[inline]
    fn ongoing_multi_frame_change(&self) -> bool
    {
        match &self.shape
        {
            Shape::Square(cb) => cb.is_dragging(),
            Shape::Triangle(cb) => cb.is_dragging(),
            Shape::Circle(cb) => cb.is_dragging(),
            Shape::FreeDraw(_) => false
        }
    }
}

impl DrawTool
{
    draw_tools!(
        (square, Square, cursor),
        (triangle, Triangle, cursor),
        (circle, Circle, cursor, settings),
        (free, FreeDraw)
    );

    /// Returns an [`ActiveTool`] in its draw tool variant using `shape`.
    #[inline]
    fn shape_tool(shape: Shape) -> ActiveTool
    {
        ActiveTool::Draw(Self {
            drawn_brushes: hv_hash_set![],
            shape
        })
    }

    //==============================================================
    // Info

    /// Returns the [`Status`] of the free draw tool, if active.
    #[inline]
    pub const fn free_draw_status(&self) -> Option<FreeDrawStatus>
    {
        match &self.shape
        {
            Shape::FreeDraw(cp) => Some(cp.status()),
            _ => None
        }
    }

    //==============================================================
    // Update

    /// Despawns the drawn [`Brush`]es.
    #[inline]
    pub fn despawn_drawn_brushes(
        &mut self,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory
    )
    {
        manager.despawn_drawn_brushes(&mut self.drawn_brushes, edits_history);
    }

    /// Updates the tool.
    #[inline]
    pub fn update(
        &mut self,
        bundle: &mut ToolUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        settings: &mut ToolsSettings
    )
    {
        if inputs.back.just_pressed()
        {
            manager.despawn_drawn_brushes(&mut self.drawn_brushes, edits_history);
            return;
        }

        match &mut self.shape
        {
            Shape::Square(cb) =>
            {
                cb.update(bundle, manager, &mut self.drawn_brushes, inputs, edits_history);
            },
            Shape::Triangle(cb) =>
            {
                cb.update(bundle, manager, &mut self.drawn_brushes, inputs, edits_history);
            },
            Shape::Circle(cb) =>
            {
                cb.update(
                    bundle,
                    manager,
                    &mut self.drawn_brushes,
                    inputs,
                    edits_history,
                    settings
                );
            },
            Shape::FreeDraw(cb) =>
            {
                cb.update(bundle, manager, &mut self.drawn_brushes, inputs, edits_history);
            }
        };
    }

    /// Inserts the free draw vertex with position `p`.
    #[inline]
    pub fn delete_free_draw_vertex(&mut self, p: Vec2)
    {
        match_or_panic!(&mut self.shape, Shape::FreeDraw(cp), cp).delete_free_draw_vertex(p);
    }

    /// Inserts a free draw vertex with position `p`.
    #[inline]
    pub fn insert_free_draw_vertex(&mut self, p: Vec2)
    {
        match_or_panic!(&mut self.shape, Shape::FreeDraw(cp), cp).insert_free_draw_vertex(p);
    }

    /// Post undo/redo spawn update.
    #[inline]
    pub fn undo_redo_spawn(&mut self, manager: &EntitiesManager, identifier: Id)
    {
        assert!(manager.entity_exists(identifier), "Entity does not exist.");
        self.drawn_brushes.asserted_insert(identifier);
    }

    /// Post undo/redo despawn update.
    #[inline]
    pub fn undo_redo_despawn(&mut self, manager: &EntitiesManager, identifier: Id)
    {
        assert!(!manager.entity_exists(identifier), "Entity exists.");
        self.drawn_brushes.asserted_remove(&identifier);
    }

    //==============================================================
    // Draw

    /// Draws the tool.
    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle, manager: &EntitiesManager, show_tooltips: bool)
    {
        let DrawBundle {
            drawer,
            window,
            camera,
            ..
        } = bundle;

        drawer.square_highlight(bundle.cursor.world_snapped(), Color::CursorPolygon);

        let mut drawn_iterated = 0;
        let drawn_len = self.drawn_brushes.len();
        let brushes = manager.visible_brushes(window, camera);
        let mut brushes = brushes.iter();

        for brush in brushes.by_ref()
        {
            let id = brush.id();

            if !manager.is_selected(id)
            {
                brush.draw_non_selected(camera, drawer);
            }
            else if self.drawn_brushes.contains(&id)
            {
                brush.draw_highlighted_selected(camera, drawer);
                drawn_iterated += 1;

                if drawn_iterated == drawn_len
                {
                    break;
                }
            }
            else
            {
                brush.draw_selected(camera, drawer);
            }
        }

        for brush in brushes
        {
            let id = brush.id();

            if manager.is_selected(id)
            {
                brush.draw_selected(camera, drawer);
            }
            else
            {
                brush.draw_non_selected(camera, drawer);
            }
        }

        match &self.shape
        {
            Shape::Square(cb) => cb.draw(drawer),
            Shape::Triangle(cb) => cb.draw(drawer),
            Shape::Circle(cb) => cb.draw(drawer),
            Shape::FreeDraw(cb) => cb.draw(bundle, show_tooltips)
        };
    }

    /// Draws the UI.
    #[inline]
    pub fn ui(&mut self, ui: &mut egui::Ui, settings: &mut ToolsSettings)
    {
        if !matches!(self.shape, Shape::Circle(_))
        {
            return;
        }

        ui.label(egui::RichText::new("CIRCLE TOOL"));

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Resolution:"));
            ui.add(
                egui::Slider::new(
                    &mut settings.circle_draw_resolution,
                    CircleCursorPolygon::circle_resolution_range()
                )
                .show_value(false)
                .text_color(Color32::WHITE)
                .integer()
            );
            ui.label(egui::RichText::new(format!("{}", settings.circle_draw_resolution)));
        });
    }
}
