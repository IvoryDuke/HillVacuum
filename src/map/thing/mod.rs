#[cfg(feature = "ui")]
pub(in crate::map) mod catalog;
#[cfg(feature = "ui")]
pub(in crate::map) mod compatibility;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{HvHashMap, HvVec, Id, Node, Value};

//=======================================================================//
// TRAITS
//
//=======================================================================//

/// A trait to associate a [`Thing`] to a type.
/// # Example
/// ```
/// use hill_vacuum::{MapThing, Thing};
///
/// struct Test;
///
/// impl MapThing for Test
/// {
///     fn thing() -> Thing { Thing::new("test", 0, 32f32, 32f32, "test").unwrap() }
/// }
///
/// let _ = Test::thing();
/// ```
pub trait MapThing
{
    /// Returns the [`Thing`] associated with `self`.
    fn thing() -> Thing;
}

//=======================================================================//

/// A trait with methods returning basic information about a type representing a thing.
#[allow(dead_code)]
pub(in crate::map) trait ThingInterface
{
    /// Returns the [`ThingId`].
    #[must_use]
    fn thing(&self) -> ThingId;

    /// Returns the position it is placed on the map.
    #[must_use]
    fn pos(&self) -> Vec2;

    /// The draw height of `self` as a float.
    #[must_use]
    fn draw_height_f32(&self) -> f32;

    /// The angle of `self`.
    #[must_use]
    fn angle_f32(&self) -> f32;
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The identifier of a [`Thing`].
#[derive(Clone, Copy, Debug, PartialEq, Hash, Serialize, Deserialize)]
pub struct ThingId(u16);

impl Eq for ThingId {}

impl ThingId
{
    /// Returns a new [`ThingId`] with value `id`.
    #[inline]
    #[must_use]
    pub const fn new(id: u16) -> Self { Self(id) }

    /// Returns the [`u16`] associated with `self`.
    #[inline]
    #[must_use]
    pub const fn value(self) -> u16 { self.0 }
}

//=======================================================================//

/// An object which can be used to create map placeable items.
#[allow(dead_code)]
#[must_use]
#[derive(Clone)]
pub struct Thing
{
    /// The name.
    name:    String,
    /// The id.
    id:      ThingId,
    /// The width of the bounding box.
    width:   f32,
    /// The height of the bounding box.
    height:  f32,
    /// The name of the texture used to draw a preview.
    preview: String
}

impl Thing
{
    /// Returns a new [`Thing`] with the requested properties, unless width or height are equal
    /// or less than zero.
    #[must_use]
    #[inline]
    pub fn new(name: &str, id: u16, width: f32, height: f32, preview: &str) -> Option<Self>
    {
        (width > 0f32 && height > 0f32).then(|| {
            Self {
                name: name.to_string(),
                id: ThingId::new(id),
                width,
                height,
                preview: preview.to_string()
            }
        })
    }

    #[inline]
    #[must_use]
    pub fn name(&self) -> &str { &self.name }

    /// Returns the [`ThingId`].
    #[inline]
    #[must_use]
    pub const fn id(&self) -> ThingId { self.id }

    /// Returns the width of the bounding box.
    #[inline]
    #[must_use]
    pub const fn width(&self) -> f32 { self.width }

    /// Returns the height of the bounding box.
    #[inline]
    #[must_use]
    pub const fn height(&self) -> f32 { self.height }

    #[inline]
    #[must_use]
    pub fn preview(&self) -> &str { &self.preview }
}

//=======================================================================//

/// An instance of a [`Thing`] placed on the map.
#[must_use]
#[derive(Serialize, Deserialize)]
pub struct ThingViewer
{
    /// The unique id.
    pub id:         Id,
    /// The id of the [`Thing`].
    pub thing_id:   ThingId,
    /// The position of the center.
    pub pos:        Vec2,
    /// The optional associated path.
    pub path:       Option<HvVec<Node>>,
    /// The associated properties.
    pub properties: HvHashMap<String, Value>
}

//=======================================================================//
// UI
//
//=======================================================================//

#[cfg(feature = "ui")]
pub mod ui_mod
{
    //=======================================================================//
    // IMPORTS
    //
    //=======================================================================//

    use bevy::{prelude::Resource, transform::components::Transform, window::Window};
    use bevy_egui::egui;
    use glam::Vec2;
    use hill_vacuum_shared::{match_or_panic, return_if_none};
    use serde::{Deserialize, Serialize};

    use super::{catalog::ThingsCatalog, MapThing, Thing, ThingInterface, ThingViewer};
    use crate::{
        map::{
            drawer::{
                color::Color,
                drawers::{EditDrawer, MapPreviewDrawer}
            },
            editor::state::{
                clipboard::{ClipboardData, CopyToClipboard},
                grid::Grid,
                manager::{Animators, Brushes}
            },
            path::{common_edit_path, EditPath, MovementSimulator, Moving, Path},
            properties::{Properties, PropertiesRefactor, ANGLE_LABEL, HEIGHT_LABEL},
            OutOfBounds
        },
        utils::{
            hull::{EntityHull, Hull},
            identifiers::{EntityCenter, EntityId},
            math::AroundEqual,
            misc::{Camera, ReplaceValue, TakeValue}
        },
        Id,
        ThingId,
        ToValue,
        Value
    };

    //=======================================================================//
    // STRUCTS
    //
    //=======================================================================//

    /// The data of [`ThingInstance`].
    #[must_use]
    #[derive(Debug, Clone, Serialize, Deserialize)]

    pub(in crate::map) struct ThingInstanceData
    {
        /// The [`ThingId`] of the [`Thing`] it represents.
        thing:      ThingId,
        /// The position on the map.
        pos:        Vec2,
        /// The bounding box.
        hull:       Hull,
        /// The path describing the [`ThingInstance`] movement, if any.
        path:       Option<Path>,
        /// The associated properties.
        properties: Properties
    }

    impl EntityHull for ThingInstanceData
    {
        #[inline]
        fn hull(&self) -> Hull { self.hull }
    }

    impl ThingInterface for ThingInstanceData
    {
        #[inline]
        fn thing(&self) -> ThingId { self.thing }

        #[inline]
        fn pos(&self) -> Vec2 { self.pos }

        #[inline]
        fn draw_height_f32(&self) -> f32 { f32::from(self.draw_height()) }

        #[inline]
        fn angle_f32(&self) -> f32 { f32::from(self.angle()) }
    }

    impl ThingInstanceData
    {
        /// Returns the [`ThingId`] of the [`Thing`] represented by `self`.
        #[inline]
        pub const fn thing(&self) -> ThingId { self.thing }

        /// Returns the [`Hull`] of [`Thing`] with center at `pos`.
        #[inline]
        #[must_use]
        fn create_hull(pos: Vec2, thing: &Thing) -> Hull
        {
            let half_width = thing.width / 2f32;
            let half_height = thing.height / 2f32;

            Hull::from_opposite_vertexes(
                pos + Vec2::new(-half_width, -half_height),
                pos + Vec2::new(half_width, half_height)
            )
        }

        /// Return the [`Hull`] of the associated [`Path`], if any.
        #[inline]
        #[must_use]
        pub fn path_hull(&self) -> Option<Hull>
        {
            self.path.as_ref().map(|path| path.hull() + self.pos)
        }

        #[inline]
        #[must_use]
        pub fn angle(&self) -> i16
        {
            match_or_panic!(self.properties.get(ANGLE_LABEL), Value::I16(value), *value)
        }

        /// Returns the draw height.
        #[inline]
        #[must_use]
        pub fn draw_height(&self) -> i8
        {
            match_or_panic!(self.properties.get(HEIGHT_LABEL), Value::I8(value), *value)
        }

        /// Sets the [`Thing`] represented by `self` to `thing`.
        /// Returns the [`ThingId`] of the previous [`Thing`] if different.
        #[inline]
        #[must_use]
        pub fn set_thing(&mut self, thing: &Thing) -> Option<ThingId>
        {
            if thing.width.around_equal_narrow(&self.hull.width()) &&
                thing.height.around_equal_narrow(&self.hull.height())
            {
                self.hull = ThingInstanceData::create_hull(self.pos, thing);
            }

            if thing.id == self.thing
            {
                return None;
            }

            self.thing.replace_value(thing.id).into()
        }

        /// Draw `self` displaced by `delta` for a prop screenshot.
        #[inline]
        pub fn draw_prop(&self, drawer: &mut EditDrawer, catalog: &ThingsCatalog, delta: Vec2)
        {
            drawer.thing(
                catalog,
                &MovedThingInstance { thing: self, delta },
                Color::NonSelectedEntity
            );
            return_if_none!(&self.path).draw_prop(drawer, self.pos + delta);
        }
    }

    //=======================================================================//

    /// An instance of a [`Thing`] which can be placed in a map.
    #[must_use]
    #[derive(Clone)]
    pub(in crate::map) struct ThingInstance
    {
        /// The id.
        id:   Id,
        /// All entity data.
        data: ThingInstanceData
    }

    impl<'a> From<(crate::map::thing::compatibility::ThingViewer, &'a ThingsCatalog)> for ThingInstance
    {
        #[allow(clippy::cast_possible_truncation)]
        #[inline]
        fn from(value: (crate::map::thing::compatibility::ThingViewer, &'a ThingsCatalog)) -> Self
        {
            let (
                crate::map::thing::compatibility::ThingViewer {
                    id,
                    thing_id,
                    pos,
                    angle,
                    draw_height,
                    path,
                    mut properties
                },
                catalog
            ) = value;

            properties.insert(ANGLE_LABEL.to_string(), angle.to_value());
            properties.insert(HEIGHT_LABEL.to_string(), draw_height.to_value());

            Self::from((
                ThingViewer {
                    id,
                    thing_id,
                    pos,
                    path,
                    properties
                },
                catalog
            ))
        }
    }

    impl ThingInterface for ThingInstance
    {
        #[inline]
        fn thing(&self) -> ThingId { self.data.thing }

        #[inline]
        fn pos(&self) -> Vec2 { self.data.pos }

        #[inline]
        fn draw_height_f32(&self) -> f32 { self.data.draw_height_f32() }

        #[inline]
        fn angle_f32(&self) -> f32 { self.data.angle_f32() }
    }

    impl<'a> From<(ThingViewer, &'a ThingsCatalog)> for ThingInstance
    {
        #[allow(clippy::cast_possible_truncation)]
        #[inline]
        fn from(value: (ThingViewer, &'a ThingsCatalog)) -> Self
        {
            let (
                ThingViewer {
                    id,
                    thing_id,
                    pos,
                    path,
                    properties
                },
                catalog
            ) = value;

            let mut thing = ThingInstance::new(
                id,
                catalog.thing_or_error(thing_id),
                pos,
                Properties::from_parts(properties)
            );
            thing.data.path = path.map(Into::into);

            thing
        }
    }

    impl EntityHull for ThingInstance
    {
        #[inline]
        fn hull(&self) -> Hull { self.data.hull }
    }

    impl EntityId for ThingInstance
    {
        #[inline]
        fn id(&self) -> Id { self.id }

        #[inline]
        fn id_as_ref(&self) -> &Id { &self.id }
    }

    impl EntityCenter for ThingInstance
    {
        #[inline]
        fn center(&self) -> Vec2 { self.data.pos }
    }

    impl CopyToClipboard for ThingInstance
    {
        #[inline]
        fn copy_to_clipboard(&self) -> ClipboardData
        {
            ClipboardData::Thing(self.data.clone(), self.id)
        }
    }

    impl Moving for ThingInstance
    {
        #[inline]
        fn path(&self) -> Option<&Path> { self.data.path.as_ref() }

        #[inline]
        fn has_path(&self) -> bool { self.data.path.is_some() }

        #[inline]
        fn possible_moving(&self) -> bool { !self.has_path() }

        #[inline]
        fn draw_highlighted_with_path_nodes(
            &self,
            window: &Window,
            camera: &Transform,
            _: Brushes,
            catalog: &ThingsCatalog,
            drawer: &mut EditDrawer
        )
        {
            self.draw_highlighted_selected(window, camera, drawer, catalog);
            self.path().unwrap().draw(window, camera, drawer, self.center());
        }

        #[inline]
        fn draw_with_highlighted_path_node(
            &self,
            window: &Window,
            camera: &Transform,
            _: Brushes,
            catalog: &ThingsCatalog,
            drawer: &mut EditDrawer,
            highlighted_node: usize
        )
        {
            self.draw_highlighted_selected(window, camera, drawer, catalog);
            self.path().unwrap().draw_with_highlighted_path_node(
                window,
                camera,
                drawer,
                self.center(),
                highlighted_node
            );
        }

        #[inline]
        fn draw_with_path_node_addition(
            &self,
            window: &Window,
            camera: &Transform,
            _: Brushes,
            catalog: &ThingsCatalog,
            drawer: &mut EditDrawer,
            pos: Vec2,
            index: usize
        )
        {
            self.draw_highlighted_selected(window, camera, drawer, catalog);
            self.path().unwrap().draw_with_node_insertion(
                window,
                camera,
                drawer,
                pos,
                index,
                self.center()
            );
        }

        #[inline]
        fn draw_movement_simulation(
            &self,
            window: &Window,
            camera: &Transform,
            _: Brushes,
            catalog: &ThingsCatalog,
            drawer: &mut EditDrawer,
            simulator: &MovementSimulator
        )
        {
            assert!(simulator.id() == self.id, "Simulator and thing have mismatching ids.");

            let movement_vec = simulator.movement_vec();

            drawer.thing(
                catalog,
                &MovedThingInstance {
                    thing: &self.data,
                    delta: movement_vec
                },
                Color::SelectedEntity
            );

            self.path().unwrap().draw_movement_simulation(
                window,
                camera,
                drawer,
                self.data.pos,
                movement_vec
            );
        }

        #[inline]
        fn draw_map_preview_movement_simulation(
            &self,
            _: &Transform,
            _: Brushes,
            catalog: &ThingsCatalog,
            drawer: &mut MapPreviewDrawer,
            animators: &Animators,
            simulator: &MovementSimulator
        )
        {
            assert!(self.id == simulator.id(), "Simulator's ID is not equal to the Thing's ID.");

            drawer.thing(
                catalog,
                &MovedThingInstance {
                    thing: &self.data,
                    delta: simulator.movement_vec()
                },
                animators
            );
        }
    }

    impl EditPath for ThingInstance
    {
        common_edit_path!();

        #[inline]
        fn set_path(&mut self, path: Path)
        {
            assert!(self.data.path.is_none(), "Thing already has a Path.");
            self.data.path = path.into();
        }

        #[inline]
        fn take_path(&mut self) -> Path { self.data.path.take_value().unwrap() }
    }

    impl ThingInstance
    {
        /// Returns a new [`ThingInstance`].
        #[inline]
        pub fn new(id: Id, thing: &Thing, pos: Vec2, default_properties: Properties) -> Self
        {
            let hull = ThingInstanceData::create_hull(pos, thing);

            Self {
                id,
                data: ThingInstanceData {
                    thing: thing.id,
                    pos,
                    hull,
                    path: None,
                    properties: default_properties
                }
            }
        }

        /// Creates a new [`ThingInstance`] from `id` and `data`.
        #[inline]
        pub const fn from_parts(id: Id, data: ThingInstanceData) -> Self { Self { id, data } }

        /// Returns a reference to the underlying [`ThingInstanceData`].
        #[inline]
        pub const fn data(&self) -> &ThingInstanceData { &self.data }

        /// Consumes `self` and returns the underlying [`ThingInstanceData`].
        #[inline]
        pub fn take_data(self) -> ThingInstanceData { self.data }

        #[inline]
        #[must_use]
        pub fn angle(&self) -> i16
        {
            match_or_panic!(self.data.properties.get(ANGLE_LABEL), Value::I16(value), *value)
        }

        /// Returns the draw height.
        #[inline]
        #[must_use]
        pub fn draw_height(&self) -> i8
        {
            match_or_panic!(self.data.properties.get(HEIGHT_LABEL), Value::I8(value), *value)
        }

        /// Returns a reference to the associated [`Properties`].
        #[inline]
        pub const fn properties(&self) -> &Properties { &self.data.properties }

        /// Whether the bounding box contains the point `p`.
        #[inline]
        #[must_use]
        pub fn contains_point(&self, p: Vec2) -> bool { self.data.hull.contains_point(p) }

        /// Returns a mutable reference to the thing's [`Path`].
        /// # Panics
        /// Panics if `self` has no associated [`Path`].
        #[inline]
        fn path_mut(&mut self) -> &mut Path { self.data.path.as_mut().unwrap() }

        /// Check whether changing the [`ThingId`] would cause `self` to have an out of bounds
        /// bounding box.
        #[inline]
        #[must_use]
        pub fn check_thing_change(&self, thing: &Thing) -> bool
        {
            let hull = ThingInstanceData::create_hull(self.data.pos, thing);
            !hull.out_of_bounds() && !self.path_hull_out_of_bounds(hull.center())
        }

        /// Sets `self` to represent an instance of another [`Thing`].
        #[inline]
        #[must_use]
        pub fn set_thing(&mut self, thing: &Thing) -> Option<ThingId> { self.data.set_thing(thing) }

        /// Check whether `self` can be moved without being out of bounds.
        #[inline]
        #[must_use]
        pub fn check_move(&self, delta: Vec2) -> bool
        {
            !(self.data.hull + delta).out_of_bounds() &&
                !self.path_hull_out_of_bounds(self.data.pos + delta)
        }

        /// Moves `self` by the vector `delta`.
        #[inline]
        pub fn move_by_delta(&mut self, delta: Vec2)
        {
            self.data.hull += delta;
            self.data.pos += delta;
        }

        /// Snaps `self` to the grid. Returns how much `self` was moved, if it was.
        #[inline]
        pub fn snap(&mut self, grid: &Grid) -> Option<Vec2>
        {
            let delta = grid.snap_point(self.center())?;
            self.check_move(delta).then_some(delta)
        }

        /// Sets the property `key` to `value`. Returns the previous value if different.
        #[inline]
        pub fn set_property(&mut self, key: &str, value: &Value) -> Option<Value>
        {
            self.data.properties.set(key, value)
        }

        /// Refactors the [`Peoperties`] based on `refactor`.
        #[inline]
        pub fn refactor_properties(&mut self, refactor: &PropertiesRefactor)
        {
            self.data.properties.refactor(refactor);
        }

        /// Draws `self` with the non selected color.
        #[inline]
        pub fn draw_non_selected(
            &self,
            window: &Window,
            camera: &Transform,
            drawer: &mut EditDrawer,
            catalog: &ThingsCatalog
        )
        {
            drawer.thing(catalog, self, Color::NonSelectedEntity);
            self.tooltip(window, camera, catalog, drawer);
        }

        /// Draws `self` with the selected color.
        #[inline]
        pub fn draw_selected(
            &self,
            window: &Window,
            camera: &Transform,
            drawer: &mut EditDrawer,
            catalog: &ThingsCatalog
        )
        {
            drawer.thing(catalog, self, Color::SelectedEntity);
            self.tooltip(window, camera, catalog, drawer);
        }

        /// Draws `self` with the highlighted non selected color.
        #[inline]
        pub fn draw_highlighted_non_selected(
            &self,
            window: &Window,
            camera: &Transform,
            drawer: &mut EditDrawer,
            catalog: &ThingsCatalog
        )
        {
            drawer.thing(catalog, self, Color::HighlightedNonSelectedEntity);
            self.tooltip(window, camera, catalog, drawer);
        }

        /// Draws `self` with the highlighted selected color.
        #[inline]
        pub fn draw_highlighted_selected(
            &self,
            window: &Window,
            camera: &Transform,
            drawer: &mut EditDrawer,
            catalog: &ThingsCatalog
        )
        {
            drawer.thing(catalog, self, Color::HighlightedSelectedEntity);
            self.tooltip(window, camera, catalog, drawer);
        }

        /// Draws `self` with the opaque color.
        #[inline]
        pub fn draw_opaque(
            &self,
            window: &Window,
            camera: &Transform,
            drawer: &mut EditDrawer,
            catalog: &ThingsCatalog
        )
        {
            drawer.thing(catalog, self, Color::OpaqueEntity);
            self.tooltip(window, camera, catalog, drawer);
        }

        /// Draws `self` as it would appear in a map.
        #[inline]
        pub fn draw_map_preview(
            &self,
            drawer: &mut MapPreviewDrawer,
            catalog: &ThingsCatalog,
            animators: &Animators
        )
        {
            drawer.thing(catalog, self, animators);
        }

        #[allow(clippy::cast_precision_loss)]
        #[inline]
        fn tooltip(
            &self,
            window: &Window,
            camera: &Transform,
            catalog: &ThingsCatalog,
            drawer: &mut EditDrawer
        )
        {
            let label = return_if_none!(drawer.tooltip_label());
            let thing = catalog.thing_or_error(self.data.thing);
            let grid = drawer.grid();

            let offset = if grid.isometric()
            {
                drawer.resources().texture_or_error(thing.preview()).size().y as f32
            }
            else
            {
                self.data.hull.half_height()
            };

            drawer.draw_tooltip_x_centered_above_pos(
                window,
                camera,
                label,
                thing.name(),
                self.data.hull.center(),
                Vec2::new(0f32, -(offset / camera.scale() + 8f32)),
                drawer.tooltip_text_color(),
                egui::Color32::WHITE
            );
        }
    }

    //=======================================================================//

    impl ThingViewer
    {
        /// Creates a new [`ThingViewer`].
        #[inline]
        pub(in crate::map) fn new(thing: ThingInstance) -> Self
        {
            let id = thing.id;
            let ThingInstanceData {
                thing,
                pos,
                path,
                properties,
                ..
            } = thing.data;

            Self {
                id,
                thing_id: thing,
                pos,
                path: path.map(Path::take_nodes),
                properties: properties.take()
            }
        }
    }

    //=======================================================================//

    /// A translated [`ThingInstance`].
    struct MovedThingInstance<'a>
    {
        /// The original [`ThingInstance`].
        thing: &'a ThingInstanceData,
        /// The translation vector.
        delta: Vec2
    }

    impl<'a> EntityHull for MovedThingInstance<'a>
    {
        #[inline]
        fn hull(&self) -> Hull { self.thing.hull() + self.delta }
    }

    impl<'a> ThingInterface for MovedThingInstance<'a>
    {
        #[inline]
        fn thing(&self) -> ThingId { self.thing.thing() }

        #[inline]
        fn pos(&self) -> Vec2 { self.thing.pos() + self.delta }

        #[inline]
        fn draw_height_f32(&self) -> f32 { self.thing.draw_height_f32() }

        #[inline]
        fn angle_f32(&self) -> f32 { self.thing.angle_f32() }
    }

    //=======================================================================//

    /// A resource containing all the [`Thing`]s to be hardcoded into the editor.
    #[must_use]
    #[derive(Resource, Default)]
    pub struct HardcodedThings(Vec<Thing>);

    impl<'a> IntoIterator for &'a HardcodedThings
    {
        type IntoIter = std::slice::Iter<'a, Thing>;
        type Item = &'a Thing;

        #[inline]
        fn into_iter(self) -> Self::IntoIter { self.iter() }
    }

    impl HardcodedThings
    {
        /// Returns a new empty [`HardcodedThings`].
        #[inline]
        pub fn new() -> Self { Self::default() }

        /// Pushes a new [`Thing`] from an object that implements the [`MapThing`] trait.
        #[inline]
        pub fn push<T: MapThing>(&mut self) { self.0.push(T::thing()); }

        /// Returns an iterator to the contained [`Thing`]s.
        #[inline]
        fn iter(&self) -> std::slice::Iter<Thing> { self.0.iter() }
    }
}

#[cfg(feature = "ui")]
pub use ui_mod::*;
