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
    ($app:expr, $($thing:ident),+) => {{
        use hill_vacuum::MapThing;

        let mut hardcoded_things = hill_vacuum::HardcodedThings::new();
        $(hardcoded_things.push::<$thing>();)+
        $app.insert_resource(hardcoded_things);
    }}
}

/// Inserts the default [`Properties`] that will be associated to all [`Brush`]es.
/// # Example
/// ```
/// use hill_vacuum::{brush_properties, BrushProperties, Value};
///
/// let mut app = bevy::prelude::App::new();
/// brush_properties!(app, [("Tag", 0u8), ("Destructible", false)]);
/// ```
#[macro_export]
macro_rules! brush_properties {
    ($app:expr, [$(($key:literal, $value:literal)),+]) => {
        $app.insert_resource(hill_vacuum::BrushProperties::new([
            $(($key, &$value as &dyn hill_vacuum::ToValue)),+
        ]));
    }
}

/// Inserts the default [`Properties`] that will be associated to all [`Thing`]s.
/// # Example
/// ```
/// use hill_vacuum::{thing_properties, BrushProperties, Value};
///
/// let mut app = bevy::prelude::App::new();
/// thing_properties!(app, [("Fire resistance", 1f32), ("Invisible", false)]);
/// ```
#[macro_export]
macro_rules! thing_properties {
    ($app:expr, [$(($key:literal, $value:literal)),+]) => {
        $app.insert_resource(hill_vacuum::ThingProperties::new([
            $(($key, &$value as &dyn hill_vacuum::ToValue)),+
        ].into_iter()));
    }
}
