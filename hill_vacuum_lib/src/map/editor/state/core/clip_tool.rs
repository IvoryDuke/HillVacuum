//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::Vec2;
use bevy_egui::egui;
use proc_macros::{EnumFromUsize, EnumSize};
use shared::{continue_if_none, match_or_panic, return_if_no_match, return_if_none};

use super::{
    draw_selected_and_non_selected_brushes,
    tool::{subtools_buttons, ChangeConditions, EnabledTool, SubTool},
    ActiveTool
};
use crate::{
    map::{
        brush::convex_polygon::ConvexPolygon,
        drawer::{color::Color, drawing_resources::DrawingResources, EditDrawer},
        editor::{
            cursor_pos::Cursor,
            state::{
                editor_state::InputsPresses,
                edits_history::EditsHistory,
                manager::EntitiesManager,
                ui::ToolsButtons
            },
            DrawBundle,
            StateUpdateBundle,
            ToolUpdateBundle
        },
        hv_vec,
        properties::Properties,
        HvVec
    },
    utils::{
        identifiers::{EntityId, Id},
        iterators::FilterSet,
        misc::{next, prev, Camera, TakeValue}
    }
};

//=======================================================================//
// MACROS
//
//=======================================================================//

macro_rules! clip_brushes {
    (
        $self:ident,
        $drawing_resources:ident,
        $manager:ident,
        $edits_history:ident,
        $clip_line:expr,
        $iter:expr
    ) => {
        let mut left_polygons = hv_vec![];
        let mut right_polygons = hv_vec![];
        let mut clipped_brushes = hv_vec![];

        // Until I figure out how to properly annotate the F lifetimes.
        for brush in $iter
        {
            let [left, right] = continue_if_none!(brush.clip($drawing_resources, $clip_line));
            left_polygons.push((left, brush.properties()));
            right_polygons.push((right, brush.properties()));
            clipped_brushes.push(brush.id());
        }

        if clipped_brushes.is_empty()
        {
            $self.0 = Status::default();
            return;
        }

        $edits_history.start_multiframe_edit();

        for id in clipped_brushes
        {
            $manager.despawn_brush(id, $edits_history, true);
        }

        $self.0 = Status::PostClip {
            status: PostClipStatus::default(),
            left_polygons,
            right_polygons
        };
    };
}

//=======================================================================//
// TYPES
//
//=======================================================================//

#[derive(Copy, Clone, Debug)]
struct ClipSide
{
    id:    Id,
    side:  [Vec2; 2],
    index: usize
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[derive(Clone, Copy, Debug, Default, EnumSize, EnumFromUsize)]
enum PostClipStatus
{
    #[default]
    Both,
    Left,
    Right
}

impl PostClipStatus
{
    #[inline]
    fn previous(&mut self) { *self = prev(*self as usize, Self::SIZE).into(); }

    #[inline]
    fn next(&mut self) { *self = next(*self as usize, Self::SIZE).into(); }
}

//=======================================================================//

/// The status of the clip tool.
#[derive(Debug)]
enum Status
{
    Inactive(Option<ClipSide>),
    Active(Vec2, Option<Vec2>),
    PostClip
    {
        status:         PostClipStatus,
        left_polygons:  HvVec<(ConvexPolygon, Properties)>,
        right_polygons: HvVec<(ConvexPolygon, Properties)>
    },
    PickSideUi(Option<ClipSide>)
}

impl Default for Status
{
    #[inline]
    #[must_use]
    fn default() -> Self { Self::Inactive(None) }
}

impl EnabledTool for Status
{
    type Item = SubTool;

    #[inline]
    fn is_tool_enabled(&self, tool: Self::Item) -> bool
    {
        tool == match self
        {
            Status::PickSideUi(_) => SubTool::ClipSide,
            _ => return false
        }
    }
}

//=======================================================================//
// TYPES
//
//=======================================================================//

#[derive(Debug)]
pub(in crate::map::editor::state::core) struct ClipTool(Status);

impl ClipTool
{
    #[inline]
    pub fn tool() -> ActiveTool { ActiveTool::Clip(ClipTool(Status::default())) }

    //==============================================================
    // Info

    #[inline]
    #[must_use]
    pub const fn ongoing_multi_frame_changes(&self) -> bool
    {
        matches!(self.0, Status::PostClip { .. })
    }

    #[inline]
    #[must_use]
    pub fn cursor_pos(&self, cursor: &Cursor) -> Option<Vec2>
    {
        matches!(self.0, Status::Inactive(_) | Status::Active(..) | Status::PickSideUi(_))
            .then_some(cursor.world_snapped())
    }

    //==============================================================
    // Update

    #[inline]
    pub fn disable_subtool(&mut self)
    {
        if matches!(self.0, Status::PickSideUi(_))
        {
            self.0 = Status::default();
        }
    }

    #[inline]
    pub fn update(
        &mut self,
        bundle: &mut ToolUpdateBundle,
        manager: &mut EntitiesManager,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory
    )
    {
        let cursor_pos = self.cursor_pos(bundle.cursor);

        match &mut self.0
        {
            Status::Inactive(clip_side) =>
            {
                let cursor_pos = cursor_pos.unwrap();
                let left_mouse_just_pressed = inputs.left_mouse.just_pressed();
                *clip_side = None;

                if inputs.alt_pressed() && manager.selected_brushes_amount() > 1
                {
                    Self::set_clip_side(manager, clip_side, bundle.cursor, bundle.camera.scale());

                    if !left_mouse_just_pressed
                    {
                        return;
                    }

                    self.clip_brushes_with_side(bundle.drawing_resources, manager, edits_history);
                }
                else if left_mouse_just_pressed
                {
                    self.0 = Status::Active(cursor_pos, None);
                }
            },
            Status::Active(co, ce) =>
            {
                if inputs.esc.just_pressed()
                {
                    self.0 = Status::Inactive(None);
                    return;
                }

                let cursor_pos = cursor_pos.unwrap();

                if *co != cursor_pos
                {
                    *ce = cursor_pos.into();
                }

                if inputs.left_mouse.just_pressed()
                {
                    self.clip_brushes_with_line(bundle.drawing_resources, manager, edits_history);
                }
            },
            Status::PostClip { status, .. } =>
            {
                if inputs.tab.just_pressed()
                {
                    if inputs.alt_pressed()
                    {
                        status.previous();
                    }
                    else
                    {
                        status.next();
                    }
                }
                else if inputs.enter.just_pressed()
                {
                    self.spawn_clipped_brushes(manager, edits_history);
                }
            },
            Status::PickSideUi(clip_side) =>
            {
                *clip_side = None;
                Self::set_clip_side(manager, clip_side, bundle.cursor, bundle.camera.scale());

                if inputs.left_mouse.just_pressed()
                {
                    self.clip_brushes_with_side(bundle.drawing_resources, manager, edits_history);
                }
            }
        };
    }

    #[inline]
    fn set_clip_side(
        manager: &EntitiesManager,
        clip_side: &mut Option<ClipSide>,
        cursor: &Cursor,
        camera_scale: f32
    )
    {
        let cursor_pos = cursor.world();

        let (id, side) = return_if_none!(manager
            .selected_brushes_at_pos(cursor_pos, camera_scale)
            .iter()
            .find_map(|brush| {
                brush
                    .nearby_side(cursor_pos, camera_scale)
                    .map(|side| (brush.id(), side))
            }));

        *clip_side = Some(ClipSide {
            id,
            side: side.0,
            index: side.1
        });
    }

    #[inline]
    fn clip_brushes_with_side(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory
    )
    {
        let clip_side = match_or_panic!(
            &self.0,
            Status::Inactive(clip_side) | Status::PickSideUi(clip_side),
            clip_side
        )
        .unwrap();

        clip_brushes!(
            self,
            drawing_resources,
            manager,
            edits_history,
            &clip_side.side,
            manager
                .selected_brushes()
                .filter_set_with_predicate(clip_side.id, |brush| brush.id())
        );
    }

    #[inline]
    fn clip_brushes_with_line(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory
    )
    {
        let clip_segment = &match_or_panic!(&self.0, Status::Active(co, Some(ce)), [*co, *ce]);

        clip_brushes!(
            self,
            drawing_resources,
            manager,
            edits_history,
            clip_segment,
            manager.selected_brushes()
        );
    }

    #[inline]
    fn spawn_clipped_brushes(
        &mut self,
        manager: &mut EntitiesManager,
        edits_history: &mut EditsHistory
    )
    {
        let (status, mut left_polygons, right_polygons) = match_or_panic!(
            &mut self.0,
            Status::PostClip {
                status,
                left_polygons,
                right_polygons
            },
            (status, left_polygons.take_value(), right_polygons.take_value())
        );

        match status
        {
            PostClipStatus::Both =>
            {
                if left_polygons[0].0.has_sprite()
                {
                    for (poly, _) in &mut left_polygons
                    {
                        _ = poly.remove_texture();
                    }
                }

                for (poly, properties) in left_polygons.into_iter().chain(right_polygons)
                {
                    manager.spawn_brush(poly, edits_history, properties);
                }
            },
            PostClipStatus::Left =>
            {
                for (poly, properties) in left_polygons
                {
                    manager.spawn_brush(poly, edits_history, properties);
                }
            },
            PostClipStatus::Right =>
            {
                for (poly, properties) in right_polygons
                {
                    manager.spawn_brush(poly, edits_history, properties);
                }
            }
        };

        edits_history.end_multiframe_edit();
        self.0 = Status::default();
    }

    //==============================================================
    // Draw

    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle, manager: &EntitiesManager)
    {
        #[inline]
        fn draw_inactive(
            bundle: &mut DrawBundle,
            manager: &EntitiesManager,
            highlighted_side: &Option<ClipSide>
        )
        {
            draw_selected_and_non_selected_brushes!(bundle, manager);

            if let Some(hgl_s) = highlighted_side
            {
                manager.brush(hgl_s.id).draw_extended_side(
                    bundle.window,
                    bundle.camera,
                    &mut bundle.drawer,
                    hgl_s.index,
                    Color::ToolCursor
                );
            }
        }

        if let Some(pos) = self.cursor_pos(bundle.cursor)
        {
            bundle.drawer.square_highlight(pos, Color::ToolCursor);
        }

        match &self.0
        {
            Status::Inactive(hgl_s) | Status::PickSideUi(hgl_s) =>
            {
                draw_inactive(bundle, manager, hgl_s);
            },
            Status::Active(co, ce) =>
            {
                draw_selected_and_non_selected_brushes!(bundle, manager);
                bundle.drawer.square_highlight(*co, Color::ToolCursor);

                // If the clip extremity is in place draw its square and the line
                // going through.
                if let Some(ce) = ce
                {
                    bundle.drawer.line_within_window_bounds(
                        bundle.window,
                        bundle.camera,
                        (*co, *ce),
                        Color::ToolCursor
                    );
                }
            },
            Status::PostClip {
                status,
                left_polygons,
                right_polygons
            } =>
            {
                #[inline]
                fn draw_sprite_with_highlight(
                    polygon: &ConvexPolygon,
                    bundle: &mut DrawBundle,
                    color: Color
                )
                {
                    if polygon.has_sprite()
                    {
                        polygon.draw_sprite_with_highlight(&mut bundle.drawer, color);
                    }
                }

                #[inline]
                fn draw_sprite_highlight(polygon: &ConvexPolygon, drawer: &mut EditDrawer)
                {
                    if polygon.has_sprite()
                    {
                        polygon.draw_sprite_highlight(drawer);
                    }
                }

                draw_selected_and_non_selected_brushes!(bundle, manager);

                match status
                {
                    PostClipStatus::Both =>
                    {
                        for (cp, _) in left_polygons
                        {
                            cp.draw(
                                bundle.camera,
                                &mut bundle.drawer,
                                Color::ClippedPolygonsToSpawn
                            );
                        }

                        for (cp, _) in right_polygons
                        {
                            cp.draw(
                                bundle.camera,
                                &mut bundle.drawer,
                                Color::ClippedPolygonsToSpawn
                            );
                            draw_sprite_highlight(cp, &mut bundle.drawer);
                        }
                    },
                    PostClipStatus::Left =>
                    {
                        for (cp, _) in left_polygons
                        {
                            cp.draw(
                                bundle.camera,
                                &mut bundle.drawer,
                                Color::ClippedPolygonsToSpawn
                            );
                            draw_sprite_with_highlight(cp, bundle, Color::ClippedPolygonsToSpawn);
                        }

                        for (cp, _) in right_polygons
                        {
                            cp.draw(
                                bundle.camera,
                                &mut bundle.drawer,
                                Color::ClippedPolygonsNotToSpawn
                            );
                        }
                    },
                    PostClipStatus::Right =>
                    {
                        for (cp, _) in right_polygons
                        {
                            cp.draw(
                                bundle.camera,
                                &mut bundle.drawer,
                                Color::ClippedPolygonsToSpawn
                            );
                            draw_sprite_with_highlight(cp, bundle, Color::ClippedPolygonsToSpawn);
                        }

                        for (cp, _) in left_polygons
                        {
                            cp.draw(
                                bundle.camera,
                                &mut bundle.drawer,
                                Color::ClippedPolygonsNotToSpawn
                            );
                        }
                    }
                };
            }
        };
    }

    #[inline]
    pub fn ui(&mut self, ui: &mut egui::Ui)
    {
        let post_clip_status =
            return_if_no_match!(&mut self.0, Status::PostClip { status, .. }, status);

        ui.label(egui::RichText::new("CLIP TOOL"));

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Keep:"));

            let both_button = ui.button(egui::RichText::new("Both"));
            let left_button = ui.button(egui::RichText::new("Left"));
            let right_button = ui.button(egui::RichText::new("Right"));

            for b in [&both_button, &right_button, &left_button]
            {
                b.surrender_focus();
            }

            if both_button.clicked()
            {
                *post_clip_status = PostClipStatus::Both;
            }
            else if left_button.clicked()
            {
                *post_clip_status = PostClipStatus::Left;
            }
            else if right_button.clicked()
            {
                *post_clip_status = PostClipStatus::Right;
            }

            match post_clip_status
            {
                PostClipStatus::Both => both_button.highlight(),
                PostClipStatus::Left => left_button.highlight(),
                PostClipStatus::Right => right_button.highlight()
            };
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
            self.0,
            ui,
            bundle,
            buttons,
            tool_change_conditions,
            (ClipSide, Status::PickSideUi(None), Status::PickSideUi(_))
        );
    }
}
