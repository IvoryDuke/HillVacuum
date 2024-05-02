#![allow(clippy::single_match_else)]

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{
    fs::File,
    io::{BufRead, BufReader}
};

use proc_macro::{Ident, TokenStream, TokenTree};
use shared::{
    continue_if_no_match,
    draw_height_to_world,
    match_or_panic,
    return_if_no_match,
    NextValue,
    TEXTURE_HEIGHT_RANGE
};

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

/// Checks whever `value` is a comma.
/// # Panics
/// Function panics if `value` is not a comma.
#[inline]
fn is_comma(value: TokenTree)
{
    assert!(match_or_panic!(value, TokenTree::Punct(p), p).as_char() == ',');
}

//=======================================================================//

/// Executes `f` for each Ident contained in `group`'s stream.
/// # Panics
/// Panics if `group` is not a `TokenTree::Group(_)`.
fn for_each_ident_in_group<F: FnMut(Ident)>(group: TokenTree, mut f: F)
{
    for ident in match_or_panic!(group, TokenTree::Group(g), g)
        .stream()
        .into_iter()
        .filter_map(|item| return_if_no_match!(item, TokenTree::Ident(ident), Some(ident), None))
    {
        f(ident);
    }
}

//=======================================================================//

/// Extracts the name of an enum for `iter`.
/// # Panics
/// Panics if `iter` does not belong to an enum.
#[inline]
#[must_use]
fn enum_ident(iter: &mut impl Iterator<Item = TokenTree>) -> Ident
{
    for item in iter.by_ref()
    {
        let ident = continue_if_no_match!(item, TokenTree::Ident(ident), ident);

        if &ident.to_string() == "enum"
        {
            return match_or_panic!(iter.next_value(), TokenTree::Ident(i), i);
        }
    }

    panic!();
}

//=======================================================================//

/// Implements a constant representing the size of the `input` enum.
/// # Panics
/// Panics if `input` does not belong to an enum.
#[proc_macro_derive(EnumSize)]
#[must_use]
pub fn enum_size(input: TokenStream) -> TokenStream
{
    let mut iter = input.into_iter();
    format!(
        "impl {} {{ pub const SIZE: usize = {}; }}",
        enum_ident(&mut iter),
        enum_len(iter)
    )
    .parse()
    .unwrap()
}

//=======================================================================//

/// Returns the amount of elements in an enum.
#[allow(clippy::missing_panics_doc)]
#[inline]
#[must_use]
fn enum_len(mut iter: impl Iterator<Item = TokenTree>) -> usize
{
    let mut i = 0;
    for_each_ident_in_group(iter.next_value(), |_| i += 1);
    i
}

//=======================================================================//

/// Implements From `usize` for a plain enum.
/// # Panics
/// Panics if `input` does not belong to an enum.
#[proc_macro_derive(EnumFromUsize)]
#[must_use]
pub fn enum_from_usize(input: TokenStream) -> TokenStream
{
    let mut iter = input.into_iter();
    let enum_ident = enum_ident(&mut iter).to_string();

    let mut from_impl = format!(
        "impl From<usize> for {enum_ident}
        {{
            #[inline]
            #[must_use] fn from(value: usize) -> Self
            {{
                match value
                {{
        "
    );

    let mut i = 0;

    for_each_ident_in_group(iter.next_value(), |ident| {
        from_impl.push_str(&format!("{i} => {enum_ident}::{ident},\n"));
        i += 1;
    });

    from_impl.push_str("_ => unreachable!() } } }");
    from_impl.parse().unwrap()
}

//=======================================================================//

/// Implements a method that returns an iterator to the values of a plain enum.
/// # Panics
/// Panics if `input` does not belong to an enum.
#[proc_macro_derive(EnumIter)]
#[must_use]
pub fn enum_iter(input: TokenStream) -> TokenStream
{
    let mut iter = input.into_iter();
    let enum_ident = enum_ident(&mut iter).to_string();
    let enum_len = enum_len(iter.clone());
    let mut enum_match = String::new();

    let mut i = 0;
    for_each_ident_in_group(iter.next_value(), |ident| {
        enum_match.push_str(&format!("{i} => Some({enum_ident}::{ident}),\n"));
        i += 1;
    });

    enum_match.push_str("_ => None");

    format!(
        "
        impl {enum_ident}
        {{
            #[inline]
            pub fn iter() -> impl ExactSizeIterator<Item = Self>
            {{
                struct EnumIterator(usize, usize);

                impl ExactSizeIterator for EnumIterator
                {{
                    #[inline]
                    #[must_use]
                    fn len(&self) -> usize {{ self.1 - self.0 }}
                }}

                impl Iterator for EnumIterator
                {{
                    type Item = {enum_ident};

                    #[inline]
                    fn next(&mut self) -> Option<Self::Item>
                    {{
                        let value = match self.0
                        {{
                            {enum_match}
                        }};

                        self.0 += 1;
                        value
                    }}
                }}

                EnumIterator(0, {enum_len})
            }}
        }}
        "
    )
    .parse()
    .unwrap()
}

//=======================================================================//

/// Generates an array of static [`str`] with name, size, and prefix defined in `stream`.
/// # Examples
/// ```
/// str_array(ARRAY, 4, i_);
/// // Equivalent to
/// const ARRAY: [&'static str; 4] = ["i_0", "i_1", "i_2", "i_3"];
/// ```
/// # Panics
/// Panics if `input` is not properly formatted.
#[proc_macro]
pub fn str_array(input: TokenStream) -> TokenStream
{
    let mut iter = input.into_iter();

    let ident = iter.next_value().to_string();
    is_comma(iter.next_value());

    let amount = iter.next_value().to_string().parse::<u16>().unwrap();

    let prefix = if let Some(v) = iter.next()
    {
        is_comma(v);
        let v = iter.next_value();
        assert!(iter.next().is_none());
        v.to_string()
    }
    else
    {
        String::new()
    };

    let mut result = format!("pub const {ident}: [&'static str; {amount}] = [");

    for i in 0..amount
    {
        result.push_str(&format!("\"{prefix}{i}\", "));
    }

    result.push_str("];");
    result.parse().unwrap()
}

//=======================================================================//

/// Generates a function which associates a f32 value representing a certain height to each provided
/// enum match arm.
#[allow(clippy::missing_panics_doc)]
#[proc_macro]
pub fn color_enum(input: TokenStream) -> TokenStream
{
    const COLOR_HEIGHT_RANGE: f32 = 2f32;

    let textures_interval: f32 = draw_height_to_world(*TEXTURE_HEIGHT_RANGE.end());
    let textures_and_lines_interval: f32 = textures_interval + COLOR_HEIGHT_RANGE;

    let mut height_func = "
    /// The height at which map elements colored with a certain [`Color`] should be drawn.
    #[inline]
    #[must_use]
    pub const fn height(self) -> f32
    {
        match self
        {
    "
    .to_string();

    let mut key_func = "
    /// The config file key relative to the drawn color associated with [`Color`].
    #[inline]
    #[must_use]
    pub const fn config_file_key(self) -> &'static str
    {
        match self
        {
    "
    .to_string();

    let mut label_func = "
    /// The text label representing [`Color`] in UI elements.
    #[inline]
    #[must_use]
    pub const fn label(self) -> &'static str
    {
        match self
        {
    "
    .to_string();

    let mut height = 0f32;

    for item in input
    {
        if let TokenTree::Punct(p) = item
        {
            if p.as_char() == ','
            {
                height_func.push_str(&format!(" => {height}f32,\n"));
                height += textures_and_lines_interval;
            }
            else
            {
                height_func.push(p.as_char());
            }

            continue;
        }

        let item = item.to_string();
        height_func.push_str(&format!("Self::{item}"));

        let mut chars = item.chars();
        let c = chars.next_value();
        key_func.push_str(&format!("Self::{item} => \"{}", c.to_ascii_lowercase()));
        label_func.push_str(&format!("Self::{item} => \"{c}"));

        for c in chars
        {
            if c.is_uppercase()
            {
                key_func.push('_');
                key_func.push(c.to_ascii_lowercase());

                label_func.push(' ');
                label_func.push(c);

                continue;
            }

            key_func.push(c);
            label_func.push(c);
        }

        key_func.push_str("\",\n");
        label_func.push_str("\",\n");
    }

    height_func.push_str(&format!(" => {height}f32,\n}}\n}}"));
    key_func.push_str("}\n}");
    label_func.push_str("}\n}");

    format!(
        "
    {height_func}
    
    /// The draw height of the lines.
    #[inline]
    #[must_use]
    pub(in crate::map::drawer) fn clip_height(self) -> f32 {{ self.height() + {}f32 }}

    /// The draw height of the lines.
    #[inline]
    #[must_use]
    pub(in crate::map::drawer) fn line_height(self) -> f32 {{ self.height() + {}f32 }}

    /// The draw height of the lines.
    #[inline]
    #[must_use]
    pub(in crate::map::drawer) fn thing_angle_indicator_height(self) -> f32 {{ self.height() + \
         {}f32 }}

    /// The draw height of the square highlights.
    #[inline]
    #[must_use]
    pub(in crate::map::drawer) fn square_hgl_height(self) -> f32 {{ self.height() + {}f32 }}

    {key_func}

    {label_func}
    ",
        textures_interval + COLOR_HEIGHT_RANGE / 4f32,
        textures_interval + COLOR_HEIGHT_RANGE / 2f32,
        textures_interval + COLOR_HEIGHT_RANGE / 4f32 * 3f32,
        textures_interval + COLOR_HEIGHT_RANGE,
    )
    .parse()
    .unwrap()
}

//=======================================================================//

/// Generates the [`Bind`] enum plus the `config_file_key()` and `label()` methods.
/// # Panics
/// Panic if the file containing the [`Tool`] enum is not at the required location.
#[proc_macro]
pub fn bind_enum(input: TokenStream) -> TokenStream
{
    let mut binds = "{".to_string();
    binds.push_str(&input.to_string());
    binds.push(',');

    let mut path = std::env::current_dir().unwrap();
    if !path.as_os_str().to_str().unwrap().contains("hill_vacuum_lib")
    {
        path.push("hill_vacuum_lib/");
    }
    path.push("src/map/editor/state/core/tool.rs");

    let mut lines = BufReader::new(File::open(path).unwrap()).lines().map(Result::unwrap);
    lines.find(|line| line.ends_with("enum Tool"));
    lines.next();

    for line in lines
    {
        binds.push_str(&line);
        binds.push('\n');

        if line.contains('}')
        {
            break;
        }
    }

    let mut iter = binds.clone().parse::<TokenStream>().unwrap().into_iter();

    let mut key_func = "
    /// Returns the string key used in the config file associated with this `Bind`. 
    #[inline]
    #[must_use]
    pub(in crate::config::controls) const fn config_file_key(self) -> &'static str
    {
        match self
        {\n"
    .to_string();

    let mut label_func = "
    /// Returns the text representing this `Bind` in UI elements.
    #[inline]
    #[must_use]
    pub const fn label(self) -> &'static str
    {
        match self
        {\n"
    .to_string();

    for item in match_or_panic!(iter.next_value(), TokenTree::Group(g), g).stream()
    {
        if let TokenTree::Ident(ident) = item
        {
            let ident = ident.to_string();
            let mut chars = ident.chars();
            let mut value = chars.next_value().to_string();

            for ch in chars
            {
                if ch.is_ascii_uppercase()
                {
                    value.push(' ');
                }

                value.push(ch);
            }

            label_func.push_str(&format!("Self::{ident} => \"{value}\",\n"));

            value = value.to_ascii_lowercase().replace(' ', "_");
            key_func.push_str(&format!("Self::{ident} => \"{value}\",\n"));
        }
    }

    for func in [&mut key_func, &mut label_func]
    {
        func.push_str("}\n}");
    }

    format!(
        "
        /// The binds associated with the editor actions.
        #[derive(Clone, Copy, Debug, PartialEq, EnumIter, EnumSize)]
        pub enum Bind
        {binds}

        impl Bind
        {{
            {key_func}

            {label_func}
        }}"
    )
    .parse()
    .unwrap()
}

//=======================================================================//

/// Generates the `header()` and `icon_file_name()` methods for the [`Tool`] and [`SubTool`] enums.
#[inline]
#[must_use]
fn tools_common(stream: TokenStream) -> [String; 2]
{
    let mut header_func = "
        /// The uppercase tool name.
        #[inline]
        #[must_use]
        fn header(self) -> &'static str
        {
            match self
            {\n"
    .to_string();

    let mut icon_file_name_func = "
        /// The file name of the associated icon.
        #[inline]
        #[must_use]
        fn icon_file_name(self) -> &'static str
        {
            match self
            {\n"
    .to_string();

    for item in stream
    {
        let ident = continue_if_no_match!(item, TokenTree::Ident(ident), ident).to_string();
        let mut chars = ident.chars();

        // Label.
        let mut value = chars.next_value().to_string();

        for ch in chars
        {
            if ch.is_ascii_uppercase()
            {
                value.push(' ');
            }

            value.push(ch);
        }

        // Header.
        value = value.to_ascii_uppercase();
        header_func.push_str(&format!("Self::{ident} => \"{value}\",\n"));

        // Icon paths.
        value = value.to_ascii_lowercase().replace(' ', "_");
        icon_file_name_func.push_str(&format!("Self::{ident} => \"{value}.png\",\n"));
    }

    for func in [&mut icon_file_name_func, &mut header_func]
    {
        func.push_str("}\n}");
    }

    [header_func, icon_file_name_func]
}

//=======================================================================//

/// Implements the vast majority of the methods of the [`Tool`] enum.
/// # Panics
/// Panics if `input` does not belong to the [`Tool`] enum.
#[proc_macro_derive(ToolEnum)]
#[must_use]
pub fn declare_tool_enum(input: TokenStream) -> TokenStream
{
    let mut iter = input.into_iter();
    assert!(enum_ident(&mut iter).to_string() == "Tool");
    let group = match_or_panic!(iter.next_value(), TokenTree::Group(g), g);
    let [header_func, icon_file_name_func] = tools_common(group.stream());

    let mut bind_func = "#[inline]
        pub fn bind(self) -> Bind
        {
            match self
            {\n"
    .to_string();

    let mut label_func = "#[inline]
        fn label(self) -> &'static str
        {
            match self
            {\n"
    .to_string();

    for item in group.stream()
    {
        let ident = continue_if_no_match!(item, TokenTree::Ident(ident), ident).to_string();
        let mut chars = ident.chars();

        // Bind
        bind_func.push_str(&format!("Self::{ident} => Bind::{ident},\n"));

        // Label.
        let mut value = chars.next_value().to_string();

        for ch in chars
        {
            if ch.is_ascii_uppercase()
            {
                value.push(' ');
            }

            value.push(ch);
        }

        label_func.push_str(&format!("Self::{ident} => \"{value}\",\n"));
    }

    for func in [&mut label_func, &mut bind_func]
    {
        func.push_str("}\n}");
    }

    format!(
        "
        impl ToolInterface for Tool
        {{
            {label_func}

            {header_func}

            {icon_file_name_func}

            #[inline]
            fn tooltip_label(self, binds: &BindsKeyCodes) -> String
            {{
                format!(\"{{}} ({{}})\", self.label(), self.keycode_str(binds))
            }}

            #[inline]
            fn change_conditions_met(self, change_conditions: &ChangeConditions) -> bool
            {{
                if change_conditions.ongoing_multi_frame_changes ||
                    change_conditions.ctrl_pressed ||
                    change_conditions.space_pressed
                {{
                    return false;
                }}

                match self
                {{
                    Self::Square | Self::Triangle | Self::Circle | Self::FreeDraw | Self::Zoom => \
         true,
                    Self::Thing => !change_conditions.things_catalog_empty || \
         change_conditions.selected_things_amount != 0,
                    Self::Entity => change_conditions.brushes_amount + \
         change_conditions.things_amount > 0,
                    Self::Paint => change_conditions.selected_brushes_amount + \
         change_conditions.selected_things_amount > 0 || !change_conditions.no_props,
                    Self::Vertex | Self::Side | Self::Clip | Self::Shatter | Self::Scale |
                    Self::Shear | Self::Rotate | Self::Flip | Self::Hollow => \
         change_conditions.selected_brushes_amount != 0,
                    Self::Path => change_conditions.selected_platforms_amount != 0 || \
         change_conditions.any_selected_possible_platforms,
                    Self::Snap => change_conditions.vertex_rounding_availability,
                    Self::Merge | Self::Intersection =>
                    {{
                        change_conditions.selected_brushes_amount > 1
                    }},
                    Self::Subtract =>
                    {{
                        change_conditions.selected_brushes_amount == 1 &&
                            change_conditions.brushes_amount > 1
                    }}
                }}
            }}

            #[inline]
            fn subtool(self) -> bool {{ false }}

            #[inline]
            fn index(self) -> usize {{ self as usize }}
        }}

        impl Tool
        {{
            {bind_func}
        }}"
    )
    .parse()
    .unwrap()
}

//=======================================================================//

/// Implements the vast majority of the methods of the [`SubTool`] enum.
/// # Panics
/// Panics if `input` does not belong to the [`SubTool`] enum.
#[proc_macro_derive(SubToolEnum)]
#[must_use]
pub fn subtool_enum(input: TokenStream) -> TokenStream
{
    let mut iter = input.into_iter();
    assert!(enum_ident(&mut iter).to_string() == "SubTool");
    let group = match_or_panic!(iter.next_value(), TokenTree::Group(g), g);
    let [header_func, icon_file_name_func] = tools_common(group.stream());

    let mut label_func = "
        #[inline]
        fn label(self) -> &'static str
        {
            match self
            {\n"
    .to_string();

    let mut tool_func = "
        #[inline]
        fn tool(self) -> Tool
        {
            match self
            {\n"
    .to_string();

    for item in group.stream()
    {
        let ident = continue_if_no_match!(item, TokenTree::Ident(ident), ident).to_string();
        let mut chars = ident.chars();

        // Label.
        let mut tool = String::new();
        let mut label = String::new();

        tool.push(chars.next_value());

        for ch in chars.by_ref()
        {
            if ch.is_ascii_uppercase()
            {
                label.push(ch);
                break;
            }

            tool.push(ch);
        }

        for ch in chars
        {
            if ch.is_ascii_uppercase()
            {
                label.push(' ');
            }

            label.push(ch);
        }

        label_func.push_str(&format!("Self::{ident} => \"{label}\",\n"));
        tool_func.push_str(&format!("Self::{ident} => Tool::{tool},\n"));
    }

    for func in [&mut label_func, &mut tool_func]
    {
        func.push_str("}\n}");
    }

    format!(
        "
        impl ToolInterface for SubTool
        {{
            {label_func}

            {header_func}

            {icon_file_name_func}

            #[inline]
            fn tooltip_label(self, binds: &BindsKeyCodes) -> String
            {{
                format!(\"{{}} ({{}})\", self.label(), self.key_combo(binds))
            }}

            #[inline]
            fn change_conditions_met(self, change_conditions: &ChangeConditions) -> bool
            {{
                use crate::map::editor::state::editor_state::TargetSwitch;

                if let Self::PathSimulation = self
                {{
                    return
                        (change_conditions.path_simulation_active ||
                            self.tool().change_conditions_met(change_conditions)) &&
                        change_conditions.selected_platforms_amount != 0;
                }}

                if !self.tool().change_conditions_met(change_conditions)
                {{
                    return false;
                }}

                match self
                {{
                    Self::RotatePivot => true,
                    Self::ThingChange => change_conditions.selected_things_amount != 0,
                    Self::EntityDragSpawn | Self::PaintCreation =>
                    {{
                        change_conditions.selected_brushes_amount + \
         change_conditions.selected_things_amount != 0
                    }},
                    Self::EntityAnchor =>
                    {{
                        change_conditions.selected_brushes_amount > 1 &&
                            change_conditions.settings.target_switch() != TargetSwitch::Texture
                    }},
                    Self::VertexSplit => change_conditions.split_available,
                    Self::VertexPolygonToPath => \
         Tool::Path.change_conditions_met(change_conditions),
                    Self::SideXtrusion => change_conditions.xtrusion_available,
                    Self::PaintQuick => change_conditions.quick_prop,
                    Self::VertexMerge | Self::SideMerge => change_conditions.vx_merge_available,
                    Self::VertexInsert |
                    Self::PathFreeDraw |
                    Self::PathAddNode => true,
                    Self::ClipSide => change_conditions.selected_brushes_amount > 1,
                    Self::PathSimulation => unreachable!()
                }}
            }}

            #[inline]
            fn subtool(self) -> bool {{ true }}

            #[inline]
            fn index(self) -> usize {{ self as usize }}
        }}

        impl SubTool
        {{
            {tool_func}
        }}
        "
    )
    .parse()
    .unwrap()
}

//=======================================================================//

/// Generates the function calls to store the embedded assets from the file names in the
/// `src/embedded_assets/` folder.
/// # Panics
/// Panics if the required folder cannot be found.
#[allow(clippy::missing_panics_doc)]
#[proc_macro]
pub fn embedded_assets(_: TokenStream) -> TokenStream
{
    let mut path = std::env::current_dir().unwrap();
    if !path.as_os_str().to_str().unwrap().contains("hill_vacuum_lib")
    {
        path.push("hill_vacuum_lib/");
    }
    path.push("src/embedded_assets/");

    // Get all the files.
    let directory = std::fs::read_dir(path).unwrap();
    let mut values = String::new();
    values.push_str("use bevy::asset::embedded_asset;\n");

    for file in directory.into_iter().map(|p| p.unwrap().file_name())
    {
        let file_name = file.to_str().unwrap();
        values.push_str(&format!("bevy::asset::embedded_asset!(app, \"{file_name}\");\n"));
    }

    values.parse().unwrap()
}

//=======================================================================//

/// Generates the vector of the indexes used to triangulate the meshes.
#[allow(clippy::missing_panics_doc)]
#[proc_macro]
pub fn meshes_indexes(stream: TokenStream) -> TokenStream
{
    let mut stream = stream.into_iter();
    let ident = stream.next_value().to_string();
    is_comma(stream.next_value());
    let size = stream.next_value().to_string().parse::<u16>().unwrap();
    assert!(stream.next().is_none());

    let mut indexes = format!(
        "
    const MAX_MESH_TRIANGLES: usize = {size};
    static mut {ident}: *mut [u16] = &mut [\n"
    );

    for i in 1..=size
    {
        indexes.push_str(&format!("0u16, {i}, {i} + 1,\n"));
    }

    indexes.push_str("];");

    indexes.parse().unwrap()
}
