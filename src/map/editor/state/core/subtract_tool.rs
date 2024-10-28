//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::utils::HashSet;
use hill_vacuum_shared::{return_if_none, NextValue};

use super::{
    item_selector::{ItemSelector, ItemsBeneathCursor},
    rect::{Rect, RectHighlightedEntity, RectTrait},
    tool::DragSelection,
    ActiveTool
};
use crate::{
    map::{
        brush::convex_polygon::SubtractResult,
        drawer::{color::Color, drawing_resources::DrawingResources},
        editor::{
            cursor::Cursor,
            state::{
                core::rect::LeftMouse,
                edits_history::EditsHistory,
                grid::Grid,
                manager::EntitiesManager
            },
            DrawBundle,
            ToolUpdateBundle
        },
        thing::catalog::ThingsCatalog
    },
    utils::{
        identifiers::{EntityId, Id},
        iterators::FilterSet,
        misc::{AssertedInsertRemove, ReplaceValues, TakeValue}
    },
    Ids
};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The brush selector.
struct Selector(ItemSelector<Id>);

impl Selector
{
    /// Returns a new [`Selector`].
    #[inline]
    #[must_use]
    fn new() -> Self
    {
        /// Selector function.
        #[inline]
        fn selector(
            _: &DrawingResources,
            _: &ThingsCatalog,
            manager: &EntitiesManager,
            cursor: &Cursor,
            _: &Grid,
            _: f32,
            items: &mut ItemsBeneathCursor<Id>
        )
        {
            let cursor_pos = cursor.world();

            for brush in manager
                .brushes_at_pos(cursor_pos, None)
                .iter()
                .filter_set_with_predicate(*manager.selected_brushes_ids().next_value(), |brush| {
                    brush.id()
                })
                .filter(|brush| brush.contains_point(cursor_pos))
            {
                items.push(brush.id(), true);
            }
        }

        Self(ItemSelector::new(selector))
    }

    /// Returns the selectable brush beneath the cursor.
    #[inline]
    #[must_use]
    fn brush_beneath_cursor(&mut self, bundle: &ToolUpdateBundle) -> Option<Id>
    {
        self.0.item_beneath_cursor(
            bundle.drawing_resources,
            bundle.things_catalog,
            bundle.manager,
            bundle.cursor,
            bundle.grid,
            0f32,
            bundle.inputs
        )
    }
}

//=======================================================================//

/// The subtract tool.
pub(in crate::map::editor::state::core) struct SubtractTool
{
    /// The rectangular selection.
    drag_selection:       RectHighlightedEntity<Id>,
    /// The brush selector.
    selector:             Selector,
    /// The [`Id`]s of the subtractee.
    subtractees:          Ids,
    /// Helper set to store the [`Id`]s for the select all routine.
    non_selected_brushes: Ids
}

impl DragSelection for SubtractTool
{
    #[inline]
    fn drag_selection(&self) -> Option<Rect> { Rect::from(self.drag_selection).into() }
}

impl SubtractTool
{
    /// Returns an [`ActiveTool`] in its subtract tool variant.
    #[inline]
    pub fn tool(drag_selection: Rect) -> ActiveTool
    {
        ActiveTool::Subtract(SubtractTool {
            drag_selection:       drag_selection.into(),
            selector:             Selector::new(),
            subtractees:          HashSet::with_capacity(4),
            non_selected_brushes: HashSet::new()
        })
    }

    //==============================================================
    // Select all

    /// Selects the non selected brushes.
    #[inline]
    pub fn select_non_selected_brushes(
        &mut self,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory
    )
    {
        self.non_selected_brushes
            .replace_values(manager.non_selected_brushes().map(EntityId::id));
        edits_history.subtractee_selection_cluster(self.non_selected_brushes.iter());
        self.subtractees.extend(&self.non_selected_brushes);
    }

    //==============================================================
    // Update

    /// Updates the tool.
    #[inline]
    #[must_use]
    pub fn update(&mut self, bundle: &mut ToolUpdateBundle) -> bool
    {
        let subtractee_beneath_cursor = self.selector.brush_beneath_cursor(bundle);

        self.drag_selection
            .drag_selection(
                bundle,
                bundle.cursor.world(),
                &mut self.subtractees,
                |rect, bundle, subtractees| {
                    let ToolUpdateBundle {
                        drawing_resources,
                        manager,
                        edits_history,
                        inputs,
                        grid,
                        ..
                    } = bundle;

                    // Apply subtraction.
                    if inputs.enter.just_pressed()
                    {
                        Self::subtract(
                            drawing_resources,
                            manager,
                            edits_history,
                            grid,
                            subtractees
                        );
                        return LeftMouse::Value(true);
                    }

                    rect.set_highlighted_entity(subtractee_beneath_cursor);

                    if !inputs.left_mouse.just_pressed()
                    {
                        return LeftMouse::NotPressed;
                    }

                    match subtractee_beneath_cursor
                    {
                        Some(id) =>
                        {
                            assert!(
                                id != *manager.selected_brushes_ids().next_value(),
                                "Tried to deselect the subtractor as a subtractee."
                            );

                            if inputs.shift_pressed()
                            {
                                if subtractees.contains(&id)
                                {
                                    subtractees.asserted_remove(&id);
                                    edits_history.subtractee_deselection(id);
                                }
                                else
                                {
                                    subtractees.asserted_insert(id);
                                    edits_history.subtractee_selection(id);
                                }
                            }
                            else if !subtractees.contains(&id)
                            {
                                edits_history.subtractee_deselection_cluster(subtractees.iter());
                                edits_history.subtractee_selection(id);

                                subtractees.replace_values(Some(id));
                            }

                            LeftMouse::NotPressed
                        },
                        None => LeftMouse::Pressed
                    }
                },
                |bundle, subtractees| {
                    if subtractee_beneath_cursor.is_none()
                    {
                        bundle
                            .edits_history
                            .subtractee_deselection_cluster(subtractees.iter());
                        subtractees.clear();
                    }

                    None
                },
                |bundle, hull, subtractees| {
                    let ids_in_range = bundle
                        .manager
                        .brushes_in_range(hull)
                        .iter()
                        .filter_set(bundle.manager.selected_brushes_ids().next_value())
                        .copied()
                        .collect::<HashSet<_>>();

                    if ids_in_range.is_empty()
                    {
                        return false.into();
                    }

                    bundle.edits_history.subtractee_selection_cluster(
                        ids_in_range.iter().filter(|id| !subtractees.contains(*id))
                    );

                    if bundle.inputs.shift_pressed()
                    {
                        subtractees.extend(ids_in_range);
                    }
                    else
                    {
                        bundle.edits_history.subtractee_deselection_cluster(
                            subtractees.iter().filter(|id| !ids_in_range.contains(*id))
                        );

                        subtractees.replace_values(ids_in_range);
                    }

                    None
                }
            )
            .unwrap_or_default()
    }

    /// Subtracts the selected brush from the subtractees.
    #[inline]
    fn subtract(
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        subtractees: &mut Ids
    )
    {
        let sel_id = *manager.selected_brushes_ids().next_value();

        for id in subtractees.take_value()
        {
            match manager.brush(id).subtract(manager.brush(sel_id))
            {
                SubtractResult::None => continue,
                SubtractResult::Despawn =>
                {
                    manager.despawn_brush(drawing_resources, edits_history, grid, id);
                },
                SubtractResult::Some { main, others } =>
                {
                    _ = manager.replace_brush_with_partition(
                        drawing_resources,
                        edits_history,
                        grid,
                        others.into_iter(),
                        id,
                        |brush| brush.set_polygon(main)
                    );

                    manager.insert_entity_selection(id);
                    edits_history.entity_selection(id);
                }
            };
        }

        edits_history.override_edit_tag("Brushes Subtraction");
    }

    /// Post undo/redo despawn.
    #[inline]
    pub fn undo_redo_despawn(&mut self, manager: &EntitiesManager, identifier: Id)
    {
        assert!(!manager.entity_exists(identifier), "Entity exist.");

        self.subtractees.remove(&identifier);

        if identifier == return_if_none!(self.drag_selection.highlighted_entity())
        {
            self.drag_selection.set_highlighted_entity(None);
        }
    }

    /// Inserts the subtractee with [`Id`] `identifier`.
    #[inline]
    pub fn insert_subtractee(&mut self, manager: &EntitiesManager, identifier: Id)
    {
        assert!(!manager.is_selected(identifier), "Entity is selected.");
        self.subtractees.asserted_insert(identifier);
    }

    /// Removes the subtractee with [`Id`] `identifier`.
    #[inline]
    pub fn remove_subtractee(&mut self, manager: &EntitiesManager, identifier: Id)
    {
        assert!(!manager.is_selected(identifier), "Entity is selected.");
        self.subtractees.asserted_remove(&identifier);
    }

    //==============================================================
    // Draw

    /// Draws the tool.
    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle)
    {
        let DrawBundle {
            drawer,
            camera,
            window,
            manager,
            ..
        } = bundle;

        let sel_id = *manager.selected_brushes_ids().next_value();
        let brush = manager.brush(sel_id);
        brush.draw_with_color(drawer, Color::SubtractorBrush);

        if let Some(hull) = self.drag_selection.hull()
        {
            drawer.hull(&hull, Color::Hull);
        }

        if let Some(hgl_s) = self.drag_selection.highlighted_entity()
        {
            let brush = manager.brush(hgl_s);

            brush.draw_with_color(
                drawer,
                if self.subtractees.contains(&hgl_s)
                {
                    Color::HighlightedSelectedEntity
                }
                else
                {
                    Color::HighlightedNonSelectedEntity
                }
            );

            for brush in manager
                .visible_brushes(window, camera, drawer.grid())
                .iter()
                .filter_set_with_predicate([sel_id, hgl_s], |brush| brush.id())
            {
                if self.subtractees.contains(&brush.id())
                {
                    brush.draw_with_color(drawer, Color::SubtracteeBrush);
                }
                else
                {
                    brush.draw_non_selected(drawer);
                }
            }

            return;
        }

        for brush in manager
            .visible_brushes(window, camera, drawer.grid())
            .iter()
            .filter_set_with_predicate(sel_id, |brush| brush.id())
        {
            if self.subtractees.contains(&brush.id())
            {
                brush.draw_with_color(drawer, Color::SubtracteeBrush);
            }
            else
            {
                brush.draw_non_selected(drawer);
            }
        }
    }
}
