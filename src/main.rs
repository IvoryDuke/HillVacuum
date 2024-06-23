#![forbid(clippy::enum_glob_use)]

fn main()
{
    std::env::set_var("RUST_BACKTRACE", "1");
    bevy::prelude::App::new()
        .add_plugins(hill_vacuum_lib::HillVacuumPlugin)
        .run();
}
