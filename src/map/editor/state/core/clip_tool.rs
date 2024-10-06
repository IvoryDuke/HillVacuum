//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;
use glam::Vec2;
use hill_vacuum_proc_macros::{EnumFromUsize, EnumSize};
use hill_vacuum_shared::{continue_if_none, match_or_panic, return_if_no_match, return_if_none};

use super::{
    draw_selected_and_non_selected_brushes,
    tool::{subtools_buttons, DisableSubtool, EnabledTool, OngoingMultiframeChange, SubTool},
    ActiveTool
};
use crate::{
    map::{
        brush::{convex_polygon::ConvexPolygon, ClipResult},
        drawer::{color::Color, drawers::EditDrawer},
        editor::{
            cursor::Cursor,
            state::{
                manager::EntitiesManager,
                ui::{ToolsButtons, UiBundle}
            },
            DrawBundle,
            ToolUpdateBundle
        }
    },
    utils::{
        identifiers::{EntityId, Id},
        iterators::FilterSet,
        misc::{next, prev, Camera, TakeValue}
    },
    HvHashMap
};

//=======================================================================//
// MACROS
//
//=======================================================================//

/// A macro to clip the selected brushes.
macro_rules! clip_brushes {
    ($self:ident, $clip_line:expr, $iter:expr) => {
        let mut results = crate::utils::collections::hv_hash_map![];

        // Until I figure out how to properly annotate the F lifetimes.
        for brush in $iter
        {
            use crate::utils::misc::AssertedInsertRemove;
            results.asserted_insert((brush.id(), continue_if_none!(brush.clip($clip_line))));
        }

        if results.is_empty()
        {
            $self.0 = Status::default();
            return;
        }

        $self.0 = Status::PostClip {
            pick: PickedPolygons::default(),
            results
        };
    };
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The polygons to use to spawn the new brushes.
#[derive(Clone, Copy, Debug, Default, EnumSize, EnumFromUsize)]
enum PickedPolygons
{
    /// Both left and right of the clip line.
    #[default]
    Both,
    /// Left of the clip line.
    Left,
    /// Right of the clip line.
    Right
}

impl PickedPolygons
{
    /// Returns the previous value of `self`.
    #[inline]
    fn previous(&mut self) { *self = prev(*self as usize, Self::SIZE).into(); }

    /// Returns the next value of `self`.
    #[inline]
    fn next(&mut self) { *self = next(*self as usize, Self::SIZE).into(); }
}

//=======================================================================//

/// The state of the clip tool.
#[derive(Debug)]
enum Status
{
    /// Inactive.
    Inactive(Option<ClipSide>),
    /// Creating the clip line.
    Active(Vec2, Option<Vec2>),
    /// Choosing the brushes to spawn.
    PostClip
    {
        /// The polygons picked to spawn the brushes.
        pick:    PickedPolygons,
        results: HvHashMap<Id, ClipResult>
    },
    /// Choosing the side to clip the brushes.
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
// STRUCTS
//
//=======================================================================//

/// The side used to clip the brushes.
#[derive(Copy, Clone, Debug)]
struct ClipSide
{
    /// The [`Id`] of the brush with the chosen side.
    id:    Id,
    /// The side.
    side:  [Vec2; 2],
    /// The index of the side.
    index: usize
}

//=======================================================================//

#[derive(Debug)]
pub(in crate::map::editor::state::core) struct ClipTool(Status);

impl DisableSubtool for ClipTool
{
    #[inline]
    fn disable_subtool(&mut self)
    {
        if matches!(self.0, Status::PickSideUi(_) | Status::Active(..))
        {
            self.0 = Status::default();
        }
    }
}

impl OngoingMultiframeChange for ClipTool
{
    #[inline]
    fn ongoing_multi_frame_change(&self) -> bool { matches!(self.0, Status::PostClip { .. }) }
}

impl ClipTool
{
    /// Returns an [`ActiveTool`] in its clip tool variant.
    #[inline]
    pub fn tool() -> ActiveTool { ActiveTool::Clip(ClipTool(Status::default())) }

    //==============================================================
    // Info

    /// Returns the cursor position used by the tool.
    #[inline]
    #[must_use]
    pub fn cursor_pos(&self, cursor: &Cursor) -> Option<Vec2>
    {
        matches!(self.0, Status::Inactive(_) | Status::Active(..) | Status::PickSideUi(_))
            .then_some(cursor.world_snapped())
    }

    //==============================================================
    // Update

    /// Updates the tool.
    #[inline]
    pub fn update(&mut self, bundle: &mut ToolUpdateBundle)
    {
        let cursor_pos = self.cursor_pos(bundle.cursor);

        match &mut self.0
        {
            Status::Inactive(clip_side) =>
            {
                let cursor_pos = cursor_pos.unwrap();
                let left_mouse_just_pressed = bundle.inputs.left_mouse.just_pressed();
                *clip_side = None;

                if bundle.inputs.alt_pressed() && bundle.manager.selected_brushes_amount() > 1
                {
                    Self::set_clip_side(bundle, clip_side);

                    if !left_mouse_just_pressed
                    {
                        return;
                    }

                    self.clip_brushes_with_side(bundle.manager);
                }
                else if left_mouse_just_pressed
                {
                    self.0 = Status::Active(cursor_pos, None);
                }
            },
            Status::Active(co, ce) =>
            {
                let cursor_pos = cursor_pos.unwrap();

                if *co != cursor_pos
                {
                    *ce = cursor_pos.into();
                }

                if bundle.inputs.left_mouse.just_pressed()
                {
                    self.clip_brushes_with_line(bundle.manager);
                }
            },
            Status::PostClip { pick, .. } =>
            {
                if bundle.inputs.tab.just_pressed()
                {
                    if bundle.inputs.alt_pressed()
                    {
                        pick.previous();
                    }
                    else
                    {
                        pick.next();
                    }
                }
                else if bundle.inputs.enter.just_pressed()
                {
                    self.spawn_clipped_brushes(bundle);
                }
            },
            Status::PickSideUi(clip_side) =>
            {
                *clip_side = None;
                Self::set_clip_side(bundle, clip_side);

                if bundle.inputs.left_mouse.just_pressed()
                {
                    self.clip_brushes_with_side(bundle.manager);
                }
            }
        };
    }

    /// Uses the side beneath the cursor, if any, to generate the clip line.
    #[inline]
    fn set_clip_side(bundle: &ToolUpdateBundle, clip_side: &mut Option<ClipSide>)
    {
        let cursor_pos = bundle.cursor.world();
        let camera_scale = bundle.camera.scale();

        let (id, side) = return_if_none!(bundle
            .manager
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

    /// Clips the selected brushes with the chosen side.
    #[inline]
    fn clip_brushes_with_side(&mut self, manager: &mut EntitiesManager)
    {
        let clip_side = match_or_panic!(
            &self.0,
            Status::Inactive(clip_side) | Status::PickSideUi(clip_side),
            clip_side
        )
        .unwrap();

        clip_brushes!(
            self,
            &clip_side.side,
            manager
                .selected_brushes()
                .filter_set_with_predicate(clip_side.id, |brush| brush.id())
        );
    }

    /// Clips the selected brushes with the clip line.
    #[inline]
    fn clip_brushes_with_line(&mut self, manager: &mut EntitiesManager)
    {
        let clip_segment = &match_or_panic!(&self.0, Status::Active(co, Some(ce)), [*co, *ce]);

        clip_brushes!(self, clip_segment, manager.selected_brushes());
    }

    /// Spawns the generated brushes.
    #[inline]
    fn spawn_clipped_brushes(&mut self, bundle: &mut ToolUpdateBundle)
    {
        let (pick, mut results) = match_or_panic!(
            &mut self.0,
            Status::PostClip { pick, results },
            (pick, results.take_value())
        );

        match pick
        {
            PickedPolygons::Both =>
            {
                for result in results.values_mut()
                {
                    if result.right.has_sprite()
                    {
                        _ = result.right.remove_texture();
                    }
                }

                for (id, result) in results
                {
                    _ = bundle.manager.replace_brush_with_partition(
                        bundle.drawing_resources,
                        bundle.edits_history,
                        bundle.grid,
                        Some(result.right).into_iter(),
                        id,
                        |brush| brush.set_polygon(result.left)
                    );
                }
            },
            PickedPolygons::Left =>
            {
                for (id, result) in results
                {
                    _ = bundle.manager.replace_brush_with_partition(
                        bundle.drawing_resources,
                        bundle.edits_history,
                        bundle.grid,
                        None.into_iter(),
                        id,
                        |brush| brush.set_polygon(result.left)
                    );
                }
            },
            PickedPolygons::Right =>
            {
                for (id, result) in results
                {
                    _ = bundle.manager.replace_brush_with_partition(
                        bundle.drawing_resources,
                        bundle.edits_history,
                        bundle.grid,
                        None.into_iter(),
                        id,
                        |brush| brush.set_polygon(result.right)
                    );
                }
            }
        };

        bundle.edits_history.override_edit_tag("Brushes Clip");
        self.0 = Status::default();
    }

    //==============================================================
    // Draw

    /// Draws the tool.
    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle)
    {
        if let Some(pos) = self.cursor_pos(bundle.cursor)
        {
            bundle.drawer.square_highlight(pos, Color::ToolCursor);
        }

        match &self.0
        {
            Status::Inactive(hgl_s) | Status::PickSideUi(hgl_s) =>
            {
                draw_selected_and_non_selected_brushes!(bundle);

                let hgl_s = return_if_none!(hgl_s);
                bundle.manager.brush(hgl_s.id).draw_extended_side(
                    bundle.drawer,
                    hgl_s.index,
                    Color::ToolCursor
                );
            },
            Status::Active(co, ce) =>
            {
                draw_selected_and_non_selected_brushes!(bundle);
                bundle.drawer.square_highlight(*co, Color::ToolCursor);

                // If the clip extremity is in place draw its square and the line
                // going through.
                if let Some(ce) = ce
                {
                    bundle.drawer.infinite_line(*co, *ce, Color::ToolCursor);
                }
            },
            Status::PostClip { pick, results } =>
            {
                /// Draws the sprite of `polygon` and its highlight.
                #[inline]
                fn draw_sprite_with_highlight(
                    polygon: &ConvexPolygon,
                    drawer: &mut EditDrawer,
                    color: Color
                )
                {
                    if polygon.has_sprite()
                    {
                        polygon.draw_sprite_with_highlight(drawer, color);
                    }
                }

                let DrawBundle {
                    window,
                    drawer,
                    camera,
                    manager,
                    ..
                } = bundle;

                for brush in manager.visible_brushes(window, camera, drawer.grid()).iter()
                {
                    let id = brush.id();

                    if results.contains_key(&id)
                    {
                        continue;
                    }

                    if manager.is_selected(id)
                    {
                        brush.draw_selected(drawer);
                    }
                    else
                    {
                        brush.draw_non_selected(drawer);
                    }
                }

                match pick
                {
                    PickedPolygons::Both =>
                    {
                        for result in results.values()
                        {
                            let collision = manager.brush(result.id).collision();

                            result.left.draw(drawer, collision, Color::ClippedPolygonsToSpawn);
                            result.right.draw(drawer, collision, Color::ClippedPolygonsToSpawn);

                            draw_sprite_with_highlight(
                                &result.right,
                                drawer,
                                Color::ClippedPolygonsToSpawn
                            );
                        }
                    },
                    PickedPolygons::Left =>
                    {
                        for result in results.values()
                        {
                            let collision = manager.brush(result.id).collision();

                            result.left.draw(drawer, collision, Color::ClippedPolygonsToSpawn);
                            draw_sprite_with_highlight(
                                &result.left,
                                drawer,
                                Color::ClippedPolygonsToSpawn
                            );

                            result.right.draw(drawer, collision, Color::OpaqueEntity);
                        }
                    },
                    PickedPolygons::Right =>
                    {
                        for result in results.values()
                        {
                            let collision = manager.brush(result.id).collision();

                            result.right.draw(drawer, collision, Color::ClippedPolygonsToSpawn);
                            draw_sprite_with_highlight(
                                &result.right,
                                drawer,
                                Color::ClippedPolygonsToSpawn
                            );

                            result.left.draw(drawer, collision, Color::OpaqueEntity);
                        }
                    }
                };
            }
        };
    }

    /// Draws the UI.
    #[inline]
    pub fn ui(&mut self, ui: &mut egui::Ui)
    {
        let pick = return_if_no_match!(&mut self.0, Status::PostClip { pick, .. }, pick);

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
                *pick = PickedPolygons::Both;
            }
            else if left_button.clicked()
            {
                *pick = PickedPolygons::Left;
            }
            else if right_button.clicked()
            {
                *pick = PickedPolygons::Right;
            }

            match pick
            {
                PickedPolygons::Both => both_button.highlight(),
                PickedPolygons::Left => left_button.highlight(),
                PickedPolygons::Right => right_button.highlight()
            };
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
            self.0,
            ui,
            bundle,
            buttons,
            (ClipSide, Status::PickSideUi(None), Status::PickSideUi(_))
        );
    }
}
