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
        drawer::color::Color,
        editor::{DrawBundle, ToolUpdateBundle}
    },
    utils::misc::Camera
};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The tool used to zoom in/out the map view.
pub(in crate::map::editor::state::core) struct ZoomTool
{
    /// The rectangular selection.
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
            previous_active_tool: Box::new(std::mem::take(active_tool))
        })
    }

    /// Updates the tool.
    #[allow(unreachable_code)]
    #[inline]
    pub fn update<'a>(
        &'a mut self,
        bundle: &mut ToolUpdateBundle
    ) -> Option<&'a mut PreviousActiveTool>
    {
        self.drag_selection.drag_selection(
            bundle,
            bundle.cursor.world_snapped(),
            &mut self.previous_active_tool,
            |_, bundle, _| bundle.inputs.left_mouse.pressed().into(),
            |_, previous_active_tool| Some(previous_active_tool),
            |bundle, hull, previous_active_tool| {
                bundle
                    .camera
                    .scale_viewport_to_hull(bundle.window, bundle.grid, hull, 0f32);
                Some(previous_active_tool)
            }
        )
    }

    /// Draws the tool.
    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle)
    {
        let DrawBundle { drawer, .. } = bundle;
        drawer.hull(&return_if_none!(self.drag_selection.hull()), Color::Hull);
    }
}
