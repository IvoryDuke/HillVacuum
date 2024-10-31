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

use crate::{
    map::{
        brush::compatibility::BrushDataViewer,
        editor::state::test_writer,
        thing::compatibility::ThingInstanceDataViewer,
        version_number,
        FILE_VERSION
    },
    warning_message,
    Id
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[must_use]
#[derive(Deserialize)]
pub(in crate::map) struct PropViewer
{
    pub entities:         Vec<ClipboardDataViewer>,
    pub attached_brushes: Range<usize>,
    pub pivot:            Vec2,
    pub center:           Vec2
}

#[must_use]
#[derive(Deserialize)]
pub(in crate::map) enum ClipboardDataViewer
{
    /// A brush.
    Brush(BrushDataViewer, Id),
    /// A [`ThingInstance`].
    Thing(ThingInstanceDataViewer, Id)
}

impl From<ClipboardDataViewer> for super::ClipboardDataViewer
{
    #[inline]
    fn from(value: ClipboardDataViewer) -> Self
    {
        match value
        {
            ClipboardDataViewer::Brush(brush_data_viewer, id) =>
            {
                super::ClipboardDataViewer::Brush(brush_data_viewer.into(), id)
            },
            ClipboardDataViewer::Thing(thing_instance_data_viewer, id) =>
            {
                super::ClipboardDataViewer::Thing(thing_instance_data_viewer.into(), id)
            },
        }
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

#[inline]
pub(in crate::map::editor::state) fn convert_09_props(
    reader: &mut BufReader<File>,
    len: usize
) -> Result<Vec<crate::map::editor::state::clipboard::prop::PropViewer>, &'static str>
{
    let mut props = vec![];

    for _ in 0..len
    {
        props.push(crate::map::editor::state::clipboard::prop::PropViewer::from(
            ciborium::from_reader::<PropViewer, _>(&mut *reader)
                .map_err(|_| "Error reading props for conversion.")?
        ));
    }

    Ok(props)
}

//=======================================================================//

#[inline]
pub(in crate::map::editor::state) fn save_imported_09_props(
    writer: &mut BufWriter<&mut Vec<u8>>,
    props: Vec<crate::map::editor::state::clipboard::prop::PropViewer>
) -> Result<(), &'static str>
{
    for prop in props
    {
        test_writer!(&prop, &mut *writer, "Error converting props.");
    }

    Ok(())
}

//=======================================================================//

#[inline]
pub(in crate::map::editor::state) fn convert_09_prps_file(
    mut path: PathBuf,
    mut reader: BufReader<File>,
    len: usize
) -> Result<BufReader<File>, &'static str>
{
    let mut file_name = path.file_stem().unwrap().to_str().unwrap().to_string();
    file_name.push_str("_10.prps");

    warning_message(&format!(
        "This file appears to use the old file structure 0.9, if it is valid it will now be \
         converted to {file_name}."
    ));

    let props = convert_09_props(&mut reader, len)?;
    drop(reader);

    let mut data = Vec::<u8>::new();
    let mut writer = BufWriter::new(&mut data);
    test_writer!(FILE_VERSION, &mut writer, "Error converting props file version.");
    test_writer!(&len, &mut writer, "Error converting props amount.");
    save_imported_09_props(&mut writer, props)?;
    drop(writer);

    path.pop();
    path.push(file_name);

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

    let mut reader = BufReader::new(File::open(&path).unwrap());
    let _ = version_number(&mut reader);
    let _ = ciborium::from_reader::<usize, _>(&mut reader)
        .map_err(|_| "Error reading converted props length.")?;
    Ok(reader)
}
