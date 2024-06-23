//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use hill_vacuum_shared::return_if_none;

use super::{draw_selected_and_non_selected_brushes, tool::ActiveTool};
use crate::{
    map::{
        brush::Brush,
        drawer::drawing_resources::DrawingResources,
        editor::{
            state::{
                editor_state::{edit_target, InputsPresses, ToolsSettings},
                edits_history::EditsHistory,
                grid::Grid,
                manager::EntitiesManager
            },
            DrawBundle,
            ToolUpdateBundle
        }
    },
    utils::{
        hull::{Flip, Hull},
        identifiers::EntityId
    }
};

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The flip tool.
#[derive(Debug)]
pub(in crate::map::editor::state::core) struct FlipTool(Hull);

impl FlipTool
{
    /// Returns an [`ActiveTool`] in its flip tool variant.
    #[inline]
    pub fn tool(manager: &EntitiesManager) -> ActiveTool
    {
        ActiveTool::Flip(Self(manager.selected_brushes_hull().unwrap()))
    }

    /// Updates the tool.
    #[inline]
    pub fn update(
        &mut self,
        bundle: &ToolUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        settings: &ToolsSettings,
        grid: Grid
    )
    {
        let dir = return_if_none!(inputs.directional_keys_vector(grid.size()));

        edit_target!(
            settings.target_switch(),
            |flip_texture| {
                #[allow(clippy::missing_docs_in_private_items)]
                type FlipSteps = (
                    fn(&mut Brush, &DrawingResources, f32, bool) -> bool,
                    fn(&mut Brush, &DrawingResources, f32, bool),
                    Flip
                );

                let (check, func, flip): FlipSteps = if dir.y > 0f32
                {
                    (Brush::check_flip_above, Brush::flip_above, Flip::Above(self.0.top()))
                }
                else if dir.y < 0f32
                {
                    (Brush::check_flip_below, Brush::flip_below, Flip::Below(self.0.bottom()))
                }
                else if dir.x < 0f32
                {
                    (Brush::check_flip_left, Brush::flip_left, Flip::Left(self.0.left()))
                }
                else
                {
                    (Brush::check_flip_right, Brush::flip_right, Flip::Right(self.0.right()))
                };

                let valid = manager.test_operation_validity(|manager| {
                    manager.selected_brushes_mut().find_map(|mut brush| {
                        (!check(&mut brush, bundle.drawing_resources, flip.mirror(), flip_texture))
                            .then_some(brush.id())
                    })
                });

                if !valid
                {
                    return;
                }

                for mut brush in manager.selected_brushes_mut()
                {
                    func(&mut brush, bundle.drawing_resources, flip.mirror(), flip_texture);
                }

                edits_history.flip(manager.selected_brushes_ids().copied(), flip, flip_texture);
                self.update_outline(manager, grid);
            },
            {
                let y = if dir.x == 0f32
                {
                    for mut brush in manager.selected_brushes_mut()
                    {
                        brush.flip_scale_y(bundle.drawing_resources);
                    }

                    true
                }
                else
                {
                    for mut brush in manager.selected_brushes_mut()
                    {
                        brush.flip_texture_scale_x(bundle.drawing_resources);
                    }

                    false
                };

                edits_history.texture_flip(manager.selected_brushes_ids().copied(), y);
            }
        );
    }

    /// Updates the [`Brush`]es outline.
    #[inline]
    pub fn update_outline(&mut self, manager: &EntitiesManager, grid: Grid)
    {
        self.0 = grid.snap_hull(&manager.selected_brushes_hull().unwrap());
    }

    /// Draws the tool.
    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle, manager: &EntitiesManager)
    {
        draw_selected_and_non_selected_brushes!(bundle, manager);
        bundle.drawer.hull(&self.0, Color::ToolCursor);
    }

    /// Draws the UI.
    #[inline]
    pub fn ui(ui: &mut egui::Ui, settings: &mut ToolsSettings)
    {
        ui.label(egui::RichText::new("FLIP TOOL"));
        settings.ui(ui, true);
    }
}
