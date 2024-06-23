//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::path::Path;

use bevy::{
    ecs::system::{Res, Resource},
    math::{UVec2, Vec2}
};
use bevy_egui::egui;
use configparser::ini::Ini;
use hill_vacuum_shared::{continue_if_err, continue_if_none};

use super::{Thing, ThingId, ThingInstance};
use crate::{
    map::{
        containers::{hv_hash_map, hv_vec},
        drawer::drawing_resources::DrawingResources,
        indexed_map::IndexedMap,
        properties::DefaultProperties,
        AssertedInsertRemove,
        HvHashMap,
        HvVec
    },
    utils::identifiers::Id,
    MapThing
};

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The catalog of all the available [`Thing`]s.
#[must_use]
pub(in crate::map) struct ThingsCatalog
{
    /// The [`Thing`]s that are hardcoded in the editor.
    hardcoded_things: HvHashMap<ThingId, Thing>,
    /// All the loaded [`Thing`]s, both hardcoded and from files.
    things:           IndexedMap<ThingId, Thing>,
    /// The [`Thing`] selected in the UI gallery, if any.
    selected_thing:   Option<usize>,
    ///The [`Thing`] used to display errors.
    error:            Thing
}

impl Default for ThingsCatalog
{
    #[inline]
    fn default() -> Self
    {
        Self {
            hardcoded_things: hv_hash_map![],
            things:           IndexedMap::new(hv_vec![], Thing::id),
            selected_thing:   None,
            error:            Self::error_thing()
        }
    }
}

impl ThingsCatalog
{
    /// The identifier reserved to the [`Thing`] representing errors.
    const ERROR_ID: u16 = u16::MAX;

    //==============================================================
    // New

    /// Returns a new [`ThingsCatalog`].
    #[inline]
    pub fn new(hardcoded_things: Option<Res<HardcodedThings>>) -> Self
    {
        let mut h_things = hv_hash_map![];

        if let Some(hardcoded_things) = hardcoded_things
        {
            for thing in hardcoded_things.as_ref()
            {
                h_things.asserted_insert((thing.id, thing.clone()));
            }
        }

        let things = Self::loaded_things(&h_things);
        let selected_thing = (!things.is_empty()).then_some(0);

        Self {
            hardcoded_things: h_things,
            things,
            selected_thing,
            error: Self::error_thing()
        }
    }

    /// The [`Thing`] representing an error.
    #[inline]
    fn error_thing() -> Thing
    {
        Thing::new("error", Self::ERROR_ID, 64f32, 64f32, "error").unwrap()
    }

    /// Combines the hardcoded and file loaded things into a single [`IndexedMap`].
    /// If a thing loaded from file has the same [`ThingId`] as an hardcoded one the latter will be
    /// overwritten. Things files are searched in the `assets/things/` folder.
    #[inline]
    fn loaded_things(hardcoded_things: &HvHashMap<ThingId, Thing>) -> IndexedMap<ThingId, Thing>
    {
        /// The directory where ini defined things are located.
        const THINGS_DIR: &str = "assets/things/";

        /// Gathers all ini files.
        #[inline]
        fn recurse(path: &Path, inis: &mut HvVec<Ini>)
        {
            if path.is_file()
            {
                let mut ini = Ini::new_cs();

                if ini.load(path).is_ok()
                {
                    inis.push(ini);
                }

                return;
            }

            for entry in std::fs::read_dir(path).unwrap()
            {
                recurse(&entry.unwrap().path(), inis);
            }
        }

        std::fs::create_dir_all(THINGS_DIR).ok();
        let mut configs = hv_vec![];
        recurse(Path::new(THINGS_DIR), &mut configs);

        let mut things = hv_vec![collect; hardcoded_things.values().cloned()];

        for ini in configs
        {
            'outer: for (name, values) in ini.get_map_ref()
            {
                /// Returns the value associated to `key` of type `t`, it it exists.
                /// Otherwise the thing loading is aborted.
                macro_rules! value {
                    ($key:literal, $t:ty) => {
                        continue_if_err!(continue_if_none!(values.get($key))
                            .as_ref()
                            .unwrap()
                            .parse::<$t>())
                    };
                }

                let id = value!("id", u16);

                if id == Self::ERROR_ID
                {
                    continue;
                }

                let new_thing = continue_if_none!(Thing::new(
                    name,
                    id,
                    value!("width", f32),
                    value!("height", f32),
                    continue_if_none!(values.get("preview")).as_ref().unwrap()
                ));
                let id = new_thing.id;

                for thing in &mut things
                {
                    if thing.id == id
                    {
                        *thing = new_thing;
                        continue 'outer;
                    }
                }

                things.push(new_thing);
            }
        }

        things.sort_by(|a, b| a.name.cmp(&b.name));
        IndexedMap::new(things, |thing| thing.id)
    }

    //==============================================================
    // Info

    /// Whever the [`ThingsCatalog`] contains any [`Thing`].
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool { self.things.is_empty() }

    /// The [`Thing`] associated with `thing`, if any.
    #[inline]
    pub fn thing(&self, thing: ThingId) -> Option<&Thing> { self.things.get(&thing) }

    /// The [`Thing`] representing an error.
    #[inline]
    pub const fn error(&self) -> &Thing { &self.error }

    /// Returns a reference to the [`Thing`] with the associated [`ThingId`].
    /// # Panics
    /// Panics if there is no [`Thing`] with such id.
    #[inline]
    pub fn thing_or_error(&self, thing: ThingId) -> &Thing
    {
        self.thing(thing).unwrap_or(&self.error)
    }

    /// Returns the name of the preview texture of the [`Thing`] with the associated [`ThingId`].
    #[inline]
    #[must_use]
    pub fn texture(&self, thing: ThingId) -> &str { &self.thing_or_error(thing).preview }

    /// Returns a reference to the [`Thing`] selected in the UI gallery.
    /// # Panics
    /// Panics if no [`Thing`] is selected.
    #[inline]
    pub fn selected_thing(&self) -> &Thing { &self.things[self.selected_thing.unwrap()] }

    /// Returns the index of the [`Thing`] selected in the UI gallery, if any.
    #[inline]
    pub const fn selected_thing_index(&self) -> Option<usize> { self.selected_thing }

    /// Returns a reference to the [`Thing`] at `index`.
    #[inline]
    pub fn thing_at_index(&self, index: usize) -> &Thing { &self.things[index] }

    //==============================================================
    // Edit

    /// Generates a [`ThingInstance`] from the provided values.
    #[inline]
    pub fn thing_instance(
        &self,
        id: Id,
        thing: ThingId,
        pos: Vec2,
        default_properties: &DefaultProperties
    ) -> ThingInstance
    {
        ThingInstance::new(id, self.thing_or_error(thing), pos, default_properties.instance())
    }

    /// Sets the selected thing index.
    #[inline]
    pub fn set_selected_thing_index(&mut self, index: usize)
    {
        assert!(
            index < self.things.len(),
            "Index {index} is out of bounds, length of things is {}",
            self.things.len()
        );
        self.selected_thing = index.into();
    }

    /// Reloads the [`Thing`]s from the files.
    #[inline]
    pub fn reload_things(&mut self)
    {
        self.things = Self::loaded_things(&self.hardcoded_things);
        self.selected_thing = (!self.things.is_empty()).then_some(0);
    }

    //==============================================================
    // Iterators

    /// Returns a [`Chunks`] iterator that iterates over the indexes, texture ids, and names of the
    /// [`Thing`]s.
    #[inline]
    #[must_use]
    pub fn chunked_things<'a>(
        &'a self,
        chunk_size: usize,
        drawing_resources: &'a DrawingResources
    ) -> impl ExactSizeIterator<Item = impl Iterator<Item = (usize, egui::TextureId, UVec2, &'a str)>>
    {
        self.things
            .chunks(chunk_size)
            .enumerate()
            .map(move |(index, things)| {
                let mut index = index * chunk_size;

                things.iter().map(move |thing| {
                    let texture = drawing_resources.egui_texture(&thing.preview);
                    let value = (index, texture.0, texture.1, thing.name.as_str());
                    index += 1;
                    value
                })
            })
    }
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
