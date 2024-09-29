//=======================================================================//
// IMPORTS
//
//=======================================================================//

use glam::Vec2;
use hill_vacuum_shared::return_if_none;

use super::item_selector::{ItemSelector, ItemsBeneathCursor};
use crate::{
    map::{
        brush::ShatterResult,
        drawer::{color::Color, drawing_resources::DrawingResources},
        editor::{
            cursor::Cursor,
            state::{
                core::{draw_selected_and_non_selected_brushes, ActiveTool},
                inputs_presses::InputsPresses,
                manager::EntitiesManager
            },
            DrawBundle,
            ToolUpdateBundle
        }
    },
    utils::{
        identifiers::{EntityId, Id},
        iterators::FilterSet,
        misc::Camera
    }
};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The brush selector.
#[derive(Debug)]
struct Selector(ItemSelector<Id>);

impl Selector
{
    /// Returns a new [`Selector`].
    #[inline]
    #[must_use]
    fn new() -> Self
    {
        /// The selector function.
        #[inline]
        fn selector(
            _: &DrawingResources,
            manager: &EntitiesManager,
            cursor: &Cursor,
            _: f32,
            items: &mut ItemsBeneathCursor<Id>
        )
        {
            let cursor_pos = cursor.world();

            for brush in manager
                .selected_brushes_at_pos(cursor_pos, None)
                .iter()
                .filter(|brush| brush.contains_point(cursor_pos))
            {
                items.push(brush.id(), true);
            }
        }

        Self(ItemSelector::new(selector))
    }

    /// Returns the selected brush beneath the cursor.
    #[inline]
    #[must_use]
    fn brush_beneath_cursor(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &EntitiesManager,
        cursor: &Cursor,
        inputs: &InputsPresses
    ) -> Option<Id>
    {
        self.0
            .item_beneath_cursor(drawing_resources, manager, cursor, 0f32, inputs)
    }
}

//=======================================================================//

/// The shatter tool.
#[derive(Debug)]
pub(in crate::map::editor::state::core) struct ShatterTool(Option<Id>, Selector);

impl ShatterTool
{
    /// Returns an [`ActiveTool`] in its shatter tool variant.
    #[inline]
    pub fn tool() -> ActiveTool { ActiveTool::Shatter(ShatterTool(None, Selector::new())) }

    /// The cursor position to be used by the tool.
    #[inline]
    #[must_use]
    const fn cursor_pos(cursor: &Cursor) -> Vec2 { cursor.world_snapped() }

    //==============================================================
    // Update

    /// Updates the tool.
    #[inline]
    pub fn update(&mut self, bundle: &mut ToolUpdateBundle)
    {
        self.0 = self.1.brush_beneath_cursor(
            bundle.drawing_resources,
            bundle.manager,
            bundle.cursor,
            bundle.inputs
        );

        if bundle.inputs.left_mouse.just_pressed()
        {
            self.shatter(bundle);
        }
    }

    /// Shatters the selected brush.
    #[inline]
    fn shatter(&mut self, bundle: &mut ToolUpdateBundle)
    {
        let ToolUpdateBundle {
            drawing_resources,
            camera,
            cursor,
            manager,
            edits_history,
            grid,
            ..
        } = bundle;

        let id = return_if_none!(self.0);
        let ShatterResult { main, shards } =
            return_if_none!(manager.brush(id).shatter(Self::cursor_pos(cursor), camera.scale()));

        _ = manager.replace_brush_with_partition(
            drawing_resources,
            edits_history,
            *grid,
            shards.into_iter(),
            id,
            |brush| brush.set_polygon(main)
        );

        edits_history.override_edit_tag("Brush Shatter");
        self.0 = None;
    }

    //==============================================================
    // Draw

    /// Draws the tool.
    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle)
    {
        let DrawBundle {
            window,
            drawer,
            camera,
            cursor,
            manager,
            ..
        } = bundle;

        drawer.square_highlight(Self::cursor_pos(cursor), Color::ToolCursor);

        if let Some(hgl_e) = self.0
        {
            manager.brush(hgl_e).draw_highlighted_selected(camera, drawer);

            for brush in manager
                .visible_brushes(window, camera, drawer.grid())
                .iter()
                .filter_set_with_predicate(hgl_e, |brush| brush.id())
            {
                if manager.is_selected(brush.id())
                {
                    brush.draw_selected(camera, drawer);
                }
                else
                {
                    brush.draw_non_selected(camera, drawer);
                }
            }
        }
        else
        {
            draw_selected_and_non_selected_brushes!(bundle);
        }
    }
}
