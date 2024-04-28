//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{fmt::Display, ops::RangeInclusive};

use bevy::prelude::Vec2;
use bevy_egui::egui::{self, emath::Numeric};
use shared::match_or_panic;

use super::{
    draw_selected_and_non_selected_brushes,
    tool::{ChangeConditions, EnabledTool, SubTool},
    ActiveTool
};
use crate::{
    map::{
        brush::{convex_polygon::ConvexPolygon, RotateResult},
        drawer::{drawing_resources::DrawingResources, texture::TextureInterface},
        editor::{
            cursor_pos::Cursor,
            state::{
                core::tool::subtools_buttons,
                editor_state::{edit_target, InputsPresses, ToolsSettings},
                edits_history::EditsHistory,
                grid::Grid,
                manager::EntitiesManager,
                ui::ToolsButtons
            },
            DrawBundle,
            StateUpdateBundle,
            ToolUpdateBundle
        },
        hv_vec,
        HvVec
    },
    utils::{
        identifiers::{EntityId, Id},
        math::{
            angles::vectors_angle_cosine,
            points::{rotate_point, vertexes_orientation, VertexesOrientation},
            AroundEqual,
            FastNormalize
        },
        misc::{Camera, PointInsideUiHighlight, TakeValue, Toggle}
    }
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[derive(Debug)]
enum Status
{
    Inactive(()),
    MovePivot,
    MovePivotUi,
    Drag(Vec2, Vec2, HvVec<(Id, ConvexPolygon)>)
}

impl Default for Status
{
    #[inline]
    #[must_use]
    fn default() -> Self { Self::Inactive(()) }
}

impl EnabledTool for Status
{
    type Item = SubTool;

    #[inline]
    fn is_tool_enabled(&self, tool: Self::Item) -> bool
    {
        tool == match self
        {
            Status::MovePivotUi => SubTool::RotatePivot,
            _ => return false
        }
    }
}

//=======================================================================//

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub(in crate::map::editor::state) enum RotateSnap
{
    Free,
    Fixed(u16)
}

impl Display for RotateSnap
{
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        match self
        {
            RotateSnap::Free => write!(f, "Free"),
            RotateSnap::Fixed(angle) => write!(f, "{angle}")
        }
    }
}

impl Default for RotateSnap
{
    #[inline]
    #[must_use]
    fn default() -> Self { Self::Fixed(Self::MAX_ROTATE_ANGLE) }
}

impl Numeric for RotateSnap
{
    const INTEGRAL: bool = true;
    const MAX: Self = Self::Fixed(Self::MAX_ROTATE_ANGLE);
    const MIN: Self = Self::Free;

    #[inline]
    #[must_use]
    fn to_f64(self) -> f64
    {
        match self
        {
            RotateSnap::Free => 0f64,
            RotateSnap::Fixed(n) => f64::from(n)
        }
    }

    #[inline]
    #[must_use]
    fn from_f64(num: f64) -> Self
    {
        #[allow(clippy::cast_possible_truncation)]
        #[allow(clippy::cast_sign_loss)]
        let num = num as u16;

        match num
        {
            0 => Self::Free,
            _ => Self::Fixed(num)
        }
    }
}

impl RotateSnap
{
    const MAX_ROTATE_ANGLE: u16 = 90;
    pub(in crate::map::editor) const MIN_ROTATE_ANGLE: u16 = 5;

    #[inline]
    #[must_use]
    pub const fn range() -> RangeInclusive<RotateSnap> { RotateSnap::MIN..=RotateSnap::MAX }

    #[inline]
    pub fn decrease(&mut self)
    {
        if let RotateSnap::Fixed(snap) = self
        {
            if *snap == Self::MIN_ROTATE_ANGLE
            {
                *self = RotateSnap::Free;
                return;
            }

            *snap -= Self::MIN_ROTATE_ANGLE;
        }
    }

    #[inline]
    pub fn increase(&mut self)
    {
        match self
        {
            RotateSnap::Free => *self = RotateSnap::Fixed(5),
            RotateSnap::Fixed(snap) =>
            {
                if *snap == Self::MAX_ROTATE_ANGLE
                {
                    return;
                }

                *snap += Self::MIN_ROTATE_ANGLE;
            }
        };
    }

    #[inline]
    #[must_use]
    pub fn snap_angle(self, angle: f32) -> f32
    {
        if let Self::Fixed(snap) = self
        {
            #[allow(clippy::cast_possible_truncation)]
            #[allow(clippy::cast_sign_loss)]
            return f32::from((angle.to_degrees() as u16 / snap) * snap).to_radians();
        }

        angle
    }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

#[derive(Debug)]
pub(in crate::map::editor::state::core) struct RotateTool
{
    status: Status,
    pivot:  Vec2
}

impl RotateTool
{
    #[inline]
    pub fn tool(manager: &EntitiesManager, settings: &ToolsSettings) -> ActiveTool
    {
        ActiveTool::Rotate(RotateTool {
            status: Status::Inactive(()),
            pivot:  Self::pivot(manager, settings)
        })
    }

    //==============================================================
    // Info

    #[inline]
    #[must_use]
    fn pivot(manager: &EntitiesManager, settings: &ToolsSettings) -> Vec2
    {
        if settings.entity_editing()
        {
            return manager.selected_brushes_center().unwrap();
        }

        manager.selected_textured_brushes_center().unwrap()
    }

    #[inline]
    #[must_use]
    pub const fn ongoing_multi_frame_changes(&self) -> bool
    {
        matches!(self.status, Status::Drag(..))
    }

    #[inline]
    #[must_use]
    const fn cursor_pos(&self, cursor: &Cursor) -> Vec2
    {
        match &self.status
        {
            Status::Inactive(()) | Status::Drag(..) => cursor.world(),
            Status::MovePivot | Status::MovePivotUi => cursor.world_snapped()
        }
    }

    //==============================================================
    // Update

    #[inline]
    fn snap(drawing_resources: &DrawingResources, manager: &mut EntitiesManager)
    {
        for mut brush in manager.selected_brushes_mut()
        {
            _ = brush.snap_vertexes(drawing_resources, Grid::new(1, false, false));
        }
    }

    #[inline]
    pub fn disable_subtool(&mut self)
    {
        if matches!(self.status, Status::MovePivotUi)
        {
            self.status = Status::default();
        }
    }

    #[inline]
    pub fn update(
        &mut self,
        bundle: &mut ToolUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        settings: &mut ToolsSettings,
        grid_size: i16
    )
    {
        let ToolUpdateBundle { camera, cursor, .. } = bundle;

        if inputs.plus.just_pressed()
        {
            settings.rotate_snap.increase();
        }
        else if inputs.minus.just_pressed()
        {
            settings.rotate_snap.decrease();
        }

        let cursor_pos = self.cursor_pos(cursor);

        match &mut self.status
        {
            Status::Inactive(()) =>
            {
                if inputs.alt_pressed()
                {
                    if let Some(dir) = inputs.directional_keys_vector(grid_size)
                    {
                        self.pivot += dir;
                    }
                }
                else if inputs.right.just_pressed()
                {
                    self.rotate_brushes_cw(bundle, manager, edits_history, settings);
                }
                else if inputs.left.just_pressed()
                {
                    self.rotate_brushes_ccw(bundle, manager, edits_history, settings);
                }
                else if inputs.left_mouse.just_pressed()
                {
                    if self.pivot.is_point_inside_ui_highlight(cursor_pos, camera.scale())
                    {
                        self.status = Status::MovePivot;
                        return;
                    }

                    self.status = Status::Drag(cursor_pos, cursor_pos, hv_vec![]);
                }
            },
            Status::MovePivot =>
            {
                if inputs.left_mouse.pressed()
                {
                    self.pivot = cursor_pos;
                    return;
                }

                self.status = Status::default();
            },
            Status::MovePivotUi =>
            {
                if let Some(dir) = inputs.directional_keys_vector(grid_size)
                {
                    self.pivot += dir;
                }
                else if inputs.left_mouse.pressed()
                {
                    self.pivot = self.cursor_pos(cursor);
                }
            },
            Status::Drag(last_pos, start_pos, backup_polygons) =>
            {
                Self::rotate_brushes_with_mouse(
                    bundle,
                    manager,
                    settings,
                    last_pos,
                    self.pivot,
                    cursor_pos,
                    backup_polygons
                );

                if inputs.left_mouse.pressed()
                {
                    return;
                }

                let angle = settings.rotate_snap.snap_angle(
                    vectors_angle_cosine(*start_pos - self.pivot, *last_pos - self.pivot).acos()
                );

                if !angle.around_equal(&0f32) && !angle.around_equal(&std::f32::consts::TAU)
                {
                    if settings.entity_editing()
                    {
                        edits_history
                            .polygon_edit_cluster(backup_polygons.take_value().into_iter());
                    }
                    else
                    {
                        edits_history.texture_angle_delta(
                            manager.selected_textured_ids().copied(),
                            angle.to_degrees()
                        );
                    }
                }

                Self::snap(bundle.drawing_resources, manager);
                self.status = Status::default();
            }
        };
    }

    #[inline]
    fn rotate_brushes_with_keyboard(
        &self,
        bundle: &ToolUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        settings: &ToolsSettings,
        direction: f32
    )
    {
        let angle = match settings.rotate_snap
        {
            RotateSnap::Free => 1f32 * direction,
            RotateSnap::Fixed(n) => f32::from(n) * direction
        }
        .to_radians();

        let mut backup_polygons = hv_vec![];

        edit_target!(
            settings.target_switch(),
            |rotate_texture| {
                if Self::rotate_brushes(
                    bundle,
                    manager,
                    self.pivot,
                    angle,
                    rotate_texture,
                    &mut backup_polygons
                )
                {
                    edits_history.polygon_edit_cluster(backup_polygons.take_value().into_iter());
                    Self::snap(bundle.drawing_resources, manager);
                }
            },
            {
                if Self::rotate_textures(bundle, manager, angle)
                {
                    edits_history.texture_angle_delta(
                        manager.selected_textured_brushes().map(EntityId::id),
                        angle.to_degrees()
                    );
                }
            }
        );
    }

    #[inline]
    fn rotate_brushes_cw(
        &self,
        bundle: &mut ToolUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        settings: &ToolsSettings
    )
    {
        self.rotate_brushes_with_keyboard(bundle, manager, edits_history, settings, -1f32);
    }

    #[inline]
    fn rotate_brushes_ccw(
        &self,
        bundle: &mut ToolUpdateBundle,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory,
        settings: &ToolsSettings
    )
    {
        self.rotate_brushes_with_keyboard(bundle, manager, edits_history, settings, 1f32);
    }

    #[inline]
    fn rotate_brushes_with_mouse(
        bundle: &ToolUpdateBundle,
        manager: &mut EntitiesManager,
        settings: &ToolsSettings,
        pos: &mut Vec2,
        pivot: Vec2,
        cursor_pos: Vec2,
        backup_polygons: &mut HvVec<(Id, ConvexPolygon)>
    )
    {
        if cursor_pos.around_equal_narrow(&pivot) || cursor_pos.around_equal_narrow(pos)
        {
            return;
        }

        // Get the angle in radians.
        let angle = vectors_angle_cosine(*pos - pivot, cursor_pos - pivot).acos();

        if angle.is_nan()
        {
            return;
        }

        // "Snap to grid" using degrees conversion.
        let mut angle = settings.rotate_snap.snap_angle(angle);

        if angle == 0f32
        {
            return;
        }

        // Invert if rotation is clockwise.
        if let VertexesOrientation::Clockwise = vertexes_orientation(&[pivot, *pos, cursor_pos])
        {
            angle.toggle();
        }

        // Rotate.
        if edit_target!(
            settings.target_switch(),
            |rotate_texture| {
                Self::rotate_brushes(bundle, manager, pivot, angle, rotate_texture, backup_polygons)
            },
            Self::rotate_textures(bundle, manager, angle)
        )
        {
            // Update last position value.
            *pos = rotate_point(*pos, pivot, angle);
        }
    }

    #[inline]
    fn rotate_brushes(
        bundle: &ToolUpdateBundle,
        manager: &mut EntitiesManager,
        pivot: Vec2,
        angle: f32,
        rotate_texture: bool,
        backup_polygons: &mut HvVec<(Id, ConvexPolygon)>
    ) -> bool
    {
        let mut payloads = hv_vec![];

        let valid = manager.test_operation_validity(|manager| {
            manager.selected_brushes_mut().find_map(|mut brush| {
                match brush.check_rotate(bundle.drawing_resources, pivot, angle, rotate_texture)
                {
                    RotateResult::Invalid => brush.id().into(),
                    RotateResult::Valid(payload) =>
                    {
                        payloads.push(payload);
                        None
                    }
                }
            })
        });

        if !valid
        {
            return false;
        }

        if backup_polygons.is_empty()
        {
            backup_polygons
                .extend(manager.selected_brushes().map(|brush| (brush.id(), brush.polygon())));
        }

        for payload in payloads
        {
            manager
                .brush_mut(payload.id())
                .set_rotation_coordinates(bundle.drawing_resources, payload);
        }

        true
    }

    #[inline]
    fn rotate_textures(bundle: &ToolUpdateBundle, manager: &mut EntitiesManager, angle: f32)
        -> bool
    {
        let angle = angle.to_degrees();

        let valid = manager.test_operation_validity(|manager| {
            manager.selected_brushes_with_sprite_mut().find_map(|mut brush| {
                let prev_angle = brush.texture_settings().unwrap().angle();

                (!brush
                    .check_texture_angle(bundle.drawing_resources, prev_angle - angle.to_degrees()))
                .then_some(brush.id())
            })
        });

        if !valid
        {
            return false;
        }

        for mut brush in manager.selected_textured_brushes_mut()
        {
            let prev_angle = brush.texture_settings().unwrap().angle();
            _ = brush.set_texture_angle(bundle.drawing_resources, prev_angle - angle);
        }

        true
    }

    #[inline]
    pub fn update_pivot(&mut self, manager: &EntitiesManager, settings: &ToolsSettings)
    {
        if !self.ongoing_multi_frame_changes()
        {
            self.pivot = Self::pivot(manager, settings);
        }
    }

    //==============================================================
    // Draw

    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle, manager: &EntitiesManager)
    {
        draw_selected_and_non_selected_brushes!(bundle, manager);

        let DrawBundle { drawer, cursor, .. } = bundle;

        drawer.square_highlight(self.pivot, Color::ToolCursor);

        if !matches!(self.status, Status::Drag(..))
        {
            drawer.square_highlight(cursor.world_snapped(), Color::ToolCursor);
            return;
        }

        let cursor_pos = self.cursor_pos(cursor);
        let pivot_to_cursor_distance = (cursor_pos - self.pivot).length();
        let last_pos = self.pivot +
            (match_or_panic!(self.status, Status::Drag(last_pos, ..), last_pos) - self.pivot)
                .fast_normalize() *
                pivot_to_cursor_distance;

        drawer.square_highlight(cursor.world(), Color::ToolCursor);
        drawer.square_highlight(last_pos, Color::ToolCursor);
        drawer.line(self.pivot, cursor_pos, Color::ToolCursor);
        bundle
            .drawer
            .circle(self.pivot, 64, pivot_to_cursor_distance, Color::ToolCursor);
    }

    #[inline]
    pub fn ui(&mut self, ui: &mut egui::Ui, settings: &mut ToolsSettings)
    {
        ui.label(egui::RichText::new("ROTATE TOOL"));

        settings.ui(ui, !self.ongoing_multi_frame_changes());

        ui.label(egui::RichText::new(format!(
            "Pivot: [{:.2}, {:.2}]",
            self.pivot.x, self.pivot.y
        )));

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Angle:"));

            ui.add(
                egui::Slider::new(&mut settings.rotate_snap, RotateSnap::range())
                    .show_value(false)
                    .step_by(f64::from(RotateSnap::MIN_ROTATE_ANGLE))
                    .integer()
            );
            ui.label(egui::RichText::new(format!("{}", settings.rotate_snap)));
        });
    }

    #[inline]
    pub fn draw_sub_tools(
        &mut self,
        ui: &mut egui::Ui,
        bundle: &StateUpdateBundle,
        buttons: &mut ToolsButtons,
        tool_change_conditions: &ChangeConditions
    )
    {
        subtools_buttons!(
            self.status,
            ui,
            bundle,
            buttons,
            tool_change_conditions,
            (RotatePivot, Status::MovePivotUi, Status::MovePivotUi)
        );
    }
}
