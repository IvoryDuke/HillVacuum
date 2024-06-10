pub(in crate::map) mod convex_polygon;
pub mod mover;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::borrow::Cow;

use arrayvec::ArrayVec;
use bevy::prelude::{Transform, Vec2, Window};
use bevy_egui::egui;
use serde::{Deserialize, Serialize};
use shared::{return_if_no_match, return_if_none};

use self::{
    convex_polygon::{
        ConvexPolygon,
        ScaleInfo,
        ShearInfo,
        SideSelectionResult,
        SubtractResult,
        TextureSetResult,
        VertexHighlightMode,
        VertexesDeletionResult,
        VertexesMove,
        XtrusionInfo
    },
    mover::Mover
};
use super::{
    containers::{HvHashMap, Ids},
    drawer::{
        animation::{Animation, Animator, Timing},
        color::Color,
        drawing_resources::DrawingResources,
        texture::{
            Sprite,
            TextureInterface,
            TextureInterfaceExtra,
            TextureRotation,
            TextureScale,
            TextureSettings
        },
        EditDrawer,
        MapPreviewDrawer
    },
    editor::state::{
        clipboard::{ClipboardData, CopyToClipboard},
        grid::Grid,
        manager::{Animators, Brushes}
    },
    hv_vec,
    path::{
        calc_path_hull,
        common_edit_path,
        EditPath,
        MovementSimulator,
        Moving,
        NodesDeletionPayload,
        Path
    },
    properties::{Properties, PropertiesRefactor, Value},
    selectable_vector::VectorSelectionResult,
    thing::catalog::ThingsCatalog,
    HvVec
};
use crate::utils::{
    hull::{EntityHull, Flip, Hull},
    identifiers::{EntityCenter, EntityId, Id},
    iterators::SlicePairIter,
    math::lines_and_segments::{line_equation, LineEquation}
};

//=======================================================================//
// MACROS
//
//=======================================================================//

macro_rules! flip_funcs {
    ($($side:ident),+) => { paste::paste! { $(
        #[inline]
        #[must_use]
        pub fn [< check_flip_ $side >](&mut self, drawing_resources: &DrawingResources, value: f32, flip_texture: bool) -> bool
        {
            match self.data.polygon.[< check_flip_ $side >](drawing_resources, value, flip_texture)
            {
                Some(new_center) => !self.path_hull_out_of_bounds(new_center),
                None => false
            }
        }

        #[inline]
        pub fn [< flip_ $side >](&mut self, drawing_resources: &DrawingResources, value: f32, flip_texture: bool)
        {
            self.data.polygon.[< flip_ $side >](drawing_resources, value, flip_texture);
        }
    )+}};
}

//=======================================================================//
// TRAITS
//
//=======================================================================//

macro_rules! impl_payload_id {
    ($($t:ty),+) => { $(
        impl EntityId for $t
        {
            #[inline]
            fn id(&self) -> Id { self.0 }

            #[inline]
            fn id_as_ref(&self) -> &Id { &self.0 }
        }
    )+ };
}

impl_payload_id!(
    VertexesMovePayload,
    SplitPayload,
    XtrusionPayload,
    SidesDeletionPayload,
    ScalePayload,
    ShearPayload,
    RotatePayload
);

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[must_use]
pub(in crate::map) enum VertexesMoveResult
{
    Invalid,
    None,
    Valid(VertexesMovePayload)
}

impl VertexesMoveResult
{
    #[inline]
    fn from_result(value: convex_polygon::VertexesMoveResult, brush: &Brush) -> Self
    {
        use convex_polygon::VertexesMoveResult;

        match value
        {
            VertexesMoveResult::None => Self::None,
            VertexesMoveResult::Invalid => Self::Invalid,
            VertexesMoveResult::Valid(value) => Self::Valid(VertexesMovePayload(brush.id(), value))
        }
    }
}

#[must_use]
pub(in crate::map) struct VertexesMovePayload(Id, VertexesMove);

impl VertexesMovePayload
{
    #[inline]
    pub fn moved_indexes(&self) -> impl Iterator<Item = u8> + '_ { self.1.moved_indexes() }

    #[inline]
    pub fn paired_moved_indexes(&self) -> Option<SlicePairIter<u8>>
    {
        self.1.paired_moved_indexes()
    }
}

//=======================================================================//

#[must_use]
pub(in crate::map) enum SplitResult
{
    None,
    Invalid,
    Valid(SplitPayload)
}

#[must_use]
#[derive(Debug)]
pub(in crate::map) struct SplitPayload(Id, ArrayVec<u8, 2>);

impl From<(convex_polygon::SplitResult, Id)> for SplitResult
{
    #[inline]
    fn from(value: (convex_polygon::SplitResult, Id)) -> Self
    {
        use convex_polygon::SplitResult;

        match value.0
        {
            SplitResult::None => Self::None,
            SplitResult::Invalid => Self::Invalid,
            SplitResult::Valid(idxs) => Self::Valid(SplitPayload(value.1, idxs))
        }
    }
}

//=======================================================================//

#[must_use]
#[derive(Debug)]
pub(in crate::map) enum XtrusionResult
{
    None,
    Invalid,
    Valid(XtrusionPayload)
}

impl From<(convex_polygon::XtrusionResult, Id)> for XtrusionResult
{
    #[inline]
    fn from(value: (convex_polygon::XtrusionResult, Id)) -> Self
    {
        use convex_polygon::XtrusionResult;

        match value.0
        {
            XtrusionResult::None => Self::None,
            XtrusionResult::Invalid => Self::Invalid,
            XtrusionResult::Valid(info) => Self::Valid(XtrusionPayload(value.1, info))
        }
    }
}

#[must_use]
#[derive(Debug, Clone)]
pub(in crate::map) struct XtrusionPayload(Id, XtrusionInfo);

impl XtrusionPayload
{
    #[inline]
    #[must_use]
    pub const fn info(&self) -> &XtrusionInfo { &self.1 }
}

//=======================================================================//

#[must_use]
#[derive(Debug)]
pub(in crate::map) enum SidesDeletionResult
{
    None,
    Invalid,
    Valid(SidesDeletionPayload)
}

impl SidesDeletionResult
{
    #[inline]
    fn from_result(value: convex_polygon::SidesDeletionResult, identifier: Id) -> Self
    {
        use convex_polygon::SidesDeletionResult;

        match value
        {
            SidesDeletionResult::None => Self::None,
            SidesDeletionResult::Invalid => Self::Invalid,
            SidesDeletionResult::Valid(vecs) => Self::Valid(SidesDeletionPayload(identifier, vecs))
        }
    }
}

#[must_use]
#[derive(Debug)]
pub(in crate::map) struct SidesDeletionPayload(Id, HvVec<(Vec2, u8, bool)>);

//=======================================================================//

#[must_use]
#[derive(Debug)]
pub(in crate::map) enum ScaleResult
{
    Invalid,
    Valid(ScalePayload)
}

impl ScaleResult
{
    #[inline]
    fn from_result(value: convex_polygon::ScaleResult, brush: &Brush) -> Self
    {
        use convex_polygon::ScaleResult;

        match value
        {
            ScaleResult::Invalid => Self::Invalid,
            ScaleResult::Valid {
                new_center,
                vxs,
                texture_move
            } =>
            {
                if brush.path_hull_out_of_bounds(new_center)
                {
                    return Self::Invalid;
                }

                Self::Valid(ScalePayload(brush.id, vxs, texture_move))
            }
        }
    }
}

#[must_use]
#[derive(Debug)]
pub(in crate::map) struct ScalePayload(Id, HvVec<Vec2>, Option<TextureScale>);

//=======================================================================//

#[must_use]
#[derive(Debug)]
pub(in crate::map) enum ShearResult
{
    Invalid,
    Valid(ShearPayload)
}

impl ShearResult
{
    #[inline]
    fn from_result(value: Option<(Vec2, HvVec<f32>)>, brush: &Brush) -> Self
    {
        match value
        {
            Some((new_center, xys)) =>
            {
                if brush.path_hull_out_of_bounds(new_center)
                {
                    return Self::Invalid;
                }

                Self::Valid(ShearPayload(brush.id, xys))
            },
            None => Self::Invalid
        }
    }
}

#[must_use]
#[derive(Debug)]
pub(in crate::map) struct ShearPayload(Id, HvVec<f32>);

//=======================================================================//

#[must_use]
#[derive(Debug)]
pub(in crate::map) enum RotateResult
{
    Invalid,
    Valid(RotatePayload)
}

impl RotateResult
{
    #[inline]
    fn from_result(value: convex_polygon::RotateResult, brush: &Brush) -> Self
    {
        use convex_polygon::RotateResult;

        match value
        {
            RotateResult::Invalid => Self::Invalid,
            RotateResult::Valid {
                new_center,
                vxs,
                texture_move
            } =>
            {
                if brush.path_hull_out_of_bounds(new_center)
                {
                    return Self::Invalid;
                }

                Self::Valid(RotatePayload(brush.id, vxs, texture_move))
            }
        }
    }
}

#[must_use]
#[derive(Debug)]
pub(in crate::map) struct RotatePayload(Id, HvVec<Vec2>, Option<TextureRotation>);

//=======================================================================//
// TYPES
//
//=======================================================================//

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::map) struct BrushData
{
    /// The polygon of the brush.
    polygon:    ConvexPolygon,
    /// Platform path and anchored brushes.
    mover:      Mover,
    /// The properties of the brush.
    properties: Properties
}

impl BrushData
{
    #[inline]
    #[must_use]
    pub fn polygon_hull(&self) -> Hull { self.polygon.hull() }

    #[inline]
    #[must_use]
    pub fn sprite_hull(&self) -> Option<Hull> { self.polygon.sprite_hull() }

    #[inline]
    #[must_use]
    pub fn path_hull(&self) -> Option<Hull>
    {
        calc_path_hull(
            return_if_no_match!(&self.mover, Mover::Motor(motor), motor.path(), None),
            self.polygon.center()
        )
        .into()
    }

    #[inline]
    #[must_use]
    pub const fn has_path(&self) -> bool { self.mover.has_path() }

    #[inline]
    #[must_use]
    pub fn has_anchors(&self) -> bool { self.mover.has_anchors() }

    #[inline]
    #[must_use]
    pub const fn is_anchored(&self) -> bool { self.mover.is_anchored().is_some() }

    #[inline]
    #[must_use]
    pub fn contains_anchor(&self, identifier: Id) -> bool
    {
        match self.mover.anchors()
        {
            Some(ids) => ids.contains(&identifier),
            None => false
        }
    }

    #[inline]
    pub const fn anchors(&self) -> Option<&Ids> { self.mover.anchors() }

    #[inline]
    #[must_use]
    pub const fn has_texture(&self) -> bool { self.polygon.has_texture() }

    #[inline]
    pub fn texture_name(&self) -> Option<&str>
    {
        self.polygon.texture_settings().map(TextureInterface::name)
    }

    #[inline]
    #[must_use]
    pub fn set_texture(
        &mut self,
        drawing_resources: &DrawingResources,
        texture: &str
    ) -> TextureSetResult
    {
        self.polygon.set_texture(drawing_resources, texture)
    }

    #[inline]
    pub fn insert_anchor(&mut self, identifier: Id) { self.mover.insert_anchor(identifier); }

    #[inline]
    pub fn remove_anchor(&mut self, identifier: Id) { self.mover.remove_anchor(identifier); }

    #[inline]
    pub fn disanchor(&mut self)
    {
        assert!(self.is_anchored(), "Tried to disanchor brush that is not anchored.");
        self.mover = Mover::None;
    }

    #[inline]
    pub fn draw_prop(&self, camera: &Transform, drawer: &mut EditDrawer, color: Color, delta: Vec2)
    {
        self.polygon.draw_prop(camera, drawer, color, delta);

        return_if_no_match!(&self.mover, Mover::Motor(motor), motor)
            .path()
            .draw_prop(drawer, self.polygon.center() + delta);
    }
}

//=======================================================================//

/// The entity representing one of the shapes that make the maps.
#[must_use]
#[derive(Debug, Serialize, Deserialize)]
pub(in crate::map) struct Brush
{
    // The id of the brush.
    id:   Id,
    data: BrushData
}

impl CopyToClipboard for Brush
{
    #[inline]
    fn copy_to_clipboard(&self) -> ClipboardData
    {
        let mut data = self.data.clone();
        data.polygon.deselect_vertexes_no_indexes();
        ClipboardData::Brush(data, self.id)
    }
}

impl EntityHull for Brush
{
    #[inline]
    fn hull(&self) -> Hull { self.data.polygon.hull() }
}

impl EntityId for Brush
{
    #[inline]
    fn id(&self) -> Id { self.id }

    #[inline]
    fn id_as_ref(&self) -> &Id { &self.id }
}

impl EntityCenter for Brush
{
    #[inline]
    fn center(&self) -> Vec2 { self.center() }
}

impl Moving for Brush
{
    #[inline]
    fn path(&self) -> Option<&Path> { self.data.mover.path() }

    #[inline]
    fn has_path(&self) -> bool { self.data.mover.has_path() }

    #[inline]
    fn possible_moving(&self) -> bool { matches!(self.data.mover, Mover::None | Mover::Anchors(_)) }

    #[inline]
    fn draw_highlighted_with_path_nodes(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        brushes: Brushes,
        _: &ThingsCatalog,
        drawer: &mut EditDrawer,
        show_tooltips: bool
    )
    {
        self.draw_with_color(camera, drawer, Color::HighlightedSelectedEntity);
        self.path().unwrap().draw(
            window,
            camera,
            egui_context,
            drawer,
            self.center(),
            show_tooltips
        );
        self.draw_anchored_brushes(camera, brushes, drawer, Self::draw_highlighted_selected);
    }

    #[inline]
    fn draw_with_highlighted_path_node(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        brushes: Brushes,
        _: &ThingsCatalog,
        drawer: &mut EditDrawer,
        highlighted_node: usize,
        show_tooltips: bool
    )
    {
        self.draw_with_color(camera, drawer, Color::HighlightedSelectedEntity);
        self.path().unwrap().draw_with_highlighted_path_node(
            window,
            camera,
            egui_context,
            drawer,
            self.center(),
            highlighted_node,
            show_tooltips
        );
        self.draw_anchored_brushes(camera, brushes, drawer, Self::draw_selected);
    }

    #[inline]
    fn draw_with_path_node_addition(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        brushes: Brushes,
        _: &ThingsCatalog,
        drawer: &mut EditDrawer,
        pos: Vec2,
        index: usize,
        show_tooltips: bool
    )
    {
        self.draw_with_color(camera, drawer, Color::HighlightedSelectedEntity);
        self.path().unwrap().draw_with_node_insertion(
            window,
            camera,
            egui_context,
            drawer,
            pos,
            index,
            self.center(),
            show_tooltips
        );
        self.draw_anchored_brushes(camera, brushes, drawer, Self::draw_selected);
    }

    #[inline]
    fn draw_movement_simulation(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        brushes: Brushes,
        _: &ThingsCatalog,
        drawer: &mut EditDrawer,
        show_tooltips: bool,
        simulator: &MovementSimulator
    )
    {
        assert!(self.id == simulator.id(), "Simulator's ID is not equal to the Brush's ID.");

        let movement_vec = simulator.movement_vec();
        let center = self.center();

        self.data
            .polygon
            .draw_movement_simulation(camera, drawer, movement_vec);
        self.path().unwrap().draw_movement_simulation(
            window,
            camera,
            egui_context,
            drawer,
            center,
            movement_vec,
            show_tooltips
        );

        let anchors = return_if_none!(self.anchors_iter());
        let center = center + movement_vec;

        for id in anchors
        {
            let a_center = brushes.get(*id).center() + movement_vec;
            drawer.square_highlight(a_center, Color::BrushAnchor);
            drawer.line(a_center, center, Color::BrushAnchor);

            brushes
                .get(*id)
                .data
                .polygon
                .draw_movement_simulation(camera, drawer, movement_vec);
        }
    }

    #[inline]
    fn draw_map_preview_movement_simulation(
        &self,
        camera: &Transform,
        brushes: Brushes,
        _: &ThingsCatalog,
        drawer: &mut MapPreviewDrawer,
        animators: &Animators,
        simulator: &MovementSimulator
    )
    {
        assert!(self.id == simulator.id(), "Simulator's ID is not equal to the Brush's ID.");

        let movement_vec = simulator.movement_vec();
        self.data.polygon.draw_map_preview_movement_simulation(
            camera,
            drawer,
            animators.get(self.id),
            movement_vec
        );
        let anchors = return_if_none!(self.anchors_iter());

        for id in anchors
        {
            brushes.get(*id).data.polygon.draw_map_preview_movement_simulation(
                camera,
                drawer,
                animators.get(*id),
                movement_vec
            );
        }
    }
}

impl EditPath for Brush
{
    common_edit_path!();

    #[inline]
    fn set_path(&mut self, path: Path) { self.data.mover.set_path(path); }

    #[inline]
    fn take_path(&mut self) -> Path { self.data.mover.take_path() }
}

impl Brush
{
    //==============================================================
    // Flip

    flip_funcs!(above, below, left, right);

    //==============================================================
    // New

    #[inline]
    pub fn from_polygon<'a>(
        polygon: impl Into<Cow<'a, ConvexPolygon>>,
        identifier: Id,
        properties: Properties
    ) -> Self
    {
        match polygon.into()
        {
            Cow::Borrowed(polygon) =>
            {
                Self {
                    data: BrushData {
                        polygon: polygon.clone(),
                        mover: Mover::None,
                        properties
                    },
                    id:   identifier
                }
            },
            Cow::Owned(polygon) =>
            {
                Self {
                    data: BrushData {
                        polygon,
                        mover: Mover::None,
                        properties
                    },
                    id:   identifier
                }
            },
        }
    }

    #[inline]
    pub fn from_parts(data: BrushData, identifier: Id) -> Self
    {
        let BrushData {
            polygon,
            mover,
            properties
        } = data;
        let mut brush = Self::from_polygon(polygon, identifier, properties);

        match mover
        {
            Mover::None => (),
            Mover::Anchors(anchors) => brush.data.mover = Mover::Anchors(anchors),
            Mover::Motor(motor) => brush.data.mover.apply_motor(motor),
            Mover::Anchored(anchor_id) =>
            {
                assert!(
                    anchor_id != identifier,
                    "Anchor ID {anchor_id:?} is equal to the Brush ID"
                );
                brush.data.mover = Mover::Anchored(anchor_id);
            }
        };

        brush
    }

    //==============================================================
    // Info

    /// Returns an iterator to the vertexes of the underlying `ConvexPolygon`.
    #[inline]
    pub fn vertexes(&self) -> impl ExactSizeIterator<Item = Vec2> + Clone + '_
    {
        self.data.polygon.vertexes()
    }

    /// Returns an iterator to the vertexes of the underlying `ConvexPolygon`.
    #[inline]
    pub fn selected_vertexes(&self) -> Option<impl Iterator<Item = Vec2>>
    {
        self.data.polygon.selected_vertexes()
    }

    /// Returns an iterator to the vertexes of the underlying `ConvexPolygon`.
    #[inline]
    pub fn selected_sides_vertexes(&self) -> Option<impl Iterator<Item = Vec2>>
    {
        self.data.polygon.selected_sides_vertexes()
    }

    /// Returns the coordinates of the mean center of the underlying
    /// `ConvexPolygon`.
    #[inline]
    #[must_use]
    pub fn center(&self) -> Vec2 { self.data.polygon.center() }

    /// Returns true if 'p' is in the area delimited by the underlying
    /// `ConvexPolygon`.
    #[inline]
    #[must_use]
    pub fn contains_point(&self, p: Vec2) -> bool { self.data.polygon.point_in_polygon(p) }

    /// Returns a copy of the underlying `ConvexPolygon`.
    #[inline]
    pub fn polygon(&self) -> ConvexPolygon { self.data.polygon.clone() }

    #[inline]
    #[must_use]
    pub fn anchors_hull(&self, brushes: Brushes) -> Option<Hull>
    {
        if !self.data.mover.has_anchors()
        {
            return None;
        }

        Hull::from_points(self.anchors_iter().unwrap().map(|id| brushes.get(*id).center())).map(
            |hull| {
                let center = self.center();
                hull.merged(&Hull::new(center.y, center.y, center.x, center.x))
                    .bumped(2f32)
            }
        )
    }

    #[inline]
    #[must_use]
    pub fn sprite_hull(&self) -> Option<Hull> { self.data.polygon.sprite_hull() }

    #[inline]
    #[must_use]
    pub fn sprite_anchor_hull(&self) -> Option<Hull>
    {
        if !self.has_sprite()
        {
            return None;
        }

        let texture = self.texture_settings().unwrap();
        let center = self.center();

        Hull::from_points(
            [
                center,
                center + Vec2::new(texture.offset_x(), texture.offset_y())
            ]
            .into_iter()
        )
        .map(|hull| hull.bumped(2f32))
    }

    #[inline]
    #[must_use]
    pub fn sprite_and_anchor_hull(&self) -> Option<Hull>
    {
        self.sprite_hull()
            .map(|hull| hull.merged(&self.sprite_anchor_hull().unwrap()))
    }

    #[inline]
    #[must_use]
    pub fn global_hull(&self) -> Hull
    {
        let mut hull = self.hull();

        if let Some(s_hull) = self.sprite_hull()
        {
            hull = hull.merged(&s_hull);
        }

        if let Some(p_hull) = self.path_hull()
        {
            hull = hull.merged(&p_hull);
        }

        hull
    }

    //==============================================================
    // General Editing

    #[inline]
    pub fn into_parts(self) -> (BrushData, Id) { (self.data, self.id) }

    /// Moves the `Brush` by the amount delta.
    #[inline]
    pub fn check_move(&self, delta: Vec2, move_texture: bool) -> bool
    {
        self.data.polygon.check_move(delta, move_texture) &&
            !self.path_hull_out_of_bounds(self.center() + delta)
    }

    #[inline]
    pub fn check_texture_move(&self, delta: Vec2) -> bool
    {
        !self.has_texture() || self.data.polygon.check_texture_move(delta)
    }

    /// Moves the `Brush` by the amount delta.
    #[inline]
    pub fn move_by_delta(
        &mut self,
        drawing_resources: &DrawingResources,
        delta: Vec2,
        move_texture: bool
    )
    {
        self.data
            .polygon
            .move_by_delta(drawing_resources, delta, move_texture);
    }

    #[inline]
    pub fn move_texture(&mut self, drawing_resources: &DrawingResources, delta: Vec2)
    {
        self.data.polygon.move_texture(drawing_resources, delta);
    }

    /// Moves the `Brush` by the amount delta.
    #[inline]
    pub fn move_polygon(
        &mut self,
        drawing_resources: &DrawingResources,
        delta: Vec2,
        move_texture: bool
    )
    {
        self.data
            .polygon
            .move_by_delta(drawing_resources, delta, move_texture);
    }

    /// Swaps the polygon of `self` and `other`.
    #[inline]
    pub fn swap_polygon(&mut self, polygon: &mut ConvexPolygon)
    {
        std::mem::swap(&mut self.data.polygon, polygon);
    }

    //==============================================================
    // Snap

    #[inline]
    #[must_use]
    fn snap<F>(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: Grid,
        f: F
    ) -> Option<HvVec<(HvVec<u8>, Vec2)>>
    where
        F: Fn(&mut ConvexPolygon, &DrawingResources, Grid) -> Option<HvVec<(HvVec<u8>, Vec2)>>
    {
        f(&mut self.data.polygon, drawing_resources, grid)
    }

    #[inline]
    #[must_use]
    pub fn snap_vertexes(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: Grid
    ) -> Option<HvVec<(HvVec<u8>, Vec2)>>
    {
        self.snap(drawing_resources, grid, ConvexPolygon::snap_vertexes)
    }

    #[inline]
    #[must_use]
    pub fn snap_selected_vertexes(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: Grid
    ) -> Option<HvVec<(HvVec<u8>, Vec2)>>
    {
        self.snap(drawing_resources, grid, ConvexPolygon::snap_selected_vertexes)
    }

    #[inline]
    #[must_use]
    pub fn snap_selected_sides(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: Grid
    ) -> Option<HvVec<(HvVec<u8>, Vec2)>>
    {
        self.snap(drawing_resources, grid, ConvexPolygon::snap_selected_sides)
    }

    //==============================================================
    // Anchors

    #[inline]
    #[must_use]
    pub fn has_anchors(&self) -> bool { self.data.mover.has_anchors() }

    #[inline]
    #[must_use]
    pub fn anchorable(&self) -> bool { !(self.has_anchors() || self.has_path()) }

    #[inline]
    pub fn anchors_iter(&self) -> Option<impl ExactSizeIterator<Item = &Id> + Clone>
    {
        self.data.mover.anchors_iter()
    }

    #[inline]
    #[must_use]
    pub const fn anchored(&self) -> Option<Id> { self.data.mover.is_anchored() }

    #[inline]
    pub fn insert_anchor(&mut self, anchor: &Self)
    {
        assert!(self.id != anchor.id, "Brush ID {:?} is equal to the anchor's ID", self.id);
        self.data.mover.insert_anchor(anchor.id);
    }

    #[inline]
    pub fn anchor(&mut self, anchor: &mut Self)
    {
        self.insert_anchor(anchor);
        anchor.attach(self.id);
    }

    #[inline]
    pub fn remove_anchor(&mut self, anchor: &Self)
    {
        assert!(self.id != anchor.id, "Brush ID {:?} is equal to the anchor's ID", self.id);
        self.data.mover.remove_anchor(anchor.id);
    }

    #[inline]
    pub fn disanchor(&mut self, anchor: &mut Self)
    {
        self.remove_anchor(anchor);
        anchor.detach();
    }

    #[inline]
    pub fn attach(&mut self, identifier: Id)
    {
        assert!(matches!(self.data.mover, Mover::None), "Brush Mover is not None");
        self.data.mover = Mover::Anchored(identifier);
    }

    #[inline]
    pub fn detach(&mut self)
    {
        assert!(matches!(self.data.mover, Mover::Anchored(_)), "Brush is not anchored.");
        self.data.mover = Mover::None;
    }

    //==============================================================
    // Path

    #[inline]
    #[must_use]
    pub const fn no_motor_nor_anchored(&self) -> bool
    {
        matches!(self.data.mover, Mover::None | Mover::Anchors(_))
    }

    #[inline]
    pub fn take_mover(&mut self) -> Option<Mover>
    {
        if matches!(self.data.mover, Mover::None)
        {
            return None;
        }

        std::mem::take(&mut self.data.mover).into()
    }

    #[inline]
    fn path_mut(&mut self) -> &mut Path { self.data.mover.path_mut() }

    //==============================================================
    // Texture

    #[inline]
    #[must_use]
    pub const fn has_texture(&self) -> bool { self.data.has_texture() }

    #[inline]
    #[must_use]
    pub fn has_sprite(&self) -> bool { self.data.polygon.has_sprite() }

    #[inline]
    #[must_use]
    pub fn was_texture_edited(&mut self) -> bool { self.data.polygon.texture_edited() }

    #[inline]
    pub const fn texture_settings(&self) -> Option<&TextureSettings>
    {
        self.data.polygon.texture_settings()
    }

    #[inline]
    pub fn animator(&self, drawing_resources: &DrawingResources) -> Option<Animator>
    {
        Animator::new(
            self.texture_settings().unwrap().overall_animation(drawing_resources),
            self.id
        )
    }

    #[inline]
    #[must_use]
    pub fn check_texture_change(
        &mut self,
        drawing_resources: &DrawingResources,
        texture: &str
    ) -> bool
    {
        self.data.polygon.check_texture_change(drawing_resources, texture)
    }

    #[inline]
    #[must_use]
    pub fn set_texture(
        &mut self,
        drawing_resources: &DrawingResources,
        texture: &str
    ) -> TextureSetResult
    {
        self.data.set_texture(drawing_resources, texture)
    }

    #[inline]
    pub fn set_texture_settings(&mut self, texture: TextureSettings)
    {
        self.data.polygon.set_texture_settings(texture);
    }

    #[inline]
    pub fn remove_texture(&mut self) -> TextureSettings { self.data.polygon.remove_texture() }

    #[inline]
    #[must_use]
    pub fn check_texture_offset_x(
        &mut self,
        drawing_resources: &DrawingResources,
        value: f32
    ) -> bool
    {
        self.data.polygon.check_texture_offset_x(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_offset_x(
        &mut self,
        drawing_resources: &DrawingResources,
        value: f32
    ) -> Option<f32>
    {
        self.data.polygon.set_texture_offset_x(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn check_texture_offset_y(
        &mut self,
        drawing_resources: &DrawingResources,
        value: f32
    ) -> bool
    {
        self.data.polygon.check_texture_offset_y(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_offset_y(
        &mut self,
        drawing_resources: &DrawingResources,
        value: f32
    ) -> Option<f32>
    {
        self.data.polygon.set_texture_offset_y(drawing_resources, value)
    }

    #[inline]
    pub fn check_texture_scale_x(
        &mut self,
        drawing_resources: &DrawingResources,
        value: f32
    ) -> bool
    {
        self.data.polygon.check_texture_scale_x(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_scale_x(
        &mut self,
        drawing_resources: &DrawingResources,
        value: f32
    ) -> Option<f32>
    {
        self.data.polygon.set_texture_scale_x(drawing_resources, value)
    }

    #[inline]
    pub fn flip_texture_scale_x(&mut self, drawing_resources: &DrawingResources)
    {
        self.data.polygon.flip_texture_scale_x(drawing_resources);
    }

    #[inline]
    pub fn check_texture_scale_y(
        &mut self,
        drawing_resources: &DrawingResources,
        value: f32
    ) -> bool
    {
        self.data.polygon.check_texture_scale_y(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_scale_y(
        &mut self,
        drawing_resources: &DrawingResources,
        value: f32
    ) -> Option<f32>
    {
        self.data.polygon.set_texture_scale_y(drawing_resources, value)
    }

    #[inline]
    pub fn flip_scale_y(&mut self, drawing_resources: &DrawingResources)
    {
        self.data.polygon.flip_texture_scale_y(drawing_resources);
    }

    #[inline]
    pub fn set_texture_scroll_x(&mut self, value: f32) -> Option<f32>
    {
        self.data.polygon.set_texture_scroll_x(value)
    }

    #[inline]
    pub fn set_texture_scroll_y(&mut self, value: f32) -> Option<f32>
    {
        self.data.polygon.set_texture_scroll_y(value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_parallax_x(&mut self, value: f32) -> Option<f32>
    {
        self.data.polygon.set_texture_parallax_x(value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_parallax_y(&mut self, value: f32) -> Option<f32>
    {
        self.data.polygon.set_texture_parallax_y(value)
    }

    #[inline]
    pub fn check_texture_angle(&mut self, drawing_resources: &DrawingResources, value: f32)
        -> bool
    {
        self.data.polygon.check_texture_angle(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_angle(
        &mut self,
        drawing_resources: &DrawingResources,
        value: f32
    ) -> Option<f32>
    {
        self.data.polygon.set_texture_angle(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_height(&mut self, value: i8) -> Option<i8>
    {
        self.data.polygon.set_texture_height(value)
    }

    #[inline]
    pub fn check_texture_sprite(
        &mut self,
        drawing_resources: &DrawingResources,
        value: bool
    ) -> bool
    {
        self.data.polygon.check_texture_sprite(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_sprite(
        &mut self,
        drawing_resources: &DrawingResources,
        value: impl Into<Sprite>
    ) -> Option<(Sprite, f32, f32)>
    {
        self.data.polygon.set_texture_sprite(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn check_texture_within_bounds(&mut self, drawing_resources: &DrawingResources) -> bool
    {
        self.data.polygon.check_texture_within_bounds(drawing_resources)
    }

    #[inline]
    #[must_use]
    pub fn check_texture_animation_change(
        &mut self,
        drawing_resources: &DrawingResources,
        animation: &Animation
    ) -> bool
    {
        self.data
            .polygon
            .check_texture_animation_change(drawing_resources, animation)
    }

    #[inline]
    pub fn set_texture_animation(
        &mut self,
        drawing_resources: &DrawingResources,
        animation: Animation
    ) -> Animation
    {
        self.data.polygon.set_texture_animation(drawing_resources, animation)
    }

    #[inline]
    pub fn set_texture_list_animation(
        &mut self,
        drawing_resources: &DrawingResources,
        texture: &str
    ) -> Animation
    {
        self.data
            .polygon
            .set_texture_list_animation(drawing_resources, texture)
    }

    #[inline]
    pub fn generate_list_animation(&mut self, drawing_resources: &DrawingResources) -> Animation
    {
        self.data.polygon.generate_list_animation(drawing_resources)
    }

    #[inline]
    #[must_use]
    pub fn check_atlas_animation_x_partition(
        &mut self,
        drawing_resources: &DrawingResources,
        value: u32
    ) -> bool
    {
        self.data
            .polygon
            .check_atlas_animation_x_partition(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_atlas_animation_x_partition(
        &mut self,
        drawing_resources: &DrawingResources,
        value: u32
    ) -> Option<u32>
    {
        self.data
            .polygon
            .set_atlas_animation_x_partition(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn check_atlas_animation_y_partition(
        &mut self,
        drawing_resources: &DrawingResources,
        value: u32
    ) -> bool
    {
        self.data
            .polygon
            .check_atlas_animation_y_partition(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_atlas_animation_y_partition(
        &mut self,
        drawing_resources: &DrawingResources,
        value: u32
    ) -> Option<u32>
    {
        self.data
            .polygon
            .set_atlas_animation_y_partition(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn texture_atlas_animation_max_len(&self) -> usize
    {
        self.data.polygon.atlas_animation_max_len()
    }

    #[inline]
    #[must_use]
    pub fn set_texture_atlas_animation_len(&mut self, value: usize) -> Option<usize>
    {
        self.data.polygon.set_atlas_animation_len(value)
    }

    #[inline]
    pub fn set_texture_atlas_animation_timing(&mut self, timing: Timing) -> Timing
    {
        self.data.polygon.set_atlas_animation_timing(timing)
    }

    #[inline]
    pub fn set_atlas_animation_uniform_timing(&mut self) -> Option<Timing>
    {
        self.data.polygon.set_atlas_animation_uniform_timing()
    }

    #[inline]
    #[must_use]
    pub fn set_atlas_animation_per_frame_timing(&mut self) -> Option<Timing>
    {
        self.data.polygon.set_atlas_animation_per_frame_timing()
    }

    #[inline]
    #[must_use]
    pub fn set_texture_atlas_animation_uniform_time(&mut self, value: f32) -> Option<f32>
    {
        self.data.polygon.set_atlas_animation_uniform_time(value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_atlas_animation_frame_time(
        &mut self,
        index: usize,
        value: f32
    ) -> Option<f32>
    {
        self.data.polygon.set_atlas_animation_frame_time(index, value)
    }

    #[inline]
    pub fn move_down_atlas_animation_frame_time(&mut self, index: usize)
    {
        self.data.polygon.move_down_atlas_animation_frame_time(index);
    }

    #[inline]
    pub fn move_up_atlas_animation_frame_time(&mut self, index: usize)
    {
        self.data.polygon.move_up_atlas_animation_frame_time(index);
    }

    #[inline]
    #[must_use]
    pub fn set_list_animation_texture(&mut self, index: usize, texture: &str) -> Option<String>
    {
        self.data.polygon.set_list_animation_texture(index, texture)
    }

    #[inline]
    #[must_use]
    pub fn texture_list_animation_frame(&self, index: usize) -> &(String, f32)
    {
        self.data.polygon.texture_list_animation_frame(index)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_list_animation_time(&mut self, index: usize, time: f32) -> Option<f32>
    {
        self.data.polygon.set_list_animation_time(index, time)
    }

    #[inline]
    pub fn move_up_list_animation_frame(&mut self, index: usize)
    {
        self.data.polygon.move_up_list_animation_frame(index);
    }

    #[inline]
    pub fn move_down_list_animation_frame(&mut self, index: usize)
    {
        self.data.polygon.move_down_list_animation_frame(index);
    }

    #[inline]
    pub fn insert_list_animation_frame(&mut self, index: usize, texture: &str, time: f32)
    {
        self.data.polygon.insert_list_animation_frame(index, texture, time);
    }

    #[inline]
    pub fn pop_list_animation_frame(&mut self) { self.data.polygon.pop_list_animation_frame(); }

    #[inline]
    pub fn remove_list_animation_frame(&mut self, index: usize)
    {
        self.data.polygon.remove_list_animation_frame(index);
    }

    #[inline]
    pub fn push_list_animation_frame(&mut self, texture: &str)
    {
        self.data.polygon.push_list_animation_frame(texture);
    }

    //==============================================================
    // Properties

    #[inline]
    #[must_use]
    pub fn set_collision(&mut self, value: bool) -> Option<bool>
    {
        self.data.polygon.set_collision(value)
    }

    #[inline]
    #[must_use]
    pub const fn collision(&self) -> bool { self.data.polygon.collision() }

    #[inline]
    pub fn properties(&self) -> Properties { self.data.properties.clone() }

    #[inline]
    pub const fn properties_as_ref(&self) -> &Properties { &self.data.properties }

    #[inline]
    pub fn set_property(&mut self, key: &str, value: &Value) -> Option<Value>
    {
        self.data.properties.set(key, value)
    }

    #[inline]
    pub fn refactor_properties(&mut self, refactor: &PropertiesRefactor)
    {
        self.data.properties.refactor(refactor);
    }

    //==============================================================
    // Vertex Editing

    #[inline]
    #[must_use]
    pub const fn has_selected_vertexes(&self) -> bool { self.data.polygon.has_selected_vertexes() }

    #[inline]
    #[must_use]
    pub const fn selected_vertexes_amount(&self) -> u8
    {
        self.data.polygon.selected_vertexes_amount()
    }

    #[inline]
    #[must_use]
    pub fn selected_sides_amount(&self) -> u8 { self.data.polygon.selected_sides_amount() }

    #[inline]
    #[must_use]
    pub fn nearby_vertex(&self, cursor_pos: Vec2, camera_scale: f32) -> Option<Vec2>
    {
        self.data
            .polygon
            .nearby_vertex(cursor_pos, camera_scale)
            .map(|idx| self.data.polygon.vertex_at_index(idx))
    }

    /// Returns a `VertexSelectionResult` describing the state of the `SelectableVertex` closest to
    /// `cursor_pos` found. If a `SelectableVertex` is found and it is not selected, it is selected,
    /// but the function still returns `VertexSelectionResult::NotSelected`.
    #[inline]
    pub fn check_vertex_proximity_and_exclusively_select(
        &mut self,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> VectorSelectionResult
    {
        self.data
            .polygon
            .check_vertex_proximity_and_exclusively_select(cursor_pos, camera_scale)
    }

    #[inline]
    #[must_use]
    pub fn try_select_vertex(&mut self, pos: Vec2) -> Option<u8>
    {
        self.data.polygon.try_select_vertex(pos)
    }

    #[inline]
    #[must_use]
    pub fn vertex_at_index(&self, index: usize) -> Vec2 { self.data.polygon.vertex_at_index(index) }

    #[inline]
    pub fn toggle_vertex_at_index(&mut self, index: usize)
    {
        self.data.polygon.toggle_vertex_at_index(index);
    }

    #[inline]
    #[must_use]
    pub fn try_exclusively_select_vertex(&mut self, pos: Vec2) -> Option<HvVec<u8>>
    {
        self.data.polygon.try_exclusively_select_vertex(pos)
    }

    #[inline]
    #[must_use]
    pub fn select_vertexes_in_range(&mut self, range: &Hull) -> Option<HvVec<u8>>
    {
        self.data.polygon.select_vertexes_in_range(range)
    }

    #[inline]
    #[must_use]
    pub fn exclusively_select_vertexes_in_range(&mut self, range: &Hull) -> Option<HvVec<u8>>
    {
        self.data.polygon.exclusively_select_vertexes_in_range(range)
    }

    #[inline]
    #[must_use]
    pub fn select_all_vertexes(&mut self) -> Option<HvVec<u8>>
    {
        self.data.polygon.select_all_vertexes()
    }

    /// Toggles the selection of the `SelectableVertex` with coordinates `pos`,
    /// if any.
    #[inline]
    #[must_use]
    pub fn toggle_vertex_at_pos(&mut self, pos: Vec2) -> Option<u8>
    {
        self.data.polygon.toggle_vertex_at_pos(pos)
    }

    /// Toggles the selection of the first `SelectableVertex` found to be close
    /// to `cursor_pos`, if any.
    #[inline]
    #[must_use]
    pub fn toggle_vertex_nearby_cursor_pos(
        &mut self,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> Option<(Vec2, u8, bool)>
    {
        self.data
            .polygon
            .toggle_vertex_nearby_cursor_pos(cursor_pos, camera_scale)
    }

    /// Deselects all `SelectableVertex` of the underlying `ConvexPolygon`.
    #[inline]
    #[must_use]
    pub fn deselect_vertexes(&mut self) -> Option<HvVec<u8>>
    {
        self.data.polygon.deselect_vertexes()
    }

    /// Deselects all selected vertexes.
    #[inline]
    pub fn deselect_vertexes_no_indexes(&mut self)
    {
        self.data.polygon.deselect_vertexes_no_indexes();
    }

    /// Adds a vertex to the polygon if it's possible to do so without losing convexity and returns
    /// whever it was possible to do so.
    #[inline]
    #[must_use]
    pub fn try_vertex_insertion_at_index(
        &mut self,
        drawing_resources: &DrawingResources,
        pos: Vec2,
        index: usize,
        selected: bool
    ) -> bool
    {
        self.data
            .polygon
            .try_vertex_insertion_at_index(drawing_resources, pos, index, selected)
    }

    /// Inserts a new vertex with position `pos` at `index`.
    #[inline]
    pub fn insert_vertex_at_index(
        &mut self,
        drawing_resources: &DrawingResources,
        pos: Vec2,
        index: usize,
        selected: bool
    )
    {
        self.data
            .polygon
            .insert_vertex_at_index(drawing_resources, pos, index, selected);
    }

    /// Returns the index the closest projection of `cursor_pos` on the shape of
    /// the underlying `ConvexPolygon` would have if it were added to it.
    #[inline]
    #[must_use]
    pub fn vx_projection_insertion_index(&self, cursor_pos: Vec2) -> Option<usize>
    {
        self.data.polygon.vertex_insertion_index(cursor_pos)
    }

    /// Returns true if inserting `pos` in the underlying `ConvexPolygon` at
    /// index `index` generates a valid polygon.
    #[inline]
    #[must_use]
    pub fn is_new_vertex_at_index_valid(&mut self, pos: Vec2, index: usize) -> bool
    {
        self.data.polygon.is_new_vertex_at_index_valid(pos, index)
    }

    /// Deletes the vertex at `index`.
    #[inline]
    pub fn delete_vertex_at_index(&mut self, drawing_resources: &DrawingResources, index: usize)
    {
        self.data.polygon.delete_vertex_at_index(drawing_resources, index);
    }

    /// Returns a [`VertexesDeletionResult`] describing the outcome of the deletion of the selected
    /// vertexes.
    #[inline]
    pub fn check_selected_vertexes_deletion(&self) -> VertexesDeletionResult
    {
        self.data.polygon.check_selected_vertexes_deletion()
    }

    /// Tries to remove the selected `SelectableVertexes`, does nothing if the
    /// result `ConvexPolygon` would have less than 3 sides.
    #[inline]
    pub fn delete_selected_vertexes(
        &mut self,
        drawing_resources: &DrawingResources
    ) -> Option<HvVec<(Vec2, u8)>>
    {
        self.data.polygon.delete_selected_vertexes(drawing_resources)
    }

    /// Moves the selected `SelectableVertexes` by the amount `delta`.
    #[inline]
    pub fn check_selected_vertexes_move(&mut self, delta: Vec2) -> VertexesMoveResult
    {
        VertexesMoveResult::from_result(self.data.polygon.check_selected_vertexes_move(delta), self)
    }

    /// Applies the vertexes move described by `payload`.
    #[inline]
    pub fn apply_vertexes_move_result(
        &mut self,
        drawing_resources: &DrawingResources,
        payload: VertexesMovePayload
    ) -> VertexesMove
    {
        assert!(payload.0 == self.id, "VertexesMovePayload's ID is not equal to the Brush's ID.");
        self.redo_vertexes_move(drawing_resources, &payload.1);
        payload.1
    }

    /// Undoes a vertexes move.
    #[inline]
    pub fn undo_vertexes_move(
        &mut self,
        drawing_resources: &DrawingResources,
        vxs_move: &VertexesMove
    )
    {
        let old_center = self.center();
        self.data.polygon.undo_vertexes_move(drawing_resources, vxs_move);

        if !self.has_path()
        {
            return;
        }

        let center = self.center();
        self.path_mut().translate(old_center - center);
    }

    /// Redoes a vertexes move.
    #[inline]
    pub fn redo_vertexes_move(
        &mut self,
        drawing_resources: &DrawingResources,
        vxs_move: &VertexesMove
    )
    {
        let old_center = self.center();
        self.data
            .polygon
            .apply_vertexes_move_result(drawing_resources, vxs_move);

        if !self.has_path()
        {
            return;
        }

        let center = self.center();
        self.path_mut().translate(old_center - center);
    }

    /// Returns a [`SplitResult`] describing whever the polygon can be split.
    #[inline]
    pub fn check_split(&self) -> SplitResult { (self.data.polygon.check_split(), self.id).into() }

    /// Splits the polygon in two halves based on the `payload`.
    #[inline]
    pub fn split(
        &mut self,
        drawing_resources: &DrawingResources,
        payload: &SplitPayload
    ) -> ConvexPolygon
    {
        assert!(payload.0 == self.id, "SplitPayload's ID is not equal to the Brush's ID.");
        self.data.polygon.split(drawing_resources, &payload.1)
    }

    /// Moves the vertexes at the indexes and by the deltas specified in the iterator.
    #[inline]
    pub fn move_vertexes_at_indexes<'a, I: Iterator<Item = &'a u8>>(
        &mut self,
        idxs: impl Iterator<Item = (I, Vec2)>
    )
    {
        self.data.polygon.move_vertexes_at_indexes(idxs);
    }

    //==============================================================
    // Side editing

    /// Returns the coordinates and index of the side near the cursor, if any.
    #[inline]
    #[must_use]
    pub fn nearby_side(&self, cursor_pos: Vec2, camera_scale: f32) -> Option<([Vec2; 2], usize)>
    {
        self.data.polygon.nearby_side(cursor_pos, camera_scale)
    }

    /// Returns a `VertexSelectionResult` describing the state of the closest to
    /// `cursor_pos` side found, if any. If a side is found and it is not
    /// selected, it is selected, but the function stil  returns
    /// `VertexSelectionResult::NotSelected`.
    #[inline]
    pub fn check_side_proximity_and_exclusively_select(
        &mut self,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> SideSelectionResult
    {
        self.data
            .polygon
            .check_side_proximity_and_select(cursor_pos, camera_scale)
    }

    /// The information required to start an xtrusion attempt based on the side near the cursor, if
    /// any.
    #[inline]
    pub fn xtrusion_info(
        &self,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> Option<([Vec2; 2], Vec2, XtrusionPayload)>
    {
        self.data
            .polygon
            .xtrusion_info(cursor_pos, camera_scale)
            .map(|(side, normal, info)| (side, normal, XtrusionPayload(self.id, info)))
    }

    /// Returns a [`XtrusionResult`] describing whever the xtrusion attempt can occur.
    #[inline]
    pub fn matching_xtrusion_info(&self, normal: Vec2) -> XtrusionResult
    {
        (self.data.polygon.matching_xtrusion_info(normal), self.id).into()
    }

    /// Tries to select the side with the same coordinates a `side`, and returns the index of the
    /// selected side, if any.
    #[inline]
    #[must_use]
    pub fn try_select_side(&mut self, side: &[Vec2; 2]) -> Option<u8>
    {
        self.data.polygon.try_select_side(side)
    }

    /// Tries to exclusively select the side with the same coordinates a `side`, and returns the
    /// indexes of the sides whose selection has changed, if any.
    #[inline]
    #[must_use]
    pub fn try_exclusively_select_side(&mut self, side: &[Vec2; 2]) -> Option<HvVec<u8>>
    {
        self.data.polygon.try_exclusively_select_side(side)
    }

    /// Selects the sides in `range` and returns the indexes of the sides that were selected, if
    /// any.
    #[inline]
    #[must_use]
    pub fn select_sides_in_range(&mut self, range: &Hull) -> Option<HvVec<u8>>
    {
        self.data.polygon.select_sides_in_range(range)
    }

    /// Exclusively selects the sides in `range` and returns the indexes of the sides whose
    /// selection has changed, if any.
    #[inline]
    #[must_use]
    pub fn exclusively_select_sides_in_range(&mut self, range: &Hull) -> Option<HvVec<u8>>
    {
        self.data.polygon.exclusively_select_sides_in_range(range)
    }

    /// Selects the side with coordinates `l`, if any.
    #[inline]
    #[must_use]
    pub fn toggle_side(&mut self, l: &[Vec2; 2]) -> Option<u8>
    {
        self.data.polygon.toggle_side_at_pos(l)
    }

    /// Toggles the selection of the first side found to be close to
    /// `cursor_pos`, if any.
    #[inline]
    #[must_use]
    pub fn toggle_side_nearby_cursor_pos(
        &mut self,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> Option<([Vec2; 2], u8, bool)>
    {
        self.data
            .polygon
            .toggle_side_nearby_cursor_pos(cursor_pos, camera_scale)
    }

    /// Returns a [`SidesDeletionResult`] describing the outcome of the deletion of the selected
    /// sides.
    #[inline]
    pub fn check_selected_sides_deletion(&self) -> SidesDeletionResult
    {
        SidesDeletionResult::from_result(self.data.polygon.check_selected_sides_deletion(), self.id)
    }

    /// Tries to remove the selected sides as long as the resulting
    /// `ConvexPolygon` has at least 3 sides.
    #[inline]
    pub fn delete_selected_sides(
        &mut self,
        drawing_resources: &DrawingResources,
        payload: SidesDeletionPayload
    ) -> HvVec<(Vec2, u8, bool)>
    {
        assert!(
            payload.id() == self.id,
            "SidesDeletionPayload's ID is not equal to the Brush's ID."
        );
        self.data.polygon.delete_selected_sides(
            drawing_resources,
            payload.1.iter().rev().map(|(_, idx, _)| *idx as usize)
        );
        payload.1
    }

    /// Moves the selected lines by the amount `delta`.
    #[inline]
    pub fn check_selected_sides_move(&mut self, delta: Vec2) -> VertexesMoveResult
    {
        VertexesMoveResult::from_result(self.data.polygon.check_selected_sides_move(delta), self)
    }

    //==============================================================
    // Clip

    /// Splits the underlying `ConvexPolygon` in two if `clip_line` crosses its
    /// shape. Returns the polygon generated by the clip, if any.
    #[inline]
    #[must_use]
    pub fn clip(
        &self,
        drawing_resources: &DrawingResources,
        clip_line: &[Vec2; 2]
    ) -> Option<[ConvexPolygon; 2]>
    {
        let hull = self.hull();
        let clip_line_equation = line_equation(clip_line);

        // Intersection check of the polygon's hull.
        match clip_line_equation
        {
            LineEquation::Horizontal(y) if !(hull.bottom()..hull.top()).contains(&y) =>
            {
                return None
            },
            LineEquation::Vertical(x) if !(hull.left()..hull.right()).contains(&x) => return None,
            LineEquation::Generic(m, q) =>
            {
                let y_at_left = m * hull.left() + q;
                let y_at_right = m * hull.right() + q;

                if (y_at_left <= hull.bottom() && y_at_right <= hull.bottom()) ||
                    (y_at_left >= hull.top() && y_at_right >= hull.top())
                {
                    return None;
                }

                let x_at_top = (hull.bottom() - q) / m;
                let x_at_bottom = (hull.top() - q) / m;

                if (x_at_top <= hull.left() && x_at_bottom <= hull.left()) ||
                    (x_at_top >= hull.right() && x_at_bottom >= hull.right())
                {
                    return None;
                }
            },
            _ => ()
        };

        self.data.polygon.clip(drawing_resources, clip_line)
    }

    //==============================================================
    // Shatter

    /// Shatters the underlying `ConvexPolygon` in triangles depending on the
    /// position of `cursor_pos` with respect to the polygon's shape.
    #[inline]
    pub fn shatter(
        &self,
        drawing_resources: &DrawingResources,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> Option<impl ExactSizeIterator<Item = ConvexPolygon>>
    {
        self.data.polygon.shatter(drawing_resources, cursor_pos, camera_scale)
    }

    //==============================================================
    // Hollow

    /// Returns the four wall [`Brush`]es generated from the shape of `self`, if any.
    #[inline]
    pub fn hollow(
        &self,
        drawing_resources: &DrawingResources,
        grid_size: f32
    ) -> Option<impl ExactSizeIterator<Item = ConvexPolygon>>
    {
        self.data.polygon.hollow(drawing_resources, grid_size)
    }

    //==============================================================
    // Intersection

    /// Returns the intersection between the shapes of `self` and `other`, if any.
    #[inline]
    #[must_use]
    pub fn intersection(&self, other: &Self) -> Option<ConvexPolygon>
    {
        self.data.polygon.intersection(&other.data.polygon)
    }

    /// Sets the shape of `other` to the intersection between `self` and `other`.
    #[inline]
    #[must_use]
    pub fn intersect(&self, other: &mut ConvexPolygon) -> bool
    {
        if let Some(cp) = self.data.polygon.intersection(other)
        {
            *other = cp;
            return true;
        }

        false
    }

    //==============================================================
    // Subtract

    /// Returns a [`SubtractResult`] describing the outcome of the subtraction of `other`'s shape
    /// from `self`'s.
    #[inline]
    pub fn subtract(&self, drawing_resources: &DrawingResources, other: &Self) -> SubtractResult
    {
        self.data.polygon.subtract(drawing_resources, &other.data.polygon)
    }

    //==============================================================
    // Scale

    /// Returns a [`ScaleResult`] describing the validity of a scale.
    #[inline]
    pub fn check_scale(
        &mut self,
        drawing_resources: &DrawingResources,
        info: &ScaleInfo,
        scale_texture: bool
    ) -> ScaleResult
    {
        ScaleResult::from_result(
            self.data.polygon.check_scale(drawing_resources, info, scale_texture),
            self
        )
    }

    /// Returns a [`ScaleResult`] describing the validity of a scale with flip.
    #[inline]
    pub fn check_flip_scale(
        &mut self,
        drawing_resources: &DrawingResources,
        info: &ScaleInfo,
        flip_queue: &ArrayVec<Flip, 2>,
        scale_texture: bool
    ) -> ScaleResult
    {
        ScaleResult::from_result(
            self.data
                .polygon
                .check_flip_scale(drawing_resources, info, flip_queue, scale_texture),
            self
        )
    }

    /// Scales `self` based on `payload`.
    #[inline]
    pub fn set_scale_coordinates(
        &mut self,
        drawing_resources: &DrawingResources,
        payload: ScalePayload
    )
    {
        assert!(payload.id() == self.id, "ScalePayload's ID is not equal to the Brush's ID.");
        self.data.polygon.set_coordinates(payload.1);

        let tex_scale = return_if_none!(payload.2);
        _ = self
            .data
            .polygon
            .set_texture_scale_x(drawing_resources, tex_scale.scale_x);
        _ = self
            .data
            .polygon
            .set_texture_scale_y(drawing_resources, tex_scale.scale_y);
        _ = self
            .data
            .polygon
            .set_texture_offset_x(drawing_resources, tex_scale.offset.x);
        _ = self
            .data
            .polygon
            .set_texture_offset_y(drawing_resources, tex_scale.offset.y);
    }

    //==============================================================
    // Shear

    /// Returns a [`ShearResult`] describing the validity of the vertical shear.
    #[inline]
    pub fn check_horizontal_shear(&self, info: &ShearInfo) -> ShearResult
    {
        ShearResult::from_result(self.data.polygon.check_horizontal_shear(info), self)
    }

    /// Sets the x coordinates of the vertexes based on `payload`.
    #[inline]
    pub fn set_x_coordinates(&mut self, payload: ShearPayload)
    {
        assert!(payload.id() == self.id, "ShearPayload's ID is not equal to the Brush's ID.");
        self.data.polygon.set_x_coordinates(payload.1);
    }

    /// Returns a [`ShearResult`] describing the validity of the vertical shear.
    #[inline]
    pub fn check_vertical_shear(&self, info: &ShearInfo) -> ShearResult
    {
        ShearResult::from_result(self.data.polygon.check_vertical_shear(info), self)
    }

    /// Sets the y coordinates of the vertexes based on `payload`.
    #[inline]
    pub fn set_y_coordinates(&mut self, payload: ShearPayload)
    {
        assert!(payload.id() == self.id, "ShearPayload's ID is not equal to the Brush's ID.");
        self.data.polygon.set_y_coordinates(payload.1);
    }

    //==============================================================
    // Rotate

    /// Returns a [`RotateResult`] describing the validity of the rotation.
    #[inline]
    pub fn check_rotate(
        &mut self,
        drawing_resources: &DrawingResources,
        pivot: Vec2,
        angle: f32,
        rotate_texture: bool
    ) -> RotateResult
    {
        RotateResult::from_result(
            self.data
                .polygon
                .check_rotation(drawing_resources, pivot, angle, rotate_texture),
            self
        )
    }

    /// Rotates `self` based on `payload`.
    #[inline]
    pub fn set_rotation_coordinates(
        &mut self,
        drawing_resources: &DrawingResources,
        payload: RotatePayload
    )
    {
        assert!(payload.id() == self.id, "RotatePayload's ID is not equal to the Brush's ID.");
        self.data.polygon.set_coordinates(payload.1);

        let tex_rotate = return_if_none!(payload.2);
        _ = self
            .data
            .polygon
            .set_texture_angle(drawing_resources, tex_rotate.angle);
        _ = self
            .data
            .polygon
            .set_texture_offset_x(drawing_resources, tex_rotate.offset.x);
        _ = self
            .data
            .polygon
            .set_texture_offset_y(drawing_resources, tex_rotate.offset.y);
    }

    //==============================================================
    // Draw

    /// Draws the polygon for the map preview.
    #[inline]
    pub fn draw_map_preview(
        &self,
        camera: &Transform,
        drawer: &mut MapPreviewDrawer,
        animator: Option<&Animator>
    )
    {
        if let Some(animator) = animator
        {
            assert!(animator.id() == self.id, "Animator's ID is not equal to the Brush's ID.");
        }

        self.data.polygon.draw_map_preview(camera, drawer, animator);
    }

    /// Draws the polygon with the desired `color`.
    #[inline]
    pub fn draw_with_color(&self, camera: &Transform, drawer: &mut EditDrawer, color: Color)
    {
        self.data.polygon.draw(camera, drawer, color);
    }

    /// Draws the polygon not-selected.
    #[inline]
    pub fn draw_non_selected(&self, camera: &Transform, drawer: &mut EditDrawer)
    {
        self.draw_with_color(camera, drawer, Color::NonSelectedEntity);
    }

    /// Draws the polygon selected.
    #[inline]
    pub fn draw_selected(&self, camera: &Transform, drawer: &mut EditDrawer)
    {
        self.draw_with_color(camera, drawer, Color::SelectedEntity);
    }

    /// Draws the polygon highlighted selected.
    #[inline]
    pub fn draw_highlighted_selected(&self, camera: &Transform, drawer: &mut EditDrawer)
    {
        self.draw_with_color(camera, drawer, Color::HighlightedSelectedEntity);
    }

    /// Draws the polygon highlighted non selected.
    #[inline]
    pub fn draw_highlighted_non_selected(&self, camera: &Transform, drawer: &mut EditDrawer)
    {
        self.draw_with_color(camera, drawer, Color::HighlightedNonSelectedEntity);
    }

    /// Draws the polygon opaque.
    #[inline]
    pub fn draw_opaque(&self, camera: &Transform, drawer: &mut EditDrawer)
    {
        self.draw_with_color(camera, drawer, Color::OpaqueEntity);
    }

    /// Draws the line passing through the side at `index`.
    #[inline]
    pub fn draw_extended_side(
        &self,
        window: &Window,
        camera: &Transform,
        drawer: &mut EditDrawer,
        index: usize,
        color: Color
    )
    {
        self.data
            .polygon
            .draw_extended_side(window, camera, drawer, index, color);
    }

    /// Draws the underlying `ConvexPolygon` with the special vertex highlight
    /// procedure.
    #[inline]
    pub fn draw_with_vertex_highlights(
        &self,
        window: &Window,
        camera: &Transform,
        drawer: &mut EditDrawer,
        egui_context: &egui::Context,
        hgl_mode: &VertexHighlightMode,
        show_tooltips: bool
    )
    {
        self.data.polygon.draw_with_vertex_highlight(
            window,
            camera,
            drawer,
            egui_context,
            hgl_mode,
            show_tooltips
        );
    }

    /// Draws the polygon with a solid color.
    #[inline]
    pub fn draw_wih_solid_color(&self, drawer: &mut EditDrawer, color: Color)
    {
        drawer.polygon_with_solid_color(self.vertexes(), color);
    }

    /// Draws the anchors connecting the center of `self` to the centers of the anchored
    /// [`Brush`]es.
    #[inline]
    pub fn draw_anchors(&self, brushes: Brushes, drawer: &mut EditDrawer)
    {
        let start = self.center();
        let anchors = return_if_none!(self.anchors_iter());
        drawer.square_highlight(start, Color::BrushAnchor);
        drawer.anchor_highlight(start, Color::BrushAnchor);

        for id in anchors
        {
            let end = brushes.get(*id).center();
            drawer.square_highlight(end, Color::BrushAnchor);
            drawer.line(start, end, Color::BrushAnchor);
        }
    }

    /// Draws the anchored [`Brush`]es based on `f`.
    #[inline]
    fn draw_anchored_brushes<F>(
        &self,
        camera: &Transform,
        brushes: Brushes,
        drawer: &mut EditDrawer,
        f: F
    ) where
        F: Fn(&Self, &Transform, &mut EditDrawer)
    {
        for brush in self.data.mover.anchors_iter().unwrap().map(|id| brushes.get(*id))
        {
            f(brush, camera, drawer);
        }
    }

    /// Draws the sprite.
    #[inline]
    pub fn draw_sprite(&self, drawer: &mut EditDrawer, color: Color)
    {
        self.data.polygon.draw_sprite(drawer, color);
        self.data.polygon.draw_sprite_highlight(drawer);
    }

    /// Draws the sprite for the map preview.
    #[inline]
    pub fn draw_map_preview_sprite(
        &self,
        drawer: &mut MapPreviewDrawer,
        animator: Option<&Animator>
    )
    {
        self.data.polygon.draw_map_preview_sprite(drawer, animator);
    }
}

//=======================================================================//

/// A convex polygon characterized by an optional [`Mover`], an optional texture, and certain
/// properties.
#[must_use]
pub struct BrushViewer
{
    /// The [`Id`].
    pub id:         Id,
    /// The vertexes.
    pub vertexes:   HvVec<Vec2>,
    /// The texture.
    pub texture:    Option<TextureSettings>,
    /// The [`Mover`].
    pub mover:      Mover,
    /// Whever collision against the polygonal shape is enabled.
    pub collision:  bool,
    /// The properties.
    pub properties: HvHashMap<String, Value>
}

impl BrushViewer
{
    /// Returns a new [`BrushViewer`].
    #[inline]
    pub(in crate::map) fn new(brush: Brush) -> Self
    {
        let (
            BrushData {
                polygon,
                mover,
                properties
            },
            id
        ) = brush.into_parts();
        let collision = polygon.collision();

        Self {
            id,
            vertexes: hv_vec![collect; polygon.vertexes()],
            texture: polygon.take_texture_settings(),
            mover,
            collision,
            properties: properties.take()
        }
    }

    /// Sets the [`Animation`] of the texture.
    #[inline]
    pub(in crate::map) fn set_texture_animation(&mut self, animation: Animation)
    {
        unsafe {
            self.texture.as_mut().unwrap().unsafe_set_animation(animation);
        }
    }
}
