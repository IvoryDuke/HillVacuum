//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{borrow::Cow, fmt::Write};

use arrayvec::ArrayVec;
use bevy::{transform::components::Transform, window::Window};
use bevy_egui::egui;
use glam::Vec2;
use hill_vacuum_shared::{continue_if_none, iterate_slice_in_triplets, return_if_none};

use crate::{
    map::{
        brush::{Brush, ClipResult, ShatterResult},
        drawer::{
            animation::{Animator, Timing},
            color::Color,
            drawers::{EditDrawer, MapPreviewDrawer},
            drawing_resources::DrawingResources,
            texture::{
                TextureInterfaceExtra,
                TextureReset,
                TextureRotation,
                TextureScale,
                TextureSettings,
                TextureSpriteSet
            },
            TextureSize
        },
        editor::state::grid::Grid,
        selectable_vector::{
            deselect_vectors,
            select_vectors_in_range,
            SelectableVector,
            VectorSelectionResult
        },
        OutOfBounds,
        MAP_RANGE,
        TOOLTIP_OFFSET
    },
    utils::{
        hull::{Flip, Hull},
        identifiers::EntityCenter,
        iterators::{
            PairIterator,
            PairIteratorMut,
            SkipIndexIterator,
            SlicePairIter,
            TripletIterator
        },
        math::{
            lines_and_segments::{
                closest_point_on_segment,
                is_point_inside_clip_edge,
                is_point_on_segment,
                lines_intersection,
                point_to_segment_distance_squared
            },
            points::{
                are_vxs_ccw,
                is_polygon_convex,
                rotate_point,
                sort_vxs_ccw,
                vertexes_orientation,
                vxs_center,
                VertexesOrientation
            },
            polygons::clip_polygon,
            AroundEqual,
            NecessaryPrecisionValue
        },
        misc::{
            next,
            next_element,
            next_n_steps,
            prev,
            prev_element,
            prev_element_n_steps,
            AssertNormalizedDegreesAngle,
            NoneIfEmpty,
            PointInsideUiHighlight,
            ReplaceValue,
            SwapValue,
            TakeValue,
            Toggle,
            VX_HGL_SIDE,
            VX_HGL_SIDE_SQUARED
        }
    },
    Animation,
    TextureInterface
};

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

pub(in crate::map) const NEW_VX: &str = "new_vx";

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[must_use]
pub(in crate::map) enum FreeDrawVertexDeletionResult
{
    None,
    Polygon(Vec2),
    Line([Vec2; 2], Vec2)
}

//=======================================================================//

#[must_use]
pub(in crate::map::brush) enum VertexesMoveResult
{
    None,
    Invalid,
    Valid(VertexesMove)
}

//=======================================================================//

#[must_use]
pub(in crate::map) struct VertexesMove
{
    merged: MergedVertexes,
    moved:  Vec<u8>,
    delta:  Vec2
}

impl VertexesMove
{
    #[inline]
    pub const fn has_merged_vertexes(&self) -> bool { self.merged.len() != 0 }

    #[inline]
    pub fn moved_indexes(&self) -> impl Iterator<Item = u8> + '_ { self.moved.iter().copied() }

    #[inline]
    pub fn paired_moved_indexes(&self) -> Option<SlicePairIter<u8>> { self.moved.pair_iter() }

    #[inline]
    #[must_use]
    pub fn merge(&mut self, other: &Self) -> bool
    {
        if other.has_merged_vertexes() || self.has_merged_vertexes()
        {
            return false;
        }

        self.delta += other.delta;
        true
    }
}

//=======================================================================//

#[must_use]
pub(in crate::map) enum MergedVertexes
{
    None,
    One((Vec2, u8)),
    Two([(Vec2, u8); 2])
}

impl MergedVertexes
{
    #[inline]
    const fn new() -> Self { Self::None }

    #[inline]
    fn push(&mut self, value: (Vec2, u8))
    {
        match self
        {
            MergedVertexes::None => *self = Self::One(value),
            MergedVertexes::One(v) => *self = Self::Two([*v, value]),
            MergedVertexes::Two(_) => panic!("There cannot be more than 2 merged vertexes.")
        }
    }

    #[inline]
    fn sort(&mut self)
    {
        match self
        {
            MergedVertexes::None | MergedVertexes::One(_) => (),
            MergedVertexes::Two([a, b]) =>
            {
                if a.1 > b.1
                {
                    a.swap_value(b);
                }
            }
        };
    }

    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize
    {
        match self
        {
            MergedVertexes::None => 0,
            MergedVertexes::One(_) => 1,
            MergedVertexes::Two(_) => 2
        }
    }

    #[inline]
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &(Vec2, u8)>
    {
        MergedVertexesIter::new(self)
    }
}

struct MergedVertexesIter<'a>
{
    values: &'a MergedVertexes,
    left:   usize,
    right:  usize
}

impl<'a> Iterator for MergedVertexesIter<'a>
{
    type Item = &'a (Vec2, u8);

    #[inline]
    fn next(&mut self) -> Option<Self::Item>
    {
        if self.left == self.right
        {
            return None;
        }

        let value = match self.values
        {
            MergedVertexes::None => unreachable!(),
            MergedVertexes::One(v) => v,
            MergedVertexes::Two(vs) => &vs[self.left]
        };

        self.left += 1;
        Some(value)
    }
}

impl DoubleEndedIterator for MergedVertexesIter<'_>
{
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item>
    {
        if self.left == self.right
        {
            return None;
        }

        let value = match self.values
        {
            MergedVertexes::None => unreachable!(),
            MergedVertexes::One(v) => v,
            MergedVertexes::Two(vs) => &vs[self.right - 1 - self.left]
        };

        self.left += 1;
        Some(value)
    }
}

impl<'a> MergedVertexesIter<'a>
{
    #[inline]
    #[must_use]
    const fn new(merged: &'a MergedVertexes) -> Self
    {
        Self {
            values: merged,
            left:   0,
            right:  merged.len()
        }
    }
}

//=======================================================================//

#[must_use]
pub(in crate::map) enum VertexesDeletionResult
{
    None,
    Invalid,
    Valid
}

//=======================================================================//

#[must_use]
pub(in crate::map::brush) enum SidesDeletionResult
{
    None,
    Invalid,
    Valid(Vec<(Vec2, u8, bool)>)
}

//=======================================================================//

#[must_use]
pub(in crate::map) enum SplitResult
{
    None,
    Invalid,
    Valid(ArrayVec<u8, 2>)
}

//=======================================================================//

#[must_use]
pub(in crate::map) enum SideSelectionResult
{
    Selected,
    NotSelected([Vec2; 2], Vec<u8>),
    None
}

//=======================================================================//

#[must_use]
pub(in crate::map::brush) enum XtrusionResult
{
    None,
    Invalid,
    Valid(XtrusionInfo)
}

//=======================================================================//

#[must_use]
pub(in crate::map) enum ExtrusionResult
{
    Invalid,
    Valid(ExtrusionResultPayload)
}

//=======================================================================//

#[must_use]
pub(in crate::map) enum SubtractResult
{
    None,
    Despawn,
    Some
    {
        main:   ConvexPolygon,
        others: Vec<ConvexPolygon>
    }
}

//=======================================================================//

#[must_use]
pub(in crate::map::brush) enum ScaleResult
{
    Invalid,
    Valid
    {
        new_center:    Vec2,
        vxs:           Vec<Vec2>,
        texture_scale: Option<TextureScale>
    }
}

//=======================================================================//

#[must_use]
pub(in crate::map::brush) enum RotateResult
{
    Invalid,
    Valid
    {
        new_center:       Vec2,
        vxs:              Vec<Vec2>,
        texture_rotation: Option<TextureRotation>
    }
}

//=======================================================================//

/// The way the brush should be drawn when the vertexes need to be highlighted.
pub(in crate::map) enum VertexHighlightMode
{
    // When the side tool is active.
    Side,
    // When the vertex tool is active.
    Vertex,
    // When a vertex is being added to the shape through the vertex tool.
    NewVertex(Vec2, usize)
}

//=======================================================================//

pub(in crate::map) enum TextureSetResult
{
    Unchanged,
    Changed(String),
    Set
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

#[derive(Copy, Clone)]
pub(in crate::map) struct XtrusionInfo
{
    prev_side: [Vec2; 2],
    next_side: [Vec2; 2]
}

impl XtrusionInfo
{
    const EXTRUSION_SIDE_INDICES: [usize; 2] = [2, 3];
    const NON_EXTRUSION_SIDE_INDICES: [usize; 2] = [0, 1];

    #[inline]
    #[must_use]
    fn xtrusion_side(&self, distance: Vec2) -> [Vec2; 2]
    {
        [self.prev_side[0] + distance, self.next_side[0] + distance]
    }

    #[inline]
    #[must_use]
    const fn original_xtrusion_side(&self) -> [Vec2; 2] { [self.next_side[0], self.prev_side[0]] }

    //==============================================================
    // Extrusion

    #[inline]
    #[must_use]
    fn extruded_side(&self, distance: Vec2) -> Option<[Vec2; 2]>
    {
        let line = self.xtrusion_side(distance);
        let side = [
            lines_intersection(&self.prev_side, &line).unwrap().0,
            lines_intersection(&self.next_side, &line).unwrap().0
        ];

        (!side.iter().any(OutOfBounds::out_of_bounds)).then_some(side)
    }

    #[inline]
    #[must_use]
    fn is_test_polygon_valid(polygon: &[Option<Vec2>]) -> bool
    {
        for vx in polygon.iter().take(3)
        {
            assert!(
                vx.is_some(),
                "Extrusion test polygon has a None value in the first three vertexes."
            );
        }

        let len = if polygon[3].is_none() { 3usize } else { 4 };

        !polygon
            .triplet_iter()
            .unwrap()
            .take(len)
            .any(|[vx_i, vx_j, vx_k]| !are_vxs_ccw(&[vx_i.unwrap(), vx_j.unwrap(), vx_k.unwrap()]))
    }

    #[inline]
    pub fn extrude_side(payload: &ExtrusionResultPayload, polygon: &mut ConvexPolygon)
    {
        polygon.vertexes[Self::EXTRUSION_SIDE_INDICES[0]].vec = payload.0;

        match payload.1
        {
            Some(vx) =>
            {
                if polygon.sides() == 3
                {
                    polygon.vertexes.push(SelectableVector::new(vx));
                }
                else
                {
                    polygon.vertexes[Self::EXTRUSION_SIDE_INDICES[1]].vec = vx;
                }
            },
            None => polygon.vertexes.truncate(3)
        };

        polygon.update_center_hull();
    }

    #[inline]
    #[must_use]
    pub fn create_extrusion_polygon(
        &self,
        distance: Vec2,
        texture: Option<&TextureSettings>
    ) -> Option<ConvexPolygon>
    {
        // Create the test polygon.
        let mut test_polygon = [None; 4];
        let side = self.original_xtrusion_side();

        for (i, j) in Self::NON_EXTRUSION_SIDE_INDICES.iter().enumerate()
        {
            test_polygon[*j] = side[i].into();
        }

        // Generate the extruded side.
        let extruded_side = self.extruded_side(distance)?;

        if extruded_side[0].around_equal_narrow(&extruded_side[1])
        {
            test_polygon[Self::EXTRUSION_SIDE_INDICES[0]] = extruded_side[0].into();
        }
        else
        {
            for (i, j) in Self::EXTRUSION_SIDE_INDICES.iter().enumerate()
            {
                test_polygon[*j] = extruded_side[i].into();
            }
        }

        // Generate the extruded polygon based on how many vertexes the test polygon
        // contains.
        if Self::is_test_polygon_valid(&test_polygon)
        {
            return Some(
                (
                    test_polygon
                        .into_iter()
                        .filter_map(|vx| vx.map(SelectableVector::new))
                        .collect::<Vec<_>>(),
                    texture
                )
                    .into()
            );
        }

        None
    }

    #[inline]
    pub fn check_side_extrusion(&self, polygon: &ConvexPolygon, distance: Vec2) -> ExtrusionResult
    {
        // Create a test polygon.
        let mut test_polygon = [None; 4];
        let iter = polygon.vertexes();

        for (i, vx) in iter.enumerate()
        {
            test_polygon[i] = vx.into();
        }

        // Extrude the side and check validity.
        let extruded_side = self.extruded_side(distance);

        if extruded_side.is_none()
        {
            return ExtrusionResult::Invalid;
        }

        let extruded_side = extruded_side.unwrap();

        if polygon.sides() == 3
        {
            assert!(
                extruded_side[0] != extruded_side[1],
                "Same extrusion vertexes when polygon is already a triangle."
            );

            for i in 0..2
            {
                test_polygon[Self::EXTRUSION_SIDE_INDICES[i]] = extruded_side[i].into();
            }
        }
        else if extruded_side[0].around_equal_narrow(&extruded_side[1])
        {
            test_polygon[Self::EXTRUSION_SIDE_INDICES[0]] = extruded_side[0].into();
            test_polygon[Self::EXTRUSION_SIDE_INDICES[1]] = None;
        }
        else
        {
            for i in 0..2
            {
                test_polygon[Self::EXTRUSION_SIDE_INDICES[i]] = extruded_side[i].into();
            }
        }

        if !Self::is_test_polygon_valid(&test_polygon)
        {
            return ExtrusionResult::Invalid;
        }

        // Return the appropriate result.
        ExtrusionResult::Valid(ExtrusionResultPayload(
            test_polygon[Self::EXTRUSION_SIDE_INDICES[0]].unwrap(),
            test_polygon[Self::EXTRUSION_SIDE_INDICES[1]]
        ))
    }

    //==============================================================
    // Intrusion

    #[inline]
    pub fn clip_polygon_at_intrusion_side(
        &self,
        brush: &Brush,
        distance: Vec2
    ) -> Option<ClipResult>
    {
        brush.clip(&self.xtrusion_side(distance))
    }
}

//=======================================================================//

pub(in crate::map) struct ExtrusionResultPayload(Vec2, Option<Vec2>);

//=======================================================================//

#[must_use]
pub(in crate::map) struct ScaleInfo
{
    pivot:        Vec2,
    new_pivot:    Vec2,
    width:        f32,
    new_width:    f32,
    width_multi:  f32,
    height:       f32,
    new_height:   f32,
    height_multi: f32,
    flip_queue:   ArrayVec<Flip, 2>
}

impl ScaleInfo
{
    #[inline]
    pub fn new<const CAP: usize>(
        hull: &Hull,
        new_hull: &Hull,
        flip_queue: &ArrayVec<Flip, CAP>
    ) -> Option<Self>
    {
        let hull = hull.flipped(flip_queue.iter().copied());

        if hull.around_equal_narrow(new_hull) && flip_queue.is_empty()
        {
            return None;
        }

        let flip_queue = flip_queue
            .into_iter()
            .map(|flip| {
                match flip
                {
                    Flip::Above(v) => Flip::Above(2f32 * v),
                    Flip::Below(v) => Flip::Below(2f32 * v),
                    Flip::Left(v) => Flip::Left(2f32 * v),
                    Flip::Right(v) => Flip::Right(2f32 * v)
                }
            })
            .collect::<ArrayVec<_, 2>>();
        let width = hull.width();
        let new_width = new_hull.width();
        let height = hull.height();
        let new_height = new_hull.height();

        Self {
            pivot: hull.top_left(),
            new_pivot: new_hull.top_left(),
            width,
            new_width,
            width_multi: new_width / width,
            height,
            new_height,
            height_multi: new_height / height,
            flip_queue
        }
        .into()
    }

    #[inline]
    #[must_use]
    pub fn texture_scale(&self, scale_x: f32, scale_y: f32) -> (f32, f32)
    {
        let mut scale_x = scale_x * self.width_multi;
        let mut scale_y = scale_y * self.height_multi;

        for flip in &self.flip_queue
        {
            match flip
            {
                Flip::Above(_) | Flip::Below(_) => scale_y = -scale_y,
                Flip::Left(_) | Flip::Right(_) => scale_x = -scale_x
            };
        }

        (scale_x, scale_y)
    }

    #[inline]
    #[must_use]
    pub fn scaled_point(&self, mut p: Vec2) -> Vec2
    {
        for flip in &self.flip_queue
        {
            match flip
            {
                Flip::Above(mirror) | Flip::Below(mirror) => p.y = *mirror - p.y,
                Flip::Left(mirror) | Flip::Right(mirror) => p.x = *mirror - p.x
            };
        }

        Vec2::new(
            self.new_pivot.x + self.new_width * ((p.x - self.pivot.x) / self.width),
            self.new_pivot.y + self.new_height * ((p.y - self.pivot.y) / self.height)
        )
    }
}

//=======================================================================//

#[must_use]
pub(in crate::map) struct ShearInfo
{
    delta:              f32,
    opposite_dimension: f32,
    pivot:              f32
}

impl ShearInfo
{
    #[inline]
    pub const fn new(delta: f32, opposite_dimension: f32, pivot: f32) -> Self
    {
        Self {
            delta,
            opposite_dimension,
            pivot
        }
    }

    #[inline]
    #[must_use]
    pub const fn delta(&self) -> f32 { self.delta }

    #[inline]
    pub const fn with_delta(&self, delta: f32) -> Self
    {
        Self {
            delta,
            opposite_dimension: self.opposite_dimension,
            pivot: self.pivot
        }
    }
}

//=======================================================================//

#[must_use]
pub(in crate::map::brush) struct HollowResult
{
    pub main:  ConvexPolygon,
    pub walls: Vec<ConvexPolygon>
}

//=======================================================================//

#[must_use]
struct MovingTextureSettings<'b>
{
    texture: &'b TextureSettings,
    delta:   Vec2
}

impl<'b> TextureInterface for MovingTextureSettings<'b>
{
    #[inline]
    fn name(&self) -> &'b str { self.texture.name() }

    #[inline]
    fn offset_x(&self) -> f32 { self.texture.offset_x() - self.delta.x }

    #[inline]
    fn offset_y(&self) -> f32 { self.texture.offset_y() - self.delta.y }

    #[inline]
    fn draw_offset(&self) -> Vec2 { self.texture.draw_offset() - self.delta }

    #[inline]
    fn draw_offset_with_parallax_and_scroll(&self, camera_pos: Vec2, elapsed_time: f32) -> Vec2
    {
        self.texture
            .draw_offset_with_parallax_and_scroll(camera_pos, elapsed_time) -
            self.delta
    }

    #[inline]
    fn scale_x(&self) -> f32 { self.texture.scale_x() }

    #[inline]
    fn scale_y(&self) -> f32 { self.texture.scale_y() }

    #[inline]
    fn scroll_x(&self) -> f32 { self.texture.scroll_x() }

    #[inline]
    fn scroll_y(&self) -> f32 { self.texture.scroll_y() }

    #[inline]
    fn parallax_x(&self) -> f32 { self.texture.parallax_x() }

    #[inline]
    fn parallax_y(&self) -> f32 { self.texture.parallax_y() }

    #[inline]
    fn height(&self) -> i8 { self.texture.height() }

    #[inline]
    fn height_f32(&self) -> f32 { self.texture.height_f32() }

    #[inline]
    fn angle(&self) -> f32 { self.texture.angle() }

    #[inline]
    fn sprite(&self) -> bool { self.texture.sprite() }

    #[inline]
    fn animation(&self) -> &Animation { self.texture.animation() }
}

impl TextureInterfaceExtra for MovingTextureSettings<'_>
{
    #[inline]
    fn overall_animation<'a>(&'a self, drawing_resources: &'a DrawingResources) -> &'a Animation
    {
        self.texture.overall_animation(drawing_resources)
    }

    #[inline]
    fn sprite_hull<T: TextureSize>(
        &self,
        resources: &T,
        grid: &Grid,
        brush_center: Vec2
    ) -> Option<Hull>
    {
        self.texture.sprite_hull(resources, grid, brush_center + self.delta)
    }

    #[inline]
    fn sprite_vxs(
        &self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        brush_center: Vec2
    ) -> Option<[Vec2; 4]>
    {
        self.texture
            .sprite_vxs(drawing_resources, grid, brush_center + self.delta)
    }

    #[inline]
    fn animated_sprite_vxs(
        &self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        animator: Option<&Animator>,
        brush_center: Vec2
    ) -> Option<[Vec2; 4]>
    {
        self.texture.animated_sprite_vxs(
            drawing_resources,
            grid,
            animator,
            brush_center + self.delta
        )
    }

    #[inline]
    fn sprite_pivot(&self, brush_center: Vec2) -> Option<Vec2>
    {
        self.texture.sprite_pivot(brush_center)
    }
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

#[must_use]
#[derive(Clone)]
pub(in crate::map) struct ConvexPolygon
{
    vertexes:          Vec<SelectableVector>,
    center:            Vec2,
    hull:              Hull,
    selected_vertexes: u8,
    texture:           Option<TextureSettings>,
    texture_edited:    bool
}

impl From<Vec<Vec2>> for ConvexPolygon
{
    #[inline]
    fn from(vertexes: Vec<Vec2>) -> Self
    {
        vertexes
            .into_iter()
            .map(SelectableVector::new)
            .collect::<Vec<_>>()
            .into()
    }
}

impl From<Vec<crate::map::selectable_vector::SelectableVector>> for ConvexPolygon
{
    #[inline]
    fn from(vertexes: Vec<crate::map::selectable_vector::SelectableVector>) -> Self
    {
        assert!(vertexes.len() >= 3, "Not enough vertexes to create a polygon.\n{vertexes:?}.");

        let center = crate::utils::math::points::vxs_center(vertexes.iter().map(|svx| svx.vec));
        let hull = crate::utils::hull::Hull::from_points(vertexes.iter().map(|svx| svx.vec));
        let selected_vertexes = vertexes.iter().fold(0, |add, svx| add + u8::from(svx.selected));
        let cp = Self {
            vertexes,
            center,
            hull,
            selected_vertexes,
            texture: None,
            texture_edited: false
        };

        assert!(cp.valid(), "Invalid polygon.");

        cp
    }
}

impl PartialEq for ConvexPolygon
{
    #[inline]
    fn eq(&self, other: &Self) -> bool
    {
        for (vx_0, vx_1) in self.vertexes().zip(other.vertexes())
        {
            if !vx_0.around_equal_narrow(&vx_1)
            {
                return false;
            }
        }

        true
    }
}

impl EntityCenter for ConvexPolygon
{
    #[inline]
    fn center(&self) -> Vec2 { self.center }
}

impl From<(Vec<SelectableVector>, Option<&TextureSettings>)> for ConvexPolygon
{
    #[inline]
    fn from(value: (Vec<SelectableVector>, Option<&TextureSettings>)) -> Self
    {
        let mut poly = Self::from(value.0);

        if let Some(tex) = value.1
        {
            if !tex.sprite()
            {
                poly.texture = tex.clone().into();
            }
        }

        poly
    }
}

impl From<(Vec<Vec2>, Option<&TextureSettings>)> for ConvexPolygon
{
    #[inline]
    fn from(value: (Vec<Vec2>, Option<&TextureSettings>)) -> Self
    {
        let mut poly =
            Self::from(value.0.into_iter().map(SelectableVector::new).collect::<Vec<_>>());

        if let Some(tex) = value.1
        {
            if !tex.sprite()
            {
                poly.texture = tex.clone().into();
            }
        }

        poly
    }
}

impl From<ConvexPolygon> for Cow<'_, ConvexPolygon>
{
    #[inline]
    fn from(val: ConvexPolygon) -> Self { Cow::Owned(val) }
}

impl<'a> From<&'a ConvexPolygon> for Cow<'a, ConvexPolygon>
{
    #[inline]
    fn from(val: &'a ConvexPolygon) -> Self { Cow::Borrowed(val) }
}

impl ConvexPolygon
{
    //==============================================================
    // New

    #[inline]
    pub fn new<T>(vxs: T) -> Self
    where
        T: IntoIterator<Item = Vec2>
    {
        vxs.into_iter().collect::<Vec<_>>().into()
    }

    /// Returns true if vxs represents a valid polygon.
    #[inline]
    #[must_use]
    fn valid(&self) -> bool
    {
        if !self
            .center
            .around_equal(&crate::utils::math::points::vxs_center(self.vertexes())) ||
            !self
                .hull
                .around_equal(&crate::utils::hull::Hull::from_points(self.vertexes()))
        {
            eprintln!("Failed center/hull assertion.");
            return false;
        }

        if self.selected_vertexes !=
            self.vertexes.iter().fold(0, |add, svx| add + u8::from(svx.selected))
        {
            eprintln!("Failed selected vertexes count.");
            return false;
        }

        if !self.vxs_valid()
        {
            eprintln!("Invalid vertexes: {:?}.", self.vertexes);
            return false;
        }

        true
    }

    #[inline]
    #[must_use]
    fn vxs_valid(&self) -> bool
    {
        let vxs = &self.vertexes;
        let len = self.sides();

        if len < 3
        {
            return false;
        }

        for i in 0..len - 1
        {
            for j in i + 1..len
            {
                if vxs[i].vec.around_equal_narrow(&vxs[j].vec)
                {
                    return false;
                }
            }
        }

        self.vertexes
            .triplet_iter()
            .unwrap()
            .all(|[svx_i, svx_j, svx_k]| are_vxs_ccw(&[svx_i.vec, svx_j.vec, svx_k.vec]))
    }

    /// Returns the amount of sides the polygon has.
    #[inline]
    #[must_use]
    pub fn sides(&self) -> usize { self.vertexes.len() }

    /// Returns an iterator to the vertexes of the polygon.
    #[inline]
    pub fn vertexes(&self) -> impl ExactSizeIterator<Item = Vec2> + Clone + '_
    {
        self.vertexes.iter().map(|svx| svx.vec)
    }

    #[inline]
    pub fn take_texture_settings(self) -> Option<TextureSettings> { self.texture }

    #[inline]
    pub fn new_sorted<T>(vxs: T, texture: Option<&TextureSettings>) -> Self
    where
        T: Iterator<Item = Vec2>
    {
        let mut vec = vxs.map(SelectableVector::new).collect::<Vec<_>>();
        let center = vxs_center(vec.iter().map(|svx| svx.vec));
        vec.sort_by(|a, b| sort_vxs_ccw(a.vec, b.vec, center));
        (vec, texture).into()
    }

    #[inline]
    #[must_use]
    fn new_cleaned_up<T: IntoIterator<Item = Vec2>>(vxs: T) -> Option<Self>
    {
        let vertexes = vxs.into_iter().map(SelectableVector::new).collect::<Vec<_>>();
        let center = vxs_center(vertexes.iter().map(|svx| svx.vec));
        let hull = Hull::from_points(vertexes.iter().map(|svx| svx.vec));
        let mut cp = ConvexPolygon {
            vertexes,
            center,
            hull,
            selected_vertexes: 0,
            texture: None,
            texture_edited: false
        };
        cp.sort_vertexes_ccw();

        // Remove doubles.
        let mut i = 0;

        while i < cp.sides() - 1
        {
            let mut j = i + 1;

            while j < cp.sides()
            {
                if cp.vertexes[i].vec.around_equal_narrow(&cp.vertexes[j].vec)
                {
                    cp.vertexes.remove(j);
                    continue;
                }

                j += 1;
            }

            i += 1;
        }

        if cp.sides() < 3
        {
            return None;
        }

        iterate_slice_in_triplets!(i, j, k, cp.sides(), {
            match vertexes_orientation(&[
                cp.vertexes[i].vec,
                cp.vertexes[j].vec,
                cp.vertexes[k].vec
            ])
            {
                VertexesOrientation::CounterClockwise => (),
                VertexesOrientation::Collinear | VertexesOrientation::Clockwise =>
                {
                    if cp.sides() == 3
                    {
                        return None;
                    }

                    cp.vertexes.remove(j);
                    i %= cp.sides();
                    j %= cp.sides();
                    continue;
                }
            };
        });

        Some(cp)
    }

    //==============================================================
    // Info

    #[inline]
    pub const fn hull(&self) -> Hull { self.hull }

    #[allow(clippy::cast_precision_loss)]
    #[inline]
    #[must_use]
    fn sides_f32(&self) -> f32 { self.sides() as f32 }

    /// Returns true if p is in the area delimited by the brush.
    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn point_in_polygon(&self, p: Vec2) -> bool
    {
        self.vertexes
            .pair_iter()
            .unwrap()
            .all(|[vx_j, vx_i]| is_point_inside_clip_edge(&[vx_j.vec, vx_i.vec], p))
    }

    #[inline]
    pub(in crate::map::brush) fn selected_vertexes(&self) -> Option<impl Iterator<Item = Vec2>>
    {
        self.vertexes
            .iter()
            .filter_map(|svx| svx.selected.then_some(svx.vec))
            .collect::<Vec<_>>()
            .none_if_empty()
            .map(IntoIterator::into_iter)
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn selected_sides_vertexes(
        &self
    ) -> Option<impl Iterator<Item = Vec2>>
    {
        let mut selected_sides_vertexes = Vec::new();
        let len = self.sides();
        let (mut j, mut i) = (len - 1, 0);

        while i < len
        {
            let svx_j = &self.vertexes[j];

            if svx_j.selected
            {
                selected_sides_vertexes.push(svx_j.vec);
                let svx_i = &self.vertexes[i];

                if !svx_i.selected
                {
                    selected_sides_vertexes.push(svx_i.vec);
                    i += 1;
                }
            }

            j = i;
            i += 1;
        }

        Some(selected_sides_vertexes.none_if_empty()?.into_iter())
    }

    #[inline]
    #[must_use]
    pub fn sprite_hull<T: TextureSize>(&self, resources: &T, grid: &Grid) -> Option<Hull>
    {
        self.texture_settings()?.sprite_hull(resources, grid, self.center)
    }

    #[inline]
    #[must_use]
    pub fn sprite_pivot(&self) -> Option<Vec2>
    {
        self.texture_settings()?.sprite_pivot(self.center)
    }

    //============================================================
    // General Editing

    /// Sorts vxs in a clockwise order.
    #[inline]
    fn sort_vertexes_ccw(&mut self)
    {
        let center = self.center();
        self.vertexes.sort_by(|a, b| sort_vxs_ccw(a.vec, b.vec, center));
    }

    #[inline]
    pub fn update_center_hull(&mut self)
    {
        self.center = vxs_center(self.vertexes());
        self.hull = Hull::from_points(self.vertexes());
    }

    #[inline]
    pub fn update_center_hull_vertexes(&mut self)
    {
        let old_center = self.center;
        self.update_center_hull();

        if self.has_sprite()
        {
            let center = self.center;
            self.texture_settings_mut_dirty().move_offset(old_center - center);
        }
    }

    /// Moves the polygon by the amount delta.
    #[inline]
    pub fn update_fields(&mut self)
    {
        self.update_center_hull();
        self.sort_vertexes_ccw();
    }

    /// Moves the polygon by the amount delta.
    #[inline]
    pub fn check_move(
        &self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        delta: Vec2,
        move_texture: bool
    ) -> bool
    {
        if (self.hull + delta).out_of_bounds()
        {
            return false;
        }

        if move_texture
        {
            return self.check_texture_move(drawing_resources, grid, self.center + delta);
        }

        true
    }

    /// Moves the polygon by the amount delta.
    #[inline]
    pub fn move_by_delta(&mut self, delta: Vec2, move_texture: bool)
    {
        for vx in &mut self.vertexes
        {
            *vx += delta;
        }

        self.center += delta;
        self.hull += delta;

        if !self.has_texture()
        {
            return;
        }

        let sprite = return_if_none!(&self.texture).sprite();

        if !move_texture
        {
            if sprite
            {
                self.texture_settings_mut_dirty().move_offset(-delta);
            }
        }
        else if !sprite
        {
            self.texture_settings_mut_dirty().move_offset(delta);
        }
    }

    #[inline]
    pub(in crate::map::brush) fn move_vertexes_at_indexes<'a, I: Iterator<Item = &'a u8>>(
        &mut self,
        idxs: impl Iterator<Item = (I, Vec2)>
    )
    {
        for (idxs, delta) in idxs
        {
            for i in idxs
            {
                self.vertexes[*i as usize] += delta;
            }
        }

        self.update_center_hull();
        assert!(self.valid(), "move_vertexes_at_indexes generated an invalid polygon.");
    }

    #[inline]
    pub fn swap_polygon(&mut self, polygon: &mut Self)
    {
        let had_texture = self.has_texture();
        self.swap_value(polygon);

        if self.has_texture() || had_texture
        {
            self.texture_edited = true;
        }
    }

    #[inline]
    pub fn set_polygon(&mut self, mut polygon: ConvexPolygon) -> ConvexPolygon
    {
        let had_texture = self.has_texture();
        self.transfer_sprite(&mut polygon);
        let poly = self.replace_value(polygon);

        if self.has_texture() || had_texture
        {
            self.texture_edited = true;
        }

        poly
    }

    //==============================================================
    // Texture

    #[inline]
    #[must_use]
    pub(in crate::map::brush) const fn has_texture(&self) -> bool { self.texture.is_some() }

    #[inline]
    #[must_use]
    pub fn has_sprite(&self) -> bool { return_if_none!(self.texture_settings(), false).sprite() }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) const fn texture_settings(&self) -> Option<&TextureSettings>
    {
        self.texture.as_ref()
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn texture_edited(&mut self) -> bool
    {
        self.texture_edited.replace_value(false)
    }

    #[inline]
    fn texture_settings_mut(&mut self) -> &mut TextureSettings { self.texture.as_mut().unwrap() }

    #[inline]
    fn texture_settings_mut_dirty(&mut self) -> &mut TextureSettings
    {
        self.texture_edited = true;
        self.texture.as_mut().unwrap()
    }

    #[inline]
    #[must_use]
    fn set_texture_updated<T>(&mut self, value: Option<T>) -> Option<T>
    {
        if value.is_some()
        {
            self.texture_edited = true;
        }

        value
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn check_texture_change(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        texture: &str
    ) -> bool
    {
        match &mut self.texture
        {
            Some(tex_set) =>
            {
                tex_set.check_texture_change(drawing_resources, grid, texture, self.center)
            },
            None => true
        }
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn set_texture(
        &mut self,
        drawing_resources: &DrawingResources,
        texture: &str
    ) -> TextureSetResult
    {
        let result = match &mut self.texture
        {
            Some(tex_set) =>
            {
                match tex_set.set_texture(drawing_resources, texture)
                {
                    Some(prev) => TextureSetResult::Changed(prev),
                    None => TextureSetResult::Unchanged
                }
            },
            None =>
            {
                self.texture = Some(drawing_resources.texture_or_error(texture).into());
                TextureSetResult::Set
            }
        };

        if !matches!(result, TextureSetResult::Unchanged)
        {
            self.texture_edited = true;
        }

        result
    }

    #[inline]
    pub fn set_texture_settings(&mut self, texture: TextureSettings)
    {
        self.texture = texture.into();
        self.texture_edited = true;
    }

    #[inline]
    pub fn remove_texture(&mut self) -> TextureSettings
    {
        self.texture_edited = true;
        self.texture.take_value().unwrap()
    }

    #[inline]
    pub(in crate::map::brush) fn check_texture_move(
        &self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        new_center: Vec2
    ) -> bool
    {
        return_if_none!(self.texture_settings(), true).check_move(
            drawing_resources,
            grid,
            new_center
        )
    }

    #[inline]
    pub(in crate::map::brush) fn move_texture(&mut self, value: Vec2)
    {
        self.texture_settings_mut_dirty().move_offset(value);
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn check_texture_offset_x(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        value: f32
    ) -> bool
    {
        let center = self.center;
        self.texture_settings_mut()
            .check_offset_x(drawing_resources, grid, value, center)
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn set_texture_offset_x(&mut self, value: f32) -> Option<f32>
    {
        let result = self.texture_settings_mut().set_offset_x(value);
        self.set_texture_updated(result)
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn check_texture_offset_y(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        value: f32
    ) -> bool
    {
        let center = self.center;
        self.texture_settings_mut()
            .check_offset_y(drawing_resources, grid, value, center)
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn set_texture_offset_y(&mut self, value: f32) -> Option<f32>
    {
        let result = self.texture_settings_mut().set_offset_y(value);

        self.set_texture_updated(result)
    }

    #[inline]
    pub(in crate::map::brush) fn check_texture_scale_x(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        value: f32
    ) -> bool
    {
        let center = self.center;
        self.texture_settings_mut()
            .check_scale_x(drawing_resources, grid, value, center)
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn set_texture_scale_x(&mut self, value: f32) -> Option<f32>
    {
        let result = self.texture_settings_mut().set_scale_x(value);
        self.set_texture_updated(result)
    }

    #[inline]
    pub(in crate::map::brush) fn flip_texture_scale_x(&mut self)
    {
        let texture = self.texture_settings_mut_dirty();
        let scale = texture.scale_x();
        _ = texture.set_scale_x(-scale);
    }

    #[inline]
    pub(in crate::map::brush) fn check_texture_scale_y(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        value: f32
    ) -> bool
    {
        let center = self.center;
        self.texture_settings_mut_dirty()
            .check_scale_y(drawing_resources, grid, value, center)
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn set_texture_scale_y(&mut self, value: f32) -> Option<f32>
    {
        let result = self.texture_settings_mut().set_scale_y(value);
        self.set_texture_updated(result)
    }

    #[inline]
    pub(in crate::map::brush) fn flip_texture_scale_y(&mut self)
    {
        let texture = self.texture_settings_mut_dirty();
        let scale = texture.scale_y();
        _ = texture.set_scale_y(-scale);
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn set_texture_scroll_x(&mut self, value: f32) -> Option<f32>
    {
        let result = self.texture_settings_mut().set_scroll_x(value);
        self.set_texture_updated(result)
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn set_texture_scroll_y(&mut self, value: f32) -> Option<f32>
    {
        let result = self.texture_settings_mut().set_scroll_y(value);
        self.set_texture_updated(result)
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn set_texture_parallax_x(&mut self, value: f32) -> Option<f32>
    {
        let result = self.texture_settings_mut().set_parallax_x(value);
        self.set_texture_updated(result)
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn set_texture_parallax_y(&mut self, value: f32) -> Option<f32>
    {
        let result = self.texture_settings_mut().set_parallax_y(value);
        self.set_texture_updated(result)
    }

    #[inline]
    pub(in crate::map::brush) fn check_texture_angle(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        value: f32
    ) -> bool
    {
        let center = self.center;
        self.texture_settings_mut()
            .check_angle(drawing_resources, grid, value, center)
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn set_texture_angle(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        value: f32
    ) -> Option<TextureRotation>
    {
        let center = self.center;
        let result = self
            .texture_settings_mut()
            .set_angle(drawing_resources, grid, value, center);
        self.set_texture_updated(result)
    }

    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn set_texture_height(&mut self, value: i8) -> Option<i8>
    {
        let result = self.texture_settings_mut().set_height(value);
        self.set_texture_updated(result)
    }

    #[inline]
    pub(in crate::map::brush) fn check_texture_sprite(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        value: bool
    ) -> bool
    {
        let center = self.center;
        self.texture_settings_mut()
            .check_sprite(drawing_resources, grid, value, center)
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn set_texture_sprite(
        &mut self,
        value: bool
    ) -> Option<TextureSpriteSet>
    {
        let result = self.texture_settings_mut().set_sprite(value);
        self.set_texture_updated(result)
    }

    #[inline]
    pub(in crate::map::brush) fn undo_redo_texture_sprite(&mut self, value: &mut TextureSpriteSet)
    {
        self.texture_settings_mut_dirty().undo_redo_sprite(value);
    }

    #[inline]
    pub(in crate::map::brush) fn transfer_sprite(&self, target: &mut Self)
    {
        if !self.has_sprite()
        {
            return;
        }

        target.texture.clone_from(&self.texture);
        let center = target.center;
        let delta = self.center - center;
        target.texture_edited = true;
        target.texture_settings_mut().move_offset(delta);
    }

    #[inline]
    #[must_use]
    pub fn check_texture_within_bounds(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid
    ) -> bool
    {
        self.texture_settings()
            .unwrap()
            .check_within_bounds(drawing_resources, grid, self.center)
    }

    #[inline]
    #[must_use]
    pub fn check_texture_animation_change(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        animation: &Animation
    ) -> bool
    {
        let center = self.center;
        self.texture_settings_mut().check_animation_change(
            drawing_resources,
            grid,
            animation,
            center
        )
    }

    #[inline]
    pub(in crate::map::brush) fn set_texture_animation(&mut self, animation: Animation)
        -> Animation
    {
        self.texture_settings_mut_dirty().set_animation(animation)
    }

    #[inline]
    pub(in crate::map::brush) fn set_texture_list_animation(&mut self, texture: &str) -> Animation
    {
        self.texture_settings_mut_dirty().set_list_animation(texture)
    }

    #[inline]
    pub(in crate::map::brush) fn generate_list_animation(&mut self) -> Animation
    {
        self.texture_settings_mut_dirty().generate_list_animation()
    }

    #[inline]
    #[must_use]
    pub fn check_atlas_animation_x_partition(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        value: u32
    ) -> bool
    {
        let center = self.center;
        self.texture_settings_mut().check_atlas_animation_x_partition(
            drawing_resources,
            grid,
            value,
            center
        )
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn set_atlas_animation_x_partition(
        &mut self,
        value: u32
    ) -> Option<u32>
    {
        let result = self.texture_settings_mut().set_atlas_animation_x_partition(value);

        self.set_texture_updated(result)
    }

    #[inline]
    #[must_use]
    pub fn check_atlas_animation_y_partition(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        value: u32
    ) -> bool
    {
        let center = self.center;
        self.texture_settings_mut().check_atlas_animation_y_partition(
            drawing_resources,
            grid,
            value,
            center
        )
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn set_atlas_animation_y_partition(
        &mut self,
        value: u32
    ) -> Option<u32>
    {
        let result = self.texture_settings_mut().set_atlas_animation_y_partition(value);

        self.set_texture_updated(result)
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn atlas_animation_max_len(&self) -> usize
    {
        self.texture_settings().unwrap().atlas_animation_max_len()
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn set_atlas_animation_len(&mut self, value: usize) -> Option<usize>
    {
        let result = self.texture_settings_mut().set_atlas_animation_len(value);
        self.set_texture_updated(result)
    }

    #[inline]
    pub(in crate::map::brush) fn set_atlas_animation_timing(&mut self, timing: Timing) -> Timing
    {
        self.texture_settings_mut_dirty().set_atlas_animation_timing(timing)
    }

    #[inline]
    pub(in crate::map::brush) fn set_atlas_animation_uniform_timing(&mut self) -> Option<Timing>
    {
        let result = self.texture_settings_mut().set_atlas_animation_uniform_timing();
        self.set_texture_updated(result)
    }

    #[inline]
    pub(in crate::map::brush) fn set_atlas_animation_per_frame_timing(&mut self) -> Option<Timing>
    {
        let result = self.texture_settings_mut().set_atlas_animation_per_frame_timing();
        self.set_texture_updated(result)
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn set_atlas_animation_uniform_time(
        &mut self,
        value: f32
    ) -> Option<f32>
    {
        let result = self.texture_settings_mut().set_atlas_animation_uniform_time(value);
        self.set_texture_updated(result)
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn set_atlas_animation_frame_time(
        &mut self,
        index: usize,
        value: f32
    ) -> Option<f32>
    {
        let result = self
            .texture_settings_mut()
            .set_atlas_animation_frame_time(index, value);
        self.set_texture_updated(result)
    }

    #[inline]
    pub(in crate::map::brush) fn move_down_atlas_animation_frame_time(&mut self, index: usize)
    {
        self.texture_settings_mut_dirty()
            .move_down_atlas_animation_frame_time(index);
    }

    #[inline]
    pub(in crate::map::brush) fn move_up_atlas_animation_frame_time(&mut self, index: usize)
    {
        self.texture_settings_mut_dirty()
            .move_up_atlas_animation_frame_time(index);
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn set_list_animation_texture(
        &mut self,
        index: usize,
        texture: &str
    ) -> Option<String>
    {
        let result = self.texture_settings_mut().set_list_animation_texture(index, texture);
        self.set_texture_updated(result)
    }

    #[inline]
    #[must_use]
    pub fn texture_list_animation_frame(&self, index: usize) -> &(String, f32)
    {
        self.texture_settings().unwrap().list_animation_frame(index)
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn set_list_animation_time(
        &mut self,
        index: usize,
        time: f32
    ) -> Option<f32>
    {
        let result = self.texture_settings_mut().set_list_animation_time(index, time);
        self.set_texture_updated(result)
    }

    #[inline]
    pub(in crate::map::brush) fn move_up_list_animation_frame(&mut self, index: usize)
    {
        self.texture_settings_mut_dirty().move_up_list_animation_frame(index);
    }

    #[inline]
    pub(in crate::map::brush) fn move_down_list_animation_frame(&mut self, index: usize)
    {
        self.texture_settings_mut_dirty()
            .move_down_list_animation_frame(index);
    }

    #[inline]
    pub(in crate::map::brush) fn insert_list_animation_frame(
        &mut self,
        index: usize,
        texture: &str,
        time: f32
    )
    {
        self.texture_settings_mut_dirty()
            .insert_list_animation_frame(index, texture, time);
    }

    #[inline]
    pub(in crate::map::brush) fn pop_list_animation_frame(&mut self)
    {
        self.texture_settings_mut_dirty().pop_list_animation_frame();
    }

    #[inline]
    pub(in crate::map::brush) fn remove_list_animation_frame(&mut self, index: usize)
    {
        self.texture_settings_mut_dirty().remove_list_animation_frame(index);
    }

    #[inline]
    pub(in crate::map::brush) fn push_list_animation_frame(&mut self, texture: &str)
    {
        self.texture_settings_mut_dirty().push_list_animation_frame(texture);
    }

    #[inline]
    pub(in crate::map::brush) fn reset_texture(&mut self) -> TextureReset
    {
        self.texture_settings_mut_dirty().reset()
    }

    #[inline]
    pub(in crate::map::brush) fn undo_redo_texture_reset(&mut self, value: &mut TextureReset)
    {
        self.texture_settings_mut_dirty().undo_redo_reset(value);
    }

    //==============================================================
    // Snap

    #[inline]
    #[must_use]
    fn snap_filtered_vertexes<F>(&mut self, grid: &Grid, f: F) -> Option<Vec<(Vec<u8>, Vec2)>>
    where
        F: Fn(&SelectableVector) -> bool
    {
        #[inline]
        fn round<F, G>(
            vertexes: &mut [SelectableVector],
            moved_vxs: &mut Vec<(Vec<u8>, Vec2)>,
            f: &F,
            g: G
        ) where
            F: Fn(&SelectableVector) -> bool,
            G: Fn(Vec2) -> Option<Vec2>
        {
            'outer: for (i, svx) in vertexes.iter_mut().enumerate().filter(|(_, svx)| f(svx))
            {
                let delta = continue_if_none!(g(svx.vec)) - svx.vec;
                svx.vec += delta;

                let idx = u8::try_from(i).unwrap();

                for (idxs, d) in &mut *moved_vxs
                {
                    if d.around_equal_narrow(&delta)
                    {
                        idxs.push(idx);
                        continue 'outer;
                    }
                }

                moved_vxs.push((vec![idx], delta));
            }
        }

        let mut moved_vxs = Vec::new();

        round(&mut self.vertexes, &mut moved_vxs, &f, |vec| grid.snap_point(vec));

        if moved_vxs.is_empty()
        {
            return None;
        }

        if self.vxs_valid()
        {
            self.update_center_hull_vertexes();
            return moved_vxs.into();
        }

        for (idxs, delta) in &moved_vxs
        {
            for idx in idxs
            {
                self.vertexes[*idx as usize] -= *delta;
            }
        }

        moved_vxs.clear();

        round(&mut self.vertexes, &mut moved_vxs, &f, |vec| {
            grid.snap_point_from_center(vec, self.center)
        });

        if !self.vxs_valid()
        {
            for (idxs, delta) in moved_vxs
            {
                for idx in idxs
                {
                    self.vertexes[idx as usize] -= delta;
                }
            }

            return None;
        }

        self.update_center_hull_vertexes();
        moved_vxs.into()
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn snap_vertexes(
        &mut self,
        grid: &Grid
    ) -> Option<Vec<(Vec<u8>, Vec2)>>
    {
        self.snap_filtered_vertexes(grid, |_| true)
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn snap_selected_vertexes(
        &mut self,
        grid: &Grid
    ) -> Option<Vec<(Vec<u8>, Vec2)>>
    {
        self.snap_filtered_vertexes(grid, |svx| svx.selected)
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn snap_selected_sides(
        &mut self,
        grid: &Grid
    ) -> Option<Vec<(Vec<u8>, Vec2)>>
    {
        let vertexes_to_deselect = self.select_vertexes_of_selected_sides();
        let result = self.snap_filtered_vertexes(grid, |svx| svx.selected);

        for idx in vertexes_to_deselect
        {
            self.vertexes[idx as usize].selected = false;
        }

        result
    }

    //==============================================================
    // Vertex Editing

    #[inline]
    #[must_use]
    pub(in crate::map::brush) const fn has_selected_vertexes(&self) -> bool
    {
        self.selected_vertexes != 0
    }

    #[inline]
    #[must_use]
    pub const fn selected_vertexes_amount(&self) -> u8 { self.selected_vertexes }

    #[inline]
    #[must_use]
    pub fn selected_sides_amount(&self) -> u8
    {
        let mut selected_vertexes = 0;
        let len = self.sides();
        let (mut j, mut i) = (len - 1, 0);

        while i < len
        {
            let svx_j = &self.vertexes[j];

            if svx_j.selected
            {
                selected_vertexes += 1;
                let svx_i = &self.vertexes[i];

                if !svx_i.selected
                {
                    selected_vertexes += 1;
                    i += 1;
                }
            }

            j = i;
            i += 1;
        }

        selected_vertexes
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn nearby_vertex(
        &self,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> Option<usize>
    {
        self.vertexes.iter().enumerate().find_map(|(i, svx)| {
            svx.vec
                .is_point_inside_ui_highlight(cursor_pos, camera_scale)
                .then_some(i)
        })
    }

    /// Returns a `VertexSelectionResult` describing the state of the
    /// `SelectableVertex` found, if any, close to `cursor_pos`. If a
    /// `SelectableVertex` is found and it is not selected, it is
    /// selected, but the function returns `VertexSelectionResult::NotSelected`.
    /// as returned value.
    #[inline]
    pub(in crate::map::brush) fn check_vertex_proximity_and_exclusively_select(
        &mut self,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> VectorSelectionResult
    {
        let mut result = self.check_vertex_proximity(cursor_pos, camera_scale);

        if let VectorSelectionResult::NotSelected(_, idxs) = &mut result
        {
            let skip = usize::from(idxs[0]);
            self.vertexes[skip].selected = true;

            for (i, svx) in self
                .vertexes
                .iter_mut()
                .enumerate()
                .skip_index(skip)
                .unwrap()
                .filter(|(_, svx)| svx.selected)
            {
                idxs.push(i.try_into().unwrap());
                svx.selected = false;
            }

            self.selected_vertexes = 1;
        }

        result
    }

    #[inline]
    fn check_vertex_proximity(
        &mut self,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> VectorSelectionResult
    {
        match self.nearby_vertex(cursor_pos, camera_scale)
        {
            Some(idx) =>
            {
                let nth = &self.vertexes[idx];

                if nth.selected
                {
                    VectorSelectionResult::Selected
                }
                else
                {
                    VectorSelectionResult::NotSelected(nth.vec, vec![u8::try_from(idx).unwrap()])
                }
            },
            None => VectorSelectionResult::None
        }
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn try_select_vertex(&mut self, pos: Vec2) -> Option<u8>
    {
        for (i, svx) in self.vertexes.iter_mut().enumerate()
        {
            if !svx.vec.around_equal_narrow(&pos)
            {
                continue;
            }

            if svx.selected.replace_value(true)
            {
                return None;
            }

            self.selected_vertexes += 1;
            return u8::try_from(i).unwrap().into();
        }

        None
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn vertex_at_index(&self, index: usize) -> Vec2
    {
        self.vertexes[index].vec
    }

    #[inline]
    pub(in crate::map::brush) fn toggle_vertex_at_index(&mut self, index: usize)
    {
        if self.vertexes[index].selected
        {
            self.selected_vertexes -= 1;
            self.vertexes[index].selected = false;
            return;
        }

        self.selected_vertexes += 1;
        self.vertexes[index].selected = true;
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn try_exclusively_select_vertex(
        &mut self,
        pos: Vec2
    ) -> Option<Vec<u8>>
    {
        let mut idxs = self.deselect_vertexes();

        if let Some(s_idx) = self.try_select_vertex(pos)
        {
            if idxs.is_none()
            {
                self.selected_vertexes = 1;
                return Some(vec![s_idx]);
            }

            let idxs_mut = idxs.as_mut().unwrap();

            for i in 0..idxs_mut.len()
            {
                if idxs_mut[i] != s_idx
                {
                    continue;
                }

                idxs_mut.remove(i);

                if idxs_mut.is_empty()
                {
                    return None;
                }

                return idxs;
            }

            self.selected_vertexes = 1;
            idxs_mut.push(s_idx);
        }

        idxs
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn select_vertexes_in_range(
        &mut self,
        range: &Hull
    ) -> Option<Vec<u8>>
    {
        if !range.overlaps(&self.hull)
        {
            return None;
        }

        let idxs =
            select_vectors_in_range(VertexesSelectionIterMut(&mut self.vertexes).iter(), range);

        if let Some(idxs) = &idxs
        {
            self.selected_vertexes += u8::try_from(idxs.len()).unwrap();
        }

        idxs
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn exclusively_select_vertexes_in_range(
        &mut self,
        range: &Hull
    ) -> Option<Vec<u8>>
    {
        if !range.overlaps(&self.hull)
        {
            return self.deselect_vertexes();
        }

        self.selected_vertexes = 0;

        self.vertexes
            .iter_mut()
            .enumerate()
            .filter_map(|(i, svx)| {
                let selected = std::mem::replace(&mut svx.selected, range.contains_point(svx.vec));

                if svx.selected
                {
                    self.selected_vertexes += 1;
                }

                (svx.selected != selected).then(|| u8::try_from(i).unwrap())
            })
            .collect::<Vec<_>>()
            .none_if_empty()
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn select_all_vertexes(&mut self) -> Option<Vec<u8>>
    {
        self.selected_vertexes = u8::try_from(self.sides()).unwrap();

        self.vertexes
            .iter_mut()
            .enumerate()
            .filter_map(|(i, svx)| {
                if svx.selected
                {
                    return None;
                }

                svx.selected = true;
                Some(u8::try_from(i).unwrap())
            })
            .collect::<Vec<_>>()
            .none_if_empty()
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn toggle_vertex_at_pos(&mut self, pos: Vec2) -> Option<u8>
    {
        let idx = self.vertexes.iter_mut().enumerate().find_map(|(i, svx)| {
            (svx.vec.around_equal_narrow(&pos)).then(|| u8::try_from(i).unwrap())
        });

        if let Some(idx) = idx
        {
            self.toggle_vertex_at_index(idx as usize);
        }

        idx
    }

    /// Returns the vertex close to `cursor_pos` if there is a close enough
    /// vertex.
    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn toggle_vertex_nearby_cursor_pos(
        &mut self,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> Option<(Vec2, u8, bool)>
    {
        let mut value = self.nearby_vertex(cursor_pos, camera_scale).map(|idx| {
            (self.vertexes[idx].vec, u8::try_from(idx).unwrap(), self.vertexes[idx].selected)
        });

        if let Some((_, idx, selected)) = &mut value
        {
            self.toggle_vertex_at_index(*idx as usize);
            selected.toggle();
        }

        value
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn deselect_vertexes(&mut self) -> Option<Vec<u8>>
    {
        self.selected_vertexes = 0;
        deselect_vectors(VertexesSelectionIterMut(&mut self.vertexes).iter())
    }

    #[inline]
    pub fn deselect_vertexes_no_indexes(&mut self)
    {
        self.selected_vertexes = 0;

        for svx in &mut self.vertexes
        {
            svx.selected = false;
        }
    }

    #[inline]
    pub fn insert_vertex_at_index(&mut self, pos: Vec2, index: usize, selected: bool)
    {
        assert!(
            self.try_vertex_insertion_at_index(pos, index, selected),
            "insert_vertex_at_index generated an invalid polygon."
        );
    }

    /// Adds a vertex to the polygon if it's possible to do so without losing convexity and
    /// returns whether it was possible to do so.
    #[inline]
    pub(in crate::map::brush) fn try_vertex_insertion_at_index(
        &mut self,
        pos: Vec2,
        index: usize,
        selected: bool
    ) -> bool
    {
        let len = self.sides();

        assert!(index <= len, "Insertion index is higher or equal to vertexes len.");

        let idx_fit = index % len;
        let p = prev(idx_fit, len);

        if [
            [self.vertexes[prev(p, len)].vec, self.vertexes[p].vec, pos],
            [self.vertexes[p].vec, pos, self.vertexes[idx_fit].vec],
            [
                pos,
                self.vertexes[idx_fit].vec,
                self.vertexes[next(idx_fit, len)].vec
            ]
        ]
        .iter()
        .any(|vxs| !are_vxs_ccw(vxs))
        {
            return false;
        }

        self.vertexes
            .insert(index, SelectableVector::with_selected(pos, selected));

        if selected
        {
            self.selected_vertexes += 1;
        }

        self.update_center_hull_vertexes();

        true
    }

    #[inline]
    #[must_use]
    pub fn try_insert_free_draw_vertex(&mut self, pos: Vec2, camera_scale: f32) -> bool
    {
        if self.nearby_vertex(pos, camera_scale).is_some()
        {
            return false;
        }

        self.vertexes.push(SelectableVector::new(pos));
        self.sort_vertexes_ccw();

        if !self.vxs_valid()
        {
            // If shape is not ok revert changes.
            let idx = self
                .vertexes()
                .position(|value| value.around_equal_narrow(&pos))
                .unwrap();
            self.vertexes.remove(idx);
            return false;
        }

        self.update_center_hull();
        true
    }

    #[inline]
    pub fn insert_free_draw_vertex(&mut self, p: Vec2)
    {
        self.vertexes.push(SelectableVector::new(p));
        self.update_fields();
        assert!(self.valid(), "insert_free_draw_vertex generated an invalid polygon.");
    }

    #[inline]
    pub fn try_delete_free_draw_vertex(
        &mut self,
        pos: Vec2,
        camera_scale: f32
    ) -> FreeDrawVertexDeletionResult
    {
        let len = self.sides();

        if let Some(i) = self.nearby_vertex(pos, camera_scale)
        {
            if len > 3
            {
                let deleted = self.vertexes[i].vec;
                self.vertexes.remove(i);
                self.update_fields();
                return FreeDrawVertexDeletionResult::Polygon(deleted);
            }

            let j = next(i, len);
            return FreeDrawVertexDeletionResult::Line(
                [self.vertexes[j].vec, self.vertexes[next(j, len)].vec],
                self.vertexes[i].vec
            );
        }

        FreeDrawVertexDeletionResult::None
    }

    #[inline]
    pub fn delete_free_draw_vertex(&mut self, p: Vec2) -> Option<[Vec2; 2]>
    {
        let len = self.sides();
        let idx = self
            .vertexes
            .iter()
            .position(|svx| svx.vec.around_equal_narrow(&p))
            .unwrap();

        if len > 3
        {
            self.vertexes.remove(idx);
            self.update_fields();
            return None;
        }

        Some([
            self.vertexes[next(idx, len)].vec,
            self.vertexes[next_n_steps(idx, 2, len)].vec
        ])
    }

    /// Returns the index the projection of `cursor_pos` on the polygon would
    /// have if it were added to the polygon. Returns None if it's not
    /// valid.
    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn vertex_insertion_index(&self, cursor_pos: Vec2) -> Option<usize>
    {
        const MAX_DISTANCE: f32 = VX_HGL_SIDE * (3f32 / 2f32);
        const MAX_DISTANCE_SQUARED: f32 = MAX_DISTANCE * MAX_DISTANCE;

        // Find the closest projection on a side of the cursor position.
        let (mut distance, mut idx) = (f32::MAX, None);

        for (i, [vx_j, vx_i]) in self
            .vertexes
            .pair_iter()
            .unwrap()
            .map(|[a, b]| [a.vec, b.vec])
            .enumerate()
        {
            let p = closest_point_on_segment(vx_j, vx_i, cursor_pos);

            if p.around_equal_narrow(&vx_j) || p.around_equal_narrow(&vx_i)
            {
                continue;
            }

            let cursor_to_p_distance = cursor_pos.distance_squared(p);

            if cursor_to_p_distance <= MAX_DISTANCE_SQUARED && cursor_to_p_distance < distance
            {
                idx = i.into();
                distance = cursor_to_p_distance;
            }
        }

        idx
    }

    /// Returns true if inserting `pos` in the shape at index `vx_idx` generates
    /// a valid polygon.
    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn is_new_vertex_at_index_valid(
        &mut self,
        pos: Vec2,
        index: usize
    ) -> bool
    {
        if self.point_in_polygon(pos)
        {
            return false;
        }

        let idx = index % self.sides();
        let vxs = &self.vertexes;
        let (vx_at_idx, prev) = (vxs[idx].vec, prev_element(idx, vxs).vec);

        ![
            [prev_element_n_steps(idx, 2, vxs).vec, prev, pos],
            [prev, pos, vx_at_idx],
            [pos, vx_at_idx, next_element(idx, vxs).vec]
        ]
        .iter()
        .any(|vxs| !are_vxs_ccw(vxs))
    }

    #[inline]
    pub fn delete_vertex_at_index(&mut self, index: usize)
    {
        if self.vertexes[index].selected
        {
            self.selected_vertexes -= 1;
        }

        self.vertexes.remove(index);
        self.update_center_hull_vertexes();
    }

    #[inline]
    pub(in crate::map::brush) fn check_selected_vertexes_deletion(&self) -> VertexesDeletionResult
    {
        // Nothing selected.
        if self.selected_vertexes == 0
        {
            return VertexesDeletionResult::None;
        }

        // Deleting too much.
        if self.sides() - (self.selected_vertexes as usize) < 3
        {
            return VertexesDeletionResult::Invalid;
        }

        VertexesDeletionResult::Valid
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn delete_selected_vertexes(&mut self) -> Option<Vec<(Vec2, u8)>>
    {
        let mut deleted_vxs = Vec::new();
        let mut i = 0;
        let mut index = 0u8;

        while i < self.sides()
        {
            if self.vertexes[i].selected
            {
                deleted_vxs.push((self.vertexes[i].vec, index));
                self.vertexes.remove(i);
            }
            else
            {
                i += 1;
            }

            index += 1;
        }

        assert!(
            self.sides() >= 3,
            "Vertexes deletion generated a polygon with {} sides only.",
            self.sides()
        );

        self.selected_vertexes = 0;

        deleted_vxs.none_if_empty().inspect(|_| {
            self.update_center_hull_vertexes();
        })
    }

    /// Moves the selected vertex by the desired delta amount.
    #[inline]
    pub(in crate::map::brush) fn check_selected_vertexes_move(
        &mut self,
        delta: Vec2
    ) -> VertexesMoveResult
    {
        let mut moved_vxs = Vec::new();

        for (idx, svx) in self.vertexes.iter().enumerate().filter(|(_, svx)| svx.selected)
        {
            if (svx.vec + delta).out_of_bounds()
            {
                return VertexesMoveResult::Invalid;
            }

            moved_vxs.push(u8::try_from(idx).unwrap());
        }

        if moved_vxs.is_empty()
        {
            return VertexesMoveResult::None;
        }

        for idx in &moved_vxs
        {
            self.vertexes[*idx as usize].vec += delta;
        }

        // Test the brush for vertex collapsing.
        let mut merged_vxs = MergedVertexes::new();

        'outer: for i in moved_vxs.iter().map(|idx| usize::from(*idx))
        {
            let svx_i = self.vertexes[i];

            for j in [prev(i, self.sides()), next(i, self.sides())]
            {
                let svx_j = self.vertexes[j];

                if svx_j.selected || !svx_i.vec.around_equal_narrow(&svx_j.vec)
                {
                    continue;
                }

                merged_vxs.push((svx_i.vec, u8::try_from(j).unwrap()));

                if merged_vxs.len() == 2
                {
                    merged_vxs.sort();
                    break 'outer;
                }
            }
        }

        // Remove the merged vertexes.
        for idx in merged_vxs.iter().rev().map(|(_, idx)| *idx as usize)
        {
            self.vertexes.remove(idx);
        }

        // Store validity.
        let valid = self.vxs_valid();

        // Revert changes.
        let vxs_move = VertexesMove {
            merged: merged_vxs,
            moved: moved_vxs,
            delta
        };

        self.execute_vertexes_move_undo(&vxs_move);

        if !valid
        {
            return VertexesMoveResult::Invalid;
        }

        VertexesMoveResult::Valid(vxs_move)
    }

    #[inline]
    pub(in crate::map::brush) fn apply_vertexes_move_result(&mut self, vxs_move: &VertexesMove)
    {
        for idx in vxs_move.moved.iter().map(|idx| usize::from(*idx))
        {
            self.vertexes[idx] += vxs_move.delta;
        }

        for idx in vxs_move.merged.iter().rev().map(|(_, idx)| usize::from(*idx))
        {
            assert!(!self.vertexes[idx].selected, "Tried to remove selected vertex.");
            self.vertexes.remove(idx);
        }

        self.update_center_hull_vertexes();
        assert!(self.valid(), "apply_vertexes_move_result generated an invalid polygon.");
    }

    #[inline]
    fn execute_vertexes_move_undo(&mut self, vxs_move: &VertexesMove)
    {
        for (vx, idx) in vxs_move.merged.iter()
        {
            self.vertexes.insert(usize::from(*idx), SelectableVector::new(*vx));
        }

        for idx in vxs_move.moved.iter().map(|idx| usize::from(*idx))
        {
            self.vertexes[idx] -= vxs_move.delta;
        }
    }

    #[inline]
    pub(in crate::map::brush) fn undo_vertexes_move(&mut self, vxs_move: &VertexesMove)
    {
        self.execute_vertexes_move_undo(vxs_move);
        self.update_center_hull_vertexes();
        assert!(self.valid(), "undo_vertexes_move generated an invalid polygon.");
    }

    #[inline]
    pub(in crate::map::brush) fn check_split(&self) -> SplitResult
    {
        if self.selected_vertexes == 0
        {
            return SplitResult::None;
        }

        if self.selected_vertexes != 2
        {
            return SplitResult::Invalid;
        }

        let mut selected_vertexes = ArrayVec::<u8, 2>::new();

        for ([_, i], [vx_j, vx_i]) in self.vertexes.pair_iter().unwrap().enumerate()
        {
            if !vx_i.selected
            {
                continue;
            }

            if vx_j.selected
            {
                return SplitResult::Invalid;
            }

            selected_vertexes.push(i.try_into().unwrap());

            if selected_vertexes.len() == 2
            {
                break;
            }
        }

        SplitResult::Valid(selected_vertexes)
    }

    #[inline]
    pub(in crate::map::brush) fn split(&mut self, indexes: &ArrayVec<u8, 2>) -> Self
    {
        let mut indexes = [usize::from(indexes[0]), usize::from(indexes[1])];
        let mut vertexes = Vec::with_capacity(indexes[1] - indexes[0]);

        vertexes.push(self.vertexes[indexes[0]]);
        indexes[0] += 1;

        for svx in self.vertexes.drain(indexes[0]..indexes[1])
        {
            vertexes.push(svx);
        }

        self.update_center_hull_vertexes();

        vertexes.push(self.vertexes[indexes[0]]);
        (vertexes, self.texture_settings()).into()
    }

    //==============================================================
    // Side editing

    #[inline]
    #[must_use]
    fn nearby_side_index(&self, cursor_pos: Vec2, camera_scale: f32) -> Option<usize>
    {
        let max_distance = VX_HGL_SIDE_SQUARED * camera_scale;

        self.vertexes
            .pair_iter()
            .unwrap()
            .enumerate()
            .find_map(|([j, _], [vx_j, vx_i])| {
                (point_to_segment_distance_squared(vx_j.vec, vx_i.vec, cursor_pos) <= max_distance)
                    .then_some(j)
            })
    }

    #[inline]
    #[must_use]
    fn is_point_on_side(&self, p: Vec2) -> bool
    {
        self.vertexes
            .pair_iter()
            .unwrap()
            .enumerate()
            .find_map(|([j, _], [vx_j, vx_i])| {
                is_point_on_segment(&[vx_j.vec, vx_i.vec], p).then_some(j)
            })
            .is_some()
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn nearby_side(
        &self,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> Option<([Vec2; 2], usize)>
    {
        self.nearby_side_index(cursor_pos, camera_scale).map(|idx| {
            (
                [
                    self.vertexes[idx].vec,
                    next_element(idx, &self.vertexes).vec
                ],
                idx
            )
        })
    }

    #[inline]
    fn check_side_proximity(&mut self, cursor_pos: Vec2, camera_scale: f32) -> SideSelectionResult
    {
        match self.nearby_side_index(cursor_pos, camera_scale)
        {
            Some(idx) =>
            {
                if self.vertexes[idx].selected
                {
                    SideSelectionResult::Selected
                }
                else
                {
                    SideSelectionResult::NotSelected(
                        [
                            self.vertexes[idx].vec,
                            next_element(idx, &self.vertexes).vec
                        ],
                        vec![u8::try_from(idx).unwrap()]
                    )
                }
            },
            None => SideSelectionResult::None
        }
    }

    #[inline]
    pub(in crate::map::brush) fn check_side_proximity_and_select(
        &mut self,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> SideSelectionResult
    {
        let mut result = self.check_side_proximity(cursor_pos, camera_scale);

        if let SideSelectionResult::NotSelected(_, idxs) = &mut result
        {
            let u_idx0 = usize::from(idxs[0]);
            self.vertexes[u_idx0].selected = true;

            for (i, svx) in self
                .vertexes
                .iter_mut()
                .enumerate()
                .skip_index(u_idx0)
                .unwrap()
                .filter(|(_, svx)| svx.selected)
            {
                idxs.push(u8::try_from(i).unwrap());
                svx.selected = false;
            }

            self.selected_vertexes = 1;
        }

        result
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn try_select_side(&mut self, side: &[Vec2; 2]) -> Option<u8>
    {
        for ([j, _], [vx_j, vx_i]) in self.vertexes.pair_iter_mut().unwrap().enumerate()
        {
            if !(side[0].around_equal_narrow(&vx_j.vec) && side[1].around_equal_narrow(&vx_i.vec)) ||
                (side[1].around_equal_narrow(&vx_j.vec) &&
                    side[0].around_equal_narrow(&vx_i.vec))
            {
                continue;
            }

            if vx_j.selected.replace_value(true)
            {
                return None;
            }

            self.selected_vertexes += 1;
            return u8::try_from(j).unwrap().into();
        }

        None
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn try_exclusively_select_side(
        &mut self,
        side: &[Vec2; 2]
    ) -> Option<Vec<u8>>
    {
        let mut idxs = self.deselect_vertexes();

        if let Some(s_idx) = self.try_select_side(&[side[0], side[1]])
        {
            if idxs.is_none()
            {
                return Some(vec![s_idx]);
            }

            let idxs_mut = idxs.as_mut().unwrap();

            for i in 0..idxs_mut.len()
            {
                if idxs_mut[i] != s_idx
                {
                    continue;
                }

                idxs_mut.remove(i);

                if idxs_mut.is_empty()
                {
                    return None;
                }

                return idxs;
            }

            idxs_mut.push(s_idx);
        }

        idxs
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn select_sides_in_range(&mut self, range: &Hull) -> Option<Vec<u8>>
    {
        if !self.hull.overlaps(range)
        {
            return None;
        }

        let mut idxs = Vec::new();

        for ([j, _], [vx_j, vx_i]) in self.vertexes.pair_iter_mut().unwrap().enumerate()
        {
            let selected = vx_j.selected;

            if range.contains_point(vx_j.vec) && range.contains_point(vx_i.vec)
            {
                vx_j.selected = true;
            }

            if vx_j.selected != selected
            {
                idxs.push(j.try_into().unwrap());
            }
        }

        self.selected_vertexes += u8::try_from(idxs.len()).unwrap();

        idxs.none_if_empty()
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn exclusively_select_sides_in_range(
        &mut self,
        range: &Hull
    ) -> Option<Vec<u8>>
    {
        if !self.hull.overlaps(range)
        {
            return self.deselect_vertexes();
        }

        let mut idxs = Vec::new();

        for ([j, _], [vx_j, vx_i]) in self.vertexes.pair_iter_mut().unwrap().enumerate()
        {
            let selected = vx_j.selected;
            vx_j.selected = range.contains_point(vx_j.vec) && range.contains_point(vx_i.vec);

            if vx_j.selected != selected
            {
                idxs.push(j.try_into().unwrap());
            }
        }

        self.selected_vertexes = u8::try_from(idxs.len()).unwrap();

        idxs.none_if_empty()
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn toggle_side_nearby_cursor_pos(
        &mut self,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> Option<([Vec2; 2], u8, bool)>
    {
        let mut idx = self.nearby_side_index(cursor_pos, camera_scale).map(|idx| {
            (
                [
                    self.vertexes[idx].vec,
                    next_element(idx, &self.vertexes).vec
                ],
                u8::try_from(idx).unwrap(),
                self.vertexes[idx].selected
            )
        });

        if let Some((_, idx, selected)) = &mut idx
        {
            self.toggle_vertex_at_index(*idx as usize);
            selected.toggle();
        }

        idx
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn toggle_side_at_pos(&mut self, l: &[Vec2; 2]) -> Option<u8>
    {
        for ([j, _], [vx_j, vx_i]) in self.vertexes.pair_iter_mut().unwrap().enumerate()
        {
            for (l_1, l_2) in [(*l).into(), (l[1], l[0])]
            {
                if vx_j.vec.around_equal_narrow(&l_1) && vx_i.vec.around_equal_narrow(&l_2)
                {
                    self.toggle_vertex_at_index(j);
                    return Some(j.try_into().unwrap());
                }
            }
        }

        None
    }

    #[inline]
    pub(in crate::map::brush) fn check_selected_sides_deletion(&self) -> SidesDeletionResult
    {
        let len = self.sides();
        let mut deletion_result = Vec::new();

        // Check deletion and create the deletion payload.
        let (mut j, mut i) = (len - 1, 0);
        let (mut j_index, mut i_index) = (u8::try_from(j).unwrap(), 0u8);

        while i < len
        {
            if self.vertexes[j].selected
            {
                deletion_result.push((self.vertexes[j].vec, j_index, true));

                if self.vertexes[i].selected
                {
                    j = i;
                    i += 1;
                }
                else
                {
                    deletion_result.push((self.vertexes[i].vec, i_index, false));

                    j = i + 1;
                    i += 2;
                }
            }
            else
            {
                j = i;
                i += 1;
            }

            j_index = i_index;
            i_index += 1;
        }

        // Nothing selected.
        if deletion_result.is_empty()
        {
            return SidesDeletionResult::None;
        }

        // Deleting too much.
        if self.sides() - deletion_result.len() < 3
        {
            return SidesDeletionResult::Invalid;
        }

        deletion_result.sort_by(|a, b| a.1.cmp(&b.1));
        SidesDeletionResult::Valid(deletion_result)
    }

    #[inline]
    pub(in crate::map::brush) fn delete_selected_sides(
        &mut self,
        deletion_indexes: impl Iterator<Item = usize>
    )
    {
        for idx in deletion_indexes
        {
            self.vertexes.remove(idx);
        }

        self.selected_vertexes = 0;

        assert!(
            self.sides() >= 3,
            "Sides deletion generated a polygon with {} sides only.",
            self.sides()
        );

        self.update_center_hull_vertexes();
    }

    #[inline]
    #[must_use]
    fn select_vertexes_of_selected_sides(&mut self) -> Vec<u8>
    {
        let mut vertexes_to_deselect = Vec::new();

        let len = self.sides();
        let (mut j, mut i) = (len - 1, 0);

        while i < len
        {
            if !self.vertexes[j].selected
            {
                j = i;
                i += 1;
                continue;
            }

            if !self.vertexes[i].selected
            {
                vertexes_to_deselect.push(u8::try_from(i).unwrap());
                self.vertexes[i].selected = true;

                j = i + 1;
                i += 2;
                continue;
            }

            j = i;
            i += 1;
        }

        vertexes_to_deselect
    }

    #[inline]
    pub(in crate::map::brush) fn check_selected_sides_move(
        &mut self,
        delta: Vec2
    ) -> VertexesMoveResult
    {
        let vertexes_to_deselect = self.select_vertexes_of_selected_sides();
        let move_result = self.check_selected_vertexes_move(delta);

        for idx in vertexes_to_deselect
        {
            self.vertexes[idx as usize].selected = false;
        }

        move_result
    }

    #[inline]
    pub(in crate::map::brush) fn xtrusion_info(
        &self,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> Option<([Vec2; 2], Vec2, XtrusionInfo)>
    {
        if self.selected_vertexes != 1
        {
            return None;
        }

        let index = self.nearby_side_index(cursor_pos, camera_scale)?;

        if !self.vertexes[index].selected
        {
            return None;
        }

        let vx_idx = self.vertexes[index].vec;
        let vx_p = prev_element(index, &self.vertexes).vec;
        let n = next(index, self.sides());
        let vx_n = self.vertexes[n].vec;
        let vx_nn = next_element(n, &self.vertexes).vec;

        Some(([vx_idx, vx_n], (vx_idx - vx_n).normalize().perp(), XtrusionInfo {
            prev_side: [vx_idx, vx_p],
            next_side: [vx_n, vx_nn]
        }))
    }

    #[inline]
    pub(in crate::map::brush) fn matching_xtrusion_info(&self, normal: Vec2) -> XtrusionResult
    {
        let mut i = 0;
        let len = self.sides();
        let mut extrusion_index = None;

        // There needs to be a selected side.
        while i < len
        {
            if self.vertexes[i].selected
            {
                extrusion_index = i.into();
                i += 1;
                break;
            }

            i += 1;
        }

        // But only one.
        while i < len
        {
            if self.vertexes[i].selected
            {
                return XtrusionResult::Invalid;
            }

            i += 1;
        }

        if extrusion_index.is_none()
        {
            return XtrusionResult::None;
        }

        let idx = extrusion_index.unwrap();
        let vx_idx = self.vertexes[idx].vec;
        let n = next(idx, self.sides());
        let vx_n = self.vertexes[n].vec;

        // If the selected side does not have the same normal the extrusion is invalid.
        if !(vx_idx - vx_n).normalize().perp().around_equal(&normal)
        {
            return XtrusionResult::Invalid;
        }

        XtrusionResult::Valid(XtrusionInfo {
            prev_side: [vx_idx, prev_element(idx, &self.vertexes).vec],
            next_side: [vx_n, next_element(n, &self.vertexes).vec]
        })
    }

    //==============================================================
    // Clip

    #[inline]
    pub(in crate::map::brush) fn clip(&self, clip_segment: &[Vec2; 2]) -> Option<[Self; 2]>
    {
        let mut right_polygon = self.clone();
        let mut left_polygon = right_polygon.clip_self(clip_segment)?;

        self.transfer_sprite(&mut right_polygon);
        self.transfer_sprite(&mut left_polygon);

        Some([left_polygon, right_polygon])
    }

    #[inline]
    fn clip_self(&mut self, clip_segment: &[Vec2; 2]) -> Option<Self>
    {
        let mut vec = Vec::new();
        vec.extend(clip_polygon(
            self.vertexes.pair_iter().unwrap().map(|[a, b]| [a.vec, b.vec]),
            clip_segment
        )?);

        let left_polygon = Self::from((vec, self.texture_settings()));

        for vx in left_polygon.vertexes()
        {
            let idx = self
                .vertexes()
                .enumerate()
                .find_map(|(idx, svx)| (svx.around_equal_narrow(&vx)).then_some(idx));

            match idx
            {
                Some(idx) =>
                {
                    if is_point_inside_clip_edge(clip_segment, vx)
                    {
                        self.vertexes.swap_remove(idx);
                    }
                },
                None => self.vertexes.push(vx.into())
            };
        }

        self.update_fields();

        if !self.valid()
        {
            return None;
        }

        Some(left_polygon)
    }

    //==============================================================
    // Hollow

    #[inline]
    pub(in crate::map::brush) fn hollow(&self, grid_size: f32) -> Option<HollowResult>
    {
        let sides = self.sides();
        let mut walls = Vec::with_capacity(sides);
        let mut leftover = self.clone();

        if leftover.has_sprite()
        {
            leftover.texture = None;
        }

        for [j, i] in (0..sides).pair_iter().unwrap()
        {
            let vx_j = self.vertexes[j].vec;
            let vx_i = self.vertexes[i].vec;
            let normal = (vx_i - vx_j).normalize().perp() * grid_size;

            let left_polygon = leftover.clip_self(&[vx_j + normal, vx_i + normal])?;
            walls.push(leftover.replace_value(left_polygon));
        }

        if walls.is_empty()
        {
            return None;
        }

        HollowResult {
            main: walls.swap_remove(0),
            walls
        }
        .into()
    }

    //==============================================================
    // Shatter

    /// Shatters the polygon into triangles that all have a common vertex in
    /// `cursor_pos`.
    #[inline]
    pub(in crate::map::brush) fn shatter(
        &self,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> Option<ShatterResult>
    {
        let (capacity, mut i, mut j, common_vx) = {
            if let Some(idx) = self.nearby_vertex(cursor_pos, camera_scale)
            {
                // Triangle cannot be shattered at the vertexes.
                if self.sides() < 4
                {
                    return None;
                }

                // A polygon shattered at a vertex generates sides - 2 triangle.
                (self.sides() - 2, idx, next(idx, self.sides()), self.vertexes[idx])
            }
            else
            {
                let cursor_svx = SelectableVector::new(cursor_pos);

                match self.nearby_side_index(cursor_pos, camera_scale)
                {
                    Some(idx) => (self.sides() - 1, idx, next(idx, self.sides()), cursor_svx),
                    None =>
                    {
                        if !self.point_in_polygon(cursor_pos)
                        {
                            return None;
                        }

                        (self.sides(), 0, 1, cursor_svx)
                    }
                }
            }
        };

        let mut shards = std::iter::from_fn(|| {
            i = j;
            j = next(i, self.sides());
            let vxs: Vec<SelectableVector> = vec![common_vx, self.vertexes[i], self.vertexes[j]];

            Some(ConvexPolygon::from((vxs, self.texture_settings())))
        })
        .take(capacity)
        .collect::<Vec<_>>();

        let mut main = shards.swap_remove(0);
        self.transfer_sprite(&mut main);

        Some(ShatterResult { main, shards })
    }

    //==============================================================
    // Intersect

    #[inline]
    pub(in crate::map::brush) fn intersection(&self, other: &Self) -> Option<Self>
    {
        let mut polygon = self.vertexes().collect::<Vec<_>>();

        for [svx_j, svx_i] in other.vertexes.pair_iter().unwrap()
        {
            polygon = clip_polygon(polygon.pair_iter().unwrap().map(|[a, b]| [*a, *b]), &[
                svx_j.vec, svx_i.vec
            ])?
            .into_iter()
            .collect();
        }

        let mut poly = Self::new_cleaned_up(polygon).unwrap();

        if self.texture.is_some() && self.texture == other.texture
        {
            poly.texture.clone_from(&self.texture);
        }

        poly.into()
    }

    //==============================================================
    // Subtract

    // Original convex polygons subtraction algorithm, please steal, gently caress my ego.
    #[inline]
    pub(in crate::map::brush) fn subtract(&self, other: &Self) -> SubtractResult
    {
        #[derive(Clone, Copy, PartialEq)]
        enum VertexTag
        {
            This,
            Other,
            Common
        }

        #[inline]
        fn simple_ear_clipping(input: Vec<Vec2>) -> impl Iterator<Item = Vec<Vec2>>
        {
            let input_len = input.len();
            let mut triangles = Vec::with_capacity(input_len - 2);

            if input_len == 3
            {
                triangles.push(input);
                return triangles.into_iter();
            }

            let i = input_len / 2 + 1;

            for (i, span) in [(0, 2..i), (input_len - 1, i..input_len - 1)]
            {
                for n in span
                {
                    triangles.push(vec![input[i], input[n], input[prev(n, input_len)]]);
                }
            }

            triangles.push(vec![input[0], input[input_len - 1], input[i - 1]]);
            triangles.into_iter()
        }

        // Get the polygon representing the intersection between this polygon and the
        // other.
        let intersection = self.intersection(other);

        if intersection.is_none()
        {
            return SubtractResult::None;
        }

        let intersection = intersection.unwrap();

        if *self == intersection
        {
            return SubtractResult::Despawn;
        }

        // Catalog the vertexes of 'intersection' and 'self' based on their properties.
        // While doing this, calculate the center of the intersection polygon.
        let mut subtract_vertexes = Vec::new();

        for svx in &self.vertexes
        {
            subtract_vertexes.push((VertexTag::This, svx.vec));
        }

        let mut center = Vec2::ZERO;
        let mut center_vx_count = 0f32;

        for vx in intersection.vertexes.into_iter().map(|svx| svx.vec)
        {
            center += vx;
            center_vx_count += 1f32;

            // Mark the common ones as Common.
            if let Some(index) = subtract_vertexes.iter().position(|(_, v)| v.around_equal(&vx))
            {
                subtract_vertexes[index].0 = VertexTag::Common;
                continue;
            }

            if self.is_point_on_side(vx)
            {
                subtract_vertexes.push((VertexTag::Common, vx));
                continue;
            }

            subtract_vertexes.push((VertexTag::Other, vx));
        }

        center /= center_vx_count;

        // Sort the vertexes cw using 'center' as a pivot.
        subtract_vertexes.sort_by(|a, b| sort_vxs_ccw(a.1, b.1, center));

        // If we have three consecutive Common vertexes remove the one in the middle.
        iterate_slice_in_triplets!(i, j, k, subtract_vertexes.len(), {
            if subtract_vertexes[i].0 == VertexTag::Common &&
                subtract_vertexes[k].0 == VertexTag::Common &&
                subtract_vertexes[j].0 == VertexTag::Common
            {
                subtract_vertexes.remove(j);
                j = prev(k, subtract_vertexes.len());
                i = prev(j, subtract_vertexes.len());
                continue;
            }
        });

        // Group the 'subtract_vertexes' elements into vertexes sets which added
        // constitute the subtract polygon. These polygon may be either convex
        // or concave. If one of the sets has 3 sides immediately add it to
        // 'polygons'.
        let mut polygons = Vec::new();

        let sub_vxs_len = subtract_vertexes.len();
        let mut scan_index = {
            let mut value = None;

            // We start scanning from a vertex followed by one by one with a different tag
            // that is not VertexTag::Common.
            for ([j, _], [vx_j, vx_i]) in subtract_vertexes.pair_iter().unwrap().enumerate()
            {
                let tag_i = vx_i.0;

                if vx_j.0 != tag_i && tag_i != VertexTag::Common
                {
                    value = j.into();
                    break;
                }
            }

            value.unwrap()
        };
        let start_index = scan_index;

        loop
        {
            let mut vxs = Vec::with_capacity(5);

            vxs.push(subtract_vertexes[scan_index].1);
            scan_index = next(scan_index, sub_vxs_len);

            vxs.push(subtract_vertexes[scan_index].1);
            let mid_tag = subtract_vertexes[scan_index].0;
            scan_index = next(scan_index, sub_vxs_len);

            loop
            {
                vxs.push(subtract_vertexes[scan_index].1);
                let last_tag = subtract_vertexes[scan_index].0;

                if last_tag != mid_tag
                {
                    let len = vxs.len();

                    // Two scenarios that guarantee convex polygons.
                    if len == 3 || mid_tag == VertexTag::This
                    {
                        polygons.push(Self::new_sorted(vxs.into_iter(), self.texture_settings()));
                    }
                    else
                    {
                        vxs.reverse();

                        // If it has four vertexes it might be convex.
                        if len == 4 && is_polygon_convex(&vxs)
                        {
                            polygons.push(Self::from((vxs, self.texture_settings())));
                        }
                        else
                        {
                            for trg in simple_ear_clipping(vxs)
                            {
                                polygons.push(Self::new_sorted(
                                    trg.into_iter(),
                                    self.texture_settings()
                                ));
                            }
                        }
                    }

                    if last_tag == VertexTag::Common
                    {
                        let next = next(scan_index, sub_vxs_len);

                        if subtract_vertexes[next].0 == VertexTag::Common
                        {
                            scan_index = next;
                        }
                    }
                    else
                    {
                        scan_index = prev(scan_index, sub_vxs_len);
                    }

                    break;
                }

                scan_index = next(scan_index, sub_vxs_len);
            }

            if scan_index == start_index
            {
                break;
            }
        }

        assert!(!polygons.is_empty(), "Subtraction generated no polygons.");

        SubtractResult::Some {
            main:   polygons.swap_remove(0),
            others: polygons
        }
    }

    //==============================================================
    // Scale

    #[inline]
    pub(in crate::map::brush) fn check_scale(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        info: &ScaleInfo,
        scale_texture: bool
    ) -> ScaleResult
    {
        let mut vxs = Vec::with_capacity(self.sides());
        let mut new_center = Vec2::ZERO;

        for vx in self.vertexes.iter().map(|svx| svx.vec)
        {
            let vx = info.scaled_point(vx);

            if vx.out_of_bounds()
            {
                return ScaleResult::Invalid;
            }

            vxs.push(vx);
            new_center += vx;
        }

        if !info.flip_queue.is_empty()
        {
            vxs.reverse();
        }

        new_center /= self.sides_f32();

        if scale_texture
        {
            let center = self.center;
            let scale = return_if_none!(
                self.texture_settings_mut().check_scale(
                    drawing_resources,
                    grid,
                    info,
                    center,
                    new_center
                ),
                ScaleResult::Invalid
            );

            return ScaleResult::Valid {
                new_center,
                vxs,
                texture_scale: scale.into()
            };
        }

        ScaleResult::Valid {
            new_center,
            vxs,
            texture_scale: None
        }
    }

    #[inline]
    pub(in crate::map) fn check_texture_scale(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        info: &ScaleInfo
    ) -> Option<TextureScale>
    {
        let center = self.center;
        self.texture_settings_mut()
            .check_scale(drawing_resources, grid, info, center, center)
    }

    #[inline]
    pub(in crate::map) fn scale_texture(&mut self, value: &mut TextureScale)
    {
        self.texture_settings_mut_dirty().scale(value);
    }

    //==============================================================
    // Shear

    #[allow(clippy::cast_precision_loss)]
    #[inline]
    pub(in crate::map::brush) fn check_horizontal_shear(
        &self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        info: &ShearInfo
    ) -> Option<(Vec2, Vec<f32>)>
    {
        let mut xs = Vec::with_capacity(self.sides());
        let mut new_center = Vec2::ZERO;

        for vx in self.vertexes()
        {
            let vx_x = vx.x + info.delta * ((vx.y - info.pivot).abs() / info.opposite_dimension);

            if !MAP_RANGE.contains(&vx_x)
            {
                return None;
            }

            new_center += Vec2::new(vx_x, vx.y);
            xs.push(vx_x);
        }

        new_center /= self.sides_f32();

        if self.check_texture_move(drawing_resources, grid, new_center)
        {
            return None;
        }

        (new_center, xs).into()
    }

    #[inline]
    pub(in crate::map::brush) fn set_x_coordinates(&mut self, xs: Vec<f32>)
    {
        for (vx, x) in self.vertexes.iter_mut().map(|svx| &mut svx.vec).zip(xs)
        {
            vx.x = x;
        }

        self.update_center_hull();
        assert!(self.valid(), "set_x_coordinates generated an invalid polygon.");
    }

    #[allow(clippy::cast_precision_loss)]
    #[inline]
    pub(in crate::map::brush) fn check_vertical_shear(
        &self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        info: &ShearInfo
    ) -> Option<(Vec2, Vec<f32>)>
    {
        let mut ys = Vec::with_capacity(self.sides());
        let mut new_center = Vec2::ZERO;

        for vx in self.vertexes()
        {
            let vx_y = vx.y + info.delta * ((vx.x - info.pivot).abs() / info.opposite_dimension);

            if vx_y.out_of_bounds()
            {
                return None;
            }

            new_center += Vec2::new(vx.x, vx_y);
            ys.push(vx_y);
        }

        new_center /= self.sides_f32();

        if self.check_texture_move(drawing_resources, grid, new_center)
        {
            return None;
        }

        (new_center, ys).into()
    }

    #[inline]
    pub(in crate::map::brush) fn set_y_coordinates(&mut self, ys: Vec<f32>)
    {
        for (vx, y) in self.vertexes.iter_mut().map(|svx| &mut svx.vec).zip(ys)
        {
            vx.y = y;
        }

        self.update_center_hull();
        assert!(self.valid(), "set_y_coordinates generated an invalid polygon.");
    }

    //==============================================================
    // Rotate

    #[allow(clippy::cast_precision_loss)]
    #[inline]
    pub(in crate::map::brush) fn check_rotation(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        pivot: Vec2,
        angle: f32,
        rotate_texture: bool
    ) -> RotateResult
    {
        angle.assert_normalized_degrees_angle();

        let angle_rad = angle.to_radians();
        let mut new_center = Vec2::ZERO;
        let mut vxs = Vec::with_capacity(self.sides());

        for vx in self.vertexes()
        {
            let vx = rotate_point(vx, pivot, angle_rad);

            if vx.out_of_bounds()
            {
                return RotateResult::Invalid;
            }

            new_center += vx;
            vxs.push(vx);
        }

        new_center /= self.sides_f32();

        if rotate_texture
        {
            let center = self.center;

            return self
                .texture_settings_mut()
                .check_rotation(drawing_resources, grid, pivot, angle, center, new_center)
                .map_or(RotateResult::Invalid, |t_rotation| {
                    RotateResult::Valid {
                        new_center,
                        vxs,
                        texture_rotation: t_rotation.into()
                    }
                });
        }

        RotateResult::Valid {
            new_center,
            vxs,
            texture_rotation: None
        }
    }

    #[allow(clippy::cast_precision_loss)]
    #[inline]
    pub(in crate::map::brush) fn check_texture_rotation(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        pivot: Vec2,
        angle: f32
    ) -> Option<TextureRotation>
    {
        let center = self.center;
        let new_center = center;
        self.texture_settings_mut().check_rotation(
            drawing_resources,
            grid,
            pivot,
            angle,
            center,
            new_center
        )
    }

    #[inline]
    pub(in crate::map::brush) fn set_coordinates(&mut self, vxs: impl IntoIterator<Item = Vec2>)
    {
        for (vx, svx) in vxs.into_iter().zip(self.vertexes.iter_mut())
        {
            svx.vec = vx;
        }

        self.update_center_hull();
        assert!(self.valid(), "set_coordinates generated an invalid polygon.");
    }

    #[inline]
    pub fn rotate_texture(&mut self, payload: &mut TextureRotation)
    {
        self.texture_settings_mut_dirty().rotate(payload);
    }

    //==============================================================
    // Flip

    #[inline]
    pub(in crate::map::brush) fn check_y_flip(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        y: f32,
        flip_texture: bool
    ) -> Option<Vec2>
    {
        let y = 2f32 * y;

        if self.vertexes().any(|vx| (y - vx.y).out_of_bounds())
        {
            return None;
        }

        let new_center = Vec2::new(self.center.x, y - self.center.y);

        if flip_texture
        {
            let center = self.center;

            return self
                .texture_settings_mut()
                .check_y_flip(drawing_resources, grid, y, center, new_center)
                .then_some(new_center);
        }

        self.check_texture_move(drawing_resources, grid, new_center)
            .then_some(new_center)
    }

    #[inline]
    pub(in crate::map::brush) fn check_x_flip(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        x: f32,
        flip_texture: bool
    ) -> Option<Vec2>
    {
        let x = 2f32 * x;

        if self.vertexes().any(|vx| (x - vx.x).out_of_bounds())
        {
            return None;
        }

        let new_center = Vec2::new(x - self.center.x, self.center.y);

        if flip_texture
        {
            let center = self.center;

            return self
                .texture_settings_mut()
                .check_x_flip(drawing_resources, grid, x, center, new_center)
                .then_some(new_center);
        }

        self.check_texture_move(drawing_resources, grid, new_center)
            .then_some(new_center)
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn check_flip_above(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        y: f32,
        flip_texture: bool
    ) -> Option<Vec2>
    {
        assert!(
            y >= self.hull.top(),
            "Y vertical flip pivot {y} is lower than the hull's top {}",
            self.hull.top()
        );
        self.check_y_flip(drawing_resources, grid, y, flip_texture)
    }

    #[inline]
    pub(in crate::map::brush) fn flip_above(&mut self, y: f32, flip_texture: bool)
    {
        assert!(
            y >= self.hull.top(),
            "Y vertical flip pivot {y} is lower than the hull's top {}",
            self.hull.top()
        );
        self.flip_horizontal(y, flip_texture);
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn check_flip_below(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        y: f32,
        flip_texture: bool
    ) -> Option<Vec2>
    {
        assert!(
            y <= self.hull.bottom(),
            "Y vertical flip pivot {y} is lower than the hull's top {}",
            self.hull.bottom()
        );
        self.check_y_flip(drawing_resources, grid, y, flip_texture)
    }

    #[inline]
    pub(in crate::map::brush) fn flip_below(&mut self, y: f32, flip_texture: bool)
    {
        assert!(
            y <= self.hull.bottom(),
            "Y vertical flip pivot {y} is lower than the hull's top {}",
            self.hull.bottom()
        );
        self.flip_horizontal(y, flip_texture);
    }

    #[inline]
    pub(in crate::map::brush) fn flip_horizontal(&mut self, y: f32, flip_texture: bool)
    {
        let y = 2f32 * y;

        for vx in self.vertexes.iter_mut().map(|svx| &mut svx.vec)
        {
            vx.y = y - vx.y;
        }

        let old_center = self.center;

        self.vertexes.reverse();
        self.update_center_hull();
        assert!(self.valid(), "flip_horizontal generated an invalid polygon.");

        if flip_texture
        {
            let center = self.center;
            self.texture_settings_mut_dirty().y_flip(y, old_center, center);
        }
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn check_flip_left(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        x: f32,
        flip_texture: bool
    ) -> Option<Vec2>
    {
        assert!(
            x <= self.hull.left(),
            "Y vertical flip pivot {x} is higher than the hull's left {}",
            self.hull.left()
        );
        self.check_x_flip(drawing_resources, grid, x, flip_texture)
    }

    #[inline]
    pub(in crate::map::brush) fn flip_left(&mut self, x: f32, flip_texture: bool)
    {
        assert!(
            x <= self.hull.left(),
            "Y vertical flip pivot {x} is higher than the hull's left {}",
            self.hull.left()
        );
        self.flip_vertical(x, flip_texture);
    }

    #[inline]
    #[must_use]
    pub(in crate::map::brush) fn check_flip_right(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        x: f32,
        flip_texture: bool
    ) -> Option<Vec2>
    {
        assert!(
            x >= self.hull.right(),
            "Y vertical flip pivot {x} is lower than the hull's right {}",
            self.hull.right()
        );
        self.check_x_flip(drawing_resources, grid, x, flip_texture)
    }

    #[inline]
    pub(in crate::map::brush) fn flip_right(&mut self, x: f32, flip_texture: bool)
    {
        assert!(
            x >= self.hull.right(),
            "Y vertical flip pivot {x} is lower than the hull's right {}",
            self.hull.right()
        );
        self.flip_vertical(x, flip_texture);
    }

    #[inline]
    pub(in crate::map::brush) fn flip_vertical(&mut self, x: f32, flip_texture: bool)
    {
        let x = 2f32 * x;

        for vx in self.vertexes.iter_mut().map(|svx| &mut svx.vec)
        {
            vx.x = x - vx.x;
        }

        let old_center = self.center;

        self.vertexes.reverse();
        self.update_center_hull();
        assert!(self.valid(), "flip_vertical generated an invalid polygon.");

        if flip_texture
        {
            let center = self.center;
            self.texture_settings_mut_dirty().x_flip(x, old_center, center);
        }
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
        drawer.brush(camera, self.vertexes(), animator, return_if_none!(self.texture_settings()));
    }

    /// Draws the polygon.
    #[inline]
    pub fn draw(&self, drawer: &mut EditDrawer, collision: bool, color: Color)
    {
        drawer.brush(self.vertexes(), color, self.texture.as_ref(), collision);
    }

    #[inline]
    pub fn draw_prop(&self, drawer: &mut EditDrawer, color: Color, delta: Vec2)
    {
        let center = self.center + delta;

        drawer.sideless_brush(
            self.vertexes().map(|vx| vx + delta),
            color,
            self.texture
                .as_ref()
                .map(|texture| MovingTextureSettings { texture, delta })
                .as_ref(),
            false
        );

        if self.has_sprite()
        {
            drawer.sprite(center, self.texture_settings().unwrap(), color, false);
        }
    }

    /// Draws the polygon.
    #[inline]
    pub fn draw_free_draw(&self, window: &Window, camera: &Transform, drawer: &mut EditDrawer)
    {
        drawer.sides(self.vertexes(), Color::CursorPolygon);

        for vx in self.vertexes()
        {
            drawer.square_highlight(vx, Color::CursorPolygon);
        }

        let mut text = String::with_capacity(6);

        for vx in self.vertexes()
        {
            let label = return_if_none!(drawer.vx_tooltip_label(vx));
            free_draw_tooltip(window, camera, drawer, vx, label, &mut text);
        }
    }

    #[inline]
    fn draw_side_mode(&self, drawer: &mut EditDrawer, collision: bool)
    {
        let mut sides_colors = Vec::with_capacity(self.sides());

        for svx in &self.vertexes
        {
            drawer.square_highlight(svx.vec, Color::NonSelectedVertex);

            if svx.selected
            {
                sides_colors.push(Color::SelectedVertex);
                continue;
            }

            sides_colors.push(Color::NonSelectedVertex);
        }

        drawer.brush_with_sides_colors(
            self.vertexes.pair_iter().unwrap().map(|[a, b]| {
                (
                    a.vec,
                    b.vec,
                    if a.selected { Color::SelectedVertex } else { Color::NonSelectedVertex }
                )
            }),
            Color::NonSelectedVertex,
            self.texture.as_ref(),
            collision
        );
    }

    #[inline]
    pub(in crate::map::brush) fn draw_extended_side(
        &self,
        drawer: &mut EditDrawer,
        index: usize,
        color: Color
    )
    {
        drawer.infinite_line(
            self.vertexes[index].vec,
            next_element(index, &self.vertexes).vec,
            color
        );
    }

    /// Draws the `ConvexPolygon` with the special vertex highlight procedure.
    #[inline]
    pub(in crate::map::brush) fn draw_with_vertex_highlight(
        &self,
        window: &Window,
        camera: &Transform,
        drawer: &mut EditDrawer,
        collision: bool,
        hgl_mode: &VertexHighlightMode
    )
    {
        macro_rules! declare_tooltip_string {
            ($label:ident) => {
                let mut $label = String::with_capacity(6);
            };
        }

        match hgl_mode
        {
            VertexHighlightMode::Side =>
            {
                self.draw_side_mode(drawer, collision);

                declare_tooltip_string!(vx_coordinates);

                // Draws the tooltips showing the coordinates of the vertexes representing
                // the extremities of the selected lines.
                for [svx_j, svx_i] in
                    self.vertexes.pair_iter().unwrap().filter(|[svx_j, _]| svx_j.selected)
                {
                    let label = return_if_none!(drawer.vx_tooltip_label(svx_j.vec));

                    vertex_tooltip(window, camera, drawer, svx_j.vec, label, &mut vx_coordinates);

                    if !svx_i.selected
                    {
                        let label = return_if_none!(drawer.vx_tooltip_label(svx_i.vec));

                        vertex_tooltip(
                            window,
                            camera,
                            drawer,
                            svx_i.vec,
                            label,
                            &mut vx_coordinates
                        );
                    }
                }
            },
            VertexHighlightMode::Vertex =>
            {
                self.draw(drawer, collision, Color::SelectedEntity);

                for svx in &self.vertexes
                {
                    if svx.selected
                    {
                        drawer.square_highlight(svx.vec, Color::SelectedVertex);
                        continue;
                    }

                    drawer.square_highlight(svx.vec, Color::NonSelectedVertex);
                }

                declare_tooltip_string!(vx_coordinates);

                for svx in self.vertexes.iter().filter(|svx| svx.selected)
                {
                    let label = return_if_none!(drawer.vx_tooltip_label(svx.vec));

                    vertex_tooltip(window, camera, drawer, svx.vec, label, &mut vx_coordinates);
                }
            },
            VertexHighlightMode::NewVertex(new_vx, idx) =>
            {
                // Draw the shape including the new vertex.
                self.draw_with_vertex_inserted_at_index(
                    drawer,
                    collision,
                    Color::SelectedEntity,
                    *new_vx,
                    *idx
                );

                for svx in &self.vertexes
                {
                    if svx.selected
                    {
                        drawer.square_highlight(svx.vec, Color::SelectedVertex);
                        continue;
                    }

                    drawer.square_highlight(svx.vec, Color::NonSelectedVertex);
                }

                drawer.square_highlight(*new_vx, Color::SelectedVertex);

                declare_tooltip_string!(vx_coordinates);

                // Draw the tooltip.
                drawer.draw_tooltip_x_centered_above_pos(
                    window,
                    camera,
                    NEW_VX,
                    format!("{} {}", new_vx.x, new_vx.y).as_str(),
                    *new_vx,
                    TOOLTIP_OFFSET,
                    drawer.tooltip_text_color(),
                    drawer.egui_color(Color::SelectedVertex)
                );

                for svx in self.vertexes.iter().filter(|svx| svx.selected)
                {
                    let label = return_if_none!(drawer.vx_tooltip_label(svx.vec));

                    vertex_tooltip(window, camera, drawer, svx.vec, label, &mut vx_coordinates);
                }
            }
        }
    }

    #[inline]
    fn draw_with_vertex_inserted_at_index(
        &self,
        drawer: &mut EditDrawer,
        collision: bool,
        color: Color,
        pos: Vec2,
        index: usize
    )
    {
        drawer.brush(
            NewVertexIterator::new(&self.vertexes, pos, index),
            color,
            self.texture.as_ref(),
            collision
        );
    }

    #[inline]
    pub(in crate::map::brush) fn draw_movement_simulation(
        &self,
        drawer: &mut EditDrawer,
        collision: bool,
        movement_vec: Vec2
    )
    {
        #[inline]
        fn moving_brush<T: TextureInterface>(
            polygon: &ConvexPolygon,
            drawer: &mut EditDrawer,
            texture: Option<&T>,
            collision: bool,
            movement_vec: Vec2
        )
        {
            drawer.brush(
                polygon.vertexes().map(|vx| vx + movement_vec),
                Color::SelectedEntity,
                texture,
                collision
            );
        }

        if let Some(settings) = self.texture_settings()
        {
            let settings = MovingTextureSettings {
                texture: settings,
                delta:   movement_vec
            };

            if settings.sprite()
            {
                moving_brush(self, drawer, None::<&TextureSettings>, collision, movement_vec);
                drawer.sprite(self.center, &settings, Color::SelectedEntity, false);
                self.sprite_highlight(drawer, self.center + movement_vec, &settings);
            }
            else
            {
                moving_brush(self, drawer, Some(&settings), collision, movement_vec);
            }

            return;
        }

        moving_brush(self, drawer, None::<&TextureSettings>, collision, movement_vec);
    }

    #[inline]
    pub(in crate::map::brush) fn draw_map_preview_movement_simulation(
        &self,
        camera: &Transform,
        drawer: &mut MapPreviewDrawer,
        animator: Option<&Animator>,
        movement_vec: Vec2
    )
    {
        let settings = MovingTextureSettings {
            texture: return_if_none!(self.texture_settings()),
            delta:   movement_vec
        };

        if settings.sprite()
        {
            drawer.sprite(self.center, animator, &settings);
            return;
        }

        drawer.brush(camera, self.vertexes().map(|vx| vx + movement_vec), animator, &settings);
    }

    #[inline]
    pub fn draw_sprite_with_highlight(&self, drawer: &mut EditDrawer, color: Color)
    {
        if !self.has_sprite()
        {
            return;
        }

        drawer.sprite(self.center, self.texture_settings().unwrap(), color, false);
        self.sprite_highlight(drawer, self.center, self.texture_settings().unwrap());
    }

    #[inline]
    pub(in crate::map::brush) fn draw_sprite(
        &self,
        drawer: &mut EditDrawer,
        color: Color,
        show_outline: bool
    )
    {
        drawer.sprite(self.center, self.texture_settings().unwrap(), color, show_outline);
    }

    #[inline]
    pub fn draw_sprite_highlight(&self, drawer: &mut EditDrawer)
    {
        self.sprite_highlight(drawer, self.center, self.texture_settings().unwrap());
    }

    #[inline]
    fn sprite_highlight<T: TextureInterfaceExtra>(
        &self,
        drawer: &mut EditDrawer,
        center: Vec2,
        settings: &T
    )
    {
        let pivot = settings.sprite_pivot(self.center).unwrap();

        drawer.square_highlight(center, Color::SpriteAnchor);
        drawer.square_highlight(pivot, Color::SpriteAnchor);
        drawer.line(pivot, center, Color::SpriteAnchor);
        drawer.sprite_highlight(center, Color::SpriteAnchor);
    }

    #[inline]
    pub(in crate::map::brush) fn draw_map_preview_sprite(
        &self,
        drawer: &mut MapPreviewDrawer,
        animator: Option<&Animator>
    )
    {
        drawer.sprite(self.center, animator, self.texture_settings().unwrap());
    }
}

//=======================================================================//

#[derive(Clone)]
struct NewVertexIterator<'a>
{
    vertexes:        &'a [SelectableVector],
    new_vx:          Vec2,
    new_vx_index:    usize,
    new_vx_returned: bool,
    left:            usize,
    right:           usize
}

impl ExactSizeIterator for NewVertexIterator<'_>
{
    #[inline]
    fn len(&self) -> usize
    {
        let len = self.vertexes.len() - self.left;

        if self.new_vx_returned
        {
            return len;
        }

        len + 1
    }
}

impl Iterator for NewVertexIterator<'_>
{
    type Item = Vec2;

    #[inline]
    fn next(&mut self) -> Option<Self::Item>
    {
        if !self.new_vx_returned && self.left == self.new_vx_index
        {
            self.new_vx_returned = true;
            return Some(self.new_vx);
        }

        if self.left == self.right
        {
            return None;
        }

        let value = self.vertexes[self.left].vec.into();
        self.left += 1;
        value
    }
}

impl<'a> NewVertexIterator<'a>
{
    #[inline]
    fn new(vertexes: &'a [SelectableVector], pos: Vec2, index: usize) -> Self
    {
        Self {
            vertexes,
            new_vx: pos,
            new_vx_index: index,
            new_vx_returned: false,
            left: 0,
            right: vertexes.len()
        }
    }
}

//=======================================================================//

struct VertexesSelectionIterMut<'a>(&'a mut Vec<SelectableVector>);

impl VertexesSelectionIterMut<'_>
{
    #[inline]
    fn iter(&mut self) -> impl ExactSizeIterator<Item = (Vec2, &mut bool)>
    {
        self.0.iter_mut().map(|svx| (svx.vec, &mut svx.selected))
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

#[inline]
pub fn vx_tooltip(
    window: &Window,
    camera: &Transform,
    drawer: &EditDrawer,
    pos: Vec2,
    label: &'static str,
    text: &mut String,
    text_color: egui::Color32,
    fill_color: egui::Color32
)
{
    text.clear();
    write!(text, "{}", pos.necessary_precision_value()).ok();

    drawer.draw_tooltip_x_centered_above_pos(
        window,
        camera,
        label,
        text,
        pos,
        TOOLTIP_OFFSET,
        text_color,
        fill_color
    );
}

//=======================================================================//

#[inline]
pub(in crate::map) fn vertex_tooltip(
    window: &Window,
    camera: &Transform,
    drawer: &EditDrawer,
    pos: Vec2,
    label: &'static str,
    text: &mut String
)
{
    vx_tooltip(
        window,
        camera,
        drawer,
        pos,
        label,
        text,
        drawer.tooltip_text_color(),
        drawer.egui_color(Color::SelectedVertex)
    );
}

//=======================================================================//

#[inline]
pub(in crate::map) fn free_draw_tooltip(
    window: &Window,
    camera: &Transform,
    drawer: &EditDrawer,
    pos: Vec2,
    label: &'static str,
    text: &mut String
)
{
    vx_tooltip(
        window,
        camera,
        drawer,
        pos,
        label,
        text,
        drawer.tooltip_text_color(),
        drawer.egui_color(Color::CursorPolygon)
    );
}
