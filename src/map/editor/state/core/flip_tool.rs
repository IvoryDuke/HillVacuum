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
        drawer::{color::Color, drawing_resources::DrawingResources},
        editor::{
            state::{
                editor_state::{edit_target, ToolsSettings},
                grid::Grid,
                manager::EntitiesManager
            },
            DrawBundle,
            StateUpdateBundle,
            ToolUpdateBundle
        }
    },
    utils::{
        hull::{Flip, Hull},
        identifiers::EntityId
    }
};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The flip tool.
pub(in crate::map::editor::state::core) struct FlipTool(Hull);

impl FlipTool
{
    /// Returns an [`ActiveTool`] in its flip tool variant.
    #[inline]
    pub fn tool(bundle: &StateUpdateBundle) -> ActiveTool
    {
        ActiveTool::Flip(Self(Self::outline(bundle.manager, bundle.grid)))
    }

    /// Updates the tool.
    #[inline]
    pub fn update(&mut self, bundle: &mut ToolUpdateBundle, settings: &ToolsSettings)
    {
        let dir = return_if_none!(bundle.inputs.directional_keys_delta());

        edit_target!(
            settings.target_switch(),
            |flip_texture| {
                #[allow(clippy::missing_docs_in_private_items)]
                type FlipSteps = (
                    fn(&mut Brush, &DrawingResources, &Grid, f32, bool) -> bool,
                    fn(&mut Brush, f32, bool),
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

                let valid = bundle.manager.test_operation_validity(|manager| {
                    manager
                        .selected_brushes_mut(bundle.drawing_resources, bundle.grid)
                        .find_map(|mut brush| {
                            (!check(
                                &mut brush,
                                bundle.drawing_resources,
                                bundle.grid,
                                flip.mirror(),
                                flip_texture
                            ))
                            .then_some(brush.id())
                        })
                });

                if !valid
                {
                    return;
                }

                for mut brush in bundle
                    .manager
                    .selected_brushes_mut(bundle.drawing_resources, bundle.grid)
                {
                    func(&mut brush, flip.mirror(), flip_texture);
                }

                bundle.edits_history.flip(
                    bundle.manager.selected_brushes_ids().copied(),
                    flip,
                    flip_texture
                );
                self.update_outline(bundle.manager, bundle.grid);
            },
            {
                let y = if dir.x == 0f32
                {
                    for mut brush in bundle
                        .manager
                        .selected_brushes_mut(bundle.drawing_resources, bundle.grid)
                    {
                        brush.flip_scale_y();
                    }

                    true
                }
                else
                {
                    for mut brush in bundle
                        .manager
                        .selected_brushes_mut(bundle.drawing_resources, bundle.grid)
                    {
                        brush.flip_texture_scale_x();
                    }

                    false
                };

                bundle
                    .edits_history
                    .texture_flip(bundle.manager.selected_brushes_ids().copied(), y);
            }
        );
    }

    /// Updates the brushes outline.
    #[inline]
    #[must_use]
    fn outline(manager: &EntitiesManager, grid: &Grid) -> Hull
    {
        grid.snap_hull(&manager.selected_brushes_hull().unwrap())
    }

    /// Updates the brushes outline.
    #[inline]
    pub fn update_outline(&mut self, manager: &EntitiesManager, grid: &Grid)
    {
        self.0 = Self::outline(manager, grid);
    }

    /// Draws the tool.
    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle)
    {
        draw_selected_and_non_selected_brushes!(bundle);
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
