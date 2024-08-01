use std::{fs::File, io::Write};

use hill_vacuum_shared::return_if_err;

fn main()
{
    #[inline]
    fn create(file: &str) -> std::io::Result<File> { File::create(file) }

    #[inline]
    fn write(f: &mut File, buffer: &str)
    {
        f.write_all(buffer.as_bytes()).expect("Unable to write data");
    }

    let mut f = return_if_err!(create("docs/crate_description.md"));
    let mut readme = String::new();
    let mut description = String::new();

    macro_rules! include {
        ($file:literal) => {{
            let str = include_str!(concat!("docs/", $file, ".md"));

            for s in [&mut readme, &mut description]
            {
                s.push_str(str);
                s.push_str("\n");
            }

            println!(concat!("cargo::rerun-if-changed=docs/", $file, ".md"));
        }};

        ($(($tag:literal, $file:literal)),+) => { $(
            let str = concat!("### ", $tag, "\n");

            for s in [&mut readme, &mut description]
            {
                s.push_str(str);
            }

            include!($file);
        )+};
    }

    include!("intro");

    readme.push_str(include_str!("docs/previews.md"));
    readme.push('\n');

    for s in [&mut readme, &mut description]
    {
        s.push_str("## Keywords\n\n");
    }

    include!(
        ("Brushes", "brushes"),
        ("Things", "things"),
        ("Properties", "properties"),
        ("Textures", "textures"),
        ("Props", "props"),
        ("Grid", "grid")
    );

    include!("outro");

    write(&mut f, &description);

    include!("faq");

    let mut f = return_if_err!(create("README.md"));
    write(&mut f, include_str!("docs/license.md"));
    write(&mut f, &readme);

    println!("cargo::rerun-if-changed=docs/license.md");
    println!("cargo::rerun-if-changed=build.rs");
}
