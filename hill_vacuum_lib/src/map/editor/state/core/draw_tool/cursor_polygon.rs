//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{iter::Copied, ops::RangeInclusive};

use bevy::prelude::Vec2;
use shared::{match_or_panic, return_if_none};

use crate::{
    map::{
        brush::convex_polygon::{free_draw_tooltip, ConvexPolygon, FreeDrawVertexDeletionResult},
        containers::{hv_vec, HvVec, Ids},
        drawer::{color::Color, EditDrawer},
        editor::{
            cursor_pos::Cursor,
            state::{
                core::drag_area::{DragArea, DragAreaTrait},
                editor_state::{InputsPresses, ToolsSettings},
                edits_history::EditsHistory,
                manager::EntitiesManager
            },
            DrawBundle,
            MAP_HALF_SIZE
        }
    },
    utils::{
        hull::{CircleIterator, Hull, TriangleOrientation},
        math::{
            points::{sort_vxs_ccw, vertexes_orientation, vxs_center, VertexesOrientation},
            AroundEqual
        },
        misc::{next, PointInsideUiHighlight, ReplaceValues, TakeValue}
    }
};

//=======================================================================//
// MACROS
//
//=======================================================================//

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
                manager: &mut EntitiesManager,
                drawn_brushes: &mut Ids,
                inputs: &InputsPresses,
                edits_history: &mut EditsHistory,
                cursor: &Cursor
                $(, $settings: &mut ToolsSettings)?
            )
            {
                self.state_update(inputs, cursor $(, $settings)?);
                $(let $orientation = self.$orientation();)?
                self.core_mut().update(inputs, cursor, |hull| {
                    Self::vertex_gen(&hull $(, $orientation)? $(, $settings)?)
                });

                if inputs.left_mouse.pressed()
                {
                    return;
                }

                let vxs = return_if_none!(self.core_mut().generate_brush(cursor, |hull| {
                    $(let $orientation = TriangleOrientation::new(cursor.world_snapped(), cursor.grid_square());)?
                    Self::vertex_gen(&hull $(, $orientation)? $(, $settings)?)
                }));

                manager.spawn_drawn_brush(
                    ConvexPolygon::new(vxs),
                    drawn_brushes,
                    edits_history
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

trait Core
{
    fn core(&self) -> &DrawMode;
    fn core_mut(&mut self) -> &mut DrawMode;
}

//=======================================================================//

pub(in crate::map::editor::state) trait DrawCursorPolygon
{
    fn draw(&self, drawer: &mut EditDrawer);
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[derive(Debug)]
enum SpawnStatus
{
    MouseNotPressed,
    MousePressed(Vec2)
}

//=======================================================================//

#[derive(Debug)]
enum DrawMode
{
    Spawn(Hull, SpawnStatus, HvVec<Vec2>),
    Drag(DragArea, HvVec<Vec2>)
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
            // Not very cool, only needed for the first frame
            hv_vec![Vec2::splat(MAP_HALF_SIZE); 4]
        )
    }
}

impl DrawMode
{
    #[inline]
    fn new<I: IntoIterator<Item = Vec2>, V: Fn(&Hull) -> I>(cursor: &Cursor, v: V) -> Self
    {
        let hull = cursor.grid_square();
        Self::Spawn(*hull, SpawnStatus::MouseNotPressed, hv_vec![collect; v(hull)])
    }

    #[inline]
    fn vertexes(&self) -> Option<Copied<std::slice::Iter<Vec2>>>
    {
        let (Self::Spawn(_, _, shape) | Self::Drag(_, shape)) = self;
        (!shape.is_empty()).then(|| shape.iter().copied())
    }

    #[inline]
    fn shape_mut(&mut self) -> &mut HvVec<Vec2>
    {
        let (Self::Spawn(_, _, shape) | Self::Drag(_, shape)) = self;
        shape
    }

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

    #[inline]
    fn update<I: IntoIterator<Item = Vec2>, V: Fn(&Hull) -> I>(
        &mut self,
        inputs: &InputsPresses,
        cursor: &Cursor,
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
                            *self = Self::Drag(DragArea::from_origin(*pos), shape.take_value());
                        }
                    }
                };
            },
            Self::Drag(da, _) => da.update_extremes(cursor_pos)
        };

        match self.hull().map(|hull| v(&hull))
        {
            Some(vxs) => self.shape_mut().replace_values(vxs),
            None => self.shape_mut().clear()
        };
    }

    #[inline]
    #[must_use]
    fn generate_brush<I: IntoIterator<Item = Vec2>, V: Fn(&Hull) -> I>(
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub(in crate::map) enum Status
{
    Inactive,
    Active,
    Polygon
}

//=======================================================================//
// TYPES
//
//=======================================================================//

shape_cursor_brush!((Square), (Triangle, orientation), (Circle | settings));

#[derive(Debug, Default)]
pub(in crate::map::editor::state) struct SquareCursorPolygon(DrawMode);

impl SquareCursorPolygon
{
    #[inline]
    #[must_use]
    pub fn new(cursor: &Cursor) -> Self { Self(DrawMode::new(cursor, Self::vertex_gen)) }

    #[inline]
    fn vertex_gen(hull: &Hull) -> std::array::IntoIter<Vec2, 4> { hull.rectangle().into_iter() }

    #[allow(clippy::unused_self)]
    #[inline]
    fn state_update(&mut self, _: &InputsPresses, _: &Cursor) {}
}

//=======================================================================//

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
    #[inline]
    #[must_use]
    pub fn new(cursor: &Cursor) -> Self
    {
        let orientation = TriangleOrientation::new(cursor.world_snapped(), cursor.grid_square());
        Self(DrawMode::new(cursor, |hull| Self::vertex_gen(hull, orientation)), orientation)
    }

    #[inline]
    fn vertex_gen(hull: &Hull, orientation: TriangleOrientation) -> std::array::IntoIter<Vec2, 3>
    {
        hull.triangle(orientation).into_iter()
    }

    #[inline]
    #[must_use]
    const fn orientation(&self) -> TriangleOrientation { self.1 }

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
    const MAX_CIRCLE_RESOLUTION: u8 = 8;
    const MIN_CIRCLE_RESOLUTION: u8 = 1;

    #[inline]
    #[must_use]
    pub fn new(cursor: &Cursor, settings: &ToolsSettings) -> Self
    {
        Self(DrawMode::new(cursor, |hull| Self::vertex_gen(hull, settings)))
    }

    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) const fn circle_resolution_range() -> RangeInclusive<u8>
    {
        Self::MIN_CIRCLE_RESOLUTION..=Self::MAX_CIRCLE_RESOLUTION
    }

    #[inline]
    pub fn increase_resolution(settings: &mut ToolsSettings)
    {
        if settings.circle_draw_resolution < Self::MAX_CIRCLE_RESOLUTION
        {
            settings.circle_draw_resolution += 1;
        }
    }

    #[inline]
    pub fn decrease_resolution(settings: &mut ToolsSettings)
    {
        if settings.circle_draw_resolution > Self::MIN_CIRCLE_RESOLUTION
        {
            settings.circle_draw_resolution -= 1;
        }
    }

    #[inline]
    fn vertex_gen(hull: &Hull, settings: &ToolsSettings) -> CircleIterator
    {
        hull.circle(settings.circle_draw_resolution * 4)
    }

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

#[derive(Clone, Debug, Default)]
pub(in crate::map::editor::state) struct FreeDrawCursorPolygon(FreeDrawStatus);

#[derive(Clone, Default, Debug)]
enum FreeDrawStatus
{
    #[default]
    None,
    Point(Vec2),
    Line([Vec2; 2]),
    Polygon(ConvexPolygon)
}

impl FreeDrawCursorPolygon
{
    #[inline]
    #[must_use]
    pub fn new() -> Self { Self::default() }

    #[inline]
    #[must_use]
    pub const fn status(&self) -> Status
    {
        match self.0
        {
            FreeDrawStatus::None => Status::Inactive,
            FreeDrawStatus::Point(_) | FreeDrawStatus::Line(_) => Status::Active,
            FreeDrawStatus::Polygon(_) => Status::Polygon
        }
    }

    #[inline]
    pub fn disable_subtool(&mut self) { self.0 = FreeDrawStatus::None; }

    #[inline]
    pub fn update(
        &mut self,
        manager: &mut EntitiesManager,
        drawn_brushes: &mut Ids,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        cursor: &Cursor,
        camera_scale: f32
    )
    {
        if inputs.enter.just_pressed()
        {
            self.generate_brush(manager, drawn_brushes, edits_history);
            edits_history.purge_free_draw_edits();
            return;
        }

        let cursor_pos = cursor.world_snapped();

        if inputs.left_mouse.just_pressed()
        {
            match &mut self.0
            {
                FreeDrawStatus::None => self.0 = FreeDrawStatus::Point(cursor_pos),
                FreeDrawStatus::Point(p) =>
                {
                    if p.is_point_inside_ui_highlight(cursor_pos, camera_scale)
                    {
                        return;
                    }

                    self.0 = FreeDrawStatus::Line([*p, cursor_pos]);
                },
                FreeDrawStatus::Line(l) =>
                {
                    for p in &*l
                    {
                        if p.is_point_inside_ui_highlight(cursor_pos, camera_scale)
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

                    self.0 = FreeDrawStatus::Polygon(ConvexPolygon::new(triangle.into_iter()));
                },
                FreeDrawStatus::Polygon(poly) =>
                {
                    if !poly.try_insert_free_draw_vertex(cursor_pos, camera_scale)
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
                FreeDrawStatus::None => (),
                FreeDrawStatus::Point(p) =>
                {
                    if p.is_point_inside_ui_highlight(cursor_pos, camera_scale)
                    {
                        edits_history.free_draw_point_deletion(*p, 0);
                        self.0 = FreeDrawStatus::None;
                    }
                },
                FreeDrawStatus::Line(l) =>
                {
                    for (i, p) in l.iter().enumerate()
                    {
                        if p.is_point_inside_ui_highlight(cursor_pos, camera_scale)
                        {
                            edits_history.free_draw_point_deletion(*p, 0);
                            self.0 = FreeDrawStatus::Point(l[next(i, 2)]);
                            break;
                        }
                    }
                },
                FreeDrawStatus::Polygon(poly) =>
                {
                    match poly.try_delete_free_draw_vertex(cursor_pos, camera_scale)
                    {
                        FreeDrawVertexDeletionResult::None => (),
                        FreeDrawVertexDeletionResult::Polygon(deleted) =>
                        {
                            edits_history.free_draw_point_deletion(deleted, 0);
                        },
                        FreeDrawVertexDeletionResult::Line(line, deleted) =>
                        {
                            edits_history.free_draw_point_deletion(deleted, 0);
                            self.0 = FreeDrawStatus::Line(line);
                        }
                    };
                }
            };
        }
    }

    #[inline]
    fn generate_brush(
        &mut self,
        manager: &mut EntitiesManager,
        drawn_brushes: &mut Ids,
        edits_history: &mut EditsHistory
    ) -> bool
    {
        if !matches!(self.0, FreeDrawStatus::Polygon(_))
        {
            return false;
        }

        let status = std::mem::take(&mut self.0);

        manager.spawn_drawn_brush(
            match_or_panic!(status, FreeDrawStatus::Polygon(poly), poly),
            drawn_brushes,
            edits_history
        );

        true
    }

    #[inline]
    pub fn delete_free_draw_vertex(&mut self, p: Vec2)
    {
        match &mut self.0
        {
            FreeDrawStatus::None => panic!("No vertexes to be removed."),
            FreeDrawStatus::Point(q) =>
            {
                assert!(p == *q, "Vertex asked to be removed is not the only one left.");
                self.0 = FreeDrawStatus::None;
            },
            FreeDrawStatus::Line([a, b]) =>
            {
                self.0 = FreeDrawStatus::Point(
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
            FreeDrawStatus::Polygon(poly) =>
            {
                self.0 = FreeDrawStatus::Line(return_if_none!(poly.delete_free_draw_vertex(p)));
            }
        }
    }

    #[inline]
    pub fn insert_free_draw_vertex(&mut self, p: Vec2)
    {
        match &mut self.0
        {
            FreeDrawStatus::None => self.0 = FreeDrawStatus::Point(p),
            FreeDrawStatus::Point(q) =>
            {
                assert!(
                    !q.around_equal(&p),
                    "New vertex has same coordinates as the only one in the shape."
                );
                self.0 = FreeDrawStatus::Line([*q, p]);
            },
            FreeDrawStatus::Line(l) =>
            {
                self.0 = FreeDrawStatus::Polygon(ConvexPolygon::new_sorted(
                    (*l).into_iter().chain(Some(p)),
                    None
                ));
            },
            FreeDrawStatus::Polygon(poly) =>
            {
                poly.insert_free_draw_vertex(p);
            }
        }
    }

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
            FreeDrawStatus::None => (),
            FreeDrawStatus::Point(p) =>
            {
                drawer.square_highlight(*p, Color::CursorPolygon);

                if !show_tooltips
                {
                    return;
                }

                free_draw_tooltip(
                    window,
                    camera,
                    egui_context,
                    *p,
                    return_if_none!(drawer.vx_tooltip_label(*p)),
                    &mut String::with_capacity(6)
                );
            },
            FreeDrawStatus::Line([start, end]) =>
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
                    free_draw_tooltip(
                        window,
                        camera,
                        egui_context,
                        *vx,
                        return_if_none!(drawer.vx_tooltip_label(*vx)),
                        &mut text
                    );
                }
            },
            FreeDrawStatus::Polygon(poly) =>
            {
                poly.draw_free_draw(window, camera, drawer, egui_context, show_tooltips);
            }
        };
    }
}
