fn main()
{
    std::env::set_var("RUST_BACKTRACE", "1");

    #[cfg(feature = "ui")]
    {
        let mut app = bevy::app::App::new();
        hill_vacuum::brush_properties!(app, [("Tag", 0u8), ("Destructible", false)]);
        app.add_plugins(hill_vacuum::HillVacuumPlugin).run();
    }

    #[cfg(not(feature = "ui"))]
    panic!("ui feature not enabled.");
}
