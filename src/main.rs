fn main()
{
    #[cfg(feature = "ui")]
    bevy::app::App::new()
        .add_plugins(hill_vacuum::HillVacuumPlugin::default())
        .run();

    #[cfg(not(feature = "ui"))]
    panic!("ui feature not enabled.");
}
