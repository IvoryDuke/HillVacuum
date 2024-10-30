## Files
HV creates three types of files, all of which are relatively simple:
- `.hv` is the map file;
```
-------------------------------
| Version number              |
-------------------------------
| Header                      |
| brushes amount              |
| things amount               |
| animations amount           |
| props amount                |
-------------------------------
| Grid settings (skew, angle) |
-------------------------------
| Animations                  |
-------------------------------
| Default brush properties    |
-------------------------------
| Default thing properties    |
-------------------------------
| Brushes                     |
-------------------------------
| Things                      |
-------------------------------
| Props                       |
-------------------------------
```
- `.anms` is the "animations only" file, which can be used to exchange animations between maps;
```
-------------------------------
| Version number              |
-------------------------------
| animations amount (usize)   |
-------------------------------
| Animations                  |
-------------------------------
```
- `.prps` is the "props only" file, which can be used to exchange props between maps.
```
-------------------------------
| Version number              |
-------------------------------
| props amount (usize)        |
-------------------------------
| Props                       |
-------------------------------
```

## Getting started
HV can be compiled as a standalone executable simply compiling the source code (Linux distributions may require the installation of extra libraries).
```sh
cargo run
```

Otherwise it can be integrated in your own project as such:
```rust
fn main()
{
    bevy::app::App::new()
        .add_plugins(hill_vacuum::HillVacuumPlugin)
        .run();
}
```

Map files can be read through the `Exporter` struct that will return lists of all the brushes and things, which can then be exported as desired.
Assuming the path of the map file was passed as an argument to the exporting executable the code will look something like this:
```rust
fn main()
{
    let exporter = hill_vacuum::Exporter::new(&std::env::args().collect::<Vec<_>>()[0]);
    // Your code.
}
```
The map being edited can be exported through such an executable through the File->Export command in the editor.
The executable can be picked through Options->Exporter.

## Features
- `dynamic_linking`: enables the `bevy` feature of the same name;
- `ui`: enables the `HillVacuumPlugin` and therefore the UI editor. Enabled by default, it is recommended to turn it off, for example, when creating an executable to export a map using the `Exporter` struct.

## !! WARNING
- [The only thing I know for real](https://youtu.be/T928kJvqTlo?si=2_YnB2pEuFSKKq-j), there will be bugs.  
  HV has been thoroughly tested but is still in its early releases, so there might be issues that lead to crashes due to unrecoverable errors. It is strongly recommended to save often;
- On Linux, HV requires the C library and development headers of GTK3 to be installed to build. If not installed, HV will crash any time a message dialog must be displayed. The package names on various distributions are (source: <https://docs.rs/rfd/latest/rfd/>):

| Distribution    | Installation Command     |
| --------------- | ------------------------ |
| Fedora          | dnf install gtk3-devel   |
| Arch            | pacman -S gtk3           |
| Debian & Ubuntu | apt install libgtk-3-dev |

## Misc
In order to close the in-editor windows through the keyboard the `F4` key needs to be pressed (similar to pressing `Alt+F4` to close OS windows).
