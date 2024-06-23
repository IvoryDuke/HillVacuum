//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::Vec2;
use bevy_egui::egui;
use hill_vacuum_shared::return_if_none;

use super::{
    cursor_delta::CursorDelta,
    draw_selected_and_non_selected_brushes,
    tool::OngoingMultiframeChange,
    ActiveTool
};
use crate::{
    map::{
        brush::{
            convex_polygon::{ConvexPolygon, ShearInfo},
            ShearResult
        },
        containers::HvVec,
        editor::{
            hv_vec,
            state::{
                editor_state::InputsPresses,
                edits_history::EditsHistory,
                grid::Grid,
                manager::EntitiesManager
            },
            DrawBundle,
            ToolUpdateBundle
        }
    },
    utils::{
        hull::{Hull, Side},
        identifiers::{EntityId, Id},
        misc::Camera
    }
};

//=======================================================================//
// ENUM
//
//=======================================================================//

/// The state of the tool.
#[derive(Debug)]
enum Status
{
    /// Shearing by keyboard.
    Keyboard,
    /// Shearing by mouse drag.
    Drag(CursorDelta, Option<ShearInfo>, HvVec<(Id, ConvexPolygon)>)
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The shear tool.
#[derive(Debug)]
pub(in crate::map::editor::state::core) struct ShearTool
{
    /// The state.
    status:        Status,
    /// The outline of the [`Brush`]es.
    outline:       Hull,
    /// The selected side of the outline.
    selected_side: Side
}

impl OngoingMultiframeChange for ShearTool
{
    #[inline]
    fn ongoing_multi_frame_change(&self) -> bool { matches!(self.status, Status::Drag(..)) }
}

impl ShearTool
{
    /// Return an [`ActiveTool`] in its shear tool variant.
    #[inline]
    pub fn tool(manager: &EntitiesManager, grid: Grid) -> ActiveTool
    {
        ActiveTool::Shear(ShearTool {
            status:        Status::Keyboard,
            outline:       Self::outline(manager, grid),
            selected_side: Side::Top
        })
    }

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
        grid: Grid
    )
    {
        let ToolUpdateBundle { camera, cursor, .. } = bundle;

        match &mut self.status
        {
            Status::Keyboard =>
            {
                if inputs.tab.just_pressed()
                {
                    if inputs.alt_pressed()
                    {
                        self.previous_side();
                    }
                    else
                    {
                        self.next_side();
                    }
                }

                if let Some(delta) = inputs.directional_keys_vector(grid.size())
                {
                    let mut backup_polygons = hv_vec![];

                    _ = return_if_none!(Self::shear_brushes(
                        manager,
                        grid,
                        self.selected_side,
                        &mut self.outline,
                        delta,
                        &mut backup_polygons
                    ));

                    Self::push_edit(edits_history, backup_polygons);
                }
                else if inputs.left_mouse.just_pressed()
                {
                    self.check_shear_side_proximity(cursor.world_snapped(), camera.scale());
                }
            },
            Status::Drag(drag, info, backup_polygons) =>
            {
                drag.conditional_update(cursor, grid, |delta| {
                    let i = return_if_none!(
                        Self::shear_brushes(
                            manager,
                            grid,
                            self.selected_side,
                            &mut self.outline,
                            delta,
                            backup_polygons
                        ),
                        false
                    );

                    match info
                    {
                        Some(info) =>
                        {
                            let delta = if delta.y == 0f32 { delta.x } else { delta.y };
                            *info = info.with_delta(info.delta() + delta);
                        },
                        None => *info = i.into()
                    };

                    true
                });

                if inputs.left_mouse.pressed()
                {
                    return;
                }

                if let Some(info) = info
                {
                    if info.delta() != 0f32
                    {
                        Self::push_edit(edits_history, std::mem::take(backup_polygons));
                    }
                }

                self.status = Status::Keyboard;
            }
        };
    }

    /// Pushes the edit to the [`EditsHistory`].
    #[inline]
    fn push_edit(edits_history: &mut EditsHistory, backup_polygons: HvVec<(Id, ConvexPolygon)>)
    {
        for (id, polygon) in backup_polygons
        {
            edits_history.polygon_edit(id, polygon);
        }
    }

    /// Shears the [`Brush`]es if possible.
    #[inline]
    fn shear_brushes(
        manager: &mut EntitiesManager,
        grid: Grid,
        selected_side: Side,
        outline: &mut Hull,
        delta: Vec2,
        backup_polygons: &mut HvVec<(Id, ConvexPolygon)>
    ) -> Option<ShearInfo>
    {
        /// Shears the brushes according to the parameters.
        macro_rules! shear {
            ($xy:ident, $dimension:ident, $pivot:ident, $check:ident, $shear:ident) => {{
                let info = ShearInfo::new(delta.$xy, outline.$dimension(), outline.$pivot());
                let mut payloads = hv_vec![capacity; manager.selected_brushes_amount()];

                let valid = manager.test_operation_validity(|manager| {
                    manager.selected_brushes().find_map(|brush| {
                        match brush.$check(&info)
                        {
                            ShearResult::Valid(payload) =>
                            {
                                payloads.push(payload);
                                None
                            },
                            ShearResult::Invalid => brush.id().into()
                        }
                    })
                });

                if !valid
                {
                    return None;
                }

                if backup_polygons.is_empty()
                {
                    backup_polygons
                        .extend(manager.selected_brushes().map(|brush| (brush.id(), brush.polygon())));
                }

                for payload in payloads.into_iter()
                {
                    manager.brush_mut(payload.id()).$shear(payload);
                }

                info
            }};
        }

        let info = match selected_side
        {
            Side::Top => shear!(x, height, bottom, check_horizontal_shear, set_x_coordinates),
            Side::Right =>
            {
                shear!(y, width, left, check_vertical_shear, set_y_coordinates)
            },
            Side::Bottom => shear!(x, height, top, check_horizontal_shear, set_x_coordinates),
            Side::Left =>
            {
                shear!(y, width, right, check_vertical_shear, set_y_coordinates)
            }
        };

        *outline = Self::outline(manager, grid);

        info.into()
    }

    /// Selects the previous side.
    #[inline]
    fn previous_side(&mut self)
    {
        self.selected_side = match self.selected_side
        {
            Side::Top => Side::Left,
            Side::Right => Side::Top,
            Side::Bottom => Side::Right,
            Side::Left => Side::Bottom
        };
    }

    /// Selects the next side.
    #[inline]
    fn next_side(&mut self)
    {
        self.selected_side = match self.selected_side
        {
            Side::Top => Side::Right,
            Side::Right => Side::Bottom,
            Side::Bottom => Side::Left,
            Side::Left => Side::Top
        };
    }

    /// Checks whever there is a side of the outline near `cursor_pos`.
    #[inline]
    fn check_shear_side_proximity(&mut self, cursor_pos: Vec2, camera_scale: f32)
    {
        self.selected_side = return_if_none!(self.outline.nearby_side(cursor_pos, camera_scale));
        self.status = Status::Drag(CursorDelta::new(cursor_pos), None, hv_vec![]);
    }

    /// Returns the [`Hull`] describing the tool outline.
    #[inline]
    #[must_use]
    fn outline(manager: &EntitiesManager, grid: Grid) -> Hull
    {
        grid.snap_hull(&manager.selected_brushes_hull().unwrap())
    }

    /// Updates the tool outline.
    #[inline]
    pub fn update_outline(&mut self, manager: &EntitiesManager, grid: Grid)
    {
        if !self.ongoing_multi_frame_change()
        {
            self.outline = Self::outline(manager, grid);
        }
    }

    //==============================================================
    // Draw

    /// Draws the tool.
    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle, manager: &EntitiesManager)
    {
        draw_selected_and_non_selected_brushes!(bundle, manager);

        bundle.drawer.hull_with_highlighted_side(
            &self.outline,
            self.selected_side,
            Color::Hull,
            Color::ToolCursor
        );
    }

    /// Draws the UI.
    #[inline]
    pub fn ui(&mut self, ui: &mut egui::Ui)
    {
        ui.label(egui::RichText::new("SHEAR TOOL"));
        ui.label(egui::RichText::new("Side:"));

        ui.horizontal_wrapped(|ui| {
            if let Status::Drag { .. } = self.status
            {
                ui.add_enabled(false, egui::Button::new(egui::RichText::new("Top")));
                ui.add_enabled(false, egui::Button::new(egui::RichText::new("Right")));
                ui.add_enabled(false, egui::Button::new(egui::RichText::new("Bottom")));
                ui.add_enabled(false, egui::Button::new(egui::RichText::new("Left")));

                return;
            }

            let top = ui.button(egui::RichText::new("Top"));
            let right = ui.button(egui::RichText::new("Right"));
            let bottom = ui.button(egui::RichText::new("Bottom"));
            let left = ui.button(egui::RichText::new("Left"));

            for b in [&top, &right, &bottom, &left]
            {
                b.surrender_focus();
            }

            if top.clicked()
            {
                self.selected_side = Side::Top;
            }
            else if right.clicked()
            {
                self.selected_side = Side::Right;
            }
            else if bottom.clicked()
            {
                self.selected_side = Side::Bottom;
            }
            else if left.clicked()
            {
                self.selected_side = Side::Left;
            }

            match self.selected_side
            {
                Side::Right => right.highlight(),
                Side::Top => top.highlight(),
                Side::Bottom => bottom.highlight(),
                Side::Left => left.highlight()
            };
        });
    }
}
