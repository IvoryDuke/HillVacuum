pub(in crate::map) mod clipboard;
pub(in crate::map) mod core;
pub(in crate::map) mod editor_state;
mod edits_history;
pub mod grid;
mod input_press;
pub(in crate::map) mod manager;
pub(in crate::map) mod ui;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{fs::File, io::BufReader};

use crate::map::properties::DefaultProperties;

//=======================================================================//
// MACROS
//
//=======================================================================//

/// Formats the texture with id `texture_id` to fit frame of the `widget`.
macro_rules! format_texture_preview {
    ($widget:ident, $ui:ident, $texture_id:expr, $size:expr, $frame_size:expr) => {{
        macro_rules! uneven {
            ($_ui: ident,$div: ident,$pad: ident) => {{
                let size = $size.as_vec2();
                let size = egui::vec2(size.x, size.y) * ($frame_size / size.$div);
                let padding = ($frame_size - size.$pad) / 2f32;

                $_ui.add_space(padding);
                ($_ui.add(egui::$widget::new(($texture_id, size))), padding)
            }};
        }

        if $size.x == $size.y
        {
            $ui.add(egui::$widget::new(($texture_id, egui::Vec2::splat($frame_size))))
        }
        else if $size.x > $size.y
        {
            let (response, padding) = uneven!($ui, x, y);
            $ui.add_space(padding);
            response
        }
        else
        {
            $ui.horizontal(|ui| {
                let response = uneven!(ui, y, x).0;
                ui.add_space(ui.available_width());
                response
            })
            .inner
        }
    }};
}

use format_texture_preview;

//=======================================================================//

/// Tests whether `$value` is an error and returns an [`Err`] wrapping the error message
/// `$err` if it is.
macro_rules! test_writer {
    ($value:expr, $writer:expr, $err:literal) => {
        if ciborium::ser::into_writer($value, $writer).is_err()
        {
            return Err($err);
        }
    };

    ($value:expr, $err:literal) => {
        if $value.is_err()
        {
            return Err($err);
        }
    };
}

use test_writer;

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

#[inline]
fn read_default_properties(
    file: &mut BufReader<File>
) -> Result<[DefaultProperties; 2], &'static str>
{
    Ok([
        ciborium::from_reader::<DefaultProperties, _>(&mut *file)
            .map_err(|_| "Error reading Brush default properties")?,
        ciborium::from_reader::<DefaultProperties, _>(&mut *file)
            .map_err(|_| "Error reading Thing default properties")?
    ])
}
