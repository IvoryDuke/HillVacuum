//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::fmt::Write;

use bevy::prelude::{Transform, Vec2, Window};
use bevy_egui::egui;
use hill_vacuum_shared::{match_or_panic, return_if_none};

use crate::{
    map::{
        drawer::{color::Color, EditDrawer},
        editor::state::edits_history::EditsHistory,
        path::{node_tooltip, FreeDrawNodeDeletionResult}
    },
    utils::{
        math::{AroundEqual, NecessaryPrecisionValue},
        misc::PointInsideUiHighlight
    },
    Path
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// An enum to create a new [`Path`].
#[derive(Default, Debug)]
pub(in crate::map::editor::state::core) enum PathCreation
{
    /// No [`Node`]s.
    #[default]
    None,
    /// One [`Node`].
    Point(Vec2),
    /// A [`Path`] (2+ [`Nodes`]).
    Path(Path)
}

impl PathCreation
{
    /// Returns the created [`Path`], if any, and resets the path creation process.
    #[inline]
    pub fn path(&mut self) -> Option<Path>
    {
        matches!(self, Self::Path(..))
            .then(|| match_or_panic!(std::mem::take(self), Self::Path(path), path))
    }

    /// Pushes a new [`Node`].
    #[inline]
    pub fn push(&mut self, edits_history: &mut EditsHistory, p: Vec2, center: Vec2)
    {
        let index = match self
        {
            Self::None =>
            {
                *self = Self::Point(p);
                0
            },
            Self::Point(q) =>
            {
                if p.around_equal(&*q)
                {
                    return;
                }

                *self = Self::Path(Path::new(&[*q, p], center));
                1
            },
            Self::Path(path) =>
            {
                let index = path.len();

                if !path.try_insert_node_at_index(p, index, center)
                {
                    return;
                }

                u8::try_from(index).unwrap()
            }
        };

        edits_history.free_draw_point_insertion(p, index);
    }

    /// Inserts a [`Node`] with position `p` at `index`.
    #[inline]
    pub fn insert_at_index(&mut self, p: Vec2, index: usize, center: Vec2)
    {
        match self
        {
            Self::None =>
            {
                assert!(index == 0, "No points but insertion index is not 0.");
                *self = Self::Point(p);
            },
            Self::Point(q) =>
            {
                assert!(index < 2, "One point in path but insertion index is not lower than 2.");

                if index == 0
                {
                    *self = Self::Path(Path::new(&[p, *q], center));
                }
                else
                {
                    *self = Self::Path(Path::new(&[*q, p], center));
                }
            },
            Self::Path(path) =>
            {
                path.insert_free_draw_node_at_index(p, index, center);
            }
        }
    }

    /// Removes the latest [`Node`] at position `p`.
    #[inline]
    pub fn remove(
        &mut self,
        edits_history: &mut EditsHistory,
        p: Vec2,
        center: Vec2,
        camera_scale: f32
    )
    {
        let (pos, index) = match self
        {
            Self::None => return,
            Self::Point(q) =>
            {
                if q.is_point_inside_ui_highlight(p, camera_scale)
                {
                    let value = (*q, 0);
                    *self = Self::None;
                    value
                }
                else
                {
                    return;
                }
            },
            Self::Path(path) =>
            {
                match path.try_delete_free_draw_node(p, center, camera_scale)
                {
                    FreeDrawNodeDeletionResult::None => return,
                    FreeDrawNodeDeletionResult::Path(deleted, idx) => (deleted, idx),
                    FreeDrawNodeDeletionResult::Point(p, deleted, idx) =>
                    {
                        *self = Self::Point(p);
                        (deleted, idx)
                    }
                }
            },
        };

        edits_history.free_draw_point_deletion(pos, index);
    }

    /// Removes the [`Node`] at `index`.
    #[inline]
    pub fn remove_index(&mut self, index: usize, center: Vec2)
    {
        match self
        {
            Self::None => panic!("No nodes left to be removed."),
            Self::Point(_) =>
            {
                assert!(index == 0, "One point left but deletion index is not 0.");
                *self = Self::None;
            },
            Self::Path(path) =>
            {
                *self = Self::Point(return_if_none!(
                    path.delete_free_draw_node_at_index(index, center)
                ));
            }
        }
    }

    /// Draws the tooltip of a [`Node`].
    #[inline]
    fn draw_point_tooltip(
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        drawer: &mut EditDrawer,
        pos: Vec2
    )
    {
        let label = return_if_none!(drawer.vx_tooltip_label(pos));
        let mut tooltip_text = String::with_capacity(4);
        write!(&mut tooltip_text, "{}", pos.necessary_precision_value()).ok();
        node_tooltip(
            window,
            camera,
            egui_context,
            pos,
            label,
            &tooltip_text,
            drawer.egui_color(Color::CursorPolygon)
        );
    }

    /// Draws the [`Path`] being drawn plus a line showing which entity it belongs to.
    #[inline]
    pub fn draw_with_knot(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        drawer: &mut EditDrawer,
        show_tooltips: bool,
        center: Vec2
    )
    {
        match self
        {
            Self::None => drawer.square_highlight(center, Color::CursorPolygon),
            Self::Point(p) =>
            {
                drawer.square_highlight(center, Color::CursorPolygon);
                drawer.square_highlight(*p, Color::CursorPolygon);
                drawer.line(center, *p, Color::CursorPolygon);

                if show_tooltips
                {
                    Self::draw_point_tooltip(window, camera, egui_context, drawer, *p);
                }
            },
            Self::Path(path) =>
            {
                path.draw_free_draw_with_knot(
                    window,
                    camera,
                    egui_context,
                    drawer,
                    center,
                    show_tooltips
                );
            }
        };
    }

    /// Draws the [`Path`] being drawn.
    #[inline]
    pub fn draw(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        drawer: &mut EditDrawer,
        show_tooltips: bool,
        center: Vec2
    )
    {
        match self
        {
            Self::None => (),
            Self::Point(p) =>
            {
                drawer.square_highlight(*p, Color::CursorPolygon);

                if show_tooltips
                {
                    Self::draw_point_tooltip(window, camera, egui_context, drawer, *p);
                }
            },
            Self::Path(path) =>
            {
                path.draw_free_draw(window, camera, egui_context, drawer, center, show_tooltips);
            }
        };
    }
}
