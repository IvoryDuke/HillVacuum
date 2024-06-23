//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{iter::Copied, ops::RangeInclusive};

use bevy::prelude::Vec2;
use hill_vacuum_shared::{match_or_panic, return_if_none};

use crate::{
    map::{
        brush::convex_polygon::{free_draw_tooltip, ConvexPolygon, FreeDrawVertexDeletionResult},
        containers::{hv_vec, HvVec, Ids},
        drawer::{color::Color, EditDrawer},
        editor::{
            cursor_pos::Cursor,
            state::{
                core::{
                    rect::{Rect, RectTrait},
                    tool::DisableSubtool
                },
                editor_state::{InputsPresses, ToolsSettings},
                edits_history::EditsHistory,
                manager::EntitiesManager
            },
            DrawBundle,
            ToolUpdateBundle,
            MAP_HALF_SIZE
        },
        properties::DefaultProperties
    },
    utils::{
        hull::{CircleIterator, Hull, TriangleOrientation},
        math::{
            points::{sort_vxs_ccw, vertexes_orientation, vxs_center, VertexesOrientation},
            AroundEqual
        },
        misc::{next, Camera, PointInsideUiHighlight, ReplaceValues, TakeValue}
    }
};

//=======================================================================//
// MACROS
//
//=======================================================================//

/// Generates some functions of the cursor polygons.
macro_rules! shape_cursor_brush {
    ($(($shape:ident $(, $orientation:ident)? $( | $settings:ident)?)),+) => { paste::paste! { $(
        impl Core for [<$shape CursorPolygon>]
        {
            #[inline]
            #[must_use]
            fn core(&self) -> &DrawMode { &self.0 }

            #[inline]
            #[must_use]
            fn core_mut(&mut self) -> &mut DrawMode { &mut self.0 }
        }

        impl [<$shape CursorPolygon>]
        {
            #[inline]
            pub fn update(
                &mut self,
                bundle: &ToolUpdateBundle,
                manager: &mut EntitiesManager,
                drawn_brushes: &mut Ids,
                inputs: &InputsPresses,
                edits_history: &mut EditsHistory
                $(, $settings: &mut ToolsSettings)?
            )
            {
                self.state_update(inputs, bundle.cursor $(, $settings)?);
                $(let $orientation = self.$orientation();)?
                self.core_mut().update(inputs, bundle.cursor, bundle.camera.scale(), |hull| {
                    Self::vertex_gen(&hull $(, $orientation)? $(, $settings)?)
                });

                if inputs.left_mouse.pressed()
                {
                    return;
                }

                let vxs = return_if_none!(self.core_mut().generate_polygon(bundle.cursor, |hull| {
                    $(let $orientation = TriangleOrientation::new(bundle.cursor.world_snapped(), bundle.cursor.grid_square());)?
                    Self::vertex_gen(&hull $(, $orientation)? $(, $settings)?)
                }));

                manager.spawn_drawn_brush(
                    ConvexPolygon::new(vxs),
                    drawn_brushes,
                    edits_history,
                    bundle.brushes_default_properties
                );
            }

            #[inline]
            #[must_use]
            pub const fn is_dragging(&self) -> bool
            {
                matches!(self.0, DrawMode::Drag(..))
            }
        }

        impl DrawCursorPolygon for [<$shape CursorPolygon>]
        {
            #[inline]
            fn draw(&self, drawer: &mut EditDrawer)
            {
                let core = self.core();

                if let Some(hull) = core.hull()
                {
                    drawer.hull(&hull, Color::CursorPolygonHull);
                    drawer.sides(core.vertexes().unwrap(), Color::CursorPolygon);
                }

                if let DrawMode::Drag(da, _) = &self.0
                {
                    drawer.square_highlight(return_if_none!(da.origin()),  Color::CursorPolygon);
                    drawer.square_highlight(return_if_none!(da.extreme()),  Color::CursorPolygon);
                }
            }
        }
    )+}};
}

//=======================================================================//
// TRAITS
//
//=======================================================================//

/// The core of a cursor polygon.
trait Core
{
    /// Returns a reference to the [`DrawMode`].
    fn core(&self) -> &DrawMode;
    /// Returns a mutable reference to the [`DrawMode`].
    fn core_mut(&mut self) -> &mut DrawMode;
}

//=======================================================================//

/// A trait for cursor polygons to draw their shape.
pub(in crate::map::editor::state) trait DrawCursorPolygon
{
    /// Draws the polygon.
    fn draw(&self, drawer: &mut EditDrawer);
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The state of the spawn variant of [`DrawMode`].
#[derive(Debug)]
enum SpawnStatus
{
    /// Mouse has not been pressed.
    MouseNotPressed,
    /// Mouse pressed at a certain position.
    MousePressed(Vec2)
}

//=======================================================================//

/// The drawig mode of a shaped cursor polygon.
#[derive(Debug)]
enum DrawMode
{
    /// Spawn on click.
    Spawn(Hull, SpawnStatus, HvVec<Vec2>),
    /// Drag cursor and release to spawn.
    Drag(Rect, HvVec<Vec2>)
}

impl Default for DrawMode
{
    #[inline]
    #[must_use]
    fn default() -> Self
    {
        Self::Spawn(
            Hull::new(0f32, 0f32, 0f32, 0f32),
            SpawnStatus::MouseNotPressed,
            // Not very cool, only needed at startup.
            hv_vec![Vec2::splat(MAP_HALF_SIZE); 4]
        )
    }
}

impl DrawMode
{
    /// Returns a new [`DrawMode`] in its spawn variant.
    #[inline]
    fn new<I: IntoIterator<Item = Vec2>, V: Fn(&Hull) -> I>(cursor: &Cursor, v: V) -> Self
    {
        let hull = cursor.grid_square();
        Self::Spawn(*hull, SpawnStatus::MouseNotPressed, hv_vec![collect; v(hull)])
    }

    /// The vertexes of the drawn shape, if any.
    #[inline]
    fn vertexes(&self) -> Option<Copied<std::slice::Iter<Vec2>>>
    {
        let (Self::Spawn(_, _, shape) | Self::Drag(_, shape)) = self;
        (!shape.is_empty()).then(|| shape.iter().copied())
    }

    /// Returns a mutable reference to the vector containing the vertexes of the drawn shape.
    #[inline]
    fn shape_mut(&mut self) -> &mut HvVec<Vec2>
    {
        let (Self::Spawn(_, _, shape) | Self::Drag(_, shape)) = self;
        shape
    }

    /// Returns the [`Hull`] encompassing the drawn shape, if any.
    #[inline]
    #[must_use]
    fn hull(&self) -> Option<Hull>
    {
        match self
        {
            Self::Spawn(hull, ..) => Some(*hull),
            Self::Drag(da, _) => da.hull()
        }
    }

    /// Updates `self`.
    #[inline]
    fn update<I: IntoIterator<Item = Vec2>, V: Fn(&Hull) -> I>(
        &mut self,
        inputs: &InputsPresses,
        cursor: &Cursor,
        camera_scale: f32,
        v: V
    )
    {
        let cursor_pos = cursor.world_snapped();

        match self
        {
            Self::Spawn(hull, status, shape) =>
            {
                *hull = *cursor.grid_square();

                match status
                {
                    SpawnStatus::MouseNotPressed =>
                    {
                        if inputs.left_mouse.just_pressed()
                        {
                            *status = SpawnStatus::MousePressed(cursor_pos);
                        }
                    },
                    SpawnStatus::MousePressed(pos) =>
                    {
                        if !pos.around_equal(&cursor_pos)
                        {
                            *self = Self::Drag(Rect::from_origin(*pos), shape.take_value());
                        }
                    }
                };
            },
            Self::Drag(da, _) => da.update_extremes(cursor_pos, camera_scale)
        };

        match self.hull().map(|hull| v(&hull))
        {
            Some(vxs) => self.shape_mut().replace_values(vxs),
            None => self.shape_mut().clear()
        };
    }

    /// Returns an iterator describing the vertexes of the [`Brush`] to draw, if any.
    #[inline]
    #[must_use]
    fn generate_polygon<I: IntoIterator<Item = Vec2>, V: Fn(&Hull) -> I>(
        &mut self,
        cursor: &Cursor,
        v: V
    ) -> Option<impl Iterator<Item = Vec2>>
    {
        match self
        {
            DrawMode::Spawn(_, SpawnStatus::MouseNotPressed, _) => return None,
            DrawMode::Drag(_, shape) if shape.is_empty() =>
            {
                *self = Self::new(cursor, v);
                return None;
            },
            _ => ()
        };

        let value = self.shape_mut().take_value().into_iter().into();
        *self = Self::new(cursor, v);
        value
    }
}

//=======================================================================//

/// A superficial description of the state of the free draw tool.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::map) enum FreeDrawStatus
{
    /// Inactive.
    Inactive,
    /// Point or line drawn.
    Active,
    /// Polygon drawn.
    Polygon
}

//=======================================================================//

/// The state of the [`FreeDrawCursorPolygon`].
#[derive(Clone, Default, Debug)]
enum Status
{
    /// Nothing drawn.
    #[default]
    None,
    /// Point drawn.
    Point(Vec2),
    /// Line drawn.
    Line([Vec2; 2]),
    /// Polygon drawn.
    Polygon(ConvexPolygon)
}

//=======================================================================//
// TYPES
//
//=======================================================================//

shape_cursor_brush!((Square), (Triangle, orientation), (Circle | settings));

/// A cursor to draw a square.
#[derive(Debug, Default)]
pub(in crate::map::editor::state) struct SquareCursorPolygon(DrawMode);

impl SquareCursorPolygon
{
    /// Returns a new [`SquareCursorPolygon`].
    #[inline]
    #[must_use]
    pub fn new(cursor: &Cursor) -> Self { Self(DrawMode::new(cursor, Self::vertex_gen)) }

    /// Returns an iterator returning the vertexes of the square.
    #[inline]
    fn vertex_gen(hull: &Hull) -> std::array::IntoIter<Vec2, 4> { hull.rectangle().into_iter() }

    /// Updates the state of `self`.
    #[allow(clippy::unused_self)]
    #[inline(always)]
    fn state_update(&mut self, _: &InputsPresses, _: &Cursor) {}
}

//=======================================================================//

/// A cursor to draw rectangle triangle.
#[derive(Debug)]
pub(in crate::map::editor::state) struct TriangleCursorPolygon(DrawMode, TriangleOrientation);

impl Default for TriangleCursorPolygon
{
    #[inline]
    #[must_use]
    fn default() -> Self { unreachable!() }
}

impl TriangleCursorPolygon
{
    /// Returns a new [`TriangleCursorPolygon`].
    #[inline]
    #[must_use]
    pub fn new(cursor: &Cursor) -> Self
    {
        let orientation = TriangleOrientation::new(cursor.world_snapped(), cursor.grid_square());
        Self(DrawMode::new(cursor, |hull| Self::vertex_gen(hull, orientation)), orientation)
    }

    /// Returns an iterator returning the vertexes of the triangle.
    #[inline]
    fn vertex_gen(hull: &Hull, orientation: TriangleOrientation) -> std::array::IntoIter<Vec2, 3>
    {
        hull.triangle(orientation).into_iter()
    }

    /// Returns the current [`TriangleOrientation`].
    #[inline]
    #[must_use]
    const fn orientation(&self) -> TriangleOrientation { self.1 }

    /// Sets the orientation of the triangle to the next one in cw order.
    #[inline]
    pub fn next_orientation(&mut self)
    {
        self.1 = match self.1
        {
            TriangleOrientation::TopLeft => TriangleOrientation::TopRight,
            TriangleOrientation::TopRight => TriangleOrientation::BottomRight,
            TriangleOrientation::BottomRight => TriangleOrientation::BottomLeft,
            TriangleOrientation::BottomLeft => TriangleOrientation::TopLeft
        };
    }

    /// Sets the orientation of the triangle to the previous one in cw order.
    #[inline]
    pub fn previous_orientation(&mut self)
    {
        self.1 = match self.1
        {
            TriangleOrientation::TopLeft => TriangleOrientation::BottomLeft,
            TriangleOrientation::TopRight => TriangleOrientation::TopLeft,
            TriangleOrientation::BottomRight => TriangleOrientation::TopRight,
            TriangleOrientation::BottomLeft => TriangleOrientation::BottomRight
        };
    }

    /// Updates the state of `self`.
    #[inline]
    fn state_update(&mut self, inputs: &InputsPresses, cursor: &Cursor)
    {
        match self.core()
        {
            DrawMode::Spawn(..) =>
            {
                if !inputs.left_mouse.pressed()
                {
                    self.1 = TriangleOrientation::new(cursor.world_snapped(), cursor.grid_square());
                }
            },
            DrawMode::Drag(..) =>
            {
                if !inputs.tab.just_pressed()
                {
                    return;
                }

                if inputs.alt_pressed()
                {
                    self.previous_orientation();
                }
                else
                {
                    self.next_orientation();
                }
            }
        };
    }
}

//=======================================================================//

/// The cursor to draw a "circle".
#[derive(Debug)]
pub(in crate::map::editor::state) struct CircleCursorPolygon(DrawMode);

impl Default for CircleCursorPolygon
{
    #[inline]
    #[must_use]
    fn default() -> Self { unreachable!() }
}

impl CircleCursorPolygon
{
    /// The maximum circle resolution.
    const MAX_CIRCLE_RESOLUTION: u8 = 8;
    /// The minimum circle resolution.
    const MIN_CIRCLE_RESOLUTION: u8 = 1;

    /// Returns a new [`CircleCursorPolygon`].
    #[inline]
    #[must_use]
    pub fn new(cursor: &Cursor, settings: &ToolsSettings) -> Self
    {
        Self(DrawMode::new(cursor, |hull| Self::vertex_gen(hull, settings)))
    }

    /// Returns the range of the possible circle resolutions.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) const fn circle_resolution_range() -> RangeInclusive<u8>
    {
        Self::MIN_CIRCLE_RESOLUTION..=Self::MAX_CIRCLE_RESOLUTION
    }

    /// Increases the resolution of the circle.
    #[inline]
    pub fn increase_resolution(settings: &mut ToolsSettings)
    {
        if settings.circle_draw_resolution < Self::MAX_CIRCLE_RESOLUTION
        {
            settings.circle_draw_resolution += 1;
        }
    }

    /// Decreases the resolution of the circle.
    #[inline]
    pub fn decrease_resolution(settings: &mut ToolsSettings)
    {
        if settings.circle_draw_resolution > Self::MIN_CIRCLE_RESOLUTION
        {
            settings.circle_draw_resolution -= 1;
        }
    }

    /// Returns an iterator returning the vertexes of the circle.
    #[inline]
    fn vertex_gen(hull: &Hull, settings: &ToolsSettings) -> CircleIterator
    {
        hull.circle(settings.circle_draw_resolution * 4)
    }

    /// Updates the state of `self`.
    #[allow(clippy::unused_self)]
    #[inline]
    fn state_update(&mut self, inputs: &InputsPresses, _: &Cursor, settings: &mut ToolsSettings)
    {
        if inputs.plus.just_pressed()
        {
            Self::increase_resolution(settings);
        }
        else if inputs.minus.just_pressed()
        {
            Self::decrease_resolution(settings);
        }
    }
}

//=======================================================================//

/// The cursor to freely draw a generic polygon.
#[derive(Clone, Debug, Default)]
pub(in crate::map::editor::state) struct FreeDrawCursorPolygon(Status);

impl DisableSubtool for FreeDrawCursorPolygon
{
    #[inline]
    fn disable_subtool(&mut self)
    {
        if !matches!(self.0, Status::None)
        {
            self.0 = Status::None;
        }
    }
}

impl FreeDrawCursorPolygon
{
    /// Returns a new [`FreeDrawCursorPolygon`].
    #[inline]
    #[must_use]
    pub fn new() -> Self { Self::default() }

    /// Returns the state of `self`.
    #[inline]
    pub const fn status(&self) -> FreeDrawStatus
    {
        match self.0
        {
            Status::None => FreeDrawStatus::Inactive,
            Status::Point(_) | Status::Line(_) => FreeDrawStatus::Active,
            Status::Polygon(_) => FreeDrawStatus::Polygon
        }
    }

    /// Updates the polygon.
    #[inline]
    pub fn update(
        &mut self,
        bundle: &ToolUpdateBundle,
        manager: &mut EntitiesManager,
        drawn_brushes: &mut Ids,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory
    )
    {
        if inputs.enter.just_pressed()
        {
            self.generate_polygon(
                manager,
                drawn_brushes,
                edits_history,
                bundle.brushes_default_properties
            );
            edits_history.purge_free_draw_edits();
            return;
        }

        let cursor_pos = bundle.cursor.world_snapped();

        if inputs.left_mouse.just_pressed()
        {
            match &mut self.0
            {
                Status::None => self.0 = Status::Point(cursor_pos),
                Status::Point(p) =>
                {
                    if p.is_point_inside_ui_highlight(cursor_pos, bundle.camera.scale())
                    {
                        return;
                    }

                    self.0 = Status::Line([*p, cursor_pos]);
                },
                Status::Line(l) =>
                {
                    for p in &*l
                    {
                        if p.is_point_inside_ui_highlight(cursor_pos, bundle.camera.scale())
                        {
                            return;
                        }
                    }

                    if let VertexesOrientation::Collinear =
                        vertexes_orientation(&[l[0], l[1], cursor_pos])
                    {
                        return;
                    }

                    let mut triangle = [l[0], l[1], cursor_pos];
                    let center = vxs_center(triangle.iter().copied());
                    triangle.sort_by(|a, b| sort_vxs_ccw(*a, *b, center));

                    self.0 = Status::Polygon(ConvexPolygon::new(triangle.into_iter()));
                },
                Status::Polygon(poly) =>
                {
                    if !poly.try_insert_free_draw_vertex(cursor_pos, bundle.camera.scale())
                    {
                        return;
                    }
                }
            };

            edits_history.free_draw_point_insertion(cursor_pos, 0);
        }
        else if inputs.right_mouse.just_pressed()
        {
            match &mut self.0
            {
                Status::None => (),
                Status::Point(p) =>
                {
                    if p.is_point_inside_ui_highlight(cursor_pos, bundle.camera.scale())
                    {
                        edits_history.free_draw_point_deletion(*p, 0);
                        self.0 = Status::None;
                    }
                },
                Status::Line(l) =>
                {
                    for (i, p) in l.iter().enumerate()
                    {
                        if p.is_point_inside_ui_highlight(cursor_pos, bundle.camera.scale())
                        {
                            edits_history.free_draw_point_deletion(*p, 0);
                            self.0 = Status::Point(l[next(i, 2)]);
                            break;
                        }
                    }
                },
                Status::Polygon(poly) =>
                {
                    match poly.try_delete_free_draw_vertex(cursor_pos, bundle.camera.scale())
                    {
                        FreeDrawVertexDeletionResult::None => (),
                        FreeDrawVertexDeletionResult::Polygon(deleted) =>
                        {
                            edits_history.free_draw_point_deletion(deleted, 0);
                        },
                        FreeDrawVertexDeletionResult::Line(line, deleted) =>
                        {
                            edits_history.free_draw_point_deletion(deleted, 0);
                            self.0 = Status::Line(line);
                        }
                    };
                }
            };
        }
    }

    /// Spawns the drawn [`Brush`].
    #[inline]
    fn generate_polygon(
        &mut self,
        manager: &mut EntitiesManager,
        drawn_brushes: &mut Ids,
        edits_history: &mut EditsHistory,
        default_properties: &DefaultProperties
    ) -> bool
    {
        if !matches!(self.0, Status::Polygon(_))
        {
            return false;
        }

        let status = std::mem::take(&mut self.0);

        manager.spawn_drawn_brush(
            match_or_panic!(status, Status::Polygon(poly), poly),
            drawn_brushes,
            edits_history,
            default_properties
        );

        true
    }

    /// Inserts the free draw vertex with position `p`.
    #[inline]
    pub fn delete_free_draw_vertex(&mut self, p: Vec2)
    {
        match &mut self.0
        {
            Status::None => panic!("No vertexes to be removed."),
            Status::Point(q) =>
            {
                assert!(p == *q, "Vertex asked to be removed is not the only one left.");
                self.0 = Status::None;
            },
            Status::Line([a, b]) =>
            {
                self.0 = Status::Point(
                    if p == *a
                    {
                        *b
                    }
                    else if p == *b
                    {
                        *a
                    }
                    else
                    {
                        panic!("No vertex with requested coordinates.")
                    }
                );
            },
            Status::Polygon(poly) =>
            {
                self.0 = Status::Line(return_if_none!(poly.delete_free_draw_vertex(p)));
            }
        }
    }

    /// Inserts a free draw vertex with position `p`.
    #[inline]
    pub fn insert_free_draw_vertex(&mut self, p: Vec2)
    {
        match &mut self.0
        {
            Status::None => self.0 = Status::Point(p),
            Status::Point(q) =>
            {
                assert!(
                    !q.around_equal(&p),
                    "New vertex has same coordinates as the only one in the shape."
                );
                self.0 = Status::Line([*q, p]);
            },
            Status::Line(l) =>
            {
                self.0 = Status::Polygon(ConvexPolygon::new_sorted(
                    (*l).into_iter().chain(Some(p)),
                    None
                ));
            },
            Status::Polygon(poly) =>
            {
                poly.insert_free_draw_vertex(p);
            }
        }
    }

    /// Draws the polygon being drawn.
    #[inline]
    pub fn draw(&self, bundle: &mut DrawBundle, show_tooltips: bool)
    {
        let DrawBundle {
            window,
            egui_context,
            drawer,
            camera,
            ..
        } = bundle;

        match &self.0
        {
            Status::None => (),
            Status::Point(p) =>
            {
                drawer.square_highlight(*p, Color::CursorPolygon);

                if !show_tooltips
                {
                    return;
                }

                let label = return_if_none!(drawer.vx_tooltip_label(*p));

                free_draw_tooltip(
                    window,
                    camera,
                    egui_context,
                    drawer.color_resources(),
                    *p,
                    label,
                    &mut String::with_capacity(6)
                );
            },
            Status::Line([start, end]) =>
            {
                drawer.line(*start, *end, Color::CursorPolygon);
                drawer.square_highlight(*start, Color::CursorPolygon);
                drawer.square_highlight(*end, Color::CursorPolygon);

                if !show_tooltips
                {
                    return;
                }

                let mut text = String::with_capacity(6);

                for vx in [start, end]
                {
                    let label = return_if_none!(drawer.vx_tooltip_label(*vx));

                    free_draw_tooltip(
                        window,
                        camera,
                        egui_context,
                        drawer.color_resources(),
                        *vx,
                        label,
                        &mut text
                    );
                }
            },
            Status::Polygon(poly) =>
            {
                poly.draw_free_draw(window, camera, drawer, egui_context, show_tooltips);
            }
        };
    }
}
