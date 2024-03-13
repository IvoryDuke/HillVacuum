pub use bevy;
pub use hill_vacuum_lib::*;

/// Loads the desided [`Thing`]s as an available resource coded into the executable.
#[macro_export]
macro_rules! hardcoded_things {
    ($app:ident, $($thing:ident),+) => {{
        use hill_vacuum::MapThing;

        let mut hardcoded_things = hill_vacuum::HardcodedThings::new();
        $(hardcoded_things.push::<$thing>();)+
        $app.insert_resource(hardcoded_things);
    }}
}
