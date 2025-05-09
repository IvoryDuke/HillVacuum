//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{iter::Copied, ops::RangeInclusive};

use glam::Vec2;
use hill_vacuum_shared::{match_or_panic, return_if_none};

use crate::{
    map::{
        brush::convex_polygon::{free_draw_tooltip, ConvexPolygon, FreeDrawVertexDeletionResult},
        drawer::{color::Color, drawers::EditDrawer},
        editor::{
            cursor::Cursor,
            state::{
                core::{
                    rect::{Rect, RectTrait},
                    tool::DisableSubtool
                },
                editor_state::ToolsSettings,
                inputs_presses::InputsPresses
            },
            DrawBundle,
            ToolUpdateBundle
        },
        MAP_HALF_SIZE,
        MAP_SIZE
    },
    utils::{
        collections::Ids,
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
            fn core(&self) -> &DrawMode { &self.0 }

            #[inline]
            fn core_mut(&mut self) -> &mut DrawMode { &mut self.0 }
        }

        impl [<$shape CursorPolygon>]
        {
            #[inline]
            pub fn update(
                &mut self,
                bundle: &mut ToolUpdateBundle,
                $($settings: &mut ToolsSettings,)?
                drawn_brushes: &mut Ids
            )
            {
                self.state_update(bundle.inputs, bundle.cursor $(, $settings)?);
                $(let $orientation = self.$orientation();)?
                self.core_mut().update(bundle, bundle.inputs, |hull| {
                    Self::vertex_gen(&hull $(, $orientation)? $(, $settings)?)
                });

                if bundle.inputs.left_mouse.pressed()
                {
                    return;
                }

                let vxs = return_if_none!(self.core_mut().generate_polygon(bundle.cursor, |hull| {
                    $(let $orientation = TriangleOrientation::new(bundle.cursor.world_snapped(), bundle.cursor.grid_square());)?
                    Self::vertex_gen(&hull $(, $orientation)? $(, $settings)?)
                }));

                bundle.manager.spawn_drawn_brush(
                    bundle.drawing_resources,
                    bundle.default_brush_properties,
                    bundle.edits_history,
                    bundle.grid,
                    ConvexPolygon::new(vxs),
                    drawn_brushes
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

                if let DrawMode::Drag(rect, _) = &self.0
                {
                    drawer.square_highlight(return_if_none!(rect.origin()),  Color::CursorPolygon);
                    drawer.square_highlight(return_if_none!(rect.extreme()),  Color::CursorPolygon);
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
enum SpawnStatus
{
    /// Mouse has not been pressed.
    MouseNotPressed,
    /// Mouse pressed at a certain position.
    MousePressed(Vec2)
}

//=======================================================================//

/// The drawig mode of a shaped cursor polygon.
enum DrawMode
{
    /// Spawn on click.
    Spawn(Hull, SpawnStatus, Vec<Vec2>),
    /// Drag cursor and release to spawn.
    Drag(Rect, Vec<Vec2>)
}

impl Default for DrawMode
{
    #[inline]
    fn default() -> Self
    {
        Self::Spawn(
            Hull::new(MAP_SIZE, MAP_SIZE - 64f32, MAP_SIZE - 64f32, MAP_SIZE).unwrap(),
            SpawnStatus::MouseNotPressed,
            // Not very cool, only needed at startup.
            vec![Vec2::splat(MAP_HALF_SIZE); 4]
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
        Self::Spawn(*hull, SpawnStatus::MouseNotPressed, v(hull).into_iter().collect())
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
    fn shape_mut(&mut self) -> &mut Vec<Vec2>
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
            Self::Drag(rect, _) =>
            {
                let hull = rect.hull()?;
                (hull.width() != 0f32 && hull.height() != 0f32).then_some(hull)
            }
        }
    }

    /// Updates `self`.
    #[inline]
    fn update<I: IntoIterator<Item = Vec2>, V: Fn(&Hull) -> I>(
        &mut self,
        bundle: &ToolUpdateBundle,
        inputs: &InputsPresses,
        v: V
    )
    {
        let ToolUpdateBundle { cursor, .. } = bundle;
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
            Self::Drag(rect, _) => rect.update_extremes(bundle.camera, cursor_pos)
        };

        match self.hull().map(|hull| v(&hull))
        {
            Some(vxs) => self.shape_mut().replace_values(vxs),
            None => self.shape_mut().clear()
        };
    }

    /// Returns an iterator describing the vertexes of the brush to draw, if any.
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
#[derive(Clone, Copy, PartialEq)]
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
#[must_use]
#[derive(Clone, Default)]
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
// STRUCTS
//
//=======================================================================//

shape_cursor_brush!((Square), (Triangle, orientation), (Circle | settings));

/// A cursor to draw a square.
#[derive(Default)]
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
    #[inline]
    fn state_update(&mut self, _: &InputsPresses, _: &Cursor) {}
}

//=======================================================================//

/// A cursor to draw rectangle triangle.
pub(in crate::map::editor::state) struct TriangleCursorPolygon(DrawMode, TriangleOrientation);

impl Default for TriangleCursorPolygon
{
    #[inline]
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
pub(in crate::map::editor::state) struct CircleCursorPolygon(DrawMode);

impl Default for CircleCursorPolygon
{
    #[inline]
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
#[derive(Clone, Default)]
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
    pub fn update(&mut self, bundle: &mut ToolUpdateBundle, drawn_brushes: &mut Ids)
    {
        if bundle.inputs.enter.just_pressed()
        {
            self.generate_polygon(bundle, drawn_brushes);
            bundle.edits_history.purge_free_draw_edits();
            return;
        }

        let cursor_pos = bundle.cursor.world_snapped();

        if bundle.inputs.left_mouse.just_pressed()
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

                    self.0 = Status::Polygon(ConvexPolygon::new(triangle));
                },
                Status::Polygon(poly) =>
                {
                    if !poly.try_insert_free_draw_vertex(cursor_pos, bundle.camera.scale())
                    {
                        return;
                    }
                }
            };

            bundle.edits_history.free_draw_point_insertion(cursor_pos, 0);
        }
        else if bundle.inputs.right_mouse.just_pressed()
        {
            match &mut self.0
            {
                Status::None => (),
                Status::Point(p) =>
                {
                    if p.is_point_inside_ui_highlight(cursor_pos, bundle.camera.scale())
                    {
                        bundle.edits_history.free_draw_point_deletion(*p, 0);
                        self.0 = Status::None;
                    }
                },
                Status::Line(l) =>
                {
                    for (i, p) in l.iter().enumerate()
                    {
                        if p.is_point_inside_ui_highlight(cursor_pos, bundle.camera.scale())
                        {
                            bundle.edits_history.free_draw_point_deletion(*p, 0);
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
                            bundle.edits_history.free_draw_point_deletion(deleted, 0);
                        },
                        FreeDrawVertexDeletionResult::Line(line, deleted) =>
                        {
                            bundle.edits_history.free_draw_point_deletion(deleted, 0);
                            self.0 = Status::Line(line);
                        }
                    };
                }
            };
        }
    }

    /// Spawns the drawn brush.
    #[inline]
    fn generate_polygon(&mut self, bundle: &mut ToolUpdateBundle, drawn_brushes: &mut Ids) -> bool
    {
        if !matches!(self.0, Status::Polygon(_))
        {
            return false;
        }

        let status = self.0.take_value();

        bundle.manager.spawn_drawn_brush(
            bundle.drawing_resources,
            bundle.default_brush_properties,
            bundle.edits_history,
            bundle.grid,
            match_or_panic!(status, Status::Polygon(poly), poly),
            drawn_brushes
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
    pub fn draw(&self, bundle: &mut DrawBundle)
    {
        let DrawBundle {
            window,
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
                let label = return_if_none!(drawer.vx_tooltip_label(*p));
                free_draw_tooltip(window, camera, drawer, *p, label, &mut String::new());
            },
            Status::Line([start, end]) =>
            {
                drawer.line(*start, *end, Color::CursorPolygon);
                drawer.square_highlight(*start, Color::CursorPolygon);
                drawer.square_highlight(*end, Color::CursorPolygon);

                let mut text = String::new();

                for vx in [start, end]
                {
                    let label = return_if_none!(drawer.vx_tooltip_label(*vx));
                    free_draw_tooltip(window, camera, drawer, *vx, label, &mut text);
                }
            },
            Status::Polygon(poly) => poly.draw_free_draw(window, camera, drawer)
        };
    }
}
