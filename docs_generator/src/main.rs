use std::{fs::File, io::Write};

use hill_vacuum_shared::{process_manual, ManualItem, NextValue};

fn main()
{
    const TEMP_MD: &str = "manual_temp.md";

    #[inline]
    fn write(f: &mut File, buffer: &str)
    {
        f.write_all(buffer.as_bytes()).expect("Unable to write data");
    }

    #[inline]
    fn push_manual_icon(string: &mut String, icon: &str)
    {
        string.push_str("<img src=\"images/");
        string.push_str(icon);
        string.push_str(".svg\" alt=\"");
        string.push_str(icon);
        string.push_str("\" height=\"48\" width=\"48\"/>  \n");
    }

    let mut f = File::create("docs/crate_description.md").unwrap();
    let mut readme = String::new();
    let mut description = String::new();

    macro_rules! push {
        ($(f: $folder:literal,)? $($file:expr),+) => {{$(
            let str = std::fs::read_to_string(concat!("docs/", $file, ".md")).unwrap();

            for s in [&mut readme, &mut description]
            {
                s.push_str(&str);
                s.push('\n');
            }
        )+}};
    }

    push!("intro");

    readme.push_str(&std::fs::read_to_string("docs/previews.md").unwrap());
    readme.push('\n');

    for s in [&mut readme, &mut description]
    {
        s.push_str("## Keywords\n\n");
    }

    push!(
        "manual/01 - general/01 - brush",
        "manual/01 - general/02 - thing",
        "manual/01 - general/03 - properties",
        "manual/01 - general/04 - texture",
        "manual/01 - general/05 - prop",
        "manual/01 - general/06 - path",
        "manual/01 - general/07 - grid"
    );

    push!("outro");

    write(&mut f, &description);

    push!("faq");

    let mut f = File::create("README.md").unwrap();
    write(&mut f, &std::fs::read_to_string("docs/license.md").unwrap());
    write(&mut f, &readme);

    let mut f = File::create("MANUAL.md").unwrap();

    let manual = process_manual(
        |_, _| {},
        |string, name, item| {
            string.push_str("## ");
            string.push_str(name);

            match item
            {
                ManualItem::Regular => string.push_str("\n\n"),
                ManualItem::Tool =>
                {
                    string.push_str(" tool\n");
                    let icon = name
                        .chars()
                        .map(|c| {
                            if c == ' '
                            {
                                return '_';
                            }

                            c.to_ascii_lowercase()
                        })
                        .collect::<String>();

                    push_manual_icon(string, &icon);

                    string.push('\n');
                },
                ManualItem::Texture => unreachable!()
            };
        },
        |string, name, file, item| {
            if let ManualItem::Texture = item
            {
                string.push_str("### TEXTURE EDITING\n");
            }

            let mut lines = file.lines();
            string.push_str(lines.next_value());
            string.push('\n');

            if let ManualItem::Tool = item
            {
                push_manual_icon(string, name);
            }

            for line in lines
            {
                string.push_str(line);
                string.push('\n');
            }

            string.push('\n');
        },
        |string| string.push_str("&nbsp;\n\n")
    );

    write(&mut f, &manual);

    if std::process::Command::new("pandoc").output().is_err()
    {
        return;
    }

    // You can't make me open a word editor.
    write(
        &mut File::create(TEMP_MD).unwrap(),
        &regex::Regex::new(
            r#"<img src="images/([a-z_]+).svg" alt="[a-z_]+" height="48" width="48"/>"#
        )
        .unwrap()
        .replace_all(&manual, "![$1](images/$1.svg){ width=40 height=40 }")
        .replace(
            "Erases the brush being drawn.\n\n&nbsp;",
            "Erases the brush being drawn.\n\n&nbsp;\n\n&nbsp;\n\n&nbsp;\n\n&nbsp;"
        )
    );

    _ = std::process::Command::new("pandoc")
        .args([
            "pandoc.yaml",
            TEMP_MD,
            "-fmarkdown-implicit_figures",
            "-o",
            "MANUAL.pdf"
        ])
        .output();

    _ = std::fs::remove_file(TEMP_MD);
}
