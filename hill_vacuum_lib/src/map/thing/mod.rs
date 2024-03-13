#[allow(clippy::module_name_repetitions)]
pub mod catalog;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::Vec2;
use serde::{Deserialize, Serialize};
use shared::{draw_height_to_world, TEXTURE_HEIGHT_RANGE};

use self::catalog::ThingsCatalog;
use super::{
    drawer::{animation::Animator, color::Color, EditDrawer, MapPreviewDrawer},
    editor::state::clipboard::{ClipboardData, CopyToClipboard},
    OutOfBounds
};
use crate::utils::{
    hull::{EntityHull, Hull},
    identifiers::{EntityId, Id},
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

/// An instance of a [`Thing`] which can be placed in a map.
#[must_use]
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct ThingInstance
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
    hull:        Hull
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

impl CopyToClipboard for ThingInstance
{
    #[inline]
    fn copy_to_clipboard(&self) -> ClipboardData
    {
        ClipboardData::Thing(self.thing, self.id, self.hull)
    }
}

impl ThingInstance
{
    /// Returns a new [`ThingInstance`].
    #[inline]
    pub(in crate::map) fn new(id: Id, thing: &Thing, pos: Vec2) -> Self
    {
        Self {
            id,
            thing: thing.id,
            pos,
            draw_height: 0,
            angle: 0f32,
            hull: Self::create_hull(pos, thing)
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

    /// Returns the [`ThingId`].
    #[inline]
    #[must_use]
    pub fn thing(&self) -> ThingId { self.thing }

    /// Returns the draw height.
    #[inline]
    #[must_use]
    pub fn draw_height(&self) -> i8 { self.draw_height }

    #[inline]
    #[must_use]
    pub fn draw_height_f32(&self) -> f32 { draw_height_to_world(self.draw_height) }

    /// Returns the position it is placed on the map.
    #[inline]
    #[must_use]
    pub fn pos(&self) -> Vec2 { self.pos }

    #[inline]
    #[must_use]
    pub fn angle(&self) -> f32 { self.angle }

    /// Whever the bounding box contains the point `p`.
    #[inline]
    #[must_use]
    pub(in crate::map) fn contains_point(&self, p: Vec2) -> bool { self.hull.contains_point(p) }

    /// Check whever changing the [`ThingId`] would cause `self` to have an out of bounds bounding
    /// box.
    #[inline]
    #[must_use]
    pub(in crate::map) fn check_thing_change(&self, thing: &Thing) -> bool
    {
        !Self::create_hull(self.pos, thing).out_of_bounds()
    }

    /// Sets `self` to represent an instance of another [`Thing`].
    #[inline]
    #[must_use]
    pub(in crate::map) fn set_thing(&mut self, thing: &Thing) -> Option<ThingId>
    {
        self.hull = Self::create_hull(self.pos, thing);

        if thing.id == self.thing
        {
            return None;
        }

        std::mem::replace(&mut self.thing, thing.id).into()
    }

    /// Check whever `self` can be moved without being out of bounds.
    #[inline]
    #[must_use]
    pub(in crate::map) fn check_move(&self, delta: Vec2) -> bool
    {
        !(self.hull + delta).out_of_bounds()
    }

    /// Moves `self` by the vector `delta`.
    #[inline]
    pub(in crate::map) fn move_by_delta(&mut self, delta: Vec2)
    {
        self.hull += delta;
        self.pos += delta;
    }

    /// Sets the draw height to `height`.
    #[inline]
    #[must_use]
    pub(in crate::map) fn set_draw_height(&mut self, height: i8) -> Option<i8>
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
    pub(in crate::map) fn set_angle(&mut self, angle: f32) -> Option<f32>
    {
        let angle = angle.floor().rem_euclid(360f32);

        if angle.around_equal_narrow(&self.angle)
        {
            return None;
        }

        std::mem::replace(&mut self.angle, angle).into()
    }

    /// Draws `self` with the non selected color.
    #[inline]
    pub(in crate::map) fn draw_non_selected(&self, drawer: &mut EditDrawer, catalog: &ThingsCatalog)
    {
        drawer.thing(catalog, self, Color::NonSelectedBrush);
    }

    /// Draws `self` with the selected color.
    #[inline]
    pub(in crate::map) fn draw_selected(&self, drawer: &mut EditDrawer, catalog: &ThingsCatalog)
    {
        drawer.thing(catalog, self, Color::SelectedBrush);
    }

    /// Draws `self` with the highlighted non selected color.
    #[inline]
    pub(in crate::map) fn draw_highlighted_non_selected(
        &self,
        drawer: &mut EditDrawer,
        catalog: &ThingsCatalog
    )
    {
        drawer.thing(catalog, self, Color::HighlightedNonSelectedBrush);
    }

    /// Draws `self` with the highlighted selected color.
    #[inline]
    pub(in crate::map) fn draw_highlighted_selected(
        &self,
        drawer: &mut EditDrawer,
        catalog: &ThingsCatalog
    )
    {
        drawer.thing(catalog, self, Color::HighlightedSelectedBrush);
    }

    /// Draws `self` with the opaque color.
    #[inline]
    pub(in crate::map) fn draw_opaque(&self, drawer: &mut EditDrawer, catalog: &ThingsCatalog)
    {
        drawer.thing(catalog, self, Color::OpaqueBrush);
    }

    /// Draws `self` as it would appear in a map.
    #[inline]
    pub(in crate::map) fn draw_map_preview(
        &self,
        drawer: &mut MapPreviewDrawer,
        catalog: &ThingsCatalog,
        animator: Option<&Animator>
    )
    {
        drawer.thing(catalog, self, animator);
    }

    /// Draws `self` as required to appear in a prop preview.
    #[inline]
    pub(in crate::map) fn draw_prop(
        &self,
        drawer: &mut EditDrawer,
        catalog: &ThingsCatalog,
        delta: Vec2
    )
    {
        let mut prop = *self;
        prop.pos += delta;

        drawer.thing(catalog, &prop, Color::HighlightedNonSelectedBrush);
    }
}
