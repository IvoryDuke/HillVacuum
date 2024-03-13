pub(in crate::map) mod clipboard;
pub(in crate::map) mod core;
pub(in crate::map) mod editor_state;
mod edits_history;
pub(in crate::map) mod grid;
mod input_press;
pub(in crate::map) mod manager;
pub(in crate::map) mod ui;

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

#[inline]
fn error_message(error: &str)
{
    rfd::MessageDialog::new()
        .set_title("ERROR")
        .set_description(error)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}
