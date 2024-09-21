#[cfg(feature = "ui")]
pub(in crate::map) mod compatibility;
#[cfg(feature = "ui")]
pub(in crate::map) mod convex_polygon;
pub mod group;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{Animation, Group, HvHashMap, HvVec, Id, TextureSettings, Value};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// A convex polygon characterized by an optional [`Group`], an optional texture, and certain
/// properties.
#[allow(clippy::unsafe_derive_deserialize)]
#[must_use]
#[derive(Serialize, Deserialize)]
pub struct BrushViewer
{
    /// The [`Id`].
    pub id:         Id,
    /// The vertexes.
    pub vertexes:   HvVec<Vec2>,
    /// The texture.
    pub texture:    Option<TextureSettings>,
    /// The group of brushes this brush belong to.
    pub group:      Group,
    /// Whether collision against the polygonal shape is enabled.
    pub collision:  bool,
    /// The associated properties.
    pub properties: HvHashMap<String, Value>
}

impl BrushViewer
{
    /// Sets the [`Animation`] of the texture.
    #[inline]
    pub(in crate::map) fn set_texture_animation(&mut self, animation: Animation)
    {
        unsafe {
            self.texture.as_mut().unwrap().unsafe_set_animation(animation);
        }
    }
}

//=======================================================================//
// UI
//
//=======================================================================//

#[cfg(feature = "ui")]
pub(in crate::map) mod ui_mod
{
    //=======================================================================//
    // IMPORTS
    //
    //=======================================================================//

    use std::borrow::Cow;

    use arrayvec::ArrayVec;
    use bevy::{transform::components::Transform, window::Window};
    use glam::Vec2;
    use hill_vacuum_shared::{return_if_no_match, return_if_none};
    use serde::{Deserialize, Serialize};

    use crate::{
        map::{
            brush::{
                convex_polygon::{
                    self,
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
                group::{Group, GroupViewer},
                BrushViewer
            },
            drawer::{
                animation::Animator,
                color::Color,
                drawers::{EditDrawer, MapPreviewDrawer},
                drawing_resources::DrawingResources,
                texture::{TextureInterfaceExtra, TextureRotation, TextureScale, TextureSpriteSet}
            },
            editor::state::{
                clipboard::{ClipboardData, CopyToClipboard},
                grid::Grid,
                manager::{Animators, Brushes}
            },
            path::{calc_path_hull, common_edit_path, EditPath, MovementSimulator, Moving, Path},
            properties::{Properties, PropertiesRefactor},
            selectable_vector::VectorSelectionResult,
            thing::catalog::ThingsCatalog
        },
        utils::{
            collections::{hv_vec, Ids},
            hull::{EntityHull, Flip, Hull},
            identifiers::{EntityCenter, EntityId},
            iterators::SlicePairIter,
            math::lines_and_segments::{line_equation, LineEquation}
        },
        Animation,
        HvVec,
        Id,
        TextureInterface,
        TextureSettings,
        Timing,
        Value
    };

    //=======================================================================//
    // MACROS
    //
    //=======================================================================//

    macro_rules! impl_convex_polygon {
        ($tex:ty) => {
            #[allow(dead_code)]
            #[must_use]
            #[derive(Clone, Debug)]
            pub(in crate::map) struct ConvexPolygon
            {
                // Vertexes of the polygon.
                vertexes:          crate::HvVec<crate::map::selectable_vector::SelectableVector>,
                center:            Vec2,
                hull:              crate::utils::hull::Hull,
                selected_vertexes: u8,
                /// How the texture should be mapped.
                texture:           Option<$tex>,
                collision:         bool,
                texture_edited:   bool
            }

            impl serde::Serialize for ConvexPolygon
            {
                #[inline]
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: serde::Serializer
                {
                    use serde::ser::SerializeStruct;

                    let mut s = serializer.serialize_struct("ConvexPolygon", 3)?;
                    s.serialize_field("vertexes", &self.vertexes)?;
                    s.serialize_field("texture", &self.texture)?;
                    s.serialize_field("collision", &self.collision)?;
                    s.end()
                }
            }

            impl<'de> serde::Deserialize<'de> for ConvexPolygon
            {
                #[inline]
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: serde::de::Deserializer<'de>
                {
                    const FIELDS: &[&str] = &["vertexes", "texture", "collision"];

                    enum Field
                    {
                        Vertexes,
                        Texture,
                        Collision
                    }

                    impl<'de> serde::Deserialize<'de> for Field
                    {
                        #[inline]
                        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                        where
                            D: serde::de::Deserializer<'de>
                        {
                            struct FieldVisitor;

                            impl<'de> serde::de::Visitor<'de> for FieldVisitor
                            {
                                type Value = Field;

                                #[inline]
                                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result
                                {
                                    formatter.write_str("`vertexes` or `texture' or 'collision'")
                                }

                                #[inline]
                                fn visit_str<E>(self, value: &str) -> Result<Field, E>
                                where
                                    E: serde::de::Error
                                {
                                    match value
                                    {
                                        "vertexes" => Ok(Field::Vertexes),
                                        "texture" => Ok(Field::Texture),
                                        "collision" => Ok(Field::Collision),
                                        _ => Err(serde::de::Error::unknown_field(value, FIELDS))
                                    }
                                }
                            }

                            deserializer.deserialize_identifier(FieldVisitor)
                        }
                    }

                    struct PolygonVisitor;

                    impl<'de> serde::de::Visitor<'de> for PolygonVisitor
                    {
                        type Value = ConvexPolygon;

                        #[inline]
                        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result
                        {
                            formatter.write_str("struct ConvexPolygon")
                        }

                        #[inline]
                        fn visit_map<V>(self, mut map: V) -> Result<Self::Value, V::Error>
                        where
                            V: serde::de::MapAccess<'de>
                        {
                            let mut vertexes: Option<crate::HvVec<crate::map::selectable_vector::SelectableVector>> = None;
                            let mut texture = None;
                            let mut collision = None;

                            while let Some(key) = map.next_key()?
                            {
                                match key
                                {
                                    Field::Vertexes =>
                                    {
                                        if vertexes.is_some()
                                        {
                                            return Err(serde::de::Error::duplicate_field("vertexes"));
                                        }
                                        vertexes = Some(map.next_value()?);
                                    },
                                    Field::Texture =>
                                    {
                                        if texture.is_some()
                                        {
                                            return Err(serde::de::Error::duplicate_field("texture"));
                                        }
                                        texture = Some(map.next_value()?);
                                    },
                                    Field::Collision =>
                                    {
                                        if collision.is_some()
                                        {
                                            return Err(serde::de::Error::duplicate_field("collision"));
                                        }
                                        collision = Some(map.next_value()?);
                                    }
                                }
                            }

                            let vertexes =
                                vertexes.ok_or_else(|| serde::de::Error::missing_field("vertexes"))?;
                            let texture = texture.ok_or_else(|| serde::de::Error::missing_field("texture"))?;
                            let collision =
                                collision.ok_or_else(|| serde::de::Error::missing_field("collision"))?;

                            let mut poly = <ConvexPolygon>::new(vertexes.into_iter().map(|svec| svec.vec));
                            poly.texture = texture;
                            poly.collision = collision;
                            Ok(poly)
                        }
                    }

                    deserializer.deserialize_struct("ConvexPolygon", FIELDS, PolygonVisitor)
                }
            }

            impl From<crate::HvVec<Vec2>> for ConvexPolygon
            {
                #[inline]
                fn from(vertexes: crate::HvVec<Vec2>) -> Self
                {
                    crate::utils::collections::hv_vec![collect; vertexes.into_iter().map(crate::map::selectable_vector::SelectableVector::new)].into()
                }
            }

            impl From<crate::HvVec<crate::map::selectable_vector::SelectableVector>> for ConvexPolygon
            {
                #[inline]
                fn from(vertexes: crate::HvVec<crate::map::selectable_vector::SelectableVector>) -> Self
                {
                    assert!(vertexes.len() >= 3, "Not enough vertexes to create a polygon.\n{vertexes:?}.");

                    let center = crate::utils::math::points::vxs_center(vertexes.iter().map(|svx| svx.vec));
                    let hull = crate::utils::hull::Hull::from_points(vertexes.iter().map(|svx| svx.vec)).unwrap();
                    let selected_vertexes = vertexes.iter().fold(0, |add, svx| add + u8::from(svx.selected));
                    let cp = Self {
                        vertexes,
                        center,
                        hull,
                        selected_vertexes,
                        texture: None,
                        collision: true,
                        texture_edited: false
                    };

                    assert!(cp.valid(), "Invalid vertexes.\n{cp:?}");

                    cp
                }
            }

            impl ConvexPolygon
            {
                #[inline]
                pub fn new<T>(vxs: T) -> Self
                where
                    T: Iterator<Item = Vec2>
                {
                    crate::utils::collections::hv_vec![collect; vxs].into()
                }

                /// Returns true if vxs represents a valid polygon.
                #[inline]
                #[must_use]
                fn valid(&self) -> bool
                {
                    use crate::utils::math::AroundEqual;

                    if !self.center.around_equal(&crate::utils::math::points::vxs_center(self.vertexes())) ||
                        !self.hull.around_equal(&crate::utils::hull::Hull::from_points(self.vertexes()).unwrap())
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

                    self.vxs_valid()
                }

                #[inline]
                #[must_use]
                fn vxs_valid(&self) -> bool
                {
                    use crate::utils::{math::{AroundEqual, points::are_vxs_ccw}, iterators::TripletIterator};

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
            }
        };
    }

    pub(in crate::map::brush) use impl_convex_polygon;

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

    macro_rules! from_compat {
        ($($v:ident),+) => { $(
            impl From<crate::map::brush::compatibility::$v::Brush> for Brush
            {
                #[inline]
                fn from(value: crate::map::brush::compatibility::$v::Brush) -> Self
                {
                    let crate::map::brush::compatibility::$v::Brush { id, data } = value;

                    let mut polygon = ConvexPolygon::from(hv_vec![collect; data.polygon.vertexes()]);
                    _ = polygon.set_collision(data.polygon.collision());

                    if let Some(tex) = data.polygon.take_texture_settings()
                    {
                        polygon.set_texture_settings(TextureSettings::from(tex));
                    }

                    Self {
                        data: BrushData {
                            polygon,
                            group: Group::from(data.mover),
                            properties: data.properties
                        },
                        id
                    }
                }
            }
        )+};
    }

    //=======================================================================//

    macro_rules! impl_convex_polygon_ui {
        () => {
            #[cfg(feature = "ui")]
            pub(in crate::map::brush) mod ui_mod
            {
                impl super::ConvexPolygon
                {
                    /// Returns an iterator to the vertexes of the polygon.
                    #[inline]
                    #[must_use]
                    pub const fn collision(&self) -> bool { self.collision }

                    #[inline]
                    pub fn take_texture_settings(self) -> Option<super::TextureSettings>
                    {
                        self.texture
                    }
                }
            }
        };
    }

    pub(in crate::map::brush) use impl_convex_polygon_ui;

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
                VertexesMoveResult::Valid(value) =>
                {
                    Self::Valid(VertexesMovePayload(brush.id(), value))
                },
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
                SidesDeletionResult::Valid(vecs) =>
                {
                    Self::Valid(SidesDeletionPayload(identifier, vecs))
                },
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
                    texture_scale
                } =>
                {
                    if brush.path_hull_out_of_bounds(new_center)
                    {
                        return Self::Invalid;
                    }

                    Self::Valid(ScalePayload(brush.id, vxs, texture_scale))
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
                    texture_rotation
                } =>
                {
                    if brush.path_hull_out_of_bounds(new_center)
                    {
                        return Self::Invalid;
                    }

                    Self::Valid(RotatePayload(brush.id, vxs, texture_rotation))
                }
            }
        }
    }

    //=======================================================================//
    // STRUCTS
    //
    //=======================================================================//

    #[must_use]
    #[derive(Debug)]
    pub(in crate::map) struct ClipResult
    {
        pub id:    Id,
        pub left:  ConvexPolygon,
        pub right: ConvexPolygon
    }

    //=======================================================================//

    #[must_use]
    pub(in crate::map) struct HollowResult
    {
        pub id:    Id,
        pub main:  ConvexPolygon,
        pub walls: HvVec<ConvexPolygon>
    }

    //=======================================================================//

    #[must_use]
    pub(in crate::map) struct ShatterResult
    {
        pub main:   ConvexPolygon,
        pub shards: HvVec<ConvexPolygon>
    }

    //=======================================================================//

    #[must_use]
    #[derive(Debug)]
    pub(in crate::map) struct RotatePayload(Id, HvVec<Vec2>, Option<TextureRotation>);

    //=======================================================================//

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub(in crate::map) struct BrushData
    {
        /// The polygon of the brush.
        polygon:    ConvexPolygon,
        /// Platform path and attached brushes.
        group:      Group,
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
        pub fn sprite_hull(&self, drawing_resources: &DrawingResources) -> Option<Hull>
        {
            self.polygon.sprite_hull(drawing_resources)
        }

        #[inline]
        #[must_use]
        pub fn path_hull(&self) -> Option<Hull>
        {
            calc_path_hull(
                return_if_no_match!(&self.group, Group::Path { path, .. }, path, None),
                self.polygon.center()
            )
            .into()
        }

        #[inline]
        #[must_use]
        pub const fn has_path(&self) -> bool { self.group.has_path() }

        #[inline]
        #[must_use]
        pub fn has_attachments(&self) -> bool { self.group.has_attachments() }

        #[inline]
        #[must_use]
        pub const fn is_attached(&self) -> bool { self.group.is_attached().is_some() }

        #[inline]
        #[must_use]
        pub fn contains_attachment(&self, identifier: Id) -> bool
        {
            match self.group.attachments()
            {
                Some(ids) => ids.contains(&identifier),
                None => false
            }
        }

        #[inline]
        pub const fn attachments(&self) -> Option<&Ids> { self.group.attachments() }

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
        pub fn insert_attachment(&mut self, identifier: Id)
        {
            self.group.insert_attachment(identifier);
        }

        #[inline]
        pub fn remove_attachment(&mut self, identifier: Id)
        {
            self.group.remove_attachment(identifier);
        }

        #[inline]
        pub fn detach(&mut self)
        {
            assert!(self.is_attached(), "Tried to detach brush that is not attached.");
            self.group = Group::None;
        }

        #[inline]
        pub fn draw_prop(
            &self,
            camera: &Transform,
            drawer: &mut EditDrawer,
            color: Color,
            delta: Vec2
        )
        {
            self.polygon.draw_prop(camera, drawer, color, delta);

            return_if_no_match!(&self.group, Group::Path { path, .. }, path)
                .draw_prop(drawer, self.polygon.center() + delta);
        }
    }

    //=======================================================================//

    /// The entity representing one of the shapes that make the maps, as saved in the .hv files.
    #[must_use]
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub(in crate::map) struct Brush
    {
        // The id of the brush.
        id:   Id,
        data: BrushData
    }

    from_compat!(_03, _04);

    impl From<BrushViewer> for Brush
    {
        #[inline]
        fn from(value: BrushViewer) -> Self
        {
            let BrushViewer {
                id,
                vertexes,
                texture,
                group,
                collision,
                properties
            } = value;

            let mut polygon = ConvexPolygon::from(vertexes);

            if let Some(tex) = texture
            {
                polygon.set_texture_settings(tex);
            }
            _ = polygon.set_collision(collision);

            Self {
                id,
                data: BrushData {
                    polygon,
                    group: group.into(),
                    properties: Properties::from_parts(properties)
                }
            }
        }
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
        fn path(&self) -> Option<&Path> { self.data.group.path() }

        #[inline]
        fn has_path(&self) -> bool { self.data.group.has_path() }

        #[inline]
        fn possible_moving(&self) -> bool
        {
            matches!(self.data.group, Group::None | Group::Attachments(_))
        }

        #[inline]
        fn draw_highlighted_with_path_nodes(
            &self,
            window: &Window,
            camera: &Transform,
            brushes: Brushes,
            _: &ThingsCatalog,
            drawer: &mut EditDrawer
        )
        {
            self.draw_with_color(camera, drawer, Color::HighlightedSelectedEntity);
            self.path().unwrap().draw(window, camera, drawer, self.center());
            self.draw_attached_brushes(camera, brushes, drawer, Self::draw_highlighted_selected);
        }

        #[inline]
        fn draw_with_highlighted_path_node(
            &self,
            window: &Window,
            camera: &Transform,
            brushes: Brushes,
            _: &ThingsCatalog,
            drawer: &mut EditDrawer,
            highlighted_node: usize
        )
        {
            self.draw_with_color(camera, drawer, Color::HighlightedSelectedEntity);
            self.path().unwrap().draw_with_highlighted_path_node(
                window,
                camera,
                drawer,
                self.center(),
                highlighted_node
            );
            self.draw_attached_brushes(camera, brushes, drawer, Self::draw_selected);
        }

        #[inline]
        fn draw_with_path_node_addition(
            &self,
            window: &Window,
            camera: &Transform,
            brushes: Brushes,
            _: &ThingsCatalog,
            drawer: &mut EditDrawer,
            pos: Vec2,
            index: usize
        )
        {
            self.draw_with_color(camera, drawer, Color::HighlightedSelectedEntity);
            self.path().unwrap().draw_with_node_insertion(
                window,
                camera,
                drawer,
                pos,
                index,
                self.center()
            );
            self.draw_attached_brushes(camera, brushes, drawer, Self::draw_selected);
        }

        #[inline]
        fn draw_movement_simulation(
            &self,
            window: &Window,
            camera: &Transform,
            brushes: Brushes,
            _: &ThingsCatalog,
            drawer: &mut EditDrawer,
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
                drawer,
                center,
                movement_vec
            );

            let attachments = return_if_none!(self.attachments_iter());
            let center = center + movement_vec;

            for id in attachments
            {
                let a_center = brushes.get(*id).center() + movement_vec;
                drawer.square_highlight(a_center, Color::BrushAnchor);
                drawer.line(a_center, center, Color::BrushAnchor);

                brushes.get(*id).data.polygon.draw_movement_simulation(
                    camera,
                    drawer,
                    movement_vec
                );
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
                animators.get_brush_animator(self.id),
                movement_vec
            );
            let attachments = return_if_none!(self.attachments_iter());

            for id in attachments
            {
                brushes.get(*id).data.polygon.draw_map_preview_movement_simulation(
                    camera,
                    drawer,
                    animators.get_brush_animator(*id),
                    movement_vec
                );
            }
        }
    }

    impl EditPath for Brush
    {
        common_edit_path!();

        #[inline]
        fn set_path(&mut self, path: Path) { self.data.group.set_path(path); }

        #[inline]
        fn take_path(&mut self) -> Path { self.data.group.take_path() }
    }

    impl From<crate::map::brush::compatibility::_061::BrushViewer> for Brush
    {
        #[inline]
        fn from(value: crate::map::brush::compatibility::_061::BrushViewer) -> Self
        {
            let crate::map::brush::compatibility::_061::BrushViewer {
                id,
                vertexes,
                texture,
                mover,
                collision,
                properties
            } = value;

            Brush::from(BrushViewer {
                id,
                vertexes,
                texture,
                group: GroupViewer::from(mover),
                collision,
                properties
            })
        }
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
                            group: Group::None,
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
                            group: Group::None,
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
                group,
                properties
            } = data;
            let mut brush = Self::from_polygon(polygon, identifier, properties);

            if let Group::Attached(owner) = group
            {
                assert!(owner != identifier, "Owner ID {owner:?} is equal to the Brush ID");
            }

            brush.data.group = group;
            brush
        }

        #[inline]
        pub fn into_parts(self) -> (BrushData, Id) { (self.data, self.id) }

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
        pub fn attachments_anchors_hull(&self, brushes: Brushes) -> Option<Hull>
        {
            if !self.data.group.has_attachments()
            {
                return None;
            }

            Hull::from_points(self.attachments_iter().unwrap().map(|id| brushes.get(*id).center()))
                .map(|hull| {
                    let center = self.center();
                    hull.merged(&Hull::new(center.y, center.y, center.x, center.x))
                        .bumped(2f32)
                })
        }

        #[inline]
        #[must_use]
        pub fn sprite_hull(&self, drawing_resources: &DrawingResources) -> Option<Hull>
        {
            self.data.polygon.sprite_hull(drawing_resources)
        }

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
        pub fn sprite_and_anchor_hull(&self, drawing_resources: &DrawingResources) -> Option<Hull>
        {
            self.sprite_hull(drawing_resources)
                .map(|hull| hull.merged(&self.sprite_anchor_hull().unwrap()))
        }

        #[inline]
        #[must_use]
        pub fn global_hull(&self, drawing_resources: &DrawingResources) -> Hull
        {
            let mut hull = self.hull();

            if let Some(s_hull) = self.sprite_hull(drawing_resources)
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

        /// Moves the `Brush` by the amount delta.
        #[inline]
        pub fn check_move(
            &self,
            drawing_resources: &DrawingResources,
            delta: Vec2,
            move_texture: bool
        ) -> bool
        {
            self.data.polygon.check_move(drawing_resources, delta, move_texture) &&
                !self.path_hull_out_of_bounds(self.center() + delta)
        }

        #[inline]
        pub fn check_texture_move(&self, drawing_resources: &DrawingResources, delta: Vec2)
            -> bool
        {
            !self.has_texture() || self.data.polygon.check_texture_move(drawing_resources, delta)
        }

        /// Moves the `Brush` by the amount delta.
        #[inline]
        pub fn move_by_delta(&mut self, delta: Vec2, move_texture: bool)
        {
            self.data.polygon.move_by_delta(delta, move_texture);
        }

        #[inline]
        pub fn move_texture(&mut self, delta: Vec2) { self.data.polygon.move_texture(delta); }

        /// Moves the `Brush` by the amount delta.
        #[inline]
        pub fn move_polygon(&mut self, delta: Vec2, move_texture: bool)
        {
            self.data.polygon.move_by_delta(delta, move_texture);
        }

        /// Swaps the polygon of `self` and `other`.
        #[inline]
        pub fn swap_polygon(&mut self, polygon: &mut ConvexPolygon)
        {
            self.data.polygon.swap_polygon(polygon);
        }

        #[inline]
        pub fn set_polygon(&mut self, polygon: ConvexPolygon) -> ConvexPolygon
        {
            self.data.polygon.set_polygon(polygon)
        }

        //==============================================================
        // Snap

        #[inline]
        #[must_use]
        fn snap<F>(&mut self, grid: Grid, f: F) -> Option<HvVec<(HvVec<u8>, Vec2)>>
        where
            F: Fn(&mut ConvexPolygon, Grid) -> Option<HvVec<(HvVec<u8>, Vec2)>>
        {
            f(&mut self.data.polygon, grid)
        }

        #[inline]
        #[must_use]
        pub fn snap_vertexes(&mut self, grid: Grid) -> Option<HvVec<(HvVec<u8>, Vec2)>>
        {
            self.snap(grid, ConvexPolygon::snap_vertexes)
        }

        #[inline]
        #[must_use]
        pub fn snap_selected_vertexes(&mut self, grid: Grid) -> Option<HvVec<(HvVec<u8>, Vec2)>>
        {
            self.snap(grid, ConvexPolygon::snap_selected_vertexes)
        }

        #[inline]
        #[must_use]
        pub fn snap_selected_sides(&mut self, grid: Grid) -> Option<HvVec<(HvVec<u8>, Vec2)>>
        {
            self.snap(grid, ConvexPolygon::snap_selected_sides)
        }

        //==============================================================
        // Anchors

        #[inline]
        #[must_use]
        pub fn has_attachments(&self) -> bool { self.data.group.has_attachments() }

        #[inline]
        #[must_use]
        pub fn attachable(&self) -> bool { !(self.has_attachments() || self.has_path()) }

        #[inline]
        pub fn attachments_iter(&self) -> Option<impl ExactSizeIterator<Item = &Id> + Clone>
        {
            self.data.group.attachments_iter()
        }

        #[inline]
        #[must_use]
        pub const fn attached(&self) -> Option<Id> { self.data.group.is_attached() }

        #[inline]
        pub fn insert_attachment(&mut self, attachment: &Self)
        {
            assert!(
                self.id != attachment.id,
                "Brush ID {:?} is equal to the attachment's ID",
                self.id
            );
            self.data.group.insert_attachment(attachment.id);
        }

        #[inline]
        pub fn attach_brush(&mut self, attachment: &mut Self)
        {
            self.insert_attachment(attachment);
            attachment.attach(self.id);
        }

        #[inline]
        pub fn remove_attachment(&mut self, attachment: &Self)
        {
            assert!(
                self.id != attachment.id,
                "Brush ID {:?} is equal to the attachment's ID",
                self.id
            );
            self.data.group.remove_attachment(attachment.id);
        }

        #[inline]
        pub fn detach_brush(&mut self, attachment: &mut Self)
        {
            self.remove_attachment(attachment);
            attachment.detach();
        }

        #[inline]
        pub fn attach(&mut self, identifier: Id)
        {
            assert!(matches!(self.data.group, Group::None), "Brush Mover is not None");
            self.data.group = Group::Attached(identifier);
        }

        #[inline]
        pub fn detach(&mut self)
        {
            assert!(matches!(self.data.group, Group::Attached(_)), "Brush is not attached.");
            self.data.group = Group::None;
        }

        //==============================================================
        // Path

        #[inline]
        #[must_use]
        pub const fn no_path_nor_attached(&self) -> bool
        {
            matches!(self.data.group, Group::None | Group::Attachments(_))
        }

        #[inline]
        pub fn take_mover(&mut self) -> Option<Group>
        {
            if matches!(self.data.group, Group::None)
            {
                return None;
            }

            std::mem::take(&mut self.data.group).into()
        }

        #[inline]
        fn path_mut(&mut self) -> &mut Path { self.data.group.path_mut() }

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
            Animator::new(self.texture_settings().unwrap().overall_animation(drawing_resources))
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

        #[allow(clippy::unused_self)]
        #[inline]
        #[must_use]
        pub const fn check_texture_scroll_x(&self, _: &DrawingResources, _: f32) -> bool { true }

        #[allow(clippy::unused_self)]
        #[inline]
        #[must_use]
        pub const fn check_texture_scroll_y(&self, _: &DrawingResources, _: f32) -> bool { true }

        #[allow(clippy::unused_self)]
        #[inline]
        #[must_use]
        pub const fn check_texture_parallax_x(&self, _: &DrawingResources, _: f32) -> bool { true }

        #[allow(clippy::unused_self)]
        #[inline]
        #[must_use]
        pub const fn check_texture_parallax_y(&self, _: &DrawingResources, _: f32) -> bool { true }

        #[allow(clippy::unused_self)]
        #[inline]
        #[must_use]
        pub const fn check_texture_height(&self, _: &DrawingResources, _: i8) -> bool { true }

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
        pub fn set_texture_offset_x(&mut self, value: f32) -> Option<f32>
        {
            self.data.polygon.set_texture_offset_x(value)
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
        pub fn set_texture_offset_y(&mut self, value: f32) -> Option<f32>
        {
            self.data.polygon.set_texture_offset_y(value)
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
        pub fn set_texture_scale_x(&mut self, value: f32) -> Option<f32>
        {
            self.data.polygon.set_texture_scale_x(value)
        }

        #[inline]
        pub fn flip_texture_scale_x(&mut self) { self.data.polygon.flip_texture_scale_x(); }

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
        pub fn set_texture_scale_y(&mut self, value: f32) -> Option<f32>
        {
            self.data.polygon.set_texture_scale_y(value)
        }

        #[inline]
        pub fn flip_scale_y(&mut self) { self.data.polygon.flip_texture_scale_y(); }

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
        pub fn check_texture_angle(
            &mut self,
            drawing_resources: &DrawingResources,
            value: f32
        ) -> bool
        {
            self.data.polygon.check_texture_angle(drawing_resources, value)
        }

        #[inline]
        #[must_use]
        pub fn set_texture_angle(&mut self, value: f32) -> Option<f32>
        {
            self.data.polygon.set_texture_angle(value)
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
        pub fn set_texture_sprite(&mut self, value: bool) -> Option<TextureSpriteSet>
        {
            self.data.polygon.set_texture_sprite(value)
        }

        #[inline]
        pub fn undo_redo_texture_sprite(&mut self, value: &mut TextureSpriteSet)
        {
            self.data.polygon.undo_redo_texture_sprite(value);
        }

        #[inline]
        #[must_use]
        pub fn check_texture_within_bounds(&mut self, drawing_resources: &DrawingResources)
            -> bool
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
        pub fn set_texture_animation(&mut self, animation: Animation) -> Animation
        {
            self.data.polygon.set_texture_animation(animation)
        }

        #[inline]
        pub fn set_texture_list_animation(&mut self, texture: &str) -> Animation
        {
            self.data.polygon.set_texture_list_animation(texture)
        }

        #[inline]
        pub fn generate_list_animation(&mut self) -> Animation
        {
            self.data.polygon.generate_list_animation()
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
        pub fn set_texture_atlas_animation_x_partition(&mut self, value: u32) -> Option<u32>
        {
            self.data.polygon.set_atlas_animation_x_partition(value)
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
        pub fn set_texture_atlas_animation_y_partition(&mut self, value: u32) -> Option<u32>
        {
            self.data.polygon.set_atlas_animation_y_partition(value)
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
        pub fn set_list_animation_texture(&mut self, index: usize, texture: &str)
            -> Option<String>
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
        pub const fn has_selected_vertexes(&self) -> bool
        {
            self.data.polygon.has_selected_vertexes()
        }

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

        /// Returns a `VertexSelectionResult` describing the state of the `SelectableVertex` closest
        /// to `cursor_pos` found. If a `SelectableVertex` is found and it is not selected,
        /// it is selected, but the function still returns
        /// `VertexSelectionResult::NotSelected`.
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
        pub fn vertex_at_index(&self, index: usize) -> Vec2
        {
            self.data.polygon.vertex_at_index(index)
        }

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

        /// Adds a vertex to the polygon if it's possible to do so without losing convexity and
        /// returns whether it was possible to do so.
        #[inline]
        #[must_use]
        pub fn try_vertex_insertion_at_index(
            &mut self,
            pos: Vec2,
            index: usize,
            selected: bool
        ) -> bool
        {
            self.data.polygon.try_vertex_insertion_at_index(pos, index, selected)
        }

        /// Inserts a new vertex with position `pos` at `index`.
        #[inline]
        pub fn insert_vertex_at_index(&mut self, pos: Vec2, index: usize, selected: bool)
        {
            self.data.polygon.insert_vertex_at_index(pos, index, selected);
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
        pub fn delete_vertex_at_index(&mut self, index: usize)
        {
            self.data.polygon.delete_vertex_at_index(index);
        }

        /// Returns a [`VertexesDeletionResult`] describing the outcome of the deletion of the
        /// selected vertexes.
        #[inline]
        pub fn check_selected_vertexes_deletion(&self) -> VertexesDeletionResult
        {
            self.data.polygon.check_selected_vertexes_deletion()
        }

        /// Tries to remove the selected `SelectableVertexes`, does nothing if the
        /// result `ConvexPolygon` would have less than 3 sides.
        #[inline]
        pub fn delete_selected_vertexes(&mut self) -> Option<HvVec<(Vec2, u8)>>
        {
            self.data.polygon.delete_selected_vertexes()
        }

        /// Moves the selected `SelectableVertexes` by the amount `delta`.
        #[inline]
        pub fn check_selected_vertexes_move(&mut self, delta: Vec2) -> VertexesMoveResult
        {
            VertexesMoveResult::from_result(
                self.data.polygon.check_selected_vertexes_move(delta),
                self
            )
        }

        /// Applies the vertexes move described by `payload`.
        #[inline]
        pub fn apply_vertexes_move_result(&mut self, payload: VertexesMovePayload) -> VertexesMove
        {
            assert!(
                payload.0 == self.id,
                "VertexesMovePayload's ID is not equal to the Brush's ID."
            );
            self.redo_vertexes_move(&payload.1);
            payload.1
        }

        /// Undoes a vertexes move.
        #[inline]
        pub fn undo_vertexes_move(&mut self, vxs_move: &VertexesMove)
        {
            let old_center = self.center();
            self.data.polygon.undo_vertexes_move(vxs_move);

            if !self.has_path()
            {
                return;
            }

            let center = self.center();
            self.path_mut().translate(old_center - center);
        }

        /// Redoes a vertexes move.
        #[inline]
        pub fn redo_vertexes_move(&mut self, vxs_move: &VertexesMove)
        {
            let old_center = self.center();
            self.data.polygon.apply_vertexes_move_result(vxs_move);

            if !self.has_path()
            {
                return;
            }

            let center = self.center();
            self.path_mut().translate(old_center - center);
        }

        /// Returns a [`SplitResult`] describing whether the polygon can be split.
        #[inline]
        pub fn check_split(&self) -> SplitResult
        {
            (self.data.polygon.check_split(), self.id).into()
        }

        /// Splits the polygon in two halves based on the `payload`.
        #[inline]
        pub fn split(&mut self, payload: &SplitPayload) -> ConvexPolygon
        {
            assert!(payload.0 == self.id, "SplitPayload's ID is not equal to the Brush's ID.");
            self.data.polygon.split(&payload.1)
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
        pub fn nearby_side(&self, cursor_pos: Vec2, camera_scale: f32)
            -> Option<([Vec2; 2], usize)>
        {
            self.data.polygon.nearby_side(cursor_pos, camera_scale)
        }

        /// Returns a `VertexSelectionResult` describing the state of the closest to
        /// `cursor_pos` side found, if any. If a side is found and it is not
        /// selected, it is selected, but the function still returns
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

        /// The information required to start an xtrusion attempt based on the side near the cursor,
        /// if any.
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

        /// Returns a [`XtrusionResult`] describing whether the xtrusion attempt can occur.
        #[inline]
        pub fn matching_xtrusion_info(&self, normal: Vec2) -> XtrusionResult
        {
            (self.data.polygon.matching_xtrusion_info(normal), self.id).into()
        }

        /// Tries to select the side with the same coordinates a `side`, and returns the index of
        /// the selected side, if any.
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
            SidesDeletionResult::from_result(
                self.data.polygon.check_selected_sides_deletion(),
                self.id
            )
        }

        /// Tries to remove the selected sides as long as the resulting
        /// `ConvexPolygon` has at least 3 sides.
        #[inline]
        pub fn delete_selected_sides(
            &mut self,
            payload: SidesDeletionPayload
        ) -> HvVec<(Vec2, u8, bool)>
        {
            assert!(
                payload.id() == self.id,
                "SidesDeletionPayload's ID is not equal to the Brush's ID."
            );
            self.data
                .polygon
                .delete_selected_sides(payload.1.iter().rev().map(|(_, idx, _)| *idx as usize));
            payload.1
        }

        /// Moves the selected lines by the amount `delta`.
        #[inline]
        pub fn check_selected_sides_move(&mut self, delta: Vec2) -> VertexesMoveResult
        {
            VertexesMoveResult::from_result(
                self.data.polygon.check_selected_sides_move(delta),
                self
            )
        }

        //==============================================================
        // Clip

        /// Splits the underlying `ConvexPolygon` in two if `clip_line` crosses its
        /// shape. Returns the polygon generated by the clip, if any.
        #[inline]
        #[must_use]
        pub fn clip(&self, clip_line: &[Vec2; 2]) -> Option<ClipResult>
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
                LineEquation::Vertical(x) if !(hull.left()..hull.right()).contains(&x) =>
                {
                    return None
                },
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

            self.data.polygon.clip(clip_line).map(|[left, right]| {
                ClipResult {
                    id: self.id,
                    left,
                    right
                }
            })
        }

        //==============================================================
        // Shatter

        /// Shatters the underlying `ConvexPolygon` in triangles depending on the
        /// position of `cursor_pos` with respect to the polygon's shape.
        #[inline]
        pub fn shatter(&self, cursor_pos: Vec2, camera_scale: f32) -> Option<ShatterResult>
        {
            self.data.polygon.shatter(cursor_pos, camera_scale)
        }

        //==============================================================
        // Hollow

        /// Returns the four wall brushes generated from the shape of `self`, if any.
        #[inline]
        pub fn hollow(&self, grid_size: f32) -> Option<HollowResult>
        {
            self.data.polygon.hollow(grid_size).map(|result| {
                HollowResult {
                    id:    self.id,
                    main:  result.main,
                    walls: result.walls
                }
            })
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

        /// Returns a [`SubtractResult`] describing the outcome of the subtraction of `other`'s
        /// shape from `self`'s.
        #[inline]
        pub fn subtract(&self, other: &Self) -> SubtractResult
        {
            self.data.polygon.subtract(&other.data.polygon)
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
                self.data.polygon.check_flip_scale(
                    drawing_resources,
                    info,
                    flip_queue,
                    scale_texture
                ),
                self
            )
        }

        /// Scales `self` based on `payload`.
        #[inline]
        pub fn set_scale_coordinates(&mut self, payload: ScalePayload)
        {
            assert!(payload.id() == self.id, "ScalePayload's ID is not equal to the Brush's ID.");
            self.data.polygon.set_coordinates(payload.1.into_iter());
            self.data.polygon.scale_texture(&return_if_none!(payload.2));
        }

        //==============================================================
        // Shear

        /// Returns a [`ShearResult`] describing the validity of the vertical shear.
        #[inline]
        pub fn check_horizontal_shear(
            &self,
            drawing_resources: &DrawingResources,
            info: &ShearInfo
        ) -> ShearResult
        {
            ShearResult::from_result(
                self.data.polygon.check_horizontal_shear(drawing_resources, info),
                self
            )
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
        pub fn check_vertical_shear(
            &self,
            drawing_resources: &DrawingResources,
            info: &ShearInfo
        ) -> ShearResult
        {
            ShearResult::from_result(
                self.data.polygon.check_vertical_shear(drawing_resources, info),
                self
            )
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
        pub fn check_rotation(
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

        #[inline]
        pub fn check_texture_rotation(
            &mut self,
            drawing_resources: &DrawingResources,
            pivot: Vec2,
            angle: f32
        ) -> Option<TextureRotation>
        {
            self.data
                .polygon
                .check_texture_rotation(drawing_resources, pivot, angle)
        }

        /// Rotates `self` based on `payload`.
        #[inline]
        pub fn set_rotation_coordinates(&mut self, mut payload: RotatePayload)
        {
            assert!(payload.id() == self.id, "RotatePayload's ID is not equal to the Brush's ID.");
            self.data.polygon.set_coordinates(payload.1.into_iter());
            self.rotate_texture(return_if_none!(&mut payload.2));
        }

        #[inline]
        pub fn rotate_texture(&mut self, payload: &mut TextureRotation)
        {
            self.data.polygon.rotate_texture(payload);
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
            hgl_mode: &VertexHighlightMode
        )
        {
            self.data
                .polygon
                .draw_with_vertex_highlight(window, camera, drawer, hgl_mode);
        }

        /// Draws the polygon with a solid color.
        #[inline]
        pub fn draw_with_solid_color(&self, drawer: &mut EditDrawer, color: Color)
        {
            drawer.polygon_with_solid_color(self.vertexes(), color);
        }

        /// Draws the attachments connecting the center of `self` to the centers of the attached
        /// brushes.
        #[inline]
        pub fn draw_anchors(&self, brushes: Brushes, drawer: &mut EditDrawer)
        {
            let start = self.center();
            let attachments = return_if_none!(self.attachments_iter());
            drawer.square_highlight(start, Color::BrushAnchor);
            drawer.attachment_highlight(start, Color::BrushAnchor);

            for id in attachments
            {
                let end = brushes.get(*id).center();
                drawer.square_highlight(end, Color::BrushAnchor);
                drawer.line(start, end, Color::BrushAnchor);
            }
        }

        /// Draws the attached brushes based on `f`.
        #[inline]
        fn draw_attached_brushes<F>(
            &self,
            camera: &Transform,
            brushes: Brushes,
            drawer: &mut EditDrawer,
            f: F
        ) where
            F: Fn(&Self, &Transform, &mut EditDrawer)
        {
            for brush in self.data.group.attachments_iter().unwrap().map(|id| brushes.get(*id))
            {
                f(brush, camera, drawer);
            }
        }

        /// Draws the sprite.
        #[inline]
        pub fn draw_sprite(&self, drawer: &mut EditDrawer, color: Color, show_outline: bool)
        {
            self.data.polygon.draw_sprite(drawer, color, show_outline);
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

    impl BrushViewer
    {
        /// Returns a new [`crate::Brush`].
        #[inline]
        pub(in crate::map) fn new(brush: Brush) -> Self
        {
            let Brush {
                data:
                    BrushData {
                        polygon,
                        group,
                        properties
                    },
                id
            } = brush;
            let collision = polygon.collision();

            Self {
                id,
                vertexes: hv_vec![collect; polygon.vertexes()],
                texture: polygon.take_texture_settings(),
                group: group.into(),
                collision,
                properties: properties.take()
            }
        }
    }
}

#[cfg(feature = "ui")]
pub(in crate::map) use ui_mod::*;
