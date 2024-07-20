#![forbid(clippy::enum_glob_use)]

fn main()
{
    std::env::set_var("RUST_BACKTRACE", "1");
    #[cfg(feature = "ui")]
    bevy::app::App::new().add_plugins(hill_vacuum::HillVacuumPlugin).run();

    #[cfg(not(feature = "ui"))]
    panic!("ui feature not enabled.");
}
