//=======================================================================//
// IMPORTS
//
//=======================================================================//

use super::{tool::ActiveTool, PreviousActiveTool};
use crate::{
    map::{
        brush::path::MovementSimulator,
        drawer::drawing_resources::DrawingResources,
        editor::{
            state::{
                core::is_moving_brush,
                manager::{Animators, EntitiesManager}
            },
            DrawBundleMapPreview,
            ToolUpdateBundle
        },
        hv_box,
        HvVec
    },
    utils::identifiers::EntityId
};

//=======================================================================//
// TYPES
//
//=======================================================================//

#[derive(Debug)]
pub(in crate::map::editor::state::core) struct MapPreviewTool
{
    prev_tool: PreviousActiveTool,
    movement:  HvVec<MovementSimulator>,
    animators: Animators
}

impl MapPreviewTool
{
    #[inline]
    pub fn tool(
        drawing_resources: &DrawingResources,
        active_tool: &mut ActiveTool,
        manager: &EntitiesManager
    ) -> ActiveTool
    {
        ActiveTool::MapPreview(MapPreviewTool {
            prev_tool: hv_box!(std::mem::take(active_tool)),
            movement:  manager.movement_simulators(),
            animators: manager.texture_animators(drawing_resources)
        })
    }

    #[inline]
    pub fn prev_tool(&mut self) -> &mut ActiveTool { &mut self.prev_tool }

    #[inline]
    pub fn update(&mut self, bundle: &ToolUpdateBundle, manager: &EntitiesManager)
    {
        for sim in &mut self.movement
        {
            sim.update(manager.brush(sim.id()), bundle.delta_time);
        }

        self.animators
            .update(bundle.drawing_resources, manager, bundle.delta_time);
    }

    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundleMapPreview, manager: &EntitiesManager)
    {
        let DrawBundleMapPreview {
            window,
            drawer,
            camera,
            things_catalog,
            ..
        } = bundle;
        let brushes = manager.brushes();

        for simulator in &self.movement
        {
            manager.brush(simulator.id()).draw_map_preview_movement_simulation(
                camera,
                brushes,
                drawer,
                &self.animators,
                simulator
            );
        }

        for brush in manager
            .visible_brushes(window, camera)
            .iter()
            .filter(|brush| !is_moving_brush!(manager, brush.id()) && !brush.has_sprite())
        {
            brush.draw_map_preview(camera, drawer, self.animators.get(brush.id()));
        }

        for brush in manager
            .visible_sprites(window, camera)
            .iter()
            .filter(|brush| !is_moving_brush!(manager, brush.id()))
        {
            brush.draw_map_preview_sprite(drawer, self.animators.get(brush.id()));
        }

        for thing in manager.visible_things(window, camera).iter()
        {
            thing.draw_map_preview(drawer, things_catalog, self.animators.get(thing.id()));
        }
    }
}
