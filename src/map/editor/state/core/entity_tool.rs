//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use glam::Vec2;
use hill_vacuum_shared::{match_or_panic, return_if_no_match, return_if_none};

use super::{
    draw_selected_and_non_selected_brushes,
    draw_selected_and_non_selected_sprites,
    draw_selected_and_non_selected_things,
    item_selector::{ItemSelector, ItemsBeneathCursor},
    rect::{Rect, RectHighlightedEntity, RectTrait},
    tool::{
        ChangeConditions,
        DisableSubtool,
        DragSelection,
        EnabledTool,
        OngoingMultiframeChange,
        SubTool
    },
    ActiveTool,
    CursorDelta
};
use crate::{
    map::{
        brush::Brush,
        drawer::drawing_resources::DrawingResources,
        editor::{
            cursor::Cursor,
            state::{
                clipboard::Clipboard,
                core::{rect, tool::subtools_buttons},
                editor_state::{edit_target, InputsPresses, TargetSwitch, ToolsSettings},
                edits_history::EditsHistory,
                grid::Grid,
                manager::EntitiesManager,
                ui::ToolsButtons
            },
            DrawBundle,
            StateUpdateBundle,
            ToolUpdateBundle
        }
    },
    utils::{
        hull::{EntityHull, Hull},
        identifiers::{EntityId, Id},
        iterators::FilterSet,
        misc::Camera
    }
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The status of the entity drag.
#[derive(Debug)]
enum Status
{
    /// Inactive.
    Inactive(RectHighlightedEntity<ItemBeneathCursor>),
    /// Dragging entities.
    Drag(CursorDelta, bool),
    /// Preparing for drag.
    PreDrag(Vec2, ItemBeneathCursor, bool),
    /// Anchoring a brush to another.
    Attach(Id, Option<Id>),
    /// Attempting a drag spawn from the UI.
    DragSpawnUi(Option<ItemBeneathCursor>)
}

impl Default for Status
{
    #[inline]
    #[must_use]
    fn default() -> Self { Self::Inactive(RectHighlightedEntity::default()) }
}

impl EnabledTool for Status
{
    type Item = SubTool;

    #[inline]
    fn is_tool_enabled(&self, tool: Self::Item) -> bool
    {
        tool == match self
        {
            Status::DragSpawnUi(_) => SubTool::EntityDragSpawn,
            _ => return false
        }
    }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The item beneath the cursor.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ItemBeneathCursor
{
    /// A polygon.
    Polygon(Id),
    /// A thing.
    Thing(Id),
    /// A sprite.
    Sprite(Id)
}

impl EntityId for ItemBeneathCursor
{
    #[inline]
    fn id(&self) -> Id { *self.id_as_ref() }

    #[inline]
    fn id_as_ref(&self) -> &Id
    {
        let (ItemBeneathCursor::Polygon(id) |
        ItemBeneathCursor::Sprite(id) |
        ItemBeneathCursor::Thing(id)) = self;
        id
    }
}

impl ItemBeneathCursor
{
    #[inline]
    #[must_use]
    const fn is_brush(&self) -> bool { matches!(self, Self::Polygon(_) | Self::Sprite(_)) }
}

//=======================================================================//

/// The entity selector.
#[must_use]
#[derive(Debug)]
struct Selector
{
    /// Selector of brushes and [`ThingInstance`]s.
    brushes_and_things: ItemSelector<ItemBeneathCursor>,
    /// Selector of brushes.
    brushes:            ItemSelector<ItemBeneathCursor>,
    /// Selector of textured brushes.
    textured_brushes:   ItemSelector<ItemBeneathCursor>,
    /// Selector of any item.
    everything:         ItemSelector<ItemBeneathCursor>
}

impl Selector
{
    /// Returns a new [`Selector`].
    #[inline]
    fn new() -> Self
    {
        /// brush and [`ThingInstance`] selection update.
        #[inline]
        fn entity_selector(
            _: &DrawingResources,
            manager: &EntitiesManager,
            cursor_pos: Vec2,
            _: f32,
            items: &mut ItemsBeneathCursor<ItemBeneathCursor>
        )
        {
            for brush in manager
                .brushes_at_pos(cursor_pos, None)
                .iter()
                .filter(|brush| brush.contains_point(cursor_pos))
            {
                let id = brush.id();
                items.push(ItemBeneathCursor::Polygon(id), manager.is_selected(id));
            }

            for thing in manager
                .things_at_pos(cursor_pos, None)
                .iter()
                .filter(|thing| thing.contains_point(cursor_pos))
            {
                let id = thing.id();
                items.push(ItemBeneathCursor::Thing(id), manager.is_selected(id));
            }
        }

        /// Polygons selection update.
        #[inline]
        fn polygon_selector(
            _: &DrawingResources,
            manager: &EntitiesManager,
            cursor_pos: Vec2,
            _: f32,
            items: &mut ItemsBeneathCursor<ItemBeneathCursor>
        )
        {
            for brush in manager
                .brushes_at_pos(cursor_pos, None)
                .iter()
                .filter(|brush| brush.contains_point(cursor_pos))
            {
                let id = brush.id();
                items.push(ItemBeneathCursor::Polygon(id), manager.is_selected(id));
            }
        }

        /// Textured brush selection update.
        #[inline]
        fn textured_brush_selector(
            drawing_resources: &DrawingResources,
            manager: &EntitiesManager,
            cursor_pos: Vec2,
            _: f32,
            items: &mut ItemsBeneathCursor<ItemBeneathCursor>
        )
        {
            for brush in manager.sprites_at_pos(cursor_pos).iter().filter(|brush| {
                brush
                    .sprite_hull(drawing_resources)
                    .unwrap()
                    .contains_point(cursor_pos)
            })
            {
                let id = brush.id();
                items.push(ItemBeneathCursor::Sprite(id), manager.is_selected(id));
            }

            for brush in manager.brushes_at_pos(cursor_pos, None).iter().filter(|brush| {
                brush.has_texture() && !brush.has_sprite() && brush.contains_point(cursor_pos)
            })
            {
                let id = brush.id();
                items.push(ItemBeneathCursor::Polygon(id), manager.is_selected(id));
            }
        }

        /// Any item selection update.
        #[inline]
        fn both_selector(
            drawing_resources: &DrawingResources,
            manager: &EntitiesManager,
            cursor_pos: Vec2,
            _: f32,
            items: &mut ItemsBeneathCursor<ItemBeneathCursor>
        )
        {
            for brush in manager.sprites_at_pos(cursor_pos).iter().filter(|brush| {
                brush
                    .sprite_hull(drawing_resources)
                    .unwrap()
                    .contains_point(cursor_pos)
            })
            {
                let id = brush.id();
                items.push(ItemBeneathCursor::Sprite(id), manager.is_selected(id));
            }

            for brush in manager
                .brushes_at_pos(cursor_pos, None)
                .iter()
                .filter(|brush| brush.contains_point(cursor_pos))
            {
                let id = brush.id();
                items.push(ItemBeneathCursor::Polygon(id), manager.is_selected(id));
            }

            for thing in manager
                .things_at_pos(cursor_pos, None)
                .iter()
                .filter(|thing| thing.contains_point(cursor_pos))
            {
                let id = thing.id();
                items.push(ItemBeneathCursor::Thing(id), manager.is_selected(id));
            }
        }

        Self {
            brushes_and_things: ItemSelector::new(entity_selector),
            brushes:            ItemSelector::new(polygon_selector),
            textured_brushes:   ItemSelector::new(textured_brush_selector),
            everything:         ItemSelector::new(both_selector)
        }
    }

    /// Returns the entity beneath the cursor.
    #[inline]
    #[must_use]
    fn entity_beneath_cursor(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &EntitiesManager,
        cursor: &Cursor,
        inputs: &InputsPresses
    ) -> Option<ItemBeneathCursor>
    {
        self.brushes_and_things.item_beneath_cursor(
            drawing_resources,
            manager,
            cursor,
            0f32,
            inputs
        )
    }

    /// Returns the brush beneath the cursor.
    #[inline]
    #[must_use]
    fn brush_beneath_cursor(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &EntitiesManager,
        cursor: &Cursor,
        inputs: &InputsPresses
    ) -> Option<ItemBeneathCursor>
    {
        self.brushes
            .item_beneath_cursor(drawing_resources, manager, cursor, 0f32, inputs)
    }

    /// Returns the textured brush or sprite beneath the cursor.
    #[inline]
    #[must_use]
    fn textured_brush_beneath_cursor(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &EntitiesManager,
        cursor: &Cursor,
        inputs: &InputsPresses
    ) -> Option<ItemBeneathCursor>
    {
        self.textured_brushes
            .item_beneath_cursor(drawing_resources, manager, cursor, 0f32, inputs)
    }

    /// Returns the entity or the sprite beneath the cursor.
    #[inline]
    #[must_use]
    fn both_beneath_cursor(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &EntitiesManager,
        cursor: &Cursor,
        inputs: &InputsPresses
    ) -> Option<ItemBeneathCursor>
    {
        self.everything
            .item_beneath_cursor(drawing_resources, manager, cursor, 0f32, inputs)
    }

    /// Returns the item beneath the cursor, if any.
    #[inline]
    #[must_use]
    fn item_beneath_cursor(
        &mut self,
        bundle: &mut ToolUpdateBundle,
        manager: &mut EntitiesManager,
        settings: &ToolsSettings,
        inputs: &InputsPresses
    ) -> Option<ItemBeneathCursor>
    {
        match settings.target_switch()
        {
            TargetSwitch::Entity =>
            {
                self.entity_beneath_cursor(bundle.drawing_resources, manager, bundle.cursor, inputs)
            },
            TargetSwitch::Both =>
            {
                self.both_beneath_cursor(bundle.drawing_resources, manager, bundle.cursor, inputs)
            },
            TargetSwitch::Texture =>
            {
                self.textured_brush_beneath_cursor(
                    bundle.drawing_resources,
                    manager,
                    bundle.cursor,
                    inputs
                )
            },
        }
    }
}

//=======================================================================//

/// The entity tool.
#[derive(Debug)]
pub(in crate::map::editor::state::core) struct EntityTool(Status, Selector);

impl Default for EntityTool
{
    #[inline]
    #[must_use]
    fn default() -> Self { Self(Status::default(), Selector::new()) }
}

impl DisableSubtool for EntityTool
{
    #[inline]
    fn disable_subtool(&mut self)
    {
        if matches!(self.0, Status::Attach(..) | Status::DragSpawnUi(_))
        {
            self.0 = Status::default();
        }
    }
}

impl OngoingMultiframeChange for EntityTool
{
    #[inline]
    fn ongoing_multi_frame_change(&self) -> bool
    {
        !matches!(self.0, Status::Inactive(_) | Status::PreDrag(..))
    }
}

impl DragSelection for EntityTool
{
    #[inline]
    fn drag_selection(&self) -> Option<Rect>
    {
        Some(
            (*return_if_no_match!(&self.0, Status::Inactive(drag_selection), drag_selection, None))
                .into()
        )
    }
}

impl EntityTool
{
    /// Returns an [`ActiveTool`] in its entity tool variant.
    #[inline]
    pub fn tool(drag_selection: Rect) -> ActiveTool
    {
        ActiveTool::Entity(Self(Status::Inactive(drag_selection.into()), Selector::new()))
    }

    //==============================================================
    // Info

    /// The cursor pos used by the tool.
    #[inline]
    #[must_use]
    const fn cursor_pos(cursor: &Cursor) -> Vec2 { cursor.world() }

    //==============================================================
    // Update

    /// Updates the tool.
    #[inline]
    pub fn update(
        &mut self,
        bundle: &mut ToolUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        clipboard: &mut Clipboard,
        grid: Grid,
        settings: &ToolsSettings
    )
    {
        match &mut self.0
        {
            Status::Inactive(ds) =>
            {
                let shift_pressed = inputs.shift_pressed();
                let item_beneath_cursor =
                    self.1.item_beneath_cursor(bundle, manager, settings, inputs);
                let cursor_pos = Self::cursor_pos(bundle.cursor);

                rect::update!(
                    ds,
                    cursor_pos,
                    bundle.camera.scale(),
                    inputs.left_mouse.pressed(),
                    {
                        if settings.entity_editing()
                        {
                            if let Some(ItemBeneathCursor::Polygon(id)) = item_beneath_cursor
                            {
                                let brush = manager.brush(id);

                                if inputs.right_mouse.just_pressed() &&
                                    manager.is_selected(id) &&
                                    brush.attachable()
                                {
                                    if let Some(owner) = brush.attached()
                                    {
                                        // Remove attachment.
                                        manager.detach(owner, id);
                                        edits_history.detach(owner, id);
                                    }
                                    else if manager.selected_brushes_amount() > 1
                                    {
                                        self.0 = Status::Attach(id, None);
                                    }

                                    return;
                                }
                            }
                        }

                        ds.set_highlighted_entity(item_beneath_cursor);

                        if let Some(item) = item_beneath_cursor
                        {
                            let id = item.id();

                            if inputs.left_mouse.just_pressed()
                            {
                                if shift_pressed
                                {
                                    Self::toggle_entity_selection(
                                        manager,
                                        inputs,
                                        edits_history,
                                        id
                                    );
                                }
                                else
                                {
                                    if !manager.is_selected(id) ||
                                        (inputs.ctrl_pressed() &&
                                            matches!(item, ItemBeneathCursor::Polygon(_)))
                                    {
                                        Self::exclusively_select_entity(
                                            manager,
                                            inputs,
                                            edits_history,
                                            id
                                        );
                                    }

                                    self.0 = Status::PreDrag(cursor_pos, item, false);
                                    return;
                                }
                            }

                            false
                        }
                        else
                        {
                            inputs.left_mouse.just_pressed()
                        }
                    },
                    {
                        if item_beneath_cursor.is_none()
                        {
                            manager.deselect_selected_entities(edits_history);
                        }
                    },
                    hull,
                    {
                        Self::select_entities_from_drag_selection(
                            manager,
                            &hull,
                            inputs,
                            edits_history,
                            settings
                        );
                    }
                );

                if inputs.back.just_pressed()
                {
                    if settings.entity_editing()
                    {
                        manager.despawn_selected_entities(bundle.drawing_resources, edits_history);
                    }
                    else
                    {
                        manager.remove_selected_textures(bundle.drawing_resources, edits_history);
                    }

                    ds.set_highlighted_entity(
                        self.1.item_beneath_cursor(bundle, manager, settings, inputs)
                    );
                    return;
                }

                if inputs.ctrl_pressed()
                {
                    return;
                }

                let delta = return_if_none!(inputs.directional_keys_vector(grid.size()));

                if inputs.alt_pressed()
                {
                    if settings.entity_editing()
                    {
                        _ = manager.duplicate_selected_entities(
                            bundle.drawing_resources,
                            edits_history,
                            clipboard,
                            delta
                        );
                    }

                    return;
                }

                edit_target!(
                    settings.target_switch(),
                    |move_texture| {
                        if Self::move_selected_entities(bundle, manager, delta, move_texture)
                        {
                            edits_history.entity_move_cluster(manager, delta, move_texture);
                        }
                    },
                    {
                        if Self::move_selected_textures(bundle, manager, delta)
                        {
                            edits_history.texture_move_cluster(manager, delta);
                        }
                    }
                );
            },
            Status::PreDrag(pos, hgl_e, forced_spawn) =>
            {
                if !inputs.left_mouse.pressed()
                {
                    self.0 = Status::Inactive(Some(*hgl_e).into());
                    return;
                }

                if !bundle.cursor.moved()
                {
                    return;
                }

                let drag = return_if_none!(CursorDelta::try_new(bundle.cursor, grid, *pos));

                // Drag the brushes.
                if *forced_spawn || inputs.alt_pressed()
                {
                    if manager.duplicate_selected_entities(
                        bundle.drawing_resources,
                        edits_history,
                        clipboard,
                        drag.delta()
                    )
                    {
                        edits_history.start_multiframe_edit();
                        self.0 = Status::Drag(drag, true);
                    }
                    else
                    {
                        self.0 = Status::Inactive((Some(*hgl_e)).into());
                    }

                    return;
                }

                self.0 = Status::Drag(drag, false);
            },
            Status::Drag(drag, drag_spawn) =>
            {
                if bundle.cursor.moved()
                {
                    drag.conditional_update(bundle.cursor, grid, |delta| {
                        if *drag_spawn
                        {
                            return Self::move_selected_entities(bundle, manager, delta, true);
                        }

                        edit_target!(
                            settings.target_switch(),
                            |move_texture| {
                                Self::move_selected_entities(bundle, manager, delta, move_texture)
                            },
                            Self::move_selected_textures(bundle, manager, delta)
                        )
                    });
                }

                if !inputs.left_mouse.pressed()
                {
                    self.finalize_entities_drag(manager, edits_history, settings);
                }
            },
            Status::Attach(id, hgl_e) =>
            {
                *hgl_e = None;

                let brush_beneath_cursor = self.1.brush_beneath_cursor(
                    bundle.drawing_resources,
                    manager,
                    bundle.cursor,
                    inputs
                );
                let brush_id = return_if_none!(brush_beneath_cursor).id();

                if brush_id == *id ||
                    !manager.is_selected(brush_id) ||
                    manager.brush(brush_id).attached().is_some()
                {
                    return;
                }

                *hgl_e = brush_id.into();

                if !inputs.right_mouse.just_pressed()
                {
                    return;
                }

                manager.attach(brush_id, *id);
                edits_history.attach(brush_id, *id);

                self.0 = Status::Inactive(brush_beneath_cursor.into());
            },
            Status::DragSpawnUi(hgl_e) =>
            {
                if let Some(delta) = inputs.directional_keys_vector(grid.size())
                {
                    _ = manager.duplicate_selected_entities(
                        bundle.drawing_resources,
                        edits_history,
                        clipboard,
                        delta
                    );
                    self.0 = Status::default();
                    return;
                }

                *hgl_e = None;
                let item_beneath_cursor =
                    return_if_none!(self.1.item_beneath_cursor(bundle, manager, settings, inputs));

                if !manager.is_selected(item_beneath_cursor.id())
                {
                    return;
                }

                *hgl_e = item_beneath_cursor.into();

                if inputs.left_mouse.just_pressed()
                {
                    self.0 =
                        Status::PreDrag(Self::cursor_pos(bundle.cursor), item_beneath_cursor, true);
                }
            }
        };
    }

    /// Finalizes the entities drag.
    #[inline]
    fn finalize_entities_drag(
        &mut self,
        manager: &EntitiesManager,
        edits_history: &mut EditsHistory,
        settings: &ToolsSettings
    )
    {
        let (drag_delta, drag_spawn) =
            match_or_panic!(&self.0, Status::Drag(drag, drag_spawn), (drag.delta(), *drag_spawn));

        if drag_delta != Vec2::ZERO
        {
            if drag_spawn
            {
                edits_history.entity_move_cluster(manager, drag_delta, true);
            }
            else
            {
                edit_target!(
                    settings.target_switch(),
                    |move_texture| {
                        edits_history.entity_move_cluster(manager, drag_delta, move_texture);
                    },
                    edits_history.texture_move_cluster(manager, drag_delta)
                );
            }
        }

        if edits_history.multiframe_edit()
        {
            edits_history.end_multiframe_edit();
            edits_history.override_edit_tag("Entities Drag Spawn");
        }

        self.0 = Status::default();
    }

    /// Toggles the selection of the entity with [`Id`] `identifier`.
    #[inline]
    fn toggle_entity_selection(
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        identifier: Id
    )
    {
        if manager.is_selected(identifier)
        {
            manager.deselect_entity(identifier, inputs, edits_history);
            return;
        }

        manager.select_entity(identifier, inputs, edits_history);
    }

    /// Exclusively selects the entity with [`Id`] `identifier`.
    #[inline]
    fn exclusively_select_entity(
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        identifier: Id
    )
    {
        manager.deselect_selected_entities(edits_history);
        manager.select_entity(identifier, inputs, edits_history);
    }

    /// Selects the entities inside the drag selection.
    #[inline]
    fn select_entities_from_drag_selection(
        manager: &mut EntitiesManager,
        drag_selection: &Hull,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        settings: &ToolsSettings
    )
    {
        // Inclusive selection.
        if inputs.shift_pressed()
        {
            manager.select_entities_in_range(drag_selection, inputs, edits_history, settings);
            return;
        }

        manager.exclusively_select_entities_in_range(
            drag_selection,
            inputs,
            edits_history,
            settings
        );
    }

    /// Moves the selected entities.
    #[inline]
    fn move_selected_entities(
        bundle: &ToolUpdateBundle,
        manager: &mut EntitiesManager,
        delta: Vec2,
        move_texture: bool
    ) -> bool
    {
        let valid = manager.test_operation_validity(|manager| {
            manager
                .selected_brushes()
                .find_map(|brush| {
                    (!brush.check_move(bundle.drawing_resources, delta, move_texture))
                        .then_some(brush.id())
                })
                .or(manager
                    .selected_things()
                    .find_map(|thing| (!thing.check_move(delta)).then_some(thing.id())))
        });

        if !valid
        {
            return false;
        }

        for mut brush in manager.selected_brushes_mut(bundle.drawing_resources)
        {
            brush.move_by_delta(delta, move_texture);
        }

        for mut thing in manager.selected_things_mut()
        {
            thing.move_by_delta(delta);
        }

        true
    }

    /// Moves the selected textures.
    #[inline]
    fn move_selected_textures(
        bundle: &ToolUpdateBundle,
        manager: &mut EntitiesManager,
        delta: Vec2
    ) -> bool
    {
        let valid = manager.test_operation_validity(|manager| {
            manager.selected_brushes_with_sprites().find_map(|brush| {
                (!brush.check_texture_move(bundle.drawing_resources, delta)).then_some(brush.id())
            })
        });

        if !valid
        {
            return false;
        }

        for mut brush in manager.selected_brushes_mut(bundle.drawing_resources)
        {
            brush.move_texture(delta);
        }

        true
    }

    /// Removes the highlighted entity.
    #[inline]
    pub fn remove_highlighted_entity(&mut self)
    {
        return_if_no_match!(&mut self.0, Status::Inactive(ds), ds).set_highlighted_entity(None);
    }

    //==============================================================
    // Draw

    /// Draws the tool.
    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle, manager: &EntitiesManager, settings: &ToolsSettings)
    {
        let texture_editing = settings.texture_editing();

        /// Draws the selected and non selected entities, except `filters`.
        macro_rules! draw_selected_and_non_selected {
            ($bundle:ident, $manager:ident $(, $filters:expr)?) => {
                draw_selected_and_non_selected_brushes!(bundle, manager $(, $filters)?);
                draw_selected_and_non_selected_things!(bundle, manager $(, $filters)?);
                draw_selected_and_non_selected_sprites!(bundle, manager, texture_editing $(, $filters)?);
            };
        }

        let hgl_e = match &self.0
        {
            Status::Inactive(ds) =>
            {
                if let Some(hull) = ds.hull()
                {
                    bundle.drawer.hull(&hull, Color::Hull);
                }

                ds.highlighted_entity()
            },
            Status::Drag(drag, _) =>
            {
                drag.draw(bundle);
                None
            },
            Status::PreDrag(_, hgl_e, _) => Some(*hgl_e),
            Status::Attach(id, hgl_e) =>
            {
                let end = if let Some(hgl_e) = *hgl_e
                {
                    draw_selected_and_non_selected!(bundle, manager, [*id, hgl_e]);

                    let brush = manager.brush(hgl_e);
                    brush.draw_highlighted_selected(bundle.camera, &mut bundle.drawer);

                    if brush.has_sprite()
                    {
                        brush.draw_sprite(
                            &mut bundle.drawer,
                            Color::HighlightedSelectedEntity,
                            texture_editing
                        );
                    }

                    brush.center()
                }
                else
                {
                    draw_selected_and_non_selected!(bundle, manager, *id);
                    Self::cursor_pos(bundle.cursor)
                };

                let brush = manager.brush(*id);
                brush.draw_highlighted_selected(bundle.camera, &mut bundle.drawer);

                if brush.has_sprite()
                {
                    brush.draw_sprite(
                        &mut bundle.drawer,
                        Color::HighlightedSelectedEntity,
                        texture_editing
                    );
                }

                let start = brush.center();
                bundle.drawer.square_highlight(start, Color::BrushAnchor);

                bundle.drawer.square_highlight(end, Color::BrushAnchor);
                bundle.drawer.line(start, end, Color::BrushAnchor);

                return;
            },
            Status::DragSpawnUi(hgl_e) => *hgl_e
        };

        let hgl_e = match hgl_e
        {
            Some(hgl_e) => hgl_e,
            None =>
            {
                draw_selected_and_non_selected!(bundle, manager);
                return;
            }
        };
        let id = hgl_e.id();

        draw_selected_and_non_selected!(bundle, manager, id);

        if hgl_e.is_brush()
        {
            let brush = manager.brush(id);

            if brush.has_sprite()
            {
                let color = if manager.is_selected(id)
                {
                    Color::HighlightedSelectedEntity
                }
                else
                {
                    Color::HighlightedNonSelectedEntity
                };

                brush.draw_sprite(&mut bundle.drawer, color, texture_editing);
            }
        }

        let hull = match hgl_e
        {
            ItemBeneathCursor::Polygon(_) =>
            {
                let brush = manager.brush(id);

                if manager.is_selected(id)
                {
                    brush.draw_highlighted_selected(bundle.camera, &mut bundle.drawer);
                    brush.hull().into()
                }
                else
                {
                    brush.draw_highlighted_non_selected(bundle.camera, &mut bundle.drawer);
                    None
                }
            },
            ItemBeneathCursor::Sprite(_) =>
            {
                let brush = manager.brush(id);

                if manager.is_selected(id)
                {
                    brush.draw_highlighted_selected(bundle.camera, &mut bundle.drawer);
                    brush.sprite_hull(bundle.drawer.resources()).unwrap().into()
                }
                else
                {
                    brush.draw_highlighted_non_selected(bundle.camera, &mut bundle.drawer);
                    None
                }
            },
            ItemBeneathCursor::Thing(_) =>
            {
                let thing = manager.thing(id);

                if manager.is_selected(id)
                {
                    thing.draw_highlighted_selected(
                        bundle.window,
                        bundle.camera,
                        &mut bundle.drawer,
                        bundle.things_catalog
                    );
                    thing.hull().into()
                }
                else
                {
                    thing.draw_highlighted_non_selected(
                        bundle.window,
                        bundle.camera,
                        &mut bundle.drawer,
                        bundle.things_catalog
                    );
                    None
                }
            }
        };

        if bundle.drawer.show_tooltips()
        {
            bundle
                .drawer
                .hull_extensions(&return_if_none!(hull), bundle.window, bundle.camera);
        }
    }

    /// Draws the tool's UI.
    #[inline]
    pub fn ui(&self, ui: &mut egui::Ui, settings: &mut ToolsSettings)
    {
        ui.label(egui::RichText::new("ENTITY TOOL"));
        settings.ui(ui, !self.ongoing_multi_frame_change());
    }

    /// Draws the subtools.
    #[inline]
    pub fn draw_subtools(
        &mut self,
        ui: &mut egui::Ui,
        bundle: &StateUpdateBundle,
        buttons: &mut ToolsButtons,
        tool_change_conditions: &ChangeConditions
    )
    {
        subtools_buttons!(
            self.0,
            ui,
            bundle,
            buttons,
            tool_change_conditions,
            (EntityDragSpawn, Status::DragSpawnUi(None), Status::DragSpawnUi(_))
        );
    }
}
