#![allow(dead_code)]

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter, Write},
    ops::Range,
    path::PathBuf
};

use glam::Vec2;
use serde::Deserialize;

use super::prop::PropViewer;
use crate::{
    map::{
        editor::state::test_writer,
        path::Path,
        properties::Properties,
        selectable_vector::SelectableVector,
        version_number,
        FILE_VERSION
    },
    utils::{
        collections::{hv_vec, Ids},
        iterators::TripletIterator,
        math::{
            points::{are_vxs_ccw, vxs_center},
            AroundEqual
        }
    },
    warning_message,
    HvVec,
    Id,
    TextureSettings,
    ThingId
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[must_use]
#[derive(Deserialize)]
pub(in crate::map::editor::state) enum ClipboardData
{
    Brush(BrushData, Id),
    Thing(ThingInstanceData, Id)
}

//=======================================================================//

#[derive(Deserialize)]
enum Group
{
    None,
    Attachments(Ids),
    Path
    {
        path:             Path,
        attached_brushes: Ids
    },
    Attached(Id)
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

#[must_use]
#[derive(Deserialize)]
pub(in crate::map::editor::state) struct Prop
{
    pub data:               HvVec<ClipboardData>,
    pub data_center:        Vec2,
    pub pivot:              Vec2,
    pub attachments_owners: usize,
    pub attached_range:     Range<usize>
}

//=======================================================================//

#[derive(Deserialize)]
struct Hull
{
    top:    f32,
    bottom: f32,
    left:   f32,
    right:  f32
}

impl AroundEqual for Hull
{
    #[inline]
    #[must_use]
    fn around_equal(&self, other: &Self) -> bool
    {
        self.top.around_equal(&other.top) &&
            self.bottom.around_equal(&other.bottom) &&
            self.left.around_equal(&other.left) &&
            self.right.around_equal(&other.right)
    }

    #[inline]
    #[must_use]
    fn around_equal_narrow(&self, other: &Self) -> bool
    {
        self.top.around_equal_narrow(&other.top) &&
            self.bottom.around_equal_narrow(&other.bottom) &&
            self.left.around_equal_narrow(&other.left) &&
            self.right.around_equal_narrow(&other.right)
    }
}

impl Hull
{
    #[inline]
    #[must_use]
    pub fn new(top: f32, bottom: f32, left: f32, right: f32) -> Option<Self>
    {
        if bottom > top || left > right
        {
            return None;
        }

        Self {
            top,
            bottom,
            left,
            right
        }
        .into()
    }

    #[inline]
    #[must_use]
    pub fn from_points(points: impl IntoIterator<Item = Vec2>) -> Self
    {
        let (mut top, mut bottom, mut left, mut right) = (f32::MIN, f32::MAX, f32::MAX, f32::MIN);

        for vx in points
        {
            if vx.y > top
            {
                top = vx.y;
            }

            if vx.y < bottom
            {
                bottom = vx.y;
            }

            if vx.x < left
            {
                left = vx.x;
            }

            if vx.x > right
            {
                right = vx.x;
            }
        }

        Hull::new(top, bottom, left, right).unwrap()
    }
}

//=======================================================================//

#[derive(Deserialize)]
pub(in crate::map::editor::state) struct BrushData
{
    polygon:    ConvexPolygon,
    group:      Group,
    properties: Properties
}

//=======================================================================//

struct ConvexPolygon
{
    vertexes:          HvVec<SelectableVector>,
    center:            Vec2,
    hull:              Hull,
    selected_vertexes: u8,
    texture:           Option<TextureSettings>,
    texture_edited:    bool
}

impl<'de> Deserialize<'de> for ConvexPolygon
{
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>
    {
        const FIELDS: &[&str] = &["vertexes", "texture"];

        enum Field
        {
            Vertexes,
            Texture
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
                        formatter.write_str("`vertexes` or `texture'")
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
                let mut vertexes: Option<HvVec<SelectableVector>> = None;
                let mut texture = None;

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
                        }
                    }
                }

                let vertexes =
                    vertexes.ok_or_else(|| serde::de::Error::missing_field("vertexes"))?;
                let texture = texture.ok_or_else(|| serde::de::Error::missing_field("texture"))?;

                let mut poly = ConvexPolygon::from(vertexes);
                poly.texture = texture;
                Ok(poly)
            }
        }

        deserializer.deserialize_struct("ConvexPolygon", FIELDS, PolygonVisitor)
    }
}

impl From<HvVec<SelectableVector>> for ConvexPolygon
{
    #[inline]
    fn from(vertexes: HvVec<SelectableVector>) -> Self
    {
        assert!(vertexes.len() >= 3, "Not enough vertexes to create a polygon.\n{vertexes:?}.");

        let center = vxs_center(vertexes.iter().map(|svx| svx.vec));
        let hull = Hull::from_points(vertexes.iter().map(|svx| svx.vec));
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

impl ConvexPolygon
{
    #[inline]
    #[must_use]
    fn valid(&self) -> bool
    {
        #[inline]
        pub fn vertexes(poly: &ConvexPolygon) -> impl ExactSizeIterator<Item = Vec2> + Clone + '_
        {
            poly.vertexes.iter().map(|svx| svx.vec)
        }

        if !self.center.around_equal(&vxs_center(vertexes(self))) ||
            !self.hull.around_equal(&Hull::from_points(vertexes(self)))
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
        let len = self.vertexes.len();

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
}

//=======================================================================//

#[derive(Deserialize)]
pub(in crate::map::editor::state) struct ThingInstanceData
{
    thing_id:   ThingId,
    pos:        Vec2,
    hull:       Hull,
    path:       Option<Path>,
    properties: Properties
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

#[inline]
pub(in crate::map::editor::state) fn convert_08_props(
    reader: &mut BufReader<File>,
    len: usize
) -> Result<HvVec<crate::map::editor::state::clipboard::prop::Prop>, &'static str>
{
    let mut props = hv_vec![];

    for _ in 0..len
    {
        props.push(crate::map::editor::state::clipboard::prop::Prop::from(
            ciborium::from_reader::<Prop, _>(&mut *reader)
                .map_err(|_| "Error reading props for conversion.")?
        ));
    }

    Ok(props)
}

//=======================================================================//

#[inline]
pub(in crate::map::editor::state) fn save_imported_08_props(
    writer: &mut BufWriter<&mut Vec<u8>>,
    props: HvVec<crate::map::editor::state::clipboard::prop::Prop>
) -> Result<(), &'static str>
{
    for prop in props.into_iter().map(PropViewer::from)
    {
        test_writer!(&prop, &mut *writer, "Error converting props.");
    }

    Ok(())
}

//=======================================================================//

#[inline]
pub(in crate::map::editor::state) fn convert_08_prps_file(
    mut path: PathBuf,
    len: usize
) -> Result<BufReader<File>, &'static str>
{
    let mut file_name = path.file_stem().unwrap().to_str().unwrap().to_string();
    file_name.push_str("_09.prps");

    warning_message(&format!(
        "This file appears to use the old file structure 0.8, if it is valid it will now be \
         converted to {file_name}."
    ));

    let mut reader = BufReader::new(File::open(&path).unwrap());
    let props = convert_08_props(&mut reader, len)?;
    drop(reader);

    let mut data = Vec::<u8>::new();
    let mut writer = BufWriter::new(&mut data);
    test_writer!(FILE_VERSION, &mut writer, "Error converting props file version.");
    test_writer!(&len, &mut writer, "Error converting props amount.");
    save_imported_08_props(&mut writer, props)?;
    drop(writer);

    test_writer!(
        BufWriter::new(
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&*path)
                .unwrap()
        )
        .write_all(&data),
        "Error saving converted props file."
    );

    path.pop();
    path.push(file_name);

    let mut reader = BufReader::new(File::open(&path).unwrap());
    let _ = version_number(&mut reader);
    let _ = ciborium::from_reader::<usize, _>(&mut reader)
        .map_err(|_| "Error reading converted props length.")?;
    Ok(reader)
}
