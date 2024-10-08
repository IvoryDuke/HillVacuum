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
    tool::{DisableSubtool, DragSelection, EnabledTool, OngoingMultiframeChange, SubTool},
    ActiveTool,
    CursorDelta
};
use crate::{
    map::{
        brush::Brush,
        drawer::{color::Color, drawing_resources::DrawingResources},
        editor::{
            cursor::Cursor,
            state::{
                core::{rect, tool::subtools_buttons},
                editor_state::{edit_target, TargetSwitch, ToolsSettings},
                grid::Grid,
                manager::EntitiesManager,
                ui::{ToolsButtons, UiBundle}
            },
            DrawBundle,
            ToolUpdateBundle
        }
    },
    utils::{
        hull::{EntityHull, Hull},
        identifiers::{EntityId, Id},
        iterators::FilterSet
    }
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The status of the entity drag.
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
// STRUCTS
//
//=======================================================================//

/// The item beneath the cursor.
#[derive(Clone, Copy, PartialEq)]
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
        #[inline]
        fn scan_brushes(
            manager: &EntitiesManager,
            cursor: &Cursor,
            items: &mut ItemsBeneathCursor<ItemBeneathCursor>
        )
        {
            let cursor_pos = cursor.world();

            for brush in manager
                .brushes_at_pos(cursor_pos, None)
                .iter()
                .filter(|brush| brush.contains_point(cursor_pos))
            {
                let id = brush.id();
                items.push(ItemBeneathCursor::Polygon(id), manager.is_selected(id));
            }
        }

        #[inline]
        fn scan_sprites(
            drawing_resources: &DrawingResources,
            manager: &EntitiesManager,
            cursor: &Cursor,
            grid: &Grid,
            items: &mut ItemsBeneathCursor<ItemBeneathCursor>
        )
        {
            let cursor_pos = cursor.world_no_grid();

            for brush in manager.sprites_at_pos(cursor_pos).iter().filter(|brush| {
                brush
                    .sprite_hull(drawing_resources, grid)
                    .unwrap()
                    .contains_point(cursor_pos)
            })
            {
                let id = brush.id();
                items.push(ItemBeneathCursor::Sprite(id), manager.is_selected(id));
            }
        }

        /// brush and [`ThingInstance`] selection update.
        #[inline]
        fn entity_selector(
            _: &DrawingResources,
            manager: &EntitiesManager,
            cursor: &Cursor,
            _: &Grid,
            _: f32,
            items: &mut ItemsBeneathCursor<ItemBeneathCursor>
        )
        {
            let cursor_pos = cursor.world();

            scan_brushes(manager, cursor, items);

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
            cursor: &Cursor,
            _: &Grid,
            _: f32,
            items: &mut ItemsBeneathCursor<ItemBeneathCursor>
        )
        {
            scan_brushes(manager, cursor, items);
        }

        /// Textured brush selection update.
        #[inline]
        fn textured_brush_selector(
            drawing_resources: &DrawingResources,
            manager: &EntitiesManager,
            cursor: &Cursor,
            grid: &Grid,
            _: f32,
            items: &mut ItemsBeneathCursor<ItemBeneathCursor>
        )
        {
            scan_sprites(drawing_resources, manager, cursor, grid, items);

            let cursor_pos = cursor.world();

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
            cursor: &Cursor,
            grid: &Grid,
            camera_scale: f32,
            items: &mut ItemsBeneathCursor<ItemBeneathCursor>
        )
        {
            scan_sprites(drawing_resources, manager, cursor, grid, items);
            entity_selector(drawing_resources, manager, cursor, grid, camera_scale, items);
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
    fn entity_beneath_cursor(&mut self, bundle: &mut ToolUpdateBundle)
        -> Option<ItemBeneathCursor>
    {
        self.brushes_and_things.item_beneath_cursor(
            bundle.drawing_resources,
            bundle.manager,
            bundle.cursor,
            bundle.grid,
            0f32,
            bundle.inputs
        )
    }

    /// Returns the brush beneath the cursor.
    #[inline]
    #[must_use]
    fn brush_beneath_cursor(&mut self, bundle: &mut ToolUpdateBundle) -> Option<ItemBeneathCursor>
    {
        self.brushes.item_beneath_cursor(
            bundle.drawing_resources,
            bundle.manager,
            bundle.cursor,
            bundle.grid,
            0f32,
            bundle.inputs
        )
    }

    /// Returns the textured brush or sprite beneath the cursor.
    #[inline]
    #[must_use]
    fn textured_brush_beneath_cursor(
        &mut self,
        bundle: &mut ToolUpdateBundle
    ) -> Option<ItemBeneathCursor>
    {
        self.textured_brushes.item_beneath_cursor(
            bundle.drawing_resources,
            bundle.manager,
            bundle.cursor,
            bundle.grid,
            0f32,
            bundle.inputs
        )
    }

    /// Returns the entity or the sprite beneath the cursor.
    #[inline]
    #[must_use]
    fn both_beneath_cursor(&mut self, bundle: &mut ToolUpdateBundle) -> Option<ItemBeneathCursor>
    {
        self.everything.item_beneath_cursor(
            bundle.drawing_resources,
            bundle.manager,
            bundle.cursor,
            bundle.grid,
            0f32,
            bundle.inputs
        )
    }

    /// Returns the item beneath the cursor, if any.
    #[inline]
    #[must_use]
    fn item_beneath_cursor(
        &mut self,
        bundle: &mut ToolUpdateBundle,
        settings: &ToolsSettings
    ) -> Option<ItemBeneathCursor>
    {
        match settings.target_switch()
        {
            TargetSwitch::Entity => self.entity_beneath_cursor(bundle),
            TargetSwitch::Both => self.both_beneath_cursor(bundle),
            TargetSwitch::Texture => self.textured_brush_beneath_cursor(bundle)
        }
    }
}

//=======================================================================//

/// The entity tool.
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
    pub fn update(&mut self, bundle: &mut ToolUpdateBundle, settings: &ToolsSettings)
    {
        match &mut self.0
        {
            Status::Inactive(ds) =>
            {
                let shift_pressed = bundle.inputs.shift_pressed();
                let item_beneath_cursor = self.1.item_beneath_cursor(bundle, settings);
                let cursor_pos = Self::cursor_pos(bundle.cursor);

                rect::update!(
                    ds,
                    cursor_pos,
                    bundle.inputs.left_mouse.pressed(),
                    {
                        if settings.entity_editing()
                        {
                            if let Some(ItemBeneathCursor::Polygon(id)) = item_beneath_cursor
                            {
                                let brush = bundle.manager.brush(id);

                                if bundle.inputs.right_mouse.just_pressed() &&
                                    bundle.manager.is_selected(id) &&
                                    brush.attachable()
                                {
                                    if let Some(owner) = brush.attached()
                                    {
                                        // Remove attachment.
                                        bundle.manager.detach(owner, id);
                                        bundle.edits_history.detach(owner, id);
                                    }
                                    else if bundle.manager.selected_brushes_amount() > 1
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

                            if bundle.inputs.left_mouse.just_pressed()
                            {
                                if shift_pressed
                                {
                                    Self::toggle_entity_selection(bundle, id);
                                }
                                else
                                {
                                    if !bundle.manager.is_selected(id)
                                    {
                                        Self::exclusively_select_entity(bundle, id);
                                    }
                                    else if bundle.inputs.ctrl_pressed() &&
                                        matches!(item, ItemBeneathCursor::Polygon(_))
                                    {
                                        bundle
                                            .manager
                                            .select_attached_brushes(id, bundle.edits_history);
                                    }

                                    self.0 = Status::PreDrag(cursor_pos, item, false);
                                    return;
                                }
                            }

                            false
                        }
                        else
                        {
                            bundle.inputs.left_mouse.just_pressed()
                        }
                    },
                    {
                        if item_beneath_cursor.is_none()
                        {
                            bundle.manager.deselect_selected_entities(bundle.edits_history);
                        }
                    },
                    hull,
                    {
                        Self::select_entities_from_drag_selection(bundle, settings, &hull);
                    }
                );

                if bundle.inputs.back.just_pressed()
                {
                    if settings.entity_editing()
                    {
                        bundle.manager.despawn_selected_entities(
                            bundle.drawing_resources,
                            bundle.edits_history,
                            bundle.grid
                        );
                    }
                    else
                    {
                        bundle.manager.remove_selected_textures(
                            bundle.drawing_resources,
                            bundle.edits_history,
                            bundle.grid
                        );
                    }

                    ds.set_highlighted_entity(self.1.item_beneath_cursor(bundle, settings));
                    return;
                }

                let delta = return_if_none!(bundle.inputs.directional_keys_delta());

                if bundle.inputs.alt_pressed()
                {
                    if settings.entity_editing()
                    {
                        _ = bundle.manager.duplicate_selected_entities(
                            bundle.drawing_resources,
                            bundle.clipboard,
                            bundle.edits_history,
                            bundle.grid,
                            delta
                        );
                    }

                    return;
                }

                edit_target!(
                    settings.target_switch(),
                    |move_texture| {
                        if Self::move_selected_entities(bundle, delta, move_texture)
                        {
                            bundle.edits_history.entity_move_cluster(
                                bundle.manager,
                                delta,
                                move_texture
                            );
                        }
                    },
                    {
                        if Self::move_selected_textures(bundle, delta) &&
                            bundle.manager.selected_textured_amount() != 0
                        {
                            bundle.edits_history.texture_move_cluster(bundle.manager, delta);
                        }
                    }
                );
            },
            Status::PreDrag(pos, hgl_e, forced_spawn) =>
            {
                if !bundle.inputs.left_mouse.pressed()
                {
                    self.0 = Status::Inactive(Some(*hgl_e).into());
                    return;
                }

                if !bundle.cursor.moved()
                {
                    return;
                }

                let drag = return_if_none!(CursorDelta::try_new(bundle.cursor, bundle.grid, *pos));

                // Drag the brushes.
                if *forced_spawn || bundle.inputs.alt_pressed()
                {
                    if bundle.manager.duplicate_selected_entities(
                        bundle.drawing_resources,
                        bundle.clipboard,
                        bundle.edits_history,
                        bundle.grid,
                        drag.delta()
                    )
                    {
                        bundle.edits_history.start_multiframe_edit();
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
                    drag.conditional_update(bundle.cursor, bundle.grid, |delta| {
                        if *drag_spawn
                        {
                            return Self::move_selected_entities(bundle, delta, true);
                        }

                        edit_target!(
                            settings.target_switch(),
                            |move_texture| {
                                Self::move_selected_entities(bundle, delta, move_texture)
                            },
                            Self::move_selected_textures(bundle, delta)
                        )
                    });
                }

                if !bundle.inputs.left_mouse.pressed()
                {
                    self.finalize_entities_drag(bundle, settings);
                }
            },
            Status::Attach(id, hgl_e) =>
            {
                *hgl_e = None;

                let brush_beneath_cursor = self.1.brush_beneath_cursor(bundle);
                let brush_id = return_if_none!(brush_beneath_cursor).id();

                if brush_id == *id ||
                    !bundle.manager.is_selected(brush_id) ||
                    bundle.manager.brush(brush_id).attached().is_some()
                {
                    return;
                }

                *hgl_e = brush_id.into();

                if !bundle.inputs.right_mouse.just_pressed()
                {
                    return;
                }

                bundle.manager.attach(brush_id, *id);
                bundle.edits_history.attach(brush_id, *id);

                self.0 = Status::Inactive(brush_beneath_cursor.into());
            },
            Status::DragSpawnUi(hgl_e) =>
            {
                if let Some(delta) = bundle.inputs.directional_keys_delta()
                {
                    _ = bundle.manager.duplicate_selected_entities(
                        bundle.drawing_resources,
                        bundle.clipboard,
                        bundle.edits_history,
                        bundle.grid,
                        delta
                    );
                    self.0 = Status::default();
                    return;
                }

                *hgl_e = None;
                let item_beneath_cursor =
                    return_if_none!(self.1.item_beneath_cursor(bundle, settings));

                if !bundle.manager.is_selected(item_beneath_cursor.id())
                {
                    return;
                }

                *hgl_e = item_beneath_cursor.into();

                if bundle.inputs.left_mouse.just_pressed()
                {
                    self.0 =
                        Status::PreDrag(Self::cursor_pos(bundle.cursor), item_beneath_cursor, true);
                }
            }
        };
    }

    /// Finalizes the entities drag.
    #[inline]
    fn finalize_entities_drag(&mut self, bundle: &mut ToolUpdateBundle, settings: &ToolsSettings)
    {
        let (drag_delta, drag_spawn) =
            match_or_panic!(&self.0, Status::Drag(drag, drag_spawn), (drag.delta(), *drag_spawn));

        if drag_delta != Vec2::ZERO
        {
            if drag_spawn
            {
                bundle
                    .edits_history
                    .entity_move_cluster(bundle.manager, drag_delta, true);
            }
            else
            {
                edit_target!(
                    settings.target_switch(),
                    |move_texture| {
                        bundle.edits_history.entity_move_cluster(
                            bundle.manager,
                            drag_delta,
                            move_texture
                        );
                    },
                    bundle.edits_history.texture_move_cluster(bundle.manager, drag_delta)
                );
            }
        }

        if bundle.edits_history.multiframe_edit()
        {
            bundle.edits_history.end_multiframe_edit();
            bundle.edits_history.override_edit_tag("Entities Drag Spawn");
        }

        self.0 = Status::default();
    }

    /// Toggles the selection of the entity with [`Id`] `identifier`.
    #[inline]
    fn toggle_entity_selection(bundle: &mut ToolUpdateBundle, identifier: Id)
    {
        if bundle.manager.is_selected(identifier)
        {
            bundle
                .manager
                .deselect_entity(identifier, bundle.inputs, bundle.edits_history);
            return;
        }

        bundle
            .manager
            .select_entity(identifier, bundle.inputs, bundle.edits_history);
    }

    /// Exclusively selects the entity with [`Id`] `identifier`.
    #[inline]
    fn exclusively_select_entity(bundle: &mut ToolUpdateBundle, identifier: Id)
    {
        bundle.manager.deselect_selected_entities(bundle.edits_history);
        bundle
            .manager
            .select_entity(identifier, bundle.inputs, bundle.edits_history);
    }

    /// Selects the entities inside the drag selection.
    #[inline]
    fn select_entities_from_drag_selection(
        bundle: &mut ToolUpdateBundle,
        settings: &ToolsSettings,
        drag_selection: &Hull
    )
    {
        // Inclusive selection.
        if bundle.inputs.shift_pressed()
        {
            bundle.manager.select_entities_in_range(
                drag_selection,
                bundle.edits_history,
                bundle.inputs,
                settings
            );

            return;
        }

        bundle.manager.exclusively_select_entities_in_range(
            drag_selection,
            bundle.edits_history,
            bundle.inputs,
            settings
        );
    }

    /// Moves the selected entities.
    #[inline]
    fn move_selected_entities(
        bundle: &mut ToolUpdateBundle,
        delta: Vec2,
        move_texture: bool
    ) -> bool
    {
        let valid = bundle.manager.test_operation_validity(|manager| {
            manager
                .selected_brushes()
                .find_map(|brush| {
                    (!brush.check_move(bundle.drawing_resources, bundle.grid, delta, move_texture))
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

        for mut brush in bundle
            .manager
            .selected_brushes_mut(bundle.drawing_resources, bundle.grid)
        {
            brush.move_by_delta(delta, move_texture);
        }

        for mut thing in bundle.manager.selected_things_mut()
        {
            thing.move_by_delta(delta);
        }

        true
    }

    /// Moves the selected textures.
    #[inline]
    fn move_selected_textures(bundle: &mut ToolUpdateBundle, delta: Vec2) -> bool
    {
        let valid = bundle.manager.test_operation_validity(|manager| {
            return_if_none!(manager.selected_brushes_with_sprites(), None).find_map(|brush| {
                (!brush.check_texture_move(bundle.drawing_resources, bundle.grid, delta))
                    .then_some(brush.id())
            })
        });

        if !valid
        {
            return false;
        }

        for mut brush in bundle
            .manager
            .selected_textured_brushes_mut(bundle.drawing_resources, bundle.grid)
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
    pub fn draw(&self, bundle: &mut DrawBundle, settings: &ToolsSettings)
    {
        let texture_editing = settings.texture_editing();

        /// Draws the selected and non selected entities, except `filters`.
        macro_rules! draw_selected_and_non_selected {
            ($bundle:ident $(, $filters:expr)?) => {
                draw_selected_and_non_selected_brushes!(bundle $(, $filters)?);
                draw_selected_and_non_selected_things!(bundle $(, $filters)?);
                draw_selected_and_non_selected_sprites!(bundle, texture_editing $(, $filters)?);
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
                    draw_selected_and_non_selected!(bundle, [*id, hgl_e]);

                    let brush = bundle.manager.brush(hgl_e);
                    brush.draw_highlighted_selected(bundle.drawer);

                    if brush.has_sprite()
                    {
                        brush.draw_sprite(
                            bundle.drawer,
                            Color::HighlightedSelectedEntity,
                            texture_editing
                        );
                    }

                    brush.center()
                }
                else
                {
                    draw_selected_and_non_selected!(bundle, *id);
                    Self::cursor_pos(bundle.cursor)
                };

                let brush = bundle.manager.brush(*id);
                brush.draw_highlighted_selected(bundle.drawer);

                if brush.has_sprite()
                {
                    brush.draw_sprite(
                        bundle.drawer,
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
                draw_selected_and_non_selected!(bundle);
                return;
            }
        };
        let id = hgl_e.id();

        draw_selected_and_non_selected!(bundle, id);

        if hgl_e.is_brush()
        {
            let brush = bundle.manager.brush(id);

            if brush.has_sprite()
            {
                let color = if bundle.manager.is_selected(id)
                {
                    Color::HighlightedSelectedEntity
                }
                else
                {
                    Color::HighlightedNonSelectedEntity
                };

                brush.draw_sprite(bundle.drawer, color, texture_editing);
            }
        }

        let hull = match hgl_e
        {
            ItemBeneathCursor::Polygon(_) =>
            {
                let brush = bundle.manager.brush(id);

                if bundle.manager.is_selected(id)
                {
                    brush.draw_highlighted_selected(bundle.drawer);
                    brush.hull().into()
                }
                else
                {
                    brush.draw_highlighted_non_selected(bundle.drawer);
                    None
                }
            },
            ItemBeneathCursor::Sprite(_) =>
            {
                let brush = bundle.manager.brush(id);

                if bundle.manager.is_selected(id)
                {
                    brush.draw_highlighted_selected(bundle.drawer);

                    if !bundle.drawer.show_tooltips()
                    {
                        return;
                    }

                    bundle.drawer.hull_extensions(
                        bundle.window,
                        bundle.camera,
                        &brush
                            .sprite_hull(bundle.drawer.resources(), bundle.drawer.grid())
                            .unwrap(),
                        |_, p| p,
                        Grid::point_projection
                    );
                }
                else
                {
                    brush.draw_highlighted_non_selected(bundle.drawer);
                }

                return;
            },
            ItemBeneathCursor::Thing(_) =>
            {
                let thing = bundle.manager.thing(id);

                if bundle.manager.is_selected(id)
                {
                    thing.draw_highlighted_selected(
                        bundle.window,
                        bundle.camera,
                        bundle.drawer,
                        bundle.things_catalog
                    );

                    thing.hull().into()
                }
                else
                {
                    thing.draw_highlighted_non_selected(
                        bundle.window,
                        bundle.camera,
                        bundle.drawer,
                        bundle.things_catalog
                    );

                    None
                }
            }
        };

        if !bundle.drawer.show_tooltips()
        {
            return;
        }

        bundle.drawer.hull_extensions(
            bundle.window,
            bundle.camera,
            return_if_none!(hull.as_ref()),
            Grid::transform_point,
            |_, p| p
        );
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
        bundle: &mut UiBundle,
        buttons: &mut ToolsButtons
    )
    {
        subtools_buttons!(
            self.0,
            ui,
            bundle,
            buttons,
            (EntityDragSpawn, Status::DragSpawnUi(None), Status::DragSpawnUi(_))
        );
    }
}
