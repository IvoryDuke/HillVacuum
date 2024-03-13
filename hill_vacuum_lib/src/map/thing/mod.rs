#[allow(clippy::module_name_repetitions)]
pub mod catalog;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::{prelude::Vec2, transform::components::Transform, window::Window};
use bevy_egui::egui;
use serde::{Deserialize, Serialize};
use shared::{draw_height_to_world, TEXTURE_HEIGHT_RANGE};

use self::catalog::ThingsCatalog;
use super::{
    containers::HvVec,
    drawer::{color::Color, EditDrawer, MapPreviewDrawer},
    editor::state::{
        clipboard::{ClipboardData, CopyToClipboard},
        manager::{Animators, Brushes}
    },
    path::{common_edit_path, EditPath, MovementSimulator, Moving, NodesDeletionPayload, Path},
    OutOfBounds
};
use crate::utils::{
    hull::{EntityHull, Hull},
    identifiers::{EntityCenter, EntityId, Id},
    math::AroundEqual
};

//=======================================================================//
// TRAITS
//
//=======================================================================//

/// A trait to associate a [`Thing`] to a type.
pub trait MapThing
{
    /// Returns the [`Thing`] associated with `self`.
    fn thing() -> Thing;
}

//=======================================================================//

pub trait ThingInterface
{
    /// Returns the [`ThingId`].
    #[must_use]
    fn thing(&self) -> ThingId;

    /// Returns the position it is placed on the map.
    #[must_use]
    fn pos(&self) -> Vec2;

    #[must_use]
    fn draw_height_f32(&self) -> f32;

    #[must_use]
    fn angle(&self) -> f32;
}

//=======================================================================//
// TYPES
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
    pub fn new(id: u16) -> Self { Self(id) }
}

//=======================================================================//

/// A [`Thing`] which can be used to create map placeable items.
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
    /// Returns a new [`Thing`] with the requested parameters, unless width or height are equal or
    /// less than zero.
    #[must_use]
    #[inline]
    pub fn new(name: &str, id: u16, width: f32, height: f32, preview: &str) -> Option<Self>
    {
        (width > 0f32 && height > 0f32).then(|| {
            Self {
                name: name.to_string(),
                id: ThingId(id),
                width,
                height,
                preview: preview.to_string()
            }
        })
    }

    /// Returns the [`ThingId`].
    #[inline]
    #[must_use]
    pub fn id(&self) -> ThingId { self.id }

    /// Returns the width of the bounding box.
    #[inline]
    #[must_use]
    pub fn width(&self) -> f32 { self.width }

    /// Returns the height of the bounding box.
    #[inline]
    #[must_use]
    pub fn height(&self) -> f32 { self.height }
}

//=======================================================================//

struct MovedThingInstance<'a>
{
    thing: &'a ThingInstance,
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
    fn thing(&self) -> ThingId { self.thing.thing }

    #[inline]
    fn pos(&self) -> Vec2 { self.thing.pos + self.delta }

    #[inline]
    fn draw_height_f32(&self) -> f32 { self.thing.draw_height_f32() }

    #[inline]
    fn angle(&self) -> f32 { self.thing.angle }
}

//=======================================================================//

/// An instance of a [`Thing`] which can be placed in a map.
#[must_use]
#[derive(Clone, Serialize, Deserialize)]
pub(in crate::map) struct ThingInstance
{
    /// The id.
    id:          Id,
    /// The [`ThingId`] of the [`Thing`] it represents.
    thing:       ThingId,
    /// The position on the map.
    pos:         Vec2,
    /// The spawn angle of the [`Thing`] in the map.
    angle:       f32,
    /// The height its preview should be drawn.
    draw_height: i8,
    /// The bounding box.
    hull:        Hull,
    center:      Vec2,
    path:        Option<Path>,
    path_edited: bool
}

impl EntityHull for ThingInstance
{
    #[inline]
    fn hull(&self) -> Hull { self.hull }
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
    fn center(&self) -> Vec2 { self.center }
}

impl CopyToClipboard for ThingInstance
{
    #[inline]
    fn copy_to_clipboard(&self) -> ClipboardData
    {
        ClipboardData::Thing(self.thing, self.id, self.hull)
    }
}

impl Moving for ThingInstance
{
    #[inline]
    fn path(&self) -> Option<&Path> { self.path.as_ref() }

    #[inline]
    fn has_path(&self) -> bool { self.path.is_some() }

    #[inline]
    fn possible_moving(&self) -> bool { !self.has_path() }

    #[inline]
    fn draw_highlighted_with_path_nodes(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        _: Brushes,
        catalog: &ThingsCatalog,
        drawer: &mut EditDrawer,
        show_tooltips: bool
    )
    {
        self.draw_highlighted_selected(drawer, catalog);
        self.path().unwrap().draw(
            window,
            camera,
            egui_context,
            drawer,
            self.center(),
            show_tooltips
        );
    }

    #[inline]
    fn draw_with_highlighted_path_node(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        _: Brushes,
        catalog: &ThingsCatalog,
        drawer: &mut EditDrawer,
        highlighted_node: usize,
        show_tooltips: bool
    )
    {
        self.draw_highlighted_selected(drawer, catalog);
        self.path().unwrap().draw_with_highlighted_path_node(
            window,
            camera,
            egui_context,
            drawer,
            self.center(),
            highlighted_node,
            show_tooltips
        );
    }

    #[inline]
    fn draw_with_path_node_addition(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        _: Brushes,
        catalog: &ThingsCatalog,
        drawer: &mut EditDrawer,
        pos: Vec2,
        idx: usize,
        show_tooltips: bool
    )
    {
        self.draw_highlighted_selected(drawer, catalog);
        self.path().unwrap().draw_with_node_insertion(
            window,
            camera,
            egui_context,
            drawer,
            pos,
            idx,
            self.center(),
            show_tooltips
        );
    }

    #[inline]
    fn draw_movement_simulation(
        &self,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context,
        _: Brushes,
        catalog: &ThingsCatalog,
        drawer: &mut EditDrawer,
        show_tooltips: bool,
        simulator: &MovementSimulator
    )
    {
        assert!(simulator.id() == self.id);

        let movement_vec = simulator.movement_vec();

        drawer.thing(
            catalog,
            &MovedThingInstance {
                thing: self,
                delta: movement_vec
            },
            Color::SelectedEntity
        );

        self.path().unwrap().draw_movement_simulation(
            window,
            camera,
            egui_context,
            drawer,
            self.center,
            movement_vec,
            show_tooltips
        );
    }

    #[inline]
    fn draw_map_preview_movement_simulation(
        &self,
        _: &Transform,
        _: Brushes,
        catalog: &ThingsCatalog,
        drawer: &mut MapPreviewDrawer,
        _: &Animators,
        simulator: &MovementSimulator
    )
    {
        assert!(self.id == simulator.id(), "Simulator's ID is not equal to the Thing's ID.");

        drawer.thing(catalog, &MovedThingInstance {
            thing: self,
            delta: simulator.movement_vec()
        });
    }
}

impl ThingInterface for ThingInstance
{
    #[inline]
    fn thing(&self) -> ThingId { self.thing }

    #[inline]
    fn pos(&self) -> Vec2 { self.pos }

    #[inline]
    fn draw_height_f32(&self) -> f32 { draw_height_to_world(self.draw_height) }

    #[inline]
    fn angle(&self) -> f32 { self.angle }
}

impl EditPath for ThingInstance
{
    common_edit_path!();

    #[inline]
    fn set_path(&mut self, path: Path)
    {
        assert!(self.path.is_none());

        self.path_edited = true;
        self.path = path.into();
    }

    #[inline]
    fn take_path(&mut self) -> Path
    {
        self.path_edited = true;
        std::mem::take(&mut self.path).unwrap()
    }
}

impl ThingInstance
{
    /// Returns a new [`ThingInstance`].
    #[inline]
    pub fn new(id: Id, thing: &Thing, pos: Vec2) -> Self
    {
        let hull = Self::create_hull(pos, thing);

        Self {
            id,
            thing: thing.id,
            pos,
            draw_height: 0,
            angle: 0f32,
            hull,
            center: hull.center(),
            path: None,
            path_edited: false
        }
    }

    /// Computes the bounding box.
    #[inline]
    #[must_use]
    fn create_hull(pos: Vec2, thing: &Thing) -> Hull
    {
        let half_width = thing.width / 2f32;
        let half_height = thing.height / 2f32;

        Hull::from_opposite_vertexes(
            pos + Vec2::new(-half_width, half_height),
            pos + Vec2::new(half_width, -half_height)
        )
        .unwrap()
    }

    /// Returns the draw height.
    #[inline]
    #[must_use]
    pub fn draw_height(&self) -> i8 { self.draw_height }

    /// Whever the bounding box contains the point `p`.
    #[inline]
    #[must_use]
    pub fn contains_point(&self, p: Vec2) -> bool { self.hull.contains_point(p) }

    #[inline]
    fn path_mut(&mut self) -> &mut Path { self.path.as_mut().unwrap() }

    #[inline]
    fn path_mut_set_dirty(&mut self) -> &mut Path
    {
        self.path_edited = true;
        self.path_mut()
    }

    /// Check whever changing the [`ThingId`] would cause `self` to have an out of bounds bounding
    /// box.
    #[inline]
    #[must_use]
    pub fn check_thing_change(&self, thing: &Thing) -> bool
    {
        let hull = Self::create_hull(self.pos, thing);
        !hull.out_of_bounds() && !self.path_hull_out_of_bounds(hull.center())
    }

    /// Sets `self` to represent an instance of another [`Thing`].
    #[inline]
    #[must_use]
    pub fn set_thing(&mut self, thing: &Thing) -> Option<ThingId>
    {
        self.hull = Self::create_hull(self.pos, thing);
        self.center = self.hull.center();

        if thing.id == self.thing
        {
            return None;
        }

        std::mem::replace(&mut self.thing, thing.id).into()
    }

    /// Check whever `self` can be moved without being out of bounds.
    #[inline]
    #[must_use]
    pub fn check_move(&self, delta: Vec2) -> bool
    {
        !(self.hull + delta).out_of_bounds() && !self.path_hull_out_of_bounds(self.center + delta)
    }

    /// Moves `self` by the vector `delta`.
    #[inline]
    pub fn move_by_delta(&mut self, delta: Vec2)
    {
        self.hull += delta;
        self.pos += delta;
        self.center += delta;
    }

    /// Sets the draw height to `height`.
    #[inline]
    #[must_use]
    pub fn set_draw_height(&mut self, height: i8) -> Option<i8>
    {
        let height = height.clamp(*TEXTURE_HEIGHT_RANGE.start(), *TEXTURE_HEIGHT_RANGE.end());

        if height == self.draw_height
        {
            return None;
        }

        std::mem::replace(&mut self.draw_height, height).into()
    }

    #[inline]
    #[must_use]
    pub fn set_angle(&mut self, angle: f32) -> Option<f32>
    {
        let angle = angle.floor().rem_euclid(360f32);

        if angle.around_equal_narrow(&self.angle)
        {
            return None;
        }

        std::mem::replace(&mut self.angle, angle).into()
    }

    #[inline]
    #[must_use]
    pub fn was_path_edited(&mut self) -> bool { std::mem::replace(&mut self.path_edited, false) }

    /// Draws `self` with the non selected color.
    #[inline]
    pub fn draw_non_selected(&self, drawer: &mut EditDrawer, catalog: &ThingsCatalog)
    {
        drawer.thing(catalog, self, Color::NonSelectedEntity);
    }

    /// Draws `self` with the selected color.
    #[inline]
    pub fn draw_selected(&self, drawer: &mut EditDrawer, catalog: &ThingsCatalog)
    {
        drawer.thing(catalog, self, Color::SelectedEntity);
    }

    /// Draws `self` with the highlighted non selected color.
    #[inline]
    pub fn draw_highlighted_non_selected(&self, drawer: &mut EditDrawer, catalog: &ThingsCatalog)
    {
        drawer.thing(catalog, self, Color::HighlightedNonSelectedBrush);
    }

    /// Draws `self` with the highlighted selected color.
    #[inline]
    pub fn draw_highlighted_selected(&self, drawer: &mut EditDrawer, catalog: &ThingsCatalog)
    {
        drawer.thing(catalog, self, Color::HighlightedSelectedBrush);
    }

    /// Draws `self` with the opaque color.
    #[inline]
    pub fn draw_opaque(&self, drawer: &mut EditDrawer, catalog: &ThingsCatalog)
    {
        drawer.thing(catalog, self, Color::Opaque);
    }

    /// Draws `self` as it would appear in a map.
    #[inline]
    pub fn draw_map_preview(&self, drawer: &mut MapPreviewDrawer, catalog: &ThingsCatalog)
    {
        drawer.thing(catalog, self);
    }

    /// Draws `self` as required to appear in a prop preview.
    #[inline]
    pub fn draw_prop(&self, drawer: &mut EditDrawer, catalog: &ThingsCatalog, delta: Vec2)
    {
        drawer.thing(catalog, &MovedThingInstance { thing: self, delta }, Color::NonSelectedEntity);
    }
}

//=======================================================================//

pub struct ThingViewer
{
    pub id:          Id,
    pub thing_id:    ThingId,
    pub pos:         Vec2,
    pub angle:       f32,
    pub draw_height: f32,
    pub path:        Option<Path>
}

impl ThingViewer
{
    #[inline]
    pub(in crate::map) fn new(mut thing: ThingInstance) -> Self
    {
        let id = thing.id;
        let thing_id = thing.thing;
        let pos = thing.pos;
        let angle = thing.angle;
        let draw_height = thing.draw_height_f32();

        Self {
            id,
            thing_id,
            pos,
            angle,
            draw_height,
            path: std::mem::take(&mut thing.path)
        }
    }
}
