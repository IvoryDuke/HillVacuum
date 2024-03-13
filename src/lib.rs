pub use bevy;
pub use hill_vacuum_lib::*;

/// Loads the desided [`Thing`]s as an available resource coded into the executable.
/// # Example
/// ```
/// use hill_vacuum::{hardcoded_things, MapThing, Thing};
///
/// struct Test;
///
/// impl MapThing for Test
/// {
///     fn thing() -> Thing { Thing::new("test", 0, 32f32, 32f32, "test").unwrap() }
/// }
///
/// let mut app = bevy::prelude::App::new();
/// hardcoded_things!(app, Test);
/// ```
#[macro_export]
macro_rules! hardcoded_things {
    ($app:ident, $($thing:ident),+) => {{
        use hill_vacuum::MapThing;

        let mut hardcoded_things = hill_vacuum::HardcodedThings::new();
        $(hardcoded_things.push::<$thing>();)+
        $app.insert_resource(hardcoded_things);
    }}
}
