//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::{prelude::Vec2, window::Window};
use bevy_egui::egui;

use super::misc::{Camera, Toggle};

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

/// The size of the tooltips' font.
const TOOLTIP_FONT_SIZE: f32 = 13f32;
/// The coefficient the tooltip's text needs to be offset to be spawned centered with respect to a
/// certain coordinate.
const TEXT_WIDTH_X_CENTER_COEFFICIENT: f32 = TOOLTIP_FONT_SIZE / 3.25;

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Converts 'p' to UI coorinates.
#[inline]
#[must_use]
pub fn to_egui_coordinates<T: Camera>(p: Vec2, window: &Window, camera: &T) -> egui::Pos2
{
    let pos = camera.pos();
    let scale = camera.scale();

    let mut q = egui::Pos2::new(p.x, p.y);
    q.y.toggle();
    q.x += (window.width() * scale) / 2f32 - pos.x;
    q.y += (window.height() * scale) / 2f32 + pos.y;
    q.x /= scale;
    q.y /= scale;
    q
}

//=======================================================================//

/// Draws a tooltip an position 'pos'.
#[inline]
pub fn draw_tooltip(
    egui_context: &egui::Context,
    label: &'static str,
    order: egui::Order,
    text: &str,
    style: egui::TextStyle,
    pos: egui::Pos2,
    text_color: egui::Color32,
    fill_color: egui::Color32,
    margin: f32,
    rounding: f32
)
{
    egui::Area::new(label.into())
        .fixed_pos(pos)
        .order(order)
        .show(egui_context, |ui| {
            egui::Frame::none()
                .fill(fill_color)
                .inner_margin(margin)
                .outer_margin(0f32)
                .rounding(rounding)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(text)
                            .color(text_color)
                            .text_style(style)
                            .size(TOOLTIP_FONT_SIZE)
                    );
                });
        });
}

//=======================================================================//

/// Returns the amount a tooltip needs to be horizontally offset to be centered with respect to a
/// certain coordinate.
#[allow(clippy::cast_precision_loss)]
#[inline]
#[must_use]
fn x_center_text_offset(text: &str) -> f32 { text.len() as f32 * TEXT_WIDTH_X_CENTER_COEFFICIENT }

//=======================================================================//

/// Draws a tooltip with center latitude equal to `pos.y`.
#[inline]
pub fn draw_tooltip_y_centered(
    egui_context: &egui::Context,
    label: &'static str,
    order: egui::Order,
    text: &str,
    style: egui::TextStyle,
    pos: egui::Pos2,
    mut offset: egui::Vec2,
    text_color: egui::Color32,
    fill_color: egui::Color32,
    margin: f32
)
{
    offset.y -= TOOLTIP_FONT_SIZE;

    draw_tooltip(
        egui_context,
        label,
        order,
        text,
        style,
        pos + offset,
        text_color,
        fill_color,
        margin,
        margin
    );
}

//=======================================================================//

/// Draws a tooltip with center at longitude `pos.x` with the bottom
/// of the frame lying right above `pos.y`.
#[inline]
pub fn draw_tooltip_x_centered_above_pos(
    egui_context: &egui::Context,
    label: &'static str,
    order: egui::Order,
    text: &str,
    style: egui::TextStyle,
    pos: egui::Pos2,
    offset: egui::Vec2,
    text_color: egui::Color32,
    fill_color: egui::Color32,
    margin: f32
)
{
    draw_tooltip(
        egui_context,
        label,
        order,
        text,
        style,
        pos + egui::Vec2::new(
            offset.x - x_center_text_offset(text) - margin,
            offset.y - TOOLTIP_FONT_SIZE - margin * 2f32
        ),
        text_color,
        fill_color,
        margin,
        margin
    );
}
