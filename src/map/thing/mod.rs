#[cfg(feature = "ui")]
pub(in crate::map) mod catalog;
#[cfg(feature = "ui")]
pub(in crate::map) mod compatibility;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::utils::HashMap;
use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{Id, Node, Value};

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
    /// Returns a new [`Thing`] with the requested properties.
    /// # Panics
    /// Panics if width and/or height are equal or less than zero.
    #[inline]
    pub fn new(name: &str, id: u16, width: f32, height: f32, preview: &str) -> Self
    {
        assert!(width > 0f32, "Thing named {name} with id {id} has width 0 or less.");
        assert!(height > 0f32, "Thing named {name} with id {id} has height 0 or less.");

        Self {
            name: name.to_string(),
            id: ThingId::new(id),
            width,
            height,
            preview: preview.to_string()
        }
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
    pub path:       Option<Vec<Node>>,
    /// The associated properties.
    pub properties: HashMap<String, Value>
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

    use bevy::{
        prelude::Resource,
        transform::components::Transform,
        utils::HashMap,
        window::Window
    };
    use bevy_egui::egui;
    use glam::Vec2;
    use hill_vacuum_shared::{match_or_panic, return_if_none};
    use serde::{Deserialize, Serialize};

    use super::{catalog::ThingsCatalog, Thing, ThingViewer};
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
            properties::{
                DefaultThingProperties,
                EngineDefaultThingProperties,
                Properties,
                PropertiesRefactor,
                ThingProperties,
                ANGLE_LABEL,
                HEIGHT_LABEL
            },
            OutOfBounds,
            Viewer,
            TOOLTIP_OFFSET
        },
        utils::{
            hull::Hull,
            identifiers::{EntityCenter, EntityId},
            misc::{Camera, ReplaceValue, TakeValue}
        },
        Id,
        Node,
        ThingId,
        Value
    };

    //=======================================================================//
    // TRAITS
    //
    //=======================================================================//

    /// A trait with methods returning basic information about a type representing a thing.
    #[allow(dead_code)]
    pub(in crate::map) trait ThingInterface
    {
        /// Returns the [`ThingId`].
        #[must_use]
        fn thing_id(&self) -> ThingId;

        /// Returns the position it is placed on the map.
        #[must_use]
        fn pos(&self) -> Vec2;

        /// The draw height of `self` as a float.
        #[must_use]
        fn draw_height_f32(&self) -> f32;

        /// The angle of `self`.
        #[must_use]
        fn angle_f32(&self) -> f32;

        fn thing_hull(&self, things_catalog: &ThingsCatalog) -> Hull;
    }

    //=======================================================================//
    // STRUCTS
    //
    //=======================================================================//

    #[must_use]
    #[derive(Serialize, Deserialize)]
    pub(in crate::map) struct ThingInstanceDataViewer
    {
        pub thing_id:   ThingId,
        pub pos:        Vec2,
        pub path:       Option<Vec<Node>>,
        pub properties: HashMap<String, Value>
    }

    impl From<super::compatibility::ThingInstanceDataViewer> for ThingInstanceDataViewer
    {
        #[inline]
        fn from(value: super::compatibility::ThingInstanceDataViewer) -> Self
        {
            let super::compatibility::ThingInstanceDataViewer {
                thing_id,
                pos,
                path,
                properties
            } = value;

            Self {
                thing_id,
                pos,
                path,
                properties: properties.0
            }
        }
    }

    //=======================================================================//

    /// The data of [`ThingInstance`].
    #[must_use]
    #[derive(Clone)]
    pub(in crate::map) struct ThingInstanceData
    {
        /// The [`ThingId`] of the [`Thing`] it represents.
        thing_id:   ThingId,
        /// The position on the map.
        pos:        Vec2,
        /// The path describing the [`ThingInstance`] movement, if any.
        path:       Option<Path>,
        /// The associated properties.
        properties: ThingProperties
    }

    impl Viewer for ThingInstanceData
    {
        type Item = ThingInstanceDataViewer;

        #[inline]
        fn from_viewer(value: Self::Item) -> Self
        {
            let Self::Item {
                thing_id,
                pos,
                path,
                properties
            } = value;

            Self {
                thing_id,
                pos,
                path: path.map(Path::from_viewer),
                properties: ThingProperties::from_parts(properties)
            }
        }

        #[inline]
        fn to_viewer(self) -> Self::Item
        {
            let Self {
                thing_id: thing,
                pos,
                path,
                properties,
                ..
            } = self;

            Self::Item {
                thing_id: thing,
                pos,
                path: path.map(Path::to_viewer),
                properties: properties.take()
            }
        }
    }

    impl ThingInterface for ThingInstanceData
    {
        #[inline]
        fn thing_id(&self) -> ThingId { self.thing_id }

        #[inline]
        fn pos(&self) -> Vec2 { self.pos }

        #[inline]
        fn draw_height_f32(&self) -> f32 { f32::from(self.draw_height()) }

        #[inline]
        fn angle_f32(&self) -> f32 { f32::from(self.angle()) }

        #[inline]
        fn thing_hull(&self, things_catalog: &ThingsCatalog) -> Hull
        {
            Self::new_thing_hull(things_catalog, self.thing_id, self.pos)
        }
    }

    impl ThingInstanceData
    {
        #[inline]
        fn new_thing_hull(things_catalog: &ThingsCatalog, thing_id: ThingId, pos: Vec2) -> Hull
        {
            let thing = things_catalog.thing_or_error(thing_id);
            let half_width = thing.width / 2f32;
            let half_height = thing.height / 2f32;

            Hull::from_opposite_vertexes(
                pos + Vec2::new(-half_width, -half_height),
                pos + Vec2::new(half_width, half_height)
            )
        }

        /// Return the [`Hull`] of the associated [`Path`], if any.
        #[inline]
        pub fn path_hull(&self) -> Option<Hull>
        {
            self.path.as_ref().map(|path| path.hull() + self.pos)
        }

        #[inline]
        pub fn hull(&self, things_catalog: &ThingsCatalog) -> Hull
        {
            let hull = self.thing_hull(things_catalog);

            match self.path_hull()
            {
                Some(h) => hull.merged(&h),
                None => hull
            }
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
        pub fn set_thing(&mut self, thing_id: ThingId) -> Option<ThingId>
        {
            if thing_id == self.thing_id
            {
                return None;
            }

            self.thing_id.replace_value(thing_id).into()
        }

        /// Draw `self` displaced by `delta` for a prop screenshot.
        #[inline]
        pub fn draw_prop(&self, drawer: &mut EditDrawer, catalog: &ThingsCatalog, delta: Vec2)
        {
            drawer.thing_texture(
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

    impl Viewer for ThingInstance
    {
        type Item = ThingViewer;

        #[inline]
        fn from_viewer(value: Self::Item) -> Self
        {
            let Self::Item {
                id,
                thing_id,
                pos,
                path,
                properties
            } = value;

            Self {
                id,
                data: ThingInstanceData::from_viewer(ThingInstanceDataViewer {
                    thing_id,
                    pos,
                    path,
                    properties
                })
            }
        }

        #[inline]
        fn to_viewer(self) -> Self::Item
        {
            let id = self.id;
            let ThingInstanceDataViewer {
                thing_id,
                pos,
                path,
                properties
            } = self.data.to_viewer();

            Self::Item {
                id,
                thing_id,
                pos,
                path,
                properties
            }
        }
    }

    impl ThingInterface for ThingInstance
    {
        #[inline]
        fn thing_id(&self) -> ThingId { self.data.thing_id }

        #[inline]
        fn pos(&self) -> Vec2 { self.data.pos }

        #[inline]
        fn draw_height_f32(&self) -> f32 { self.data.draw_height_f32() }

        #[inline]
        fn angle_f32(&self) -> f32 { self.data.angle_f32() }

        #[inline]
        fn thing_hull(&self, things_catalog: &ThingsCatalog) -> Hull
        {
            self.data.thing_hull(things_catalog)
        }
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
        pub fn new(
            id: Id,
            thing_id: ThingId,
            pos: Vec2,
            default_properties: &DefaultThingProperties
        ) -> Self
        {
            Self {
                id,
                data: ThingInstanceData {
                    thing_id,
                    pos,
                    path: None,
                    properties: default_properties.instance()
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
        pub const fn properties(&self) -> &ThingProperties { &self.data.properties }

        /// Returns the overall [`Hull`] of both the thing and the [`Path`].
        #[inline]
        pub fn hull(&self, things_catalog: &ThingsCatalog) -> Hull
        {
            self.data.hull(things_catalog)
        }

        /// Whether the bounding box contains the point `p`.
        #[inline]
        #[must_use]
        pub fn contains_point(&self, things_catalog: &ThingsCatalog, p: Vec2) -> bool
        {
            self.data.thing_hull(things_catalog).contains_point(p)
        }

        /// Returns a mutable reference to the thing's [`Path`].
        /// # Panics
        /// Panics if `self` has no associated [`Path`].
        #[inline]
        fn path_mut(&mut self) -> &mut Path { self.data.path.as_mut().unwrap() }

        /// Check whether changing the [`ThingId`] would cause `self` to have an out of bounds
        /// bounding box.
        #[inline]
        #[must_use]
        pub fn check_thing_change(&self, things_catalog: &ThingsCatalog, thing_id: ThingId)
            -> bool
        {
            !ThingInstanceData::new_thing_hull(things_catalog, thing_id, self.data.pos)
                .out_of_bounds()
        }

        /// Sets `self` to represent an instance of another [`Thing`].
        #[inline]
        #[must_use]
        pub fn set_thing(&mut self, thing_id: ThingId) -> Option<ThingId>
        {
            self.data.set_thing(thing_id)
        }

        /// Check whether `self` can be moved without being out of bounds.
        #[inline]
        #[must_use]
        pub fn check_move(&self, things_catalog: &ThingsCatalog, delta: Vec2) -> bool
        {
            !(self.hull(things_catalog) + delta).out_of_bounds()
        }

        /// Moves `self` by the vector `delta`.
        #[inline]
        pub fn move_by_delta(&mut self, delta: Vec2) { self.data.pos += delta; }

        /// Snaps `self` to the grid. Returns how much `self` was moved, if it was.
        #[inline]
        pub fn snap(&mut self, things_catalog: &ThingsCatalog, grid: &Grid) -> Option<Vec2>
        {
            let delta = grid.snap_point(self.center())?;
            self.check_move(things_catalog, delta).then_some(delta)
        }

        /// Sets the property `key` to `value`. Returns the previous value if different.
        #[inline]
        pub fn set_property(&mut self, key: &str, value: &Value) -> Option<Value>
        {
            self.data.properties.set(key, value)
        }

        /// Refactors the [`Peoperties`] based on `refactor`.
        #[inline]
        pub fn refactor_properties(
            &mut self,
            refactor: &PropertiesRefactor<EngineDefaultThingProperties>
        )
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
            let thing = catalog.thing_or_error(self.data.thing_id);
            let grid = drawer.grid();

            let offset = if grid.isometric()
            {
                drawer.resources().texture_or_error(thing.preview()).size().y as f32
            }
            else
            {
                self.thing_hull(catalog).half_height()
            };

            drawer.draw_tooltip_x_centered_above_pos(
                window,
                camera,
                label,
                thing.name(),
                self.center(),
                Vec2::new(0f32, -offset / camera.scale() + TOOLTIP_OFFSET.y),
                drawer.tooltip_text_color(),
                egui::Color32::WHITE
            );
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

    impl ThingInterface for MovedThingInstance<'_>
    {
        #[inline]
        fn thing_id(&self) -> ThingId { self.thing.thing_id() }

        #[inline]
        fn pos(&self) -> Vec2 { self.thing.pos() + self.delta }

        #[inline]
        fn draw_height_f32(&self) -> f32 { self.thing.draw_height_f32() }

        #[inline]
        fn angle_f32(&self) -> f32 { self.thing.angle_f32() }

        #[inline]
        fn thing_hull(&self, things_catalog: &ThingsCatalog) -> Hull
        {
            self.thing.thing_hull(things_catalog) + self.delta
        }
    }

    //=======================================================================//

    /// A resource containing all the [`Thing`]s to be hardcoded into the editor.
    #[must_use]
    #[derive(Resource, Default)]
    pub(crate) struct HardcodedThings(pub Vec<Thing>);
}

#[cfg(feature = "ui")]
pub(crate) use ui_mod::*;
