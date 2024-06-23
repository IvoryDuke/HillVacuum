//=======================================================================//
// IMPORTS
//
//=======================================================================//

use hill_vacuum_shared::return_if_none;

use super::{
    rect::{Rect, RectTrait},
    tool::DragSelection,
    ActiveTool,
    PreviousActiveTool
};
use crate::{
    map::{
        containers::hv_box,
        drawer::color::Color,
        editor::{
            state::{core::rect, editor_state::InputsPresses},
            DrawBundle,
            ToolUpdateBundle
        }
    },
    utils::misc::Camera
};

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The tool used to zoom in/out the map view.
#[derive(Debug)]
pub(in crate::map::editor::state::core) struct ZoomTool
{
    /// The drag selection.
    drag_selection:           Rect,
    /// The tool that was being used before enabling the zoom tool.
    pub previous_active_tool: PreviousActiveTool
}

impl DragSelection for ZoomTool
{
    #[inline]
    fn drag_selection(&self) -> Option<Rect> { self.drag_selection.into() }
}

impl ZoomTool
{
    /// Returns a new [`ActiveTool`] in its zoom tool variant.
    #[inline]
    pub fn tool(drag_selection: Rect, active_tool: &mut ActiveTool) -> ActiveTool
    {
        ActiveTool::Zoom(Self {
            drag_selection,
            previous_active_tool: hv_box!(std::mem::take(active_tool))
        })
    }

    /// Updates the tool.
    #[allow(unreachable_code)]
    #[inline]
    pub fn update<'a>(
        &'a mut self,
        bundle: &mut ToolUpdateBundle,
        inputs: &InputsPresses
    ) -> Option<&'a mut PreviousActiveTool>
    {
        let ToolUpdateBundle {
            window,
            camera,
            cursor,
            ..
        } = bundle;

        rect::update!(
            self.drag_selection,
            cursor.world_snapped(),
            camera.scale(),
            inputs.left_mouse.pressed(),
            inputs.left_mouse.just_pressed(),
            {
                return Some(&mut self.previous_active_tool);
            },
            hull,
            {
                camera.scale_viewport_ui_constricted_to_hull(window, &hull, 0f32);
                return Some(&mut self.previous_active_tool);
            }
        );

        None
    }

    /// Draws the tool.
    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle)
    {
        let DrawBundle { drawer, .. } = bundle;
        drawer.hull(&return_if_none!(self.drag_selection.hull()), Color::Hull);
    }
}
