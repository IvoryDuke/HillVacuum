use std::{fs::File, io::Write};

fn main()
{
    #[inline]
    #[must_use]
    fn create(file: &str) -> File { File::create(file).expect("Unable to create file") }

    #[inline]
    fn write(f: &mut File, buffer: &str)
    {
        f.write_all(buffer.as_bytes()).expect("Unable to write data");
    }

    let mut readme = String::new();

    macro_rules! include {
        ($file:literal) => {{
            let str = include_str!(concat!("docs/", $file, ".md"));
            readme.push_str(str);
            readme.push_str("\n");
            println!(concat!("cargo::rerun-if-changed=docs/", $file, ".md"));
        }};

        ($(($tag:literal, $file:literal)),+) => { $(
            readme.push_str(concat!("### ", $tag, "\n"));
            include!($file);
        )+};
    }

    include!("intro");

    include!(
        ("Brushes", "brushes"),
        ("Things", "things"),
        ("Properties", "properties"),
        ("Textures", "textures"),
        ("Props", "props"),
        ("Grid", "grid")
    );

    include!("outro");

    let mut f = create("docs/crate_description.md");
    write(&mut f, &readme);

    include!("faq");

    let mut f = create("README.md");
    write(&mut f, include_str!("docs/license.md"));
    write(&mut f, &readme);

    println!("cargo::rerun-if-changed=docs/license.md");
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=src/map/editor/state/ui/manual/docs.rs");
}
