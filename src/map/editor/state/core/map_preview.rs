//=======================================================================//
// IMPORTS
//
//=======================================================================//

use super::{tool::ActiveTool, PreviousActiveTool};
use crate::{
    map::{
        editor::{
            state::manager::{Animators, EntitiesManager},
            DrawBundleMapPreview,
            StateUpdateBundle,
            ToolUpdateBundle
        },
        path::MovementSimulator
    },
    utils::identifiers::{EntityId, Id}
};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The map preview tool.
pub(in crate::map::editor::state::core) struct MapPreviewTool
{
    /// The previously active tool.
    prev_tool: PreviousActiveTool,
    /// The movement simulators.
    movement:  Vec<MovementSimulator>,
    /// The texture animators.
    animators: Animators
}

impl MapPreviewTool
{
    /// Returns an [`ActiveTool`] in its map preview variant.
    #[inline]
    pub fn tool(bundle: &StateUpdateBundle, active_tool: &mut ActiveTool) -> ActiveTool
    {
        ActiveTool::MapPreview(MapPreviewTool {
            prev_tool: Box::new(std::mem::take(active_tool)),
            movement:  bundle.manager.movement_simulators(),
            animators: bundle.manager.texture_animators(bundle)
        })
    }

    /// Returns a mutable reference to the previously used tool.
    #[inline]
    pub fn prev_tool(&mut self) -> &mut ActiveTool { &mut self.prev_tool }

    /// Updates the tool.
    #[inline]
    pub fn update(&mut self, bundle: &ToolUpdateBundle)
    {
        for sim in &mut self.movement
        {
            sim.update(bundle.manager.moving(sim.id()), bundle.delta_time);
        }

        self.animators.update(bundle);
    }

    /// Draws the tool.
    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundleMapPreview)
    {
        let DrawBundleMapPreview {
            window,
            drawer,
            camera,
            things_catalog,
            manager,
            ..
        } = bundle;
        let brushes = manager.brushes();

        for simulator in &self.movement
        {
            manager.moving(simulator.id()).draw_map_preview_movement_simulation(
                camera,
                brushes,
                things_catalog,
                drawer,
                &self.animators,
                simulator
            );
        }

        for brush in manager
            .visible_brushes(window, camera, drawer.grid())
            .iter()
            .filter(|brush| !is_moving(manager, brush.id()) && !brush.has_sprite())
        {
            brush.draw_map_preview(camera, drawer, self.animators.get_brush_animator(brush.id()));
        }

        for brush in manager
            .visible_sprites(window, camera, drawer.grid())
            .iter()
            .filter(|brush| !is_moving(manager, brush.id()))
        {
            brush.draw_map_preview_sprite(drawer, self.animators.get_brush_animator(brush.id()));
        }

        for thing in manager
            .visible_things(window, camera, drawer.grid())
            .iter()
            .filter(|brush| !is_moving(manager, brush.id()))
        {
            thing.draw_map_preview(drawer, things_catalog, &self.animators);
        }
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Whether the entity with [`Id`] `identifier` moves.
#[inline]
#[must_use]
fn is_moving(manager: &EntitiesManager, identifier: Id) -> bool
{
    let moving = manager.is_moving(identifier);

    if manager.is_thing(identifier)
    {
        return moving;
    }

    moving || manager.brush(identifier).attached().is_some()
}
