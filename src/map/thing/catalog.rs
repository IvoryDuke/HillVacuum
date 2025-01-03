//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::path::Path;

use bevy_egui::egui;
use configparser::ini::Ini;
use glam::UVec2;
use hill_vacuum_shared::{continue_if_err, continue_if_none};

use super::{HardcodedThings, Thing, ThingId};
use crate::{
    map::{drawer::drawing_resources::DrawingResources, hash_map},
    utils::{
        collections::{index_map, HashMap, IndexMap},
        misc::TakeValue
    }
};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

#[derive(Clone, Copy)]
pub(in crate::map) struct UiThing<'a>
{
    pub index:    usize,
    pub name:     &'a str,
    pub tex_id:   egui::TextureId,
    pub tex_size: UVec2
}

//=======================================================================//

/// The catalog of all the available [`Thing`]s.
#[must_use]
pub(in crate::map) struct ThingsCatalog
{
    /// The [`Thing`]s that are hardcoded in the editor.
    hardcoded_things: HashMap<ThingId, Thing>,
    /// All the loaded [`Thing`]s, both hardcoded and from files.
    things:           IndexMap<ThingId, Thing>,
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
            hardcoded_things: hash_map![],
            things:           index_map![],
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
    pub fn new(hardcoded_things: &mut HardcodedThings) -> Self
    {
        let h_things = hardcoded_things
            .0
            .take_value()
            .into_iter()
            .map(|thing| (thing.id(), thing))
            .collect();
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
    fn error_thing() -> Thing { Thing::new("error", Self::ERROR_ID, 64f32, 64f32, "error") }

    /// Combines the hardcoded and file loaded things into a single [`IndexedMap`].
    /// If a thing loaded from file has the same [`ThingId`] as an hardcoded one the latter will be
    /// overwritten. Things files are searched in the `assets/things/` folder.
    #[inline]
    fn loaded_things(hardcoded_things: &HashMap<ThingId, Thing>) -> IndexMap<ThingId, Thing>
    {
        /// The directory where ini defined things are located.
        const THINGS_DIR: &str = "assets/things/";

        /// Gathers all ini files.
        #[inline]
        fn recurse(path: &Path, inis: &mut Vec<Ini>)
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
        let mut configs = Vec::new();
        recurse(Path::new(THINGS_DIR), &mut configs);

        let mut things = hardcoded_things.values().cloned().collect::<Vec<_>>();

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

                let new_thing = Thing::new(
                    name,
                    id,
                    value!("width", f32),
                    value!("height", f32),
                    continue_if_none!(values.get("preview")).as_ref().unwrap()
                );
                let id = new_thing.id();

                for thing in &mut things
                {
                    if thing.id() == id
                    {
                        *thing = new_thing;
                        continue 'outer;
                    }
                }

                things.push(new_thing);
            }
        }

        things.sort_by(|a, b| a.name().cmp(b.name()));
        things.into_iter().map(|thing| (thing.id, thing)).collect()
    }

    //==============================================================
    // Info

    /// Whether the [`ThingsCatalog`] contains any [`Thing`].
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool { self.things.is_empty() }

    /// The [`Thing`] associated with `thing`, if any.
    #[inline]
    pub fn thing(&self, thing: ThingId) -> Option<&Thing> { self.things.get(&thing) }

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
    pub fn texture(&self, thing: ThingId) -> &str { self.thing_or_error(thing).preview() }

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
    pub fn ui_iter<'a>(
        &'a self,
        drawing_resources: &'a DrawingResources
    ) -> impl ExactSizeIterator<Item = UiThing<'a>>
    {
        self.things.iter().enumerate().map(move |(index, (_, thing))| {
            let texture = drawing_resources.egui_texture(thing.preview());
            UiThing {
                index,
                name: thing.name(),
                tex_id: texture.0,
                tex_size: texture.1
            }
        })
    }
}
