pub(in crate::map) mod clipboard;
pub(in crate::map) mod core;
pub(in crate::map) mod editor_state;
mod edits_history;
pub(in crate::map) mod grid;
mod input_press;
pub(in crate::map) mod manager;
pub(in crate::map) mod ui;

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
