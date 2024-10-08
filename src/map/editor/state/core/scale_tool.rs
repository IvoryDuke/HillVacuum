//=======================================================================//
// IMPORTS
//
//=======================================================================//

use arrayvec::ArrayVec;
use bevy_egui::egui;
use glam::Vec2;
use hill_vacuum_shared::return_if_none;

use super::{fill_backup_polygons, tool::OngoingMultiframeChange, ActiveTool};
use crate::{
    map::{
        brush::{
            convex_polygon::{ConvexPolygon, ScaleInfo},
            Brush,
            ScalePayload,
            TextureScalePayload,
            TextureScaleResult
        },
        drawer::{color::Color, drawing_resources::DrawingResources},
        editor::{
            cursor::Cursor,
            state::{
                core::draw_selected_and_non_selected_brushes,
                editor_state::{edit_target, TargetSwitch, ToolsSettings},
                grid::Grid,
                manager::EntitiesManager
            },
            DrawBundle,
            StateUpdateBundle,
            ToolUpdateBundle
        }
    },
    utils::{
        collections::hv_vec,
        hull::{Corner, EntityHull, Flip, Hull, ScaleResult},
        identifiers::{EntityId, Id},
        math::AroundEqual,
        misc::{Camera, TakeValue}
    },
    HvVec,
    TextureInterface
};

//=======================================================================//
// MACROS
//
//=======================================================================//

macro_rules! scale_func {
    (
        $bundle:ident,
        $hull:ident,
        $selected_corner:ident,
        $new_corner_position:ident,
        $ret:ty,
        $f:expr
        $(, $scale_texture:ident)?
    ) => {{
        #[inline]
        fn scale<const CAP: usize>(
            bundle: &mut ToolUpdateBundle,
            flip_queue: &ArrayVec<Flip, CAP>,
            hull: &Hull,
            new_hull: &Hull
            $(, $scale_texture: bool)?
        ) -> Option<HvVec<$ret>>
        {
            let info = ScaleInfo::new(hull, new_hull, flip_queue)?;
            let mut payloads = hv_vec![capacity; bundle.manager.selected_brushes_amount()];

            let valid = bundle.manager.test_operation_validity(|manager| {
                manager.selected_brushes_mut(bundle.drawing_resources, bundle.grid).find_map(|mut brush| {
                    $f(bundle.drawing_resources, bundle.grid, &mut brush, &info, &mut payloads $(, $scale_texture)?)
            })});

            if !valid
            {
                return None;
            }

            payloads.into()
        }

        match $hull.scaled($selected_corner, $new_corner_position)
        {
            ScaleResult::None => return,
            ScaleResult::Scale(new_hull) =>
            {
                (
                    new_hull,
                    return_if_none!(scale(
                        $bundle,
                        &ArrayVec::<_, 0>::new(),
                        $hull,
                        &new_hull
                        $(, $scale_texture)?
                    ))
                )
            },
            ScaleResult::Flip(flip_queue, new_hull) =>
            {
                (
                    new_hull,
                    return_if_none!(scale(
                        $bundle,
                        &flip_queue,
                        $hull,
                        &new_hull
                        $(, $scale_texture)?
                    ))
                )
            },
        }
    }};
}

//=======================================================================//

macro_rules! common_scale_textures {
    ($bundle:ident, $hull:ident, $selected_corner:ident, $new_corner_position:ident) => {
        scale_func!(
            $bundle,
            $hull,
            $selected_corner,
            $new_corner_position,
            TextureScalePayload,
            |drawing_resources,
             grid,
             brush: &mut Brush,
             info,
             payloads: &mut HvVec<TextureScalePayload>| {
                let id = brush.id();

                match brush.check_texture_scale(drawing_resources, grid, info)
                {
                    TextureScaleResult::Valid(p) =>
                    {
                        payloads.push(p);
                        None
                    },
                    TextureScaleResult::Invalid => id.into()
                }
            }
        )
    };
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The state of the tool.
enum Status
{
    /// Scaling with the keyboard.
    Keyboard,
    /// Scaling with cursor drag.
    Drag(HvVec<(Id, ConvexPolygon)>, Vec2, Hull)
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The scale tool.
pub(in crate::map::editor::state::core) struct ScaleTool
{
    /// The state of the tool.
    status:          Status,
    /// The outline of the tool.
    outline:         Hull,
    /// The selected [`Corner`] of the outline.
    selected_corner: Corner
}

impl OngoingMultiframeChange for ScaleTool
{
    #[inline]
    fn ongoing_multi_frame_change(&self) -> bool { matches!(self.status, Status::Drag(..)) }
}

impl ScaleTool
{
    /// Returns an [`ActiveTool`] in its scale variant.
    #[inline]
    pub fn tool(bundle: &StateUpdateBundle, settings: &ToolsSettings) -> ActiveTool
    {
        ActiveTool::Scale(ScaleTool {
            status:          Status::Keyboard,
            outline:         Self::outline(
                bundle.drawing_resources,
                bundle.manager,
                bundle.grid,
                settings
            )
            .unwrap(),
            selected_corner: Corner::TopLeft
        })
    }

    //==============================================================
    // Info

    /// The cursor position used by the tool.
    #[inline]
    #[must_use]
    const fn cursor_pos(cursor: &Cursor) -> Vec2 { cursor.world_snapped() }

    //==============================================================
    // Update

    /// Updates the tool.
    #[inline]
    pub fn update(&mut self, bundle: &mut ToolUpdateBundle, settings: &mut ToolsSettings)
    {
        match &mut self.status
        {
            Status::Keyboard =>
            {
                if bundle.inputs.tab.just_pressed()
                {
                    if bundle.inputs.alt_pressed()
                    {
                        self.selected_corner = self.selected_corner.previous();
                    }
                    else
                    {
                        self.selected_corner = self.selected_corner.next();
                    }
                }

                if bundle.inputs.left_mouse.just_pressed()
                {
                    let cursor_pos = Self::cursor_pos(bundle.cursor);
                    self.selected_corner = return_if_none!(self
                        .outline
                        .nearby_corner(cursor_pos, bundle.camera.scale()));
                    self.status = Status::Drag(hv_vec![], cursor_pos, self.outline);
                }
                else if let Some(dir) = bundle.inputs.directional_keys_delta()
                {
                    self.keyboard_scale(bundle, settings, dir);
                }
            },
            Status::Drag(backup_polygons, start_pos, hull) =>
            {
                #[inline]
                fn scale_textures(
                    bundle: &mut ToolUpdateBundle,
                    hull: &mut Hull,
                    selected_corner: &mut Corner,
                    new_corner_position: Vec2,
                    backup_polygons: &mut HvVec<(Id, ConvexPolygon)>
                )
                {
                    let (new_hull, payloads) =
                        common_scale_textures!(bundle, hull, selected_corner, new_corner_position);

                    *hull = new_hull;

                    fill_backup_polygons(bundle.manager, backup_polygons);

                    for p in payloads
                    {
                        _ = bundle
                            .manager
                            .brush_mut(bundle.drawing_resources, bundle.grid, p.id())
                            .apply_texture_scale(p);
                    }
                }

                let cursor_pos = Self::cursor_pos(bundle.cursor);

                edit_target!(
                    settings.target_switch(),
                    |scale_textures| {
                        Self::scale_brushes(
                            bundle,
                            hull,
                            &mut self.selected_corner,
                            cursor_pos,
                            backup_polygons,
                            scale_textures
                        );
                    },
                    scale_textures(
                        bundle,
                        hull,
                        &mut self.selected_corner,
                        cursor_pos,
                        backup_polygons
                    )
                );

                if bundle.inputs.left_mouse.pressed()
                {
                    return;
                }

                if cursor_pos.around_equal_narrow(start_pos) || backup_polygons.is_empty()
                {
                    for (id, polygon) in backup_polygons.take_value()
                    {
                        _ = bundle
                            .manager
                            .brush_mut(bundle.drawing_resources, bundle.grid, id)
                            .set_polygon(polygon);
                    }
                }
                else
                {
                    bundle
                        .edits_history
                        .polygon_edit_cluster(backup_polygons.take_value());

                    if settings.entity_editing()
                    {
                        bundle.edits_history.override_edit_tag("Brushes Scale");
                    }
                    else
                    {
                        bundle.edits_history.override_edit_tag("Textures Scale");
                    }
                }

                self.status = Status::Keyboard;
            }
        };
    }

    /// Scales with the keyboard inputs.
    #[inline]
    fn keyboard_scale(&mut self, bundle: &mut ToolUpdateBundle, settings: &ToolsSettings, dir: Vec2)
    {
        #[inline]
        fn scale_textures(
            bundle: &mut ToolUpdateBundle,
            hull: &mut Hull,
            selected_corner: &mut Corner,
            new_corner_position: Vec2
        )
        {
            let (new_hull, payloads) =
                common_scale_textures!(bundle, hull, selected_corner, new_corner_position);

            *hull = new_hull;

            bundle
                .edits_history
                .texture_scale_cluster(payloads.into_iter().map(|p| {
                    (
                        p.id(),
                        bundle
                            .manager
                            .brush_mut(bundle.drawing_resources, bundle.grid, p.id())
                            .apply_texture_scale(p)
                    )
                }));
        }

        let new_corner_position = self.outline.corner_vertex(self.selected_corner) + dir;

        edit_target!(
            settings.target_switch(),
            |scale_texture| {
                let mut backup_polygons = hv_vec![];

                Self::scale_brushes(
                    bundle,
                    &mut self.outline,
                    &mut self.selected_corner,
                    new_corner_position,
                    &mut backup_polygons,
                    scale_texture
                );

                if !backup_polygons.is_empty()
                {
                    bundle
                        .edits_history
                        .polygon_edit_cluster(backup_polygons.take_value().into_iter());
                    bundle.edits_history.override_edit_tag("Brushes Scale");
                }
            },
            scale_textures(
                bundle,
                &mut self.outline,
                &mut self.selected_corner,
                new_corner_position
            )
        );
    }

    /// Scales the selected brushes.
    #[inline]
    fn scale_brushes(
        bundle: &mut ToolUpdateBundle,
        hull: &mut Hull,
        selected_corner: &mut Corner,
        new_corner_position: Vec2,
        backup_polygons: &mut HvVec<(Id, ConvexPolygon)>,
        scale_texture: bool
    )
    {
        let (new_hull, payloads) = scale_func!(
            bundle,
            hull,
            selected_corner,
            new_corner_position,
            ScalePayload,
            |drawing_resources,
             grid,
             brush: &mut Brush,
             info,
             payloads: &mut HvVec<ScalePayload>,
             scale_texture| {
                use crate::map::brush::ScaleResult;

                match brush.check_scale(drawing_resources, grid, info, scale_texture)
                {
                    ScaleResult::Invalid => brush.id().into(),
                    ScaleResult::Valid(p) =>
                    {
                        payloads.push(p);
                        None
                    }
                }
            },
            scale_texture
        );

        *hull = new_hull;

        fill_backup_polygons(bundle.manager, backup_polygons);

        for payload in payloads
        {
            bundle
                .manager
                .brush_mut(bundle.drawing_resources, bundle.grid, payload.id())
                .scale(payload);
        }
    }

    /// Returns the outline of the tool, if any.
    #[inline]
    #[must_use]
    fn outline(
        drawing_resources: &DrawingResources,
        manager: &EntitiesManager,
        grid: &Grid,
        settings: &ToolsSettings
    ) -> Option<Hull>
    {
        #[inline]
        #[must_use]
        fn textures_hull(
            drawing_resources: &DrawingResources,
            manager: &EntitiesManager,
            grid: &Grid
        ) -> Option<Hull>
        {
            if grid.isometric()
            {
                Hull::from_hulls_iter(manager.selected_textured_brushes().map(|brush| {
                    let tex = brush.texture_settings().unwrap();

                    if tex.sprite()
                    {
                        #[allow(clippy::cast_precision_loss)]
                        let half_size =
                            drawing_resources.texture(tex.name()).unwrap().size().x as f32;
                        Hull::new(half_size, -half_size, -half_size, half_size).unwrap() +
                            brush.sprite_pivot().unwrap()
                    }
                    else
                    {
                        brush.hull()
                    }
                }))
            }
            else
            {
                Hull::from_hulls_iter(manager.selected_textured_brushes().map(|brush| {
                    brush.sprite_hull(drawing_resources, grid).unwrap_or(brush.hull())
                }))
            }
        }

        match settings.target_switch()
        {
            TargetSwitch::Entity => manager.selected_brushes_hull(),
            TargetSwitch::Both =>
            {
                manager.selected_brushes_hull().map(|hull| {
                    hull.merged(&textures_hull(drawing_resources, manager, grid).unwrap())
                })
            },
            TargetSwitch::Texture => textures_hull(drawing_resources, manager, grid)
        }
        .map(|hull| grid.snap_hull(&hull))
    }

    /// Updates the outline of the tool.
    #[inline]
    pub fn update_outline(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &EntitiesManager,
        grid: &Grid,
        settings: &ToolsSettings
    )
    {
        self.outline = Self::outline(drawing_resources, manager, grid, settings).unwrap();
    }

    //==============================================================
    // Draw

    /// Draws the tool.
    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle)
    {
        draw_selected_and_non_selected_brushes!(bundle);

        let DrawBundle { drawer, .. } = bundle;

        match &self.status
        {
            Status::Keyboard =>
            {
                drawer.hull_with_corner_highlights(
                    &self.outline,
                    self.selected_corner,
                    Color::Hull,
                    Color::ToolCursor
                );
            },
            Status::Drag(_, _, hull) =>
            {
                drawer.hull_with_corner_highlights(
                    hull,
                    self.selected_corner,
                    Color::Hull,
                    Color::ToolCursor
                );
            }
        };
    }

    /// Draws the UI of the tool.
    #[inline]
    pub fn ui(&mut self, ui: &mut egui::Ui, settings: &mut ToolsSettings)
    {
        ui.label(egui::RichText::new("SCALE TOOL"));
        settings.ui(ui, !self.ongoing_multi_frame_change());
        ui.label(egui::RichText::new("Corner:"));

        ui.horizontal_wrapped(|ui| {
            if let Status::Drag { .. } = self.status
            {
                ui.add_enabled(false, egui::Button::new(egui::RichText::new("Top left")));
                ui.add_enabled(false, egui::Button::new(egui::RichText::new("Top right")));
                ui.add_enabled(false, egui::Button::new(egui::RichText::new("Bottom left")));
                ui.add_enabled(false, egui::Button::new(egui::RichText::new("Bottom right")));

                return;
            }

            let top_left = ui.button(egui::RichText::new("Top left"));
            let top_right = ui.button(egui::RichText::new("Top right"));
            let bottom_right = ui.button(egui::RichText::new("Bottom right"));
            let bottom_left = ui.button(egui::RichText::new("Bottom left"));

            for b in [&top_left, &top_right, &bottom_left, &bottom_right]
            {
                b.surrender_focus();
            }

            if top_left.clicked()
            {
                self.selected_corner = Corner::TopLeft;
            }
            else if top_right.clicked()
            {
                self.selected_corner = Corner::TopRight;
            }
            else if bottom_left.clicked()
            {
                self.selected_corner = Corner::BottomLeft;
            }
            else if bottom_right.clicked()
            {
                self.selected_corner = Corner::BottomRight;
            }

            match self.selected_corner
            {
                Corner::TopRight => top_right.highlight(),
                Corner::TopLeft => top_left.highlight(),
                Corner::BottomLeft => bottom_left.highlight(),
                Corner::BottomRight => bottom_right.highlight()
            };
        });
    }
}
