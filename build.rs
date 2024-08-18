use std::{fs::File, io::Write, path::PathBuf};

use hill_vacuum_shared::{process_manual, return_if_err, ManualItem};

fn main()
{
    #[inline]
    fn write(f: &mut File, buffer: &str)
    {
        f.write_all(buffer.as_bytes()).expect("Unable to write data");
    }

    let mut f = return_if_err!(File::create("docs/crate_description.md"));

    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=docs/intro.md");
    println!("cargo::rerun-if-changed=docs/outro.md");
    println!("cargo::rerun-if-changed=docs/faq.md");
    println!("cargo::rerun-if-changed=docs/license.md");

    for dir in std::fs::read_dir(PathBuf::from("docs/manual/"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
    {
        for path in std::fs::read_dir(&dir).unwrap().map(|entry| entry.unwrap().path())
        {
            println!("cargo::rerun-if-changed={}", path.as_os_str().to_str().unwrap());
        }
    }

    let mut readme = String::new();
    let mut description = String::new();

    macro_rules! include {
        ($(f: $folder:literal,)? $($file:expr),+) => {{$(
            let str = include_str!(concat!("docs/", $file, ".md"));

            for s in [&mut readme, &mut description]
            {
                s.push_str(str);
                s.push('\n');
            }
        )+}};
    }

    include!("intro");

    readme.push_str(include_str!("docs/previews.md"));
    readme.push('\n');

    for s in [&mut readme, &mut description]
    {
        s.push_str("## Keywords\n\n");
    }

    include!(
        "manual/01 - general/01 - brush",
        "manual/01 - general/02 - thing",
        "manual/01 - general/03 - properties",
        "manual/01 - general/04 - texture",
        "manual/01 - general/05 - prop",
        "manual/01 - general/06 - path",
        "manual/01 - general/07 - grid"
    );

    include!("outro");

    write(&mut f, &description);

    include!("faq");

    let mut f = return_if_err!(File::create("README.md"));
    write(&mut f, include_str!("docs/license.md"));
    write(&mut f, &readme);

    let mut f = return_if_err!(File::create("MANUAL.md"));

    let manual = process_manual(
        "# Manual\n\n",
        |_, _| {},
        |string, name, item| {
            string.push_str("## ");
            string.push_str(name);

            match item
            {
                ManualItem::Regular => string.push_str("\n\n"),
                ManualItem::Tool => string.push_str(" tool\n\n"),
                ManualItem::Texture => unreachable!()
            };
        },
        |string, _, file, item| {
            if let ManualItem::Texture = item
            {
                string.push_str("### TEXTURE EDITING\n");
            }

            string.push_str(&file);
            string.push('\n');
        },
        |_| {}
    );

    write(&mut f, &manual);
}
