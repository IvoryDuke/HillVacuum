pub(in crate::map) mod convex_polygon;
pub mod mover;
pub mod path;
pub(in crate::map) mod selectable_vector;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::borrow::Cow;

use arrayvec::ArrayVec;
use bevy::prelude::{Transform, Vec2, Window};
use bevy_egui::egui;
use selectable_vector::SelectableVector;
use serde::{
    de::{MapAccess, Visitor},
    ser::SerializeStruct,
    Deserialize,
    Deserializer,
    Serialize
};
use shared::return_if_none;

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
    mover::{Motor, Mover},
    path::{
        overall_values::OverallMovement,
        MovementSimulator,
        MovementValueEdit,
        NodeSelectionResult,
        NodesMove,
        Path,
        StandbyValueEdit
    },
    selectable_vector::VectorSelectionResult
};
use super::{
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
        manager::{Animators, Brushes, BrushesMut}
    },
    hv_vec,
    HvVec,
    OutOfBounds
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
            match self.polygon.[< check_flip_ $side >](drawing_resources, value, flip_texture)
            {
                Some(new_center) => !self.path_hull_out_of_bounds(new_center),
                None => false
            }
        }

        #[inline]
        pub fn [< flip_ $side >](&mut self, drawing_resources: &DrawingResources, value: f32, flip_texture: bool)
        {
            self.polygon.[< flip_ $side >](drawing_resources, value, flip_texture);
        }
    )+}};
}

//=======================================================================//

macro_rules! path_nodes_value {
    ($(($value:ident, $t:ty)),+) => { paste::paste! { $(
        #[inline]
        pub fn [< set_selected_path_nodes_ $value >](&mut self, value: f32) -> Option<$t>
        {
            self.path_mut_set_dirty().[< set_selected_nodes_ $value >](value)
        }

        #[inline]
        pub fn [< undo_path_nodes_ $value _edit >](&mut self, edit: &$t)
        {
            self.path_mut_set_dirty().[< undo_ $value _edit >](edit)
        }

        #[inline]
        pub fn [< redo_path_nodes_ $value _edit >](&mut self, edit: &$t)
        {
            self.path_mut_set_dirty().[< redo_ $value _edit >](edit)
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
    NodesDeletionPayload,
    SidesDeletionPayload,
    NodesMovePayload,
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
pub(in crate::map) enum NodesMoveResult
{
    None,
    Invalid,
    Valid(NodesMovePayload)
}

impl From<(path::NodesMoveResult, Id)> for NodesMoveResult
{
    #[inline]
    fn from(value: (path::NodesMoveResult, Id)) -> Self
    {
        use path::NodesMoveResult;

        match value.0
        {
            NodesMoveResult::None => Self::None,
            NodesMoveResult::Invalid => Self::Invalid,
            NodesMoveResult::Valid(m) => Self::Valid(NodesMovePayload(value.1, m))
        }
    }
}

#[must_use]
pub(in crate::map) struct NodesMovePayload(Id, NodesMove);

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
pub(in crate::map) enum NodesDeletionResult
{
    None,
    Invalid,
    Valid(NodesDeletionPayload)
}

impl From<(path::NodesDeletionResult, Id)> for NodesDeletionResult
{
    #[inline]
    fn from(value: (path::NodesDeletionResult, Id)) -> Self
    {
        use path::NodesDeletionResult;

        match value.0
        {
            NodesDeletionResult::None => Self::None,
            NodesDeletionResult::Invalid => Self::Invalid,
            NodesDeletionResult::Valid(nodes) => Self::Valid(NodesDeletionPayload(value.1, nodes))
        }
    }
}

#[must_use]
#[derive(Debug)]
pub(in crate::map) struct NodesDeletionPayload(Id, HvVec<(Vec2, u8)>);

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

/// The entity representing one of the shapes that make the maps.
#[must_use]
#[derive(Debug)]
pub(in crate::map) struct Brush
{
    // The polygon of the brush.
    polygon:     ConvexPolygon,
    // The id of the brush.
    id:          Id,
    // Platform path and anchored brushes.
    mover:       Mover,
    path_edited: bool
}

impl Serialize for Brush
{
    #[inline]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer
    {
        let mut s = serializer.serialize_struct("Brush", 3)?;
        s.serialize_field("polygon", &self.polygon)?;
        s.serialize_field("id", &self.id)?;
        s.serialize_field("mover", &self.mover)?;
        s.end()
    }
}

impl<'de> Deserialize<'de> for Brush
{
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>
    {
        const FIELDS: &[&str] = &["polygon", "id", "mover"];

        enum Field
        {
            Polygon,
            Id,
            Mover
        }

        impl<'de> Deserialize<'de> for Field
        {
            #[inline]
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>
            {
                struct FieldVisitor;

                impl<'de> Visitor<'de> for FieldVisitor
                {
                    type Value = Field;

                    #[inline]
                    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result
                    {
                        formatter.write_str("`polygon` or `id` or 'mover'")
                    }

                    #[inline]
                    fn visit_str<E>(self, value: &str) -> Result<Field, E>
                    where
                        E: serde::de::Error
                    {
                        match value
                        {
                            "polygon" => Ok(Field::Polygon),
                            "id" => Ok(Field::Id),
                            "mover" => Ok(Field::Mover),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS))
                        }
                    }
                }

                deserializer.deserialize_identifier(FieldVisitor)
            }
        }

        struct BrushVisitor;

        impl<'de> Visitor<'de> for BrushVisitor
        {
            type Value = Brush;

            #[inline]
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result
            {
                formatter.write_str("struct Brush")
            }

            #[inline]
            fn visit_map<V>(self, mut map: V) -> Result<Brush, V::Error>
            where
                V: MapAccess<'de>
            {
                let mut polygon = None;
                let mut id = None;
                let mut mover = None;

                while let Some(key) = map.next_key()?
                {
                    match key
                    {
                        Field::Polygon =>
                        {
                            if polygon.is_some()
                            {
                                return Err(serde::de::Error::duplicate_field("polygon"));
                            }
                            polygon = Some(map.next_value()?);
                        },
                        Field::Id =>
                        {
                            if id.is_some()
                            {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id = Some(map.next_value()?);
                        },
                        Field::Mover =>
                        {
                            if mover.is_some()
                            {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            mover = Some(map.next_value()?);
                        }
                    }
                }

                Ok(Brush {
                    polygon:     polygon
                        .ok_or_else(|| serde::de::Error::missing_field("polygon"))?,
                    id:          id.ok_or_else(|| serde::de::Error::missing_field("id"))?,
                    mover:       mover.ok_or_else(|| serde::de::Error::missing_field("mover"))?,
                    path_edited: false
                })
            }
        }

        deserializer.deserialize_struct("Brush", FIELDS, BrushVisitor)
    }
}

impl CopyToClipboard for Brush
{
    #[inline]
    fn copy_to_clipboard(&self) -> ClipboardData
    {
        let mut poly = self.polygon.clone();
        poly.deselect_vertexes_no_indexes();
        ClipboardData::Brush(poly, self.id, self.mover.clone().into())
    }
}

impl EntityHull for Brush
{
    #[inline]
    fn hull(&self) -> Hull { self.polygon.hull() }
}

impl EntityId for Brush
{
    #[inline]
    fn id(&self) -> Id { self.id }

    #[inline]
    fn id_as_ref(&self) -> &Id { &self.id }
}

impl Brush
{
    path_nodes_value!(
        (standby_time, StandbyValueEdit),
        (max_speed, MovementValueEdit),
        (min_speed, MovementValueEdit),
        (accel_travel_percentage, MovementValueEdit),
        (decel_travel_percentage, MovementValueEdit)
    );

    //==============================================================
    // Flip

    flip_funcs!(above, below, left, right);

    //==============================================================
    // New

    #[inline]
    pub fn from_polygon<'a>(polygon: impl Into<Cow<'a, ConvexPolygon>>, identifier: Id) -> Self
    {
        match polygon.into()
        {
            Cow::Borrowed(polygon) =>
            {
                Self {
                    polygon:     polygon.clone(),
                    id:          identifier,
                    mover:       Mover::None,
                    path_edited: false
                }
            },
            Cow::Owned(polygon) =>
            {
                Self {
                    polygon,
                    id: identifier,
                    mover: Mover::None,
                    path_edited: false
                }
            },
        }
    }

    #[inline]
    pub fn from_parts<'a, 'b>(
        mut brushes: BrushesMut<'b>,
        polygon: impl Into<Cow<'a, ConvexPolygon>>,
        mover: Mover,
        identifier: Id
    ) -> Self
    {
        let mut brush = Self::from_polygon(polygon, identifier);

        match mover
        {
            Mover::None => (),
            mover @ Mover::Anchors(..) =>
            {
                brush.mover = mover;
                brush.attach_anchors(brushes);
            },
            Mover::Motor(motor) =>
            {
                brush.mover.apply_motor(motor);
                brush.attach_anchors(brushes);
            },
            Mover::Anchored(anchor_id) =>
            {
                assert!(
                    anchor_id != identifier,
                    "Anchor ID {anchor_id:?} is equal to the Brush ID"
                );
                brushes.get_mut(anchor_id).insert_anchor(&mut brush);
            }
        };

        brush
    }

    //==============================================================
    // Despawn

    #[inline]
    pub fn despawn(identifier: Id, mut brushes: BrushesMut)
    {
        let mut brush =
            unsafe { std::ptr::addr_of_mut!(brushes).as_mut().unwrap() }.get_mut(identifier);

        match &brush.mover
        {
            Mover::None => (),
            Mover::Anchors(..) | Mover::Motor(_) => brush.detach_anchors(brushes),
            Mover::Anchored(id) => brushes.get_mut(*id).mover.remove_anchor(brush.id)
        };
    }

    //==============================================================
    // Info

    #[inline]
    #[must_use]
    pub fn sides(&self) -> u8 { u8::try_from(self.polygon.sides()).unwrap() }

    /// Returns an iterator to the vertexes of the underlying `ConvexPolygon`.
    #[inline]
    pub fn vertexes(&self) -> impl ExactSizeIterator<Item = Vec2> + Clone + '_
    {
        self.polygon.vertexes()
    }

    /// Returns an iterator to the vertexes of the underlying `ConvexPolygon`.
    #[inline]
    pub fn selected_vertexes(&self) -> Option<impl Iterator<Item = Vec2>>
    {
        self.polygon.selected_vertexes()
    }

    /// Returns an iterator to the vertexes of the underlying `ConvexPolygon`.
    #[inline]
    pub fn selected_sides_vertexes(&self) -> Option<impl Iterator<Item = Vec2>>
    {
        self.polygon.selected_sides_vertexes()
    }

    /// Returns the coordinates of the mean center of the underlying
    /// `ConvexPolygon`.
    #[inline]
    #[must_use]
    pub fn center(&self) -> Vec2 { self.polygon.center() }

    /// Returns true if 'p' is in the area delimited by the underlying
    /// `ConvexPolygon`.
    #[inline]
    #[must_use]
    pub fn contains_point(&self, p: Vec2) -> bool { self.polygon.point_in_polygon(p) }

    /// Returns a copy of the underlying `ConvexPolygon`.
    #[inline]
    pub fn polygon(&self) -> ConvexPolygon { self.polygon.clone() }

    #[inline]
    #[must_use]
    pub fn path_hull(&self) -> Option<Hull>
    {
        if !self.has_motor()
        {
            return None;
        }

        calc_path_hull(self.path(), self.center()).into()
    }

    #[inline]
    #[must_use]
    fn path_hull_out_of_bounds(&self, center: Vec2) -> bool
    {
        if !self.has_motor()
        {
            return false;
        }

        calc_path_hull(self.path(), center).out_of_bounds()
    }

    #[inline]
    #[must_use]
    pub fn anchors_hull(&self, brushes: Brushes) -> Option<Hull>
    {
        if !self.mover.has_anchors()
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
    pub fn sprite_hull(&self) -> Option<Hull> { self.polygon.sprite_hull() }

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
    pub fn sprite_and_anchor_hull(&self) -> Option<(Hull, Hull)>
    {
        self.sprite_hull()
            .map(|hull| (hull, self.sprite_anchor_hull().unwrap()))
    }

    #[inline]
    #[must_use]
    pub fn global_hull(&self) -> Hull
    {
        let mut hull = self.hull();

        if let Some(sprite_hull) = self.sprite_hull()
        {
            hull = hull.merged(&sprite_hull);
        }

        hull
    }

    //==============================================================
    // General Editing

    #[inline]
    pub fn into_parts(self) -> (ConvexPolygon, Mover, Id) { (self.polygon, self.mover, self.id) }

    /// Moves the `Brush` by the amount delta.
    #[inline]
    pub fn check_move(&self, delta: Vec2, move_texture: bool) -> bool
    {
        self.polygon.check_move(delta, move_texture) &&
            !self.path_hull_out_of_bounds(self.center() + delta)
    }

    #[inline]
    pub fn check_texture_move(&self, delta: Vec2) -> bool
    {
        !self.has_texture() || self.polygon.check_texture_move(delta)
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
        self.polygon.move_by_delta(drawing_resources, delta, move_texture);
    }

    #[inline]
    pub fn move_texture(&mut self, drawing_resources: &DrawingResources, delta: Vec2)
    {
        self.polygon.move_texture(drawing_resources, delta);
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
        self.polygon.move_by_delta(drawing_resources, delta, move_texture);
    }

    /// Swaps the polygon of `self` and `other`.
    #[inline]
    pub fn swap_polygon(&mut self, polygon: &mut ConvexPolygon)
    {
        std::mem::swap(&mut self.polygon, polygon);
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
        f(&mut self.polygon, drawing_resources, grid)
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

    #[inline]
    #[must_use]
    pub fn snap_selected_path_nodes(
        &mut self,
        _: &DrawingResources,
        grid: Grid
    ) -> Option<HvVec<(HvVec<u8>, Vec2)>>
    {
        let center = self.center();
        self.path_mut().snap_selected_nodes(grid, center)
    }

    //==============================================================
    // Anchors

    #[inline]
    #[must_use]
    pub fn has_anchors(&self) -> bool { self.mover.has_anchors() }

    #[inline]
    #[must_use]
    pub fn anchorable(&self) -> bool { !(self.has_anchors() || self.has_motor()) }

    #[inline]
    pub fn anchors_iter(&self) -> Option<impl ExactSizeIterator<Item = &Id> + Clone>
    {
        self.mover.anchors_iter()
    }

    #[inline]
    #[must_use]
    pub const fn anchored(&self) -> Option<Id> { self.mover.is_anchored() }

    #[inline]
    #[must_use]
    pub fn contains_anchor(&self, identifier: Id) -> bool { self.mover.contains_anchor(identifier) }

    #[inline]
    pub fn insert_anchor(&mut self, anchor: &mut Self)
    {
        assert!(self.id != anchor.id, "Brush ID {:?} is equal to the anchor's ID", self.id);
        self.mover.insert_anchor(anchor.id);
        anchor.attach(self.id);
    }

    #[inline]
    pub fn remove_anchor(&mut self, anchor: &mut Self)
    {
        assert!(self.id != anchor.id, "Brush ID {:?} is equal to the anchor's ID", self.id);
        self.mover.remove_anchor(anchor.id);
        anchor.detach();
    }

    #[inline]
    fn attach(&mut self, identifier: Id)
    {
        assert!(matches!(self.mover, Mover::None), "Brush Mover is not None");
        self.mover = Mover::Anchored(identifier);
    }

    #[inline]
    fn detach(&mut self)
    {
        assert!(matches!(self.mover, Mover::Anchored(_)), "Brush is not anchored.");
        self.mover = Mover::None;
    }

    #[inline]
    fn attach_anchors(&mut self, mut brushes: BrushesMut)
    {
        for id in self.anchors_iter().unwrap()
        {
            brushes.get_mut(*id).attach(self.id);
        }
    }

    #[inline]
    fn detach_anchors(&mut self, mut brushes: BrushesMut)
    {
        for id in self.anchors_iter().unwrap()
        {
            brushes.get_mut(*id).detach();
        }
    }

    //==============================================================
    // Motor-Path

    #[inline]
    #[must_use]
    pub const fn has_motor(&self) -> bool { self.mover.has_motor() }

    #[inline]
    #[must_use]
    pub const fn no_motor_nor_anchored(&self) -> bool
    {
        matches!(self.mover, Mover::None | Mover::Anchors(_))
    }

    #[inline]
    #[must_use]
    pub fn was_path_edited(&mut self) -> bool { std::mem::replace(&mut self.path_edited, false) }

    #[inline]
    pub fn create_motor(&mut self, path: Path) { self.mover.create_motor(path); }

    #[inline]
    pub const fn path(&self) -> &Path { self.mover.path() }

    #[inline]
    fn path_mut(&mut self) -> &mut Path { self.mover.path_mut() }

    #[inline]
    fn path_mut_set_dirty(&mut self) -> &mut Path
    {
        self.path_edited = true;
        self.mover.path_mut()
    }

    #[inline]
    pub fn take_motor(&mut self) -> Motor
    {
        self.path_edited = true;
        self.mover.take_motor()
    }

    #[inline]
    pub fn set_motor(&mut self, motor: impl Into<Motor>)
    {
        self.path_edited = true;
        self.mover.set_motor(motor.into());
    }

    #[inline]
    pub fn try_insert_path_node_at_index(&mut self, cursor_pos: Vec2, index: usize) -> bool
    {
        let center = self.center();
        self.path_mut().try_insert_node_at_index(cursor_pos, index, center)
    }

    #[inline]
    pub fn insert_path_node_at_index(&mut self, pos: Vec2, idx: usize)
    {
        let center = self.center();
        self.path_mut().insert_node_at_index(pos, idx, center);
    }

    #[inline]
    pub fn delete_path_nodes_at_indexes(&mut self, idxs: impl Iterator<Item = usize>)
    {
        self.path_mut_set_dirty().delete_nodes_at_indexes(idxs);
    }

    #[inline]
    pub fn check_selected_nodes_deletion(&self) -> NodesDeletionResult
    {
        (self.path().check_selected_nodes_deletion(), self.id).into()
    }

    #[inline]
    pub fn delete_selected_path_nodes(&mut self, payload: NodesDeletionPayload)
        -> HvVec<(Vec2, u8)>
    {
        assert!(
            self.id == payload.id(),
            "NodesDeletionPayload ID is not equal to the Brush's ID."
        );
        self.path_mut_set_dirty()
            .delete_selected_nodes(payload.1.iter().rev().map(|(_, idx)| *idx as usize));
        payload.1
    }

    #[inline]
    pub fn remove_nodes(&mut self, to_remove: impl Iterator<Item = Vec2>)
    {
        self.path_mut_set_dirty().delete_nodes(to_remove);
    }

    #[inline]
    pub fn insert_path_nodes_at_indexes(
        &mut self,
        to_insert: impl Iterator<Item = (Vec2, usize, bool)>
    )
    {
        self.path_mut_set_dirty().insert_nodes_at_indexes(to_insert);
    }

    #[inline]
    pub fn path_nodes_nearby_cursor_pos(
        &self,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> impl Iterator<Item = (u8, bool)> + '_
    {
        self.path().nearby_nodes(cursor_pos, self.center(), camera_scale)
    }

    #[inline]
    pub fn toggle_path_node_at_index(&mut self, idx: usize) -> bool
    {
        self.path_mut_set_dirty().toggle_node_at_index(idx)
    }

    #[inline]
    pub fn exclusively_select_path_node_at_index(&mut self, index: usize) -> NodeSelectionResult
    {
        let center = self.center();
        self.path_mut_set_dirty()
            .exclusively_select_path_node_at_index(center, index)
    }

    #[inline]
    #[must_use]
    pub fn deselect_path_nodes(&mut self) -> Option<HvVec<u8>>
    {
        let center = self.center();
        self.path_mut_set_dirty().deselect_nodes(center)
    }

    #[inline]
    pub fn deselect_path_nodes_no_indexes(&mut self)
    {
        let center = self.center();
        self.path_mut_set_dirty().deselect_nodes_no_indexes(center);
    }

    #[inline]
    #[must_use]
    pub fn select_path_nodes_in_range(&mut self, range: &Hull) -> Option<HvVec<u8>>
    {
        let center = self.center();
        self.path_mut_set_dirty().select_nodes_in_range(center, range)
    }

    #[inline]
    #[must_use]
    pub fn exclusively_select_path_nodes_in_range(&mut self, range: &Hull) -> Option<HvVec<u8>>
    {
        let center = self.center();
        self.path_mut_set_dirty()
            .exclusively_select_nodes_in_range(center, range)
    }

    #[inline]
    #[must_use]
    pub fn select_all_path_nodes(&mut self) -> Option<HvVec<u8>>
    {
        self.path_mut_set_dirty().select_all_nodes()
    }

    #[inline]
    pub fn check_selected_path_nodes_move(&self, delta: Vec2) -> NodesMoveResult
    {
        (self.path().check_selected_nodes_move(delta), self.id).into()
    }

    #[inline]
    pub fn apply_selected_path_nodes_move(&mut self, payload: NodesMovePayload) -> NodesMove
    {
        assert!(payload.0 == self.id, "NodesMovePayload's ID is not equal to the Brush's ID.");
        self.redo_path_nodes_move(&payload.1);
        payload.1
    }

    #[inline]
    pub fn undo_path_nodes_move(&mut self, nodes_move: &NodesMove)
    {
        self.path_mut().undo_nodes_move(nodes_move);
    }

    #[inline]
    pub fn redo_path_nodes_move(&mut self, nodes_move: &NodesMove)
    {
        self.path_mut().apply_selected_nodes_move(nodes_move);
    }

    #[inline]
    pub fn move_path_nodes_at_indexes(&mut self, idxs: impl Iterator<Item = usize>, delta: Vec2)
    {
        self.path_mut().move_nodes_at_indexes(idxs, delta);
    }

    #[inline]
    pub fn movement_simulator(&self) -> MovementSimulator
    {
        self.path().movement_simulator(self.id)
    }

    #[inline]
    pub fn overall_selected_path_nodes_movement(&self) -> OverallMovement
    {
        self.path().overall_selected_nodes_movement()
    }

    //==============================================================
    // Texture

    #[inline]
    #[must_use]
    pub const fn has_texture(&self) -> bool { self.polygon.has_texture() }

    #[inline]
    #[must_use]
    pub fn has_sprite(&self) -> bool { self.polygon.has_sprite() }

    #[inline]
    #[must_use]
    pub fn was_texture_edited(&mut self) -> bool { self.polygon.texture_edited() }

    #[inline]
    pub const fn texture_settings(&self) -> Option<&TextureSettings>
    {
        self.polygon.texture_settings()
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
        self.polygon.check_texture_change(drawing_resources, texture)
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
    pub fn set_texture_settings(&mut self, texture: TextureSettings)
    {
        self.polygon.set_texture_settings(texture);
    }

    #[inline]
    pub fn remove_texture(&mut self) -> TextureSettings { self.polygon.remove_texture() }

    #[inline]
    #[must_use]
    pub fn check_texture_offset_x(
        &mut self,
        drawing_resources: &DrawingResources,
        value: f32
    ) -> bool
    {
        self.polygon.check_texture_offset_x(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_offset_x(
        &mut self,
        drawing_resources: &DrawingResources,
        value: f32
    ) -> Option<f32>
    {
        self.polygon.set_texture_offset_x(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn check_texture_offset_y(
        &mut self,
        drawing_resources: &DrawingResources,
        value: f32
    ) -> bool
    {
        self.polygon.check_texture_offset_y(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_offset_y(
        &mut self,
        drawing_resources: &DrawingResources,
        value: f32
    ) -> Option<f32>
    {
        self.polygon.set_texture_offset_y(drawing_resources, value)
    }

    #[inline]
    pub fn check_texture_scale_x(
        &mut self,
        drawing_resources: &DrawingResources,
        value: f32
    ) -> bool
    {
        self.polygon.check_texture_scale_x(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_scale_x(
        &mut self,
        drawing_resources: &DrawingResources,
        value: f32
    ) -> Option<f32>
    {
        self.polygon.set_texture_scale_x(drawing_resources, value)
    }

    #[inline]
    pub fn flip_texture_scale_x(&mut self, drawing_resources: &DrawingResources)
    {
        self.polygon.flip_texture_scale_x(drawing_resources);
    }

    #[inline]
    pub fn check_texture_scale_y(
        &mut self,
        drawing_resources: &DrawingResources,
        value: f32
    ) -> bool
    {
        self.polygon.check_texture_scale_y(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_scale_y(
        &mut self,
        drawing_resources: &DrawingResources,
        value: f32
    ) -> Option<f32>
    {
        self.polygon.set_texture_scale_y(drawing_resources, value)
    }

    #[inline]
    pub fn flip_scale_y(&mut self, drawing_resources: &DrawingResources)
    {
        self.polygon.flip_texture_scale_y(drawing_resources);
    }

    #[inline]
    pub fn set_texture_scroll_x(&mut self, value: f32) -> Option<f32>
    {
        self.polygon.set_texture_scroll_x(value)
    }

    #[inline]
    pub fn set_texture_scroll_y(&mut self, value: f32) -> Option<f32>
    {
        self.polygon.set_texture_scroll_y(value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_parallax_x(&mut self, value: f32) -> Option<f32>
    {
        self.polygon.set_texture_parallax_x(value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_parallax_y(&mut self, value: f32) -> Option<f32>
    {
        self.polygon.set_texture_parallax_y(value)
    }

    #[inline]
    pub fn check_texture_angle(&mut self, drawing_resources: &DrawingResources, value: f32)
        -> bool
    {
        self.polygon.check_texture_angle(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_angle(
        &mut self,
        drawing_resources: &DrawingResources,
        value: f32
    ) -> Option<f32>
    {
        self.polygon.set_texture_angle(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_height(&mut self, value: i8) -> Option<i8>
    {
        self.polygon.set_texture_height(value)
    }

    #[inline]
    pub fn check_texture_sprite(
        &mut self,
        drawing_resources: &DrawingResources,
        value: bool
    ) -> bool
    {
        self.polygon.check_texture_sprite(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_sprite(
        &mut self,
        drawing_resources: &DrawingResources,
        value: impl Into<Sprite>
    ) -> Option<(Sprite, f32, f32)>
    {
        self.polygon.set_texture_sprite(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn check_texture_within_bounds(&mut self, drawing_resources: &DrawingResources) -> bool
    {
        self.polygon.check_texture_within_bounds(drawing_resources)
    }

    #[inline]
    #[must_use]
    pub fn check_texture_animation_change(&mut self, drawing_resources: &DrawingResources) -> bool
    {
        self.polygon.check_texture_animation_change(drawing_resources)
    }

    #[inline]
    pub fn set_texture_animation(
        &mut self,
        drawing_resources: &DrawingResources,
        animation: Animation
    ) -> Animation
    {
        self.polygon.set_texture_animation(drawing_resources, animation)
    }

    #[inline]
    pub fn set_texture_list_animation(
        &mut self,
        drawing_resources: &DrawingResources,
        texture: &str
    ) -> Animation
    {
        self.polygon.set_texture_list_animation(drawing_resources, texture)
    }

    #[inline]
    pub fn generate_list_animation(&mut self, drawing_resources: &DrawingResources) -> Animation
    {
        self.polygon.generate_list_animation(drawing_resources)
    }

    #[inline]
    #[must_use]
    pub fn check_atlas_animation_x_partition(
        &mut self,
        drawing_resources: &DrawingResources,
        value: u32
    ) -> bool
    {
        self.polygon
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
        self.polygon.set_atlas_animation_x_partition(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn check_atlas_animation_y_partition(
        &mut self,
        drawing_resources: &DrawingResources,
        value: u32
    ) -> bool
    {
        self.polygon
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
        self.polygon.set_atlas_animation_y_partition(drawing_resources, value)
    }

    #[inline]
    #[must_use]
    pub fn texture_atlas_animation_max_len(&self) -> usize
    {
        self.polygon.atlas_animation_max_len()
    }

    #[inline]
    #[must_use]
    pub fn set_texture_atlas_animation_len(&mut self, value: usize) -> Option<usize>
    {
        self.polygon.set_atlas_animation_len(value)
    }

    #[inline]
    pub fn set_texture_atlas_animation_timing(&mut self, timing: Timing) -> Timing
    {
        self.polygon.set_atlas_animation_timing(timing)
    }

    #[inline]
    pub fn set_atlas_animation_uniform_timing(&mut self) -> Option<Timing>
    {
        self.polygon.set_atlas_animation_uniform_timing()
    }

    #[inline]
    #[must_use]
    pub fn set_atlas_animation_per_frame_timing(&mut self) -> Option<Timing>
    {
        self.polygon.set_atlas_animation_per_frame_timing()
    }

    #[inline]
    #[must_use]
    pub fn set_texture_atlas_animation_uniform_time(&mut self, value: f32) -> Option<f32>
    {
        self.polygon.set_atlas_animation_uniform_time(value)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_atlas_animation_frame_time(
        &mut self,
        index: usize,
        value: f32
    ) -> Option<f32>
    {
        self.polygon.set_atlas_animation_frame_time(index, value)
    }

    #[inline]
    pub fn move_down_atlas_animation_frame_time(&mut self, index: usize)
    {
        self.polygon.move_down_atlas_animation_frame_time(index);
    }

    #[inline]
    pub fn move_up_atlas_animation_frame_time(&mut self, index: usize)
    {
        self.polygon.move_up_atlas_animation_frame_time(index);
    }

    #[inline]
    #[must_use]
    pub fn set_list_animation_texture(&mut self, index: usize, texture: &str) -> Option<String>
    {
        self.polygon.set_list_animation_texture(index, texture)
    }

    #[inline]
    #[must_use]
    pub fn texture_list_animation_frame(&self, index: usize) -> &(String, f32)
    {
        self.polygon.texture_list_animation_frame(index)
    }

    #[inline]
    #[must_use]
    pub fn set_texture_list_animation_time(&mut self, index: usize, time: f32) -> Option<f32>
    {
        self.polygon.set_list_animation_time(index, time)
    }

    #[inline]
    pub fn move_up_list_animation_frame(&mut self, index: usize)
    {
        self.polygon.move_up_list_animation_frame(index);
    }

    #[inline]
    pub fn move_down_list_animation_frame(&mut self, index: usize)
    {
        self.polygon.move_down_list_animation_frame(index);
    }

    #[inline]
    pub fn insert_list_animation_frame(&mut self, index: usize, texture: &str, time: f32)
    {
        self.polygon.insert_list_animation_frame(index, texture, time);
    }

    #[inline]
    pub fn pop_list_animation_frame(&mut self) { self.polygon.pop_list_animation_frame(); }

    #[inline]
    pub fn remove_list_animation_frame(&mut self, index: usize)
    {
        self.polygon.remove_list_animation_frame(index);
    }

    #[inline]
    pub fn push_list_animation_frame(&mut self, texture: &str)
    {
        self.polygon.push_list_animation_frame(texture);
    }

    //==============================================================
    // Collision

    #[inline]
    #[must_use]
    pub fn set_collision(&mut self, value: bool) -> Option<bool>
    {
        self.polygon.set_collision(value)
    }

    #[inline]
    #[must_use]
    pub fn collision(&self) -> bool { self.polygon.collision() }

    //==============================================================
    // Vertex Editing

    #[inline]
    #[must_use]
    pub const fn has_selected_vertexes(&self) -> bool { self.polygon.has_selected_vertexes() }

    #[inline]
    #[must_use]
    pub const fn selected_vertexes_amount(&self) -> u8 { self.polygon.selected_vertexes_amount() }

    #[inline]
    #[must_use]
    pub fn nearby_vertex(&self, cursor_pos: Vec2, camera_scale: f32) -> Option<Vec2>
    {
        self.polygon
            .nearby_vertex(cursor_pos, camera_scale)
            .map(|idx| self.polygon.vertex_at_index(idx))
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
        self.polygon
            .check_vertex_proximity_and_exclusively_select(cursor_pos, camera_scale)
    }

    #[inline]
    #[must_use]
    pub fn try_select_vertex(&mut self, vx: Vec2) -> Option<u8>
    {
        self.polygon.try_select_vertex(vx)
    }

    #[inline]
    #[must_use]
    pub fn vertex_at_index(&self, idx: usize) -> Vec2 { self.polygon.vertex_at_index(idx) }

    #[inline]
    pub fn toggle_vertex_at_index(&mut self, idx: usize)
    {
        self.polygon.toggle_vertex_at_index(idx);
    }

    #[inline]
    #[must_use]
    pub fn try_exclusively_select_vertex(&mut self, vx: Vec2) -> Option<HvVec<u8>>
    {
        self.polygon.try_exclusively_select_vertex(vx)
    }

    #[inline]
    #[must_use]
    pub fn select_vertexes_in_range(&mut self, range: &Hull) -> Option<HvVec<u8>>
    {
        self.polygon.select_vertexes_in_range(range)
    }

    #[inline]
    #[must_use]
    pub fn exclusively_select_vertexes_in_range(&mut self, range: &Hull) -> Option<HvVec<u8>>
    {
        self.polygon.exclusively_select_vertexes_in_range(range)
    }

    #[inline]
    #[must_use]
    pub fn select_all_vertexes(&mut self) -> Option<HvVec<u8>>
    {
        self.polygon.select_all_vertexes()
    }

    /// Toggles the selection of the `SelectableVertex` with coordinates `vx`,
    /// if any.
    #[inline]
    #[must_use]
    pub fn toggle_vertex_at_pos(&mut self, vx: Vec2) -> Option<u8>
    {
        self.polygon.toggle_vertex_at_pos(vx)
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
        self.polygon.toggle_vertex_nearby_cursor_pos(cursor_pos, camera_scale)
    }

    /// Deselects all `SelectableVertex` of the underlying `ConvexPolygon`.
    #[inline]
    #[must_use]
    pub fn deselect_vertexes(&mut self) -> Option<HvVec<u8>> { self.polygon.deselect_vertexes() }

    #[inline]
    pub fn deselect_vertexes_no_indexes(&mut self) { self.polygon.deselect_vertexes_no_indexes(); }

    #[inline]
    #[must_use]
    pub fn try_vertex_insertion_at_index(
        &mut self,
        drawing_resources: &DrawingResources,
        vx: Vec2,
        idx: usize,
        selected: bool
    ) -> bool
    {
        self.polygon
            .try_vertex_insertion_at_index(drawing_resources, vx, idx, selected)
    }

    #[inline]
    pub fn insert_vertex_at_index(
        &mut self,
        drawing_resources: &DrawingResources,
        vx: Vec2,
        idx: usize,
        selected: bool
    )
    {
        self.polygon
            .insert_vertex_at_index(drawing_resources, vx, idx, selected);
    }

    /// Returns the index the closest projection of `cursor_pos` on the shape of
    /// the underlying `ConvexPolygon` would have if it were added to it.
    #[inline]
    #[must_use]
    pub fn vx_projection_insertion_index(&self, cursor_pos: Vec2) -> Option<usize>
    {
        self.polygon.vertex_insertion_index(cursor_pos)
    }

    /// Returns true if inserting `vx` in the underlying `ConvexPolygon` at
    /// index `vx_idx` generates a valid polygon.
    #[inline]
    #[must_use]
    pub fn is_new_vertex_at_index_valid(&mut self, vx: Vec2, idx: usize) -> bool
    {
        self.polygon.is_new_vertex_at_index_valid(vx, idx)
    }

    #[inline]
    pub fn delete_vertex_at_index(&mut self, drawing_resources: &DrawingResources, idx: usize)
    {
        self.polygon.delete_vertex_at_index(drawing_resources, idx);
    }

    #[inline]
    pub fn check_selected_vertexes_deletion(&self) -> VertexesDeletionResult
    {
        self.polygon.check_selected_vertexes_deletion()
    }

    /// Tries to remove the selected `SelectableVertexes`, does nothing if the
    /// result `ConvexPolygon` would have less than 3 sides.
    #[inline]
    pub fn delete_selected_vertexes(
        &mut self,
        drawing_resources: &DrawingResources
    ) -> Option<HvVec<(Vec2, u8)>>
    {
        self.polygon.delete_selected_vertexes(drawing_resources)
    }

    /// Moves the selected `SelectableVertexes` by the amount `delta`.
    #[inline]
    pub fn check_selected_vertexes_move(&mut self, delta: Vec2) -> VertexesMoveResult
    {
        VertexesMoveResult::from_result(self.polygon.check_selected_vertexes_move(delta), self)
    }

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

    #[inline]
    pub fn undo_vertexes_move(
        &mut self,
        drawing_resources: &DrawingResources,
        vxs_move: &VertexesMove
    )
    {
        let old_center = self.center();
        self.polygon.undo_vertexes_move(drawing_resources, vxs_move);

        if !self.has_motor()
        {
            return;
        }

        let center = self.center();
        self.path_mut().translate(old_center - center);
    }

    #[inline]
    pub fn redo_vertexes_move(
        &mut self,
        drawing_resources: &DrawingResources,
        vxs_move: &VertexesMove
    )
    {
        let old_center = self.center();
        self.polygon.apply_vertexes_move_result(drawing_resources, vxs_move);

        if !self.has_motor()
        {
            return;
        }

        let center = self.center();
        self.path_mut().translate(old_center - center);
    }

    #[inline]
    pub fn check_split(&self) -> SplitResult { (self.polygon.check_split(), self.id).into() }

    #[inline]
    pub fn split(
        &mut self,
        drawing_resources: &DrawingResources,
        payload: &SplitPayload
    ) -> ConvexPolygon
    {
        assert!(payload.0 == self.id, "SplitPayload's ID is not equal to the Brush's ID.");
        self.polygon.split(drawing_resources, &payload.1)
    }

    #[inline]
    pub fn move_vertexes_at_indexes(&mut self, idxs: impl Iterator<Item = usize>, delta: Vec2)
    {
        self.polygon.move_vertexes_at_indexes(idxs, delta);
    }

    //==============================================================
    // Side editing

    #[inline]
    #[must_use]
    pub fn nearby_side(&self, cursor_pos: Vec2, camera_scale: f32) -> Option<([Vec2; 2], usize)>
    {
        self.polygon.nearby_side(cursor_pos, camera_scale)
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
        self.polygon.check_side_proximity_and_select(cursor_pos, camera_scale)
    }

    #[inline]
    pub fn xtrusion_info(
        &self,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> Option<([Vec2; 2], Vec2, XtrusionPayload)>
    {
        self.polygon
            .xtrusion_info(cursor_pos, camera_scale)
            .map(|(side, normal, info)| (side, normal, XtrusionPayload(self.id, info)))
    }

    #[inline]
    pub fn matching_xtrusion_info(&self, normal: Vec2) -> XtrusionResult
    {
        (self.polygon.matching_xtrusion_info(normal), self.id).into()
    }

    #[inline]
    #[must_use]
    pub fn try_select_side(&mut self, side: &[Vec2; 2]) -> Option<u8>
    {
        self.polygon.try_select_side(side)
    }

    #[inline]
    #[must_use]
    pub fn try_exclusively_select_side(&mut self, side: &[Vec2; 2]) -> Option<HvVec<u8>>
    {
        self.polygon.try_exclusively_select_side(side)
    }

    #[inline]
    #[must_use]
    pub fn select_sides_in_range(&mut self, range: &Hull) -> Option<HvVec<u8>>
    {
        self.polygon.select_sides_in_range(range)
    }

    #[inline]
    #[must_use]
    pub fn exclusively_select_sides_in_range(&mut self, range: &Hull) -> Option<HvVec<u8>>
    {
        self.polygon.exclusively_select_sides_in_range(range)
    }

    /// Selects the side with coordinates `l`, if any.
    #[inline]
    #[must_use]
    pub fn toggle_side(&mut self, l: &[Vec2; 2]) -> Option<u8>
    {
        self.polygon.toggle_side_at_pos(l)
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
        self.polygon.toggle_side_nearby_cursor_pos(cursor_pos, camera_scale)
    }

    #[inline]
    pub fn check_selected_sides_deletion(&self) -> SidesDeletionResult
    {
        SidesDeletionResult::from_result(self.polygon.check_selected_sides_deletion(), self.id)
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
        self.polygon.delete_selected_sides(
            drawing_resources,
            payload.1.iter().rev().map(|(_, idx, _)| *idx as usize)
        );
        payload.1
    }

    /// Moves the selected lines by the amount `delta`.
    #[inline]
    pub fn check_selected_sides_move(&mut self, delta: Vec2) -> VertexesMoveResult
    {
        VertexesMoveResult::from_result(self.polygon.check_selected_sides_move(delta), self)
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

        self.polygon.clip(drawing_resources, clip_line)
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
    ) -> Option<impl Iterator<Item = ConvexPolygon>>
    {
        self.polygon.shatter(drawing_resources, cursor_pos, camera_scale)
    }

    //==============================================================
    // Hollow

    #[inline]
    pub fn hollow(
        &self,
        drawing_resources: &DrawingResources,
        grid_size: f32
    ) -> Option<impl Iterator<Item = ConvexPolygon>>
    {
        self.polygon.hollow(drawing_resources, grid_size)
    }

    //==============================================================
    // Intersection

    #[inline]
    #[must_use]
    pub fn intersection(&self, other: &Self) -> Option<ConvexPolygon>
    {
        self.polygon.intersection(&other.polygon)
    }

    #[inline]
    #[must_use]
    pub fn intersect(&self, other: &mut ConvexPolygon) -> bool
    {
        if let Some(cp) = self.polygon.intersection(other)
        {
            *other = cp;
            return true;
        }

        false
    }

    //==============================================================
    // Subtract

    #[inline]
    pub fn subtract(&self, drawing_resources: &DrawingResources, other: &Self) -> SubtractResult
    {
        self.polygon.subtract(drawing_resources, &other.polygon)
    }

    //==============================================================
    // Scale

    #[inline]
    pub fn check_scale(
        &mut self,
        drawing_resources: &DrawingResources,
        info: &ScaleInfo,
        scale_texture: bool
    ) -> ScaleResult
    {
        ScaleResult::from_result(
            self.polygon.check_scale(drawing_resources, info, scale_texture),
            self
        )
    }

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
            self.polygon
                .check_flip_scale(drawing_resources, info, flip_queue, scale_texture),
            self
        )
    }

    #[inline]
    pub fn set_scale_coordinates(
        &mut self,
        drawing_resources: &DrawingResources,
        payload: ScalePayload
    )
    {
        assert!(payload.id() == self.id, "ScalePayload's ID is not equal to the Brush's ID.");
        self.polygon.set_coordinates(payload.1);

        let tex_scale = return_if_none!(payload.2);
        _ = self.polygon.set_texture_scale_x(drawing_resources, tex_scale.scale_x);
        _ = self.polygon.set_texture_scale_y(drawing_resources, tex_scale.scale_y);
        _ = self
            .polygon
            .set_texture_offset_x(drawing_resources, tex_scale.offset.x);
        _ = self
            .polygon
            .set_texture_offset_y(drawing_resources, tex_scale.offset.y);
    }

    //==============================================================
    // Shear

    #[inline]
    pub fn check_horizontal_shear(&self, info: &ShearInfo) -> ShearResult
    {
        ShearResult::from_result(self.polygon.check_horizontal_shear(info), self)
    }

    #[inline]
    pub fn set_x_coordinates(&mut self, payload: ShearPayload)
    {
        assert!(payload.id() == self.id, "ShearPayload's ID is not equal to the Brush's ID.");
        self.polygon.set_x_coordinates(payload.1);
    }

    #[inline]
    pub fn shear_horizontally(&mut self, info: &ShearInfo)
    {
        self.polygon.shear_horizontally(info);
    }

    #[inline]
    pub fn check_vertical_shear(&self, info: &ShearInfo) -> ShearResult
    {
        ShearResult::from_result(self.polygon.check_vertical_shear(info), self)
    }

    #[inline]
    pub fn set_y_coordinates(&mut self, payload: ShearPayload)
    {
        assert!(payload.id() == self.id, "ShearPayload's ID is not equal to the Brush's ID.");
        self.polygon.set_y_coordinates(payload.1);
    }

    #[inline]
    pub fn shear_vertically(&mut self, info: &ShearInfo) { self.polygon.shear_vertically(info); }

    //==============================================================
    // Rotate

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
            self.polygon
                .check_rotation(drawing_resources, pivot, angle, rotate_texture),
            self
        )
    }

    #[inline]
    pub fn set_rotation_coordinates(
        &mut self,
        drawing_resources: &DrawingResources,
        payload: RotatePayload
    )
    {
        assert!(payload.id() == self.id, "RotatePayload's ID is not equal to the Brush's ID.");
        self.polygon.set_coordinates(payload.1);

        let tex_rotate = return_if_none!(payload.2);
        _ = self.polygon.set_texture_angle(drawing_resources, tex_rotate.angle);
        _ = self
            .polygon
            .set_texture_offset_x(drawing_resources, tex_rotate.offset.x);
        _ = self
            .polygon
            .set_texture_offset_y(drawing_resources, tex_rotate.offset.y);
    }

    //==============================================================
    // Draw

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

        self.polygon.draw_map_preview(camera, drawer, animator);
    }

    /// Draws the `Brush` with the desired `Color`.
    #[inline]
    pub fn draw_with_color(&self, camera: &Transform, drawer: &mut EditDrawer, color: Color)
    {
        self.polygon.draw(camera, drawer, color);
    }

    /// Draws the `Brush` with the not-selected `Color`.
    #[inline]
    pub fn draw_non_selected(&self, camera: &Transform, drawer: &mut EditDrawer)
    {
        self.draw_with_color(camera, drawer, Color::NonSelectedBrush);
    }

    /// Draws the `Brush` with the selected `Color`.
    #[inline]
    pub fn draw_selected(&self, camera: &Transform, drawer: &mut EditDrawer)
    {
        self.draw_with_color(camera, drawer, Color::SelectedBrush);
    }

    /// Draws the `Brush` with the highlight `Color`.
    #[inline]
    pub fn draw_highlighted_selected(&self, camera: &Transform, drawer: &mut EditDrawer)
    {
        self.draw_with_color(camera, drawer, Color::HighlightedSelectedBrush);
    }

    /// Draws the `Brush` with the highlight `Color`.
    #[inline]
    pub fn draw_highlighted_non_selected(&self, camera: &Transform, drawer: &mut EditDrawer)
    {
        self.draw_with_color(camera, drawer, Color::HighlightedNonSelectedBrush);
    }

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
        self.polygon.draw_extended_side(window, camera, drawer, index, color);
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
        self.polygon.draw_with_vertex_highlight(
            window,
            camera,
            drawer,
            egui_context,
            hgl_mode,
            show_tooltips
        );
    }

    #[inline]
    pub fn draw_wih_solid_color(&self, drawer: &mut EditDrawer, color: Color)
    {
        drawer.polygon_with_solid_color(self.vertexes(), color);
    }

    #[inline]
    pub fn draw_path(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        drawer: &mut EditDrawer,
        show_tooltips: bool
    )
    {
        self.path()
            .draw(window, camera, egui_context, drawer, self.center(), show_tooltips);
    }

    #[inline]
    pub fn draw_semitransparent_path(&self, drawer: &mut EditDrawer)
    {
        self.path().draw_semitransparent(drawer, self.center());
    }

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
        for brush in self.mover.anchors_iter().unwrap().map(|id| brushes.get(*id))
        {
            f(brush, camera, drawer);
        }
    }

    #[inline]
    pub fn draw_highlighted_with_path_nodes(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        brushes: Brushes,
        drawer: &mut EditDrawer,
        show_tooltips: bool
    )
    {
        self.draw_with_color(camera, drawer, Color::HighlightedSelectedBrush);
        self.path()
            .draw(window, camera, egui_context, drawer, self.center(), show_tooltips);
        self.draw_anchored_brushes(camera, brushes, drawer, Self::draw_highlighted_selected);
    }

    #[inline]
    pub fn draw_with_highlighted_path_node(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        brushes: Brushes,
        drawer: &mut EditDrawer,
        highlighted_node: usize,
        show_tooltips: bool
    )
    {
        self.draw_with_color(camera, drawer, Color::HighlightedSelectedBrush);
        self.path().draw_with_highlighted_path_node(
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
    pub fn draw_with_path_node_addition(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        brushes: Brushes,
        drawer: &mut EditDrawer,
        pos: Vec2,
        idx: usize,
        show_tooltips: bool
    )
    {
        self.draw_with_color(camera, drawer, Color::HighlightedSelectedBrush);
        self.path().draw_with_node_insertion(
            window,
            camera,
            egui_context,
            drawer,
            pos,
            idx,
            self.center(),
            show_tooltips
        );
        self.draw_anchored_brushes(camera, brushes, drawer, Self::draw_selected);
    }

    #[inline]
    pub fn draw_movement_simulation(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        brushes: Brushes,
        drawer: &mut EditDrawer,
        show_tooltips: bool,
        simulator: &MovementSimulator
    )
    {
        assert!(self.id == simulator.id(), "Simulator's ID is not equal to the Brush's ID.");

        let movement_vec = simulator.movement_vec();
        let center = self.center();

        self.polygon.draw_movement_simulation(camera, drawer, movement_vec);
        self.path().draw_movement_simulation(
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
                .polygon
                .draw_movement_simulation(camera, drawer, movement_vec);
        }
    }

    #[inline]
    pub fn draw_map_preview_movement_simulation(
        &self,
        camera: &Transform,
        brushes: Brushes,
        drawer: &mut MapPreviewDrawer,
        animators: &Animators,
        simulator: &MovementSimulator
    )
    {
        assert!(self.id == simulator.id(), "Simulator's ID is not equal to the Brush's ID.");

        let movement_vec = simulator.movement_vec();
        self.polygon.draw_map_preview_movement_simulation(
            camera,
            drawer,
            animators.get(self.id),
            movement_vec
        );
        let anchors = return_if_none!(self.anchors_iter());

        for id in anchors
        {
            brushes.get(*id).polygon.draw_map_preview_movement_simulation(
                camera,
                drawer,
                animators.get(*id),
                movement_vec
            );
        }
    }

    #[inline]
    pub fn draw_sprite(&self, drawer: &mut EditDrawer, color: Color)
    {
        self.polygon.draw_sprite(drawer, color);
    }

    #[inline]
    pub fn draw_sprite_highlight(&self, drawer: &mut EditDrawer)
    {
        self.polygon.draw_sprite_highlight(drawer);
    }

    #[inline]
    pub fn draw_map_preview_sprite(
        &self,
        drawer: &mut MapPreviewDrawer,
        animator: Option<&Animator>
    )
    {
        self.polygon.draw_map_preview_sprite(drawer, animator);
    }
}

//=======================================================================//

#[must_use]
pub struct BrushViewer
{
    pub id:        Id,
    pub vertexes:  HvVec<Vec2>,
    pub texture:   Option<TextureSettings>,
    pub mover:     Mover,
    pub collision: bool
}

impl BrushViewer
{
    #[inline]
    pub(in crate::map) fn new(brush: Brush) -> Self
    {
        let (poly, mover, id) = brush.into_parts();
        let collision = poly.collision();

        Self {
            id,
            vertexes: hv_vec![collect; poly.vertexes()],
            texture: poly.take_texture_settings(),
            mover,
            collision
        }
    }

    #[inline]
    pub(in crate::map) fn set_texture_animation(&mut self, animation: Animation)
    {
        unsafe {
            self.texture.as_mut().unwrap().unsafe_set_animation(animation);
        }
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

#[inline]
#[must_use]
fn calc_path_hull(path: &Path, center: Vec2) -> Hull
{
    (path.hull() + center)
        .merged(&Some(center).into_iter().into())
        .bumped(2f32)
}
