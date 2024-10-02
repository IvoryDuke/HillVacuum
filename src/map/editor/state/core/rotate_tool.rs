//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{fmt::Display, ops::RangeInclusive};

use bevy_egui::egui::{self, emath::Numeric};
use glam::Vec2;
use hill_vacuum_shared::{match_or_panic, return_if_none};

use super::{
    draw_selected_and_non_selected_brushes,
    fill_backup_polygons,
    tool::{DisableSubtool, EnabledTool, OngoingMultiframeChange, SubTool},
    ActiveTool
};
use crate::{
    map::{
        brush::{
            convex_polygon::ConvexPolygon,
            RotateResult,
            TextureRotationPayload,
            TextureRotationResult
        },
        drawer::color::Color,
        editor::{
            cursor::Cursor,
            state::{
                core::tool::subtools_buttons,
                editor_state::{edit_target, ToolsSettings},
                manager::EntitiesManager,
                ui::{ToolsButtons, UiBundle}
            },
            DrawBundle,
            ToolUpdateBundle
        }
    },
    utils::{
        collections::hv_vec,
        identifiers::{EntityId, Id},
        math::{
            angles::vectors_angle_cosine,
            points::{rotate_point, vertexes_orientation, VertexesOrientation},
            AroundEqual,
            FastNormalize
        },
        misc::{TakeValue, Toggle}
    },
    HvVec
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The state of the tool.
#[derive(Debug)]
enum Status
{
    /// Inactive.
    Inactive(()),
    /// Moving the pivot.
    MovePivot,
    /// Moving the pivot through the UI.
    MovePivotUi,
    /// Dragging the mouse to rotate.
    Drag(Vec2, HvVec<(Id, ConvexPolygon)>, f32)
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

/// The rotation angle.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub(in crate::map::editor::state) enum RotateAngle
{
    /// Free.
    Free,
    /// A fixed value.
    Fixed(u16)
}

impl Display for RotateAngle
{
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        match self
        {
            RotateAngle::Free => write!(f, "Free"),
            RotateAngle::Fixed(angle) => write!(f, "{angle}")
        }
    }
}

impl Default for RotateAngle
{
    #[inline]
    #[must_use]
    fn default() -> Self { Self::Fixed(Self::MAX_ROTATE_ANGLE) }
}

impl Numeric for RotateAngle
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
            RotateAngle::Free => 0f64,
            RotateAngle::Fixed(n) => f64::from(n)
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

impl RotateAngle
{
    /// The maximum fixed rotation angle.
    const MAX_ROTATE_ANGLE: u16 = 90;
    /// The minimum fixed rotation angle.
    const MIN_ROTATE_ANGLE: u16 = 5;

    /// Returns the range of possible the rotation angles.
    #[inline]
    #[must_use]
    const fn range() -> RangeInclusive<RotateAngle> { RotateAngle::MIN..=RotateAngle::MAX }

    /// Decreases the rotation angle.
    #[inline]
    fn decrease(&mut self)
    {
        if let RotateAngle::Fixed(snap) = self
        {
            if *snap == Self::MIN_ROTATE_ANGLE
            {
                *self = RotateAngle::Free;
                return;
            }

            *snap -= Self::MIN_ROTATE_ANGLE;
        }
    }

    /// Increases the rotation angle.
    #[inline]
    fn increase(&mut self)
    {
        match self
        {
            RotateAngle::Free => *self = RotateAngle::Fixed(5),
            RotateAngle::Fixed(snap) =>
            {
                if *snap == Self::MAX_ROTATE_ANGLE
                {
                    return;
                }

                *snap += Self::MIN_ROTATE_ANGLE;
            }
        };
    }

    /// Returns the rotation angle in radians.
    #[inline]
    #[must_use]
    fn snap_angle(self, angle: f32) -> f32
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
// STRUCTS
//
//=======================================================================//

/// The rotate tool.
#[derive(Debug)]
pub(in crate::map::editor::state::core) struct RotateTool
{
    /// The state of the tool.
    status: Status,
    /// The rotation pivot.
    pivot:  Vec2
}

impl DisableSubtool for RotateTool
{
    #[inline]
    fn disable_subtool(&mut self)
    {
        if matches!(self.status, Status::MovePivotUi)
        {
            self.status = Status::default();
        }
    }
}

impl OngoingMultiframeChange for RotateTool
{
    #[inline]
    fn ongoing_multi_frame_change(&self) -> bool { matches!(self.status, Status::Drag(..)) }
}

impl RotateTool
{
    /// Returns an [`ActiveTool`] in its rotate tool variant.
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

    /// Returns the center of the selected brushes' polygons if the entities are being edited,
    /// otherwise the center of the textures.
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

    /// Returns the cursor position used by the tool.
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

    /// Updates the tool.
    #[inline]
    pub fn update(&mut self, bundle: &mut ToolUpdateBundle, settings: &mut ToolsSettings)
    {
        if bundle.inputs.plus.just_pressed()
        {
            settings.rotate_angle.increase();
        }
        else if bundle.inputs.minus.just_pressed()
        {
            settings.rotate_angle.decrease();
        }

        let cursor_pos = self.cursor_pos(bundle.cursor);

        match &mut self.status
        {
            Status::Inactive(()) =>
            {
                if bundle.inputs.alt_pressed()
                {
                    if let Some(dir) = bundle.inputs.directional_keys_vector(bundle.grid.size())
                    {
                        self.pivot += dir;
                    }
                    else if bundle.inputs.left_mouse.pressed()
                    {
                        self.status = Status::MovePivot;
                    }
                }
                else if bundle.inputs.left_mouse.just_pressed()
                {
                    self.status = Status::Drag(cursor_pos, hv_vec![], 0f32);
                }
                else if !bundle.inputs.ctrl_pressed()
                {
                    if bundle.inputs.right.just_pressed()
                    {
                        self.rotate_brushes_cw(bundle, settings);
                    }
                    else if bundle.inputs.left.just_pressed()
                    {
                        self.rotate_brushes_ccw(bundle, settings);
                    }
                }
            },
            Status::MovePivot =>
            {
                if bundle.inputs.left_mouse.pressed()
                {
                    self.pivot = cursor_pos;
                    return;
                }

                self.status = Status::default();
            },
            Status::MovePivotUi =>
            {
                if let Some(dir) = bundle.inputs.directional_keys_vector(bundle.grid.size())
                {
                    self.pivot += dir;
                }
                else if bundle.inputs.left_mouse.pressed()
                {
                    self.status = Status::MovePivot;
                }
            },
            Status::Drag(last_pos, backup_polygons, angle) =>
            {
                Self::rotate_brushes_with_mouse(
                    bundle,
                    settings,
                    last_pos,
                    self.pivot,
                    cursor_pos,
                    backup_polygons,
                    angle
                );

                if bundle.inputs.left_mouse.pressed()
                {
                    return;
                }

                let angle = angle.rem_euclid(360f32);

                if angle != 0f32
                {
                    bundle
                        .edits_history
                        .polygon_edit_cluster(backup_polygons.take_value());

                    if settings.entity_editing()
                    {
                        bundle.edits_history.override_edit_tag("Brushes Rotation");
                    }
                    else
                    {
                        bundle.edits_history.override_edit_tag("Textures Rotation");
                    }
                }

                self.status = Status::default();
            }
        };
    }

    /// Rotates the selected brushes.
    #[inline]
    fn rotate_brushes_with_keyboard(
        &self,
        bundle: &mut ToolUpdateBundle,
        settings: &ToolsSettings,
        direction: f32
    )
    {
        #[inline]
        fn rotate_textures(bundle: &mut ToolUpdateBundle, pivot: Vec2, angle: f32)
        {
            let payloads =
                return_if_none!(RotateTool::check_textures_rotation(bundle, pivot, angle));

            bundle
                .edits_history
                .texture_rotation_cluster(payloads.into_iter().map(|p| {
                    (
                        p.id(),
                        bundle
                            .manager
                            .brush_mut(bundle.drawing_resources, bundle.grid, p.id())
                            .apply_texture_rotation(&p)
                    )
                }));
        }

        let angle = match settings.rotate_angle
        {
            RotateAngle::Free => 1f32 * direction,
            RotateAngle::Fixed(n) => f32::from(n) * direction
        }
        .rem_euclid(360f32);

        let mut backup_polygons = hv_vec![];

        edit_target!(
            settings.target_switch(),
            |rotate_texture| {
                if Self::rotate_brushes(
                    bundle,
                    self.pivot,
                    angle,
                    rotate_texture,
                    &mut backup_polygons
                )
                {
                    bundle
                        .edits_history
                        .polygon_edit_cluster(backup_polygons.take_value());
                    bundle.edits_history.override_edit_tag("Brushes rotation");
                }
            },
            rotate_textures(bundle, self.pivot, angle)
        );
    }

    /// Rotates the selected brushes clockwise.
    #[inline]
    fn rotate_brushes_cw(&self, bundle: &mut ToolUpdateBundle, settings: &ToolsSettings)
    {
        self.rotate_brushes_with_keyboard(bundle, settings, -1f32);
    }

    /// Rotates the selected brushes counter-clockwise.
    #[inline]
    fn rotate_brushes_ccw(&self, bundle: &mut ToolUpdateBundle, settings: &ToolsSettings)
    {
        self.rotate_brushes_with_keyboard(bundle, settings, 1f32);
    }

    #[inline]
    fn check_textures_rotation(
        bundle: &mut ToolUpdateBundle,
        pivot: Vec2,
        angle: f32
    ) -> Option<HvVec<TextureRotationPayload>>
    {
        let mut payloads = hv_vec![];

        let valid = bundle.manager.test_operation_validity(|manager| {
            manager
                .selected_textured_brushes_mut(bundle.drawing_resources, bundle.grid)
                .find_map(|mut brush| {
                    match brush.check_texture_rotation(
                        bundle.drawing_resources,
                        bundle.grid,
                        pivot,
                        angle
                    )
                    {
                        TextureRotationResult::Invalid => brush.id().into(),
                        TextureRotationResult::Valid(p) =>
                        {
                            payloads.push(p);
                            None
                        }
                    }
                })
        });

        valid.then_some(payloads)
    }

    /// Rotates the selected brushes through the cursor drag.
    #[inline]
    fn rotate_brushes_with_mouse(
        bundle: &mut ToolUpdateBundle,
        settings: &ToolsSettings,
        pos: &mut Vec2,
        pivot: Vec2,
        cursor_pos: Vec2,
        backup_polygons: &mut HvVec<(Id, ConvexPolygon)>,
        cumulative_angle: &mut f32
    )
    {
        #[inline]
        #[must_use]
        fn rotate_textures(
            bundle: &mut ToolUpdateBundle,
            pivot: Vec2,
            angle: f32,
            backup_polygons: &mut HvVec<(Id, ConvexPolygon)>
        ) -> bool
        {
            let payloads =
                return_if_none!(RotateTool::check_textures_rotation(bundle, pivot, angle), false);

            fill_backup_polygons(bundle.manager, backup_polygons);

            for p in payloads
            {
                _ = bundle
                    .manager
                    .brush_mut(bundle.drawing_resources, bundle.grid, p.id())
                    .apply_texture_rotation(&p);
            }

            true
        }

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
        let mut angle = settings.rotate_angle.snap_angle(angle);

        if angle == 0f32
        {
            return;
        }

        // Invert if rotation is clockwise.
        if let VertexesOrientation::Clockwise = vertexes_orientation(&[pivot, *pos, cursor_pos])
        {
            angle.toggle();
        }

        let deg_angle = angle.to_degrees().rem_euclid(360f32);
        *cumulative_angle += deg_angle;

        // Rotate.
        if edit_target!(
            settings.target_switch(),
            |rotate_texture| {
                Self::rotate_brushes(bundle, pivot, deg_angle, rotate_texture, backup_polygons)
            },
            rotate_textures(bundle, pivot, deg_angle, backup_polygons)
        )
        {
            // Update last position value.
            *pos = rotate_point(*pos, pivot, angle);
        }
    }

    /// Rotates the selected brushes. Returns whether it was possible.
    #[inline]
    fn rotate_brushes(
        bundle: &mut ToolUpdateBundle,
        pivot: Vec2,
        angle: f32,
        rotate_texture: bool,
        backup_polygons: &mut HvVec<(Id, ConvexPolygon)>
    ) -> bool
    {
        let mut payloads = hv_vec![];

        let valid = bundle.manager.test_operation_validity(|manager| {
            manager
                .selected_brushes_mut(bundle.drawing_resources, bundle.grid)
                .find_map(|mut brush| {
                    match brush.check_rotation(
                        bundle.drawing_resources,
                        bundle.grid,
                        pivot,
                        angle,
                        rotate_texture
                    )
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

        fill_backup_polygons(bundle.manager, backup_polygons);

        for payload in payloads
        {
            bundle
                .manager
                .brush_mut(bundle.drawing_resources, bundle.grid, payload.id())
                .set_rotation_coordinates(payload);
        }

        true
    }

    //==============================================================
    // Draw

    /// Draws the tool.
    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle)
    {
        draw_selected_and_non_selected_brushes!(bundle);

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

    /// Draws the UI elements.
    #[inline]
    pub fn ui(&mut self, ui: &mut egui::Ui, settings: &mut ToolsSettings)
    {
        ui.label(egui::RichText::new("ROTATE TOOL"));

        settings.ui(ui, !self.ongoing_multi_frame_change());

        ui.label(egui::RichText::new(format!(
            "Pivot: [{:.2}, {:.2}]",
            self.pivot.x, self.pivot.y
        )));

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Angle:"));

            ui.add(
                egui::Slider::new(&mut settings.rotate_angle, RotateAngle::range())
                    .step_by(f64::from(RotateAngle::MIN_ROTATE_ANGLE))
                    .integer()
            );
        });
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
            self.status,
            ui,
            bundle,
            buttons,
            (RotatePivot, Status::MovePivotUi, Status::MovePivotUi)
        );
    }
}
