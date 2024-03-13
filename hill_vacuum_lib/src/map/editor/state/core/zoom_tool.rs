//=======================================================================//
// IMPORTS
//
//=======================================================================//

use shared::return_if_none;

use super::{
    drag_area::{DragArea, DragAreaTrait},
    ActiveTool,
    PreviousActiveTool
};
use crate::{
    map::{
        drawer::color::Color,
        editor::{
            state::{core::drag_area, editor_state::InputsPresses},
            DrawBundle,
            ToolUpdateBundle
        },
        hv_box
    },
    utils::misc::Camera
};

//=======================================================================//
// TYPES
//
//=======================================================================//

#[derive(Debug)]
pub(in crate::map::editor::state::core) struct ZoomTool
{
    drag_selection:           DragArea,
    pub previous_active_tool: PreviousActiveTool
}

impl ZoomTool
{
    #[inline]
    pub fn tool(drag_selection: DragArea, active_tool: &mut ActiveTool) -> ActiveTool
    {
        ActiveTool::Zoom(Self {
            drag_selection,
            previous_active_tool: hv_box!(std::mem::take(active_tool))
        })
    }

    #[inline]
    pub const fn drag_selection(&self) -> DragArea { self.drag_selection }

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

        drag_area::update!(
            self.drag_selection,
            cursor.world_snapped(),
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

    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle)
    {
        let DrawBundle { drawer, .. } = bundle;
        drawer.hull(&return_if_none!(self.drag_selection.hull()), Color::Hull);
    }
}
