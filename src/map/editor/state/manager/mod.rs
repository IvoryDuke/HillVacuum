mod entities_trees;
mod iterators;
mod quad_tree;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{
    borrow::Cow,
    cell::Ref,
    fs::File,
    io::BufReader,
    ops::{Deref, DerefMut}
};

use bevy::prelude::{Transform, Vec2, Window};
use hill_vacuum_shared::{continue_if_none, return_if_none, NextValue};

use self::{
    entities_trees::Trees,
    iterators::{
        BrushesIter,
        IdsInRange,
        MovingsIter,
        SelectedBrushesIter,
        SelectedBrushesMut,
        SelectedMovingsIter,
        SelectedMovingsMut,
        SelectedThingsIter,
        SelectedThingsMut,
        ThingsIter
    }
};
use super::{
    clipboard::{ClipboardData, CopyToClipboard},
    core::Core,
    editor_state::{InputsPresses, ToolsSettings},
    edits_history::EditsHistory,
    grid::Grid,
    ui::Ui
};
use crate::{
    map::{
        brush::{
            convex_polygon::{ConvexPolygon, TextureSetResult},
            Brush,
            BrushData
        },
        containers::{hv_hash_map, hv_hash_set, Ids},
        drawer::{
            animation::Animator,
            color::Color,
            drawing_resources::DrawingResources,
            texture::{Sprite, TextureInterface, TextureInterfaceExtra, TextureSettings}
        },
        editor::{
            state::{editor_state::TargetSwitch, manager::quad_tree::QuadTreeIds},
            AllDefaultProperties,
            DrawBundle,
            ToolUpdateBundle
        },
        hv_vec,
        path::{EditPath, MovementSimulator, Moving},
        properties::{DefaultProperties, Properties, PropertiesRefactor},
        thing::{catalog::ThingsCatalog, ThingInstance, ThingInstanceData, ThingInterface},
        AssertedInsertRemove,
        HvHashMap,
        HvVec,
        MapHeader,
        OutOfBounds
    },
    utils::{
        hull::{EntityHull, Hull},
        identifiers::{EntityCenter, EntityId, Id, IdGenerator},
        math::AroundEqual,
        misc::{Blinker, ReplaceValues}
    },
    Path
};

//=======================================================================//
// TRAIT
//
//=======================================================================//

/// A trait to return the draw height of an entity.
pub(in crate::map::editor::state) trait DrawHeight
{
    /// Returns the draw height of an entity.
    #[must_use]
    fn draw_height(&self) -> Option<f32>;
}

impl DrawHeight for Brush
{
    #[inline]
    fn draw_height(&self) -> Option<f32>
    {
        self.texture_settings().map(TextureInterface::height_f32)
    }
}

impl DrawHeight for ThingInstance
{
    #[inline]
    fn draw_height(&self) -> Option<f32> { self.draw_height_f32().into() }
}

//=======================================================================//

/// A trait that defines properties common to all entities.
pub(in crate::map::editor::state) trait Entity:
    EntityHull + EntityId + DrawHeight + CopyToClipboard
{
}

impl Entity for Brush {}
impl Entity for ThingInstance {}

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The result of a texture change.
pub(in crate::map::editor::state) enum TextureResult
{
    /// The texture could be changed.
    Invalid,
    /// The texture was changed.
    Valid,
    /// The texture was changed and the tool outline must be refreshed.
    ValidRefreshOutline
}

//=======================================================================//

/// The modality of the scheduled overall properties update.
#[derive(Default)]
enum PropertyUpdate
{
    /// No update.
    #[default]
    None,
    /// Update all the properties.
    Total,
    /// Update the property with the stored key.
    Single(String)
}

//=======================================================================//
// TYPES
//
//=======================================================================//

/// A [`HvHashSet`] of [`Id`]s used to avoid unsafe operations in certain [`EntitiesManager`]'s
/// procedures.
#[must_use]
struct AuxiliaryIds(Ids);

impl<'a> Extend<&'a Id> for AuxiliaryIds
{
    #[inline]
    fn extend<T: IntoIterator<Item = &'a Id>>(&mut self, iter: T) { self.0.extend(iter); }
}

impl<'a> ReplaceValues<&'a Id> for AuxiliaryIds
{
    #[inline]
    fn replace_values<I: IntoIterator<Item = &'a Id>>(&mut self, iter: I)
    {
        self.0.replace_values(iter);
    }
}

impl<'a> IntoIterator for &'a AuxiliaryIds
{
    type IntoIter = hashbrown::hash_set::Iter<'a, Id>;
    type Item = &'a Id;

    #[inline]
    fn into_iter(self) -> Self::IntoIter { self.0.iter() }
}

impl AuxiliaryIds
{
    /// Returns a new [`AuxiliaryIds`].
    #[inline]
    fn new() -> Self { Self(hv_hash_set![capacity; 10]) }

    /// Whever `self` contains `id`.
    #[inline]
    #[must_use]
    fn contains(&self, id: Id) -> bool { self.0.contains(&id) }

    /// Returns an iterator to the contained elements.
    #[inline]
    pub fn iter(&self) -> hashbrown::hash_set::Iter<Id> { self.0.iter() }

    /// Pushes the [`Id`]s of the anchored [`Brush`]es of `brushes`.
    #[inline]
    fn store_anchored_ids<'a>(&mut self, brushes: impl Iterator<Item = &'a Brush>)
    {
        self.0.clear();

        for brush in brushes
        {
            self.0.extend(continue_if_none!(brush.anchors_iter()));
        }
    }

    /// Removes all elements.
    #[inline]
    fn clear(&mut self) { self.0.clear(); }

    /// Retains only the elements specified by the predicate.
    #[inline]
    fn retain<F: Fn(&Id) -> bool>(&mut self, f: F) { self.0.retain(f); }
}

//=======================================================================//

/// The error drawer.
#[must_use]
struct ErrorHighlight
{
    /// The latest occured error.
    error:   Option<Id>,
    /// The timer that makes to entity who caused the error blink on screen.
    blinker: Blinker,
    /// The amount of times left the entity error will be blinked.
    blinks:  u8
}

impl ErrorHighlight
{
    /// The duration of an entity blink.
    const BLINK_INTERVAL: f32 = 1f32 / 8f32;
    /// The amount of time the error blinks.
    const ERROR_BLINKS: u8 = 4 * 2;

    /// Returns a new [`ErrorHighlight`].
    #[inline]
    const fn new() -> Self
    {
        Self {
            error:   None,
            blinker: Blinker::new(f32::INFINITY),
            blinks:  0
        }
    }

    /// Sets the error and enables the blinking.
    #[inline]
    fn set_error(&mut self, error: Id)
    {
        let error = error.into();

        if error == self.error
        {
            return;
        }

        self.error = error;
        self.blinker = Blinker::new(Self::BLINK_INTERVAL);
        self.blinks = Self::ERROR_BLINKS;
    }

    /// Check whever the stored error concerns the entity `error` and removes it if that is the
    /// case.
    #[inline]
    fn check_entity_error_removal(&mut self, error: Id)
    {
        if self.error == error.into()
        {
            self.error = None;
        }
    }

    /// Draws the error on screen.
    #[inline]
    fn draw(&mut self, bundle: &mut DrawBundle) -> Option<Id>
    {
        if self.blinks == 0
        {
            return None;
        }

        let prev = self.blinker.on();
        let cur = self.blinker.update(bundle.delta_time);

        if prev != cur
        {
            self.blinks -= 1;

            if self.blinks == 0
            {
                self.error = None;
                return None;
            }
        }

        if cur
        {
            return self.error;
        }

        None
    }
}

//=======================================================================//

/// A collection of [`Animator`] that animate the textures on screen during the map preview.
#[must_use]
#[derive(Debug)]
pub(in crate::map) struct Animators(HvHashMap<Id, Animator>);

impl Animators
{
    /// Returns a new [`Animators`].
    #[inline]
    pub fn new<'a>(
        drawing_resources: &DrawingResources,
        brushes: impl Iterator<Item = &'a Brush>
    ) -> Self
    {
        Self(hv_hash_map![collect; brushes.filter_map(|brush| {
            brush.animator(drawing_resources).map(|anim| (brush.id(), anim))
        })])
    }

    /// Returns the [`Animator`] associated with `identifier`, if any.
    #[inline]
    pub fn get(&self, identifier: Id) -> Option<&Animator> { self.0.get(&identifier) }

    /// Updates the contained [`Animator`]s based of the time that has passed since the last update.
    #[inline]
    pub(in crate::map::editor::state) fn update(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &EntitiesManager,
        delta_time: f32
    )
    {
        for (id, a) in &mut self.0
        {
            match a
            {
                Animator::List(a) =>
                {
                    a.update(
                        manager
                            .brush(*id)
                            .texture_settings()
                            .unwrap()
                            .overall_animation(drawing_resources)
                            .get_list_animation(),
                        delta_time
                    );
                },
                Animator::Atlas(a) =>
                {
                    a.update(
                        manager
                            .brush(*id)
                            .texture_settings()
                            .unwrap()
                            .overall_animation(drawing_resources)
                            .get_atlas_animation(),
                        delta_time
                    );
                }
            };
        }
    }
}

//=======================================================================//

/// The core of the [`EntitiesManager`].
#[must_use]
struct Innards
{
    /// All the [`Brush`]es on the map.
    brushes: HvHashMap<Id, Brush>,
    /// All the [`Thing`]s on the map.
    things: HvHashMap<Id, ThingInstance>,
    /// The currently selected [`Brush`]es.
    selected_brushes: Ids,
    /// The currently selected [`Thing`]s.
    selected_things: Ids,
    /// The [`Id`]s of all the moving [`Brush`]es.
    moving: Ids,
    /// The [`Id`]s of the selected moving [`Brush`]es.
    selected_moving: Ids,
    /// The [`Id`]s of the entities that do not have a [`Path`] but could have one.
    possible_moving: Ids,
    /// The [`Id`]s of the selected entities that do not have a [`Path`] but could have one.
    selected_possible_moving: Ids,
    /// The [`Id`]s of the textured moving [`Brush`]es.
    textured: Ids,
    /// The [`Id`]s of the selected textured [`Brush`]es.
    selected_textured: Ids,
    /// The [`Id`]s of the selected [`Brush`]es with associated sprites.
    selected_sprites: HvHashMap<String, Ids>,
    /// The [`Id`]s of the  moving [`Brush`]es with anchors.
    brushes_with_anchors: HvHashMap<Id, Hull>,
    /// The generator of the [`Id`]s of the new entities.
    id_generator: IdGenerator,
    /// The error drawer.
    error_highlight: ErrorHighlight,
    /// Whever the tool outline should be updated.
    outline_update: bool,
    /// The [`Id`]s of the [`Brush`]es whose amount of selected vertexes changed, necessary for the
    /// update of the vertex and side tools.
    selected_vertexes_update: Ids,
    /// Whever the texture displayed in the texture editor should be updated.
    overall_texture_update: bool,
    /// Whever the info displayed in the platform tool's node editor should be updated.
    overall_node_update: bool,
    /// Whever the overall value of the selected [`Brush`]es' collision should be updated.
    overall_collision_update: bool,
    /// Whever the overall properties of the [`Brush`]es should be updated.
    overall_brushes_properties_update: PropertyUpdate,
    /// Whever the overall value of the draw height of the selected [`Thing`]s should be updated.
    overall_things_info_update: bool,
    /// Whever the overall properties of the [`ThingInstance`]s should be updated.
    overall_things_properties_update: PropertyUpdate,
    /// Whever the properties where refactored after loading a map file.
    refactored_properties: bool
}

impl Innards
{
    /// Returns a new [`Innards`].
    #[inline]
    pub fn new() -> Self
    {
        Self {
            brushes: hv_hash_map![],
            things: hv_hash_map![],
            selected_brushes: hv_hash_set![capacity; 10],
            selected_things: hv_hash_set![capacity; 10],
            moving: hv_hash_set![capacity; 10],
            selected_moving: hv_hash_set![capacity; 10],
            possible_moving: hv_hash_set![capacity; 10],
            selected_possible_moving: hv_hash_set![capacity; 10],
            textured: hv_hash_set![capacity; 10],
            selected_textured: hv_hash_set![capacity; 10],
            selected_sprites: hv_hash_map![capacity; 10],
            brushes_with_anchors: hv_hash_map![],
            id_generator: IdGenerator::default(),
            error_highlight: ErrorHighlight::new(),
            outline_update: false,
            selected_vertexes_update: hv_hash_set![capacity; 10],
            overall_texture_update: false,
            overall_node_update: false,
            overall_collision_update: false,
            overall_brushes_properties_update: PropertyUpdate::default(),
            overall_things_info_update: false,
            overall_things_properties_update: PropertyUpdate::default(),
            refactored_properties: false
        }
    }

    /// Reads the [`Brush`]es and [`Thing`]s from `file`.
    /// Returns an error if it occurred.
    #[inline]
    pub fn load(
        &mut self,
        header: &MapHeader,
        file: &mut BufReader<File>,
        things_catalog: &ThingsCatalog,
        drawing_resources: &DrawingResources,
        default_properties: &mut AllDefaultProperties,
        quad_trees: &mut Trees
    ) -> Result<(), &'static str>
    {
        /// Tests the validity of `value`.
        macro_rules! test {
            ($value:expr, $error:literal) => {
                match $value
                {
                    Ok(value) => value,
                    Err(_) => return Err($error)
                }
            };
        }

        /// Stores in `map_default_properties` the desired properties and returns a
        /// [`PropertiesRefactor`] if the default and file properties do not match.
        #[inline]
        #[must_use]
        fn mismatching_properties<'a>(
            default_properties: &'a DefaultProperties,
            map_default_properties: &mut DefaultProperties,
            file_default_properties: DefaultProperties,
            entity: &str
        ) -> Option<PropertiesRefactor<'a>>
        {
            if *default_properties == file_default_properties
            {
                return None;
            }

            let description = format!(
                "The engine default {entity} properties are different from the ones stored in the \
                 map file.\nIf you decide to use the engine defined ones, all values currently \
                 contained in the {entity} that do not match will be removed, and the missing \
                 ones will be inserted.\nPress OK to use the engine properties, press Cancel to \
                 use the map properties.\n\nHere are the two property lists:\n\nENGINE: \
                 {default_properties}\n\nMAP: {file_default_properties}"
            );

            match rfd::MessageDialog::new()
                .set_title("WARNING")
                .set_description(description)
                .set_buttons(rfd::MessageButtons::OkCancel)
                .show()
            {
                rfd::MessageDialogResult::Ok =>
                {
                    let refactor = file_default_properties.refactor(default_properties);
                    *map_default_properties = default_properties.clone();
                    refactor.into()
                },
                rfd::MessageDialogResult::Cancel =>
                {
                    *map_default_properties = file_default_properties;
                    None
                },
                _ => unreachable!()
            }
        }

        let mut max_id = Id::ZERO;
        let mut brushes = hv_vec![];
        let mut with_anchors = hv_vec![];

        let file_brushes_default_properties = test!(
            ciborium::from_reader::<DefaultProperties, _>(&mut *file),
            "Error reading default brushes properties"
        );
        let file_things_default_properties = test!(
            ciborium::from_reader::<DefaultProperties, _>(&mut *file),
            "Error reading default things properties"
        );

        let b_refactor = mismatching_properties(
            default_properties.brushes,
            default_properties.map_brushes,
            file_brushes_default_properties,
            "brushes"
        );

        for _ in 0..header.brushes
        {
            let mut brush =
                test!(ciborium::from_reader::<Brush, _>(&mut *file), "Error reading brushes");

            if brush.has_sprite()
            {
                let texture = drawing_resources
                    .texture_or_error(brush.texture_settings().unwrap().name())
                    .name();

                if !brush.check_texture_change(drawing_resources, texture)
                {
                    continue;
                }

                _ = brush.set_texture(drawing_resources, texture);
            }
            else if brush.out_of_bounds()
            {
                continue;
            }

            max_id = max_id.max(brush.id());

            if brush.has_anchors()
            {
                with_anchors.push(brush);
                continue;
            }

            if brush.anchored().is_some()
            {
                _ = brush.take_mover();
            }

            brushes.push(brush);
        }

        if let Some(refactor) = &b_refactor
        {
            for brush in &mut brushes
            {
                brush.refactor_properties(refactor);
            }
        }

        let mut things = hv_vec![];
        let t_refactor = mismatching_properties(
            default_properties.things,
            default_properties.map_things,
            file_things_default_properties,
            "things"
        );

        for _ in 0..header.things
        {
            let mut thing_i = test!(
                ciborium::from_reader::<ThingInstance, _>(&mut *file),
                "Error reading things"
            );
            let thing = things_catalog.thing_or_error(thing_i.thing());

            if !thing_i.check_thing_change(thing)
            {
                continue;
            }

            _ = thing_i.set_thing(thing);

            max_id = max_id.max(thing_i.id());

            things.push(thing_i);
        }

        if let Some(refactor) = &t_refactor
        {
            for thing in &mut things
            {
                thing.refactor_properties(refactor);
            }
        }

        for brush in brushes
        {
            self.insert_brush(quad_trees, brush, false);
        }

        for brush in with_anchors
        {
            self.insert_brush(quad_trees, brush, false);
        }

        for thing in things
        {
            self.insert_thing(thing, quad_trees, false);
        }

        self.id_generator.reset(max_id);
        _ = self.id_generator.new_id();
        self.refactored_properties = b_refactor.is_some() || t_refactor.is_some();

        Ok(())
    }

    //==============================================================
    // General

    /// Whever `identifier` is a selected entity.
    #[inline]
    #[must_use]
    fn is_selected(&self, identifier: Id) -> bool
    {
        self.selected_brushes.contains(&identifier) || self.selected_things.contains(&identifier)
    }

    /// Whever `identifier` belongs to an entity that exists.
    #[inline]
    #[must_use]
    fn entity_exists(&self, identifier: Id) -> bool
    {
        self.brushes.get(&identifier).is_some() || self.things.get(&identifier).is_some()
    }

    /// Returns the [`Entity`] trait object associated with `identifier`.
    /// # Panics
    /// Panics if `identifier` belongs to an entity that does not exist.
    #[inline]
    #[must_use]
    pub fn entity(&self, identifier: Id) -> &dyn Entity
    {
        self.brushes
            .get(&identifier)
            .map(|brush| brush as &dyn Entity)
            .or(self.things.get(&identifier).map(|thing| thing as &dyn Entity))
            .unwrap()
    }

    /// Spawns an entity pasted from the [`Clipboard`].
    #[inline]
    #[must_use]
    pub fn spawn_pasted_entity(
        &mut self,
        drawing_resources: &DrawingResources,
        edits_history: &mut EditsHistory,
        quad_trees: &mut Trees,
        data: ClipboardData,
        delta: Vec2
    ) -> Id
    {
        let id = self.new_id();

        match data
        {
            ClipboardData::Brush(data, _) =>
            {
                let mut brush = Brush::from_parts(data, id);
                brush.move_by_delta(drawing_resources, delta, true);
                edits_history.brush_spawn(brush.id(), true);
                self.insert_brush(quad_trees, brush, true);
            },
            ClipboardData::Thing(data, _) =>
            {
                let mut thing = ThingInstance::from_parts(id, data);
                thing.move_by_delta(delta);
                self.spawn_thing(thing, quad_trees, edits_history);
            }
        };

        id
    }

    //==============================================================
    // Selected entities

    /// Returns the amount of selected [`Brush`]es.
    #[inline]
    #[must_use]
    pub fn selected_brushes_amount(&self) -> usize { self.selected_brushes.len() }

    /// Returns the [`Id`]s of the selected [`Brush`]es.
    #[inline]
    pub fn selected_brushes_ids(&self) -> impl ExactSizeIterator<Item = &Id> + Clone
    {
        self.selected_brushes.iter()
    }

    /// Returns the [`Id`]s of the selected [`Brush`]es and [`Thing`]s.
    #[inline]
    pub fn selected_entities_ids(&self) -> impl Iterator<Item = &Id>
    {
        self.selected_brushes_ids().chain(&self.selected_things)
    }

    /// Updates the value related to entity selection for the entity `identifier`.
    /// Returns true if entity is a [`ThingInstance`].
    /// # Panics
    /// Panics if the entity does not exist, or it belongs to a textured [`Brush`] and it is not
    /// part of the set of textured [`Brush`]es [`Id`]s.
    #[inline]
    fn insert_entity_selection(&mut self, identifier: Id) -> bool
    {
        assert!(self.entity_exists(identifier), "Entity does not exist.");

        {
            let path = self.moving(identifier);

            if path.has_path()
            {
                assert!(
                    self.moving.contains(&identifier),
                    "Entity has path but is not in the platforms set."
                );
                self.selected_moving.asserted_insert(identifier);
            }
            else if path.possible_moving()
            {
                self.selected_possible_moving.asserted_insert(identifier);
            }
        }

        if self.is_thing(identifier)
        {
            self.overall_things_info_update = true;
            self.overall_things_properties_update = PropertyUpdate::Total;
            self.selected_things.asserted_insert(identifier);
            return true;
        }

        self.selected_brushes.asserted_insert(identifier);
        self.outline_update = true;
        self.overall_texture_update = true;
        self.overall_collision_update = true;
        self.overall_brushes_properties_update = PropertyUpdate::Total;

        let brush = self.brush(identifier);
        let has_texture = brush.has_texture();
        let has_sprite = brush.has_sprite();

        if !has_texture
        {
            return false;
        }

        assert!(
            self.textured.contains(&identifier),
            "Textures brushes set does not include the id."
        );
        self.selected_textured.asserted_insert(identifier);

        if has_sprite
        {
            self.insert_selected_sprite(identifier);
        }

        false
    }

    /// Updates the value related to entity deselection for the entity `identifier`.
    /// Returns true if entity is a [`ThingInstance`].
    /// # Panics
    /// Panics if the entity does not exist.
    #[inline]
    fn remove_entity_selection(&mut self, identifier: Id) -> bool
    {
        assert!(self.entity_exists(identifier), "Entity does not exist.");

        for ids in [
            &mut self.selected_moving,
            &mut self.selected_possible_moving
        ]
        {
            ids.remove(&identifier);
        }

        if self.is_thing(identifier)
        {
            self.overall_things_info_update = true;
            self.overall_things_properties_update = PropertyUpdate::Total;
            self.selected_things.asserted_remove(&identifier);
            return true;
        }

        self.overall_collision_update = true;
        self.overall_brushes_properties_update = PropertyUpdate::Total;
        self.selected_brushes.asserted_remove(&identifier);

        for ids in [
            &mut self.selected_moving,
            &mut self.selected_possible_moving,
            &mut self.selected_textured
        ]
        {
            ids.remove(&identifier);
        }

        if self.brush(identifier).has_sprite()
        {
            self.selected_sprites
                .get_mut(
                    self.brushes
                        .get(&identifier)
                        .unwrap()
                        .texture_settings()
                        .unwrap()
                        .name()
                )
                .unwrap()
                .asserted_remove(&identifier);
        }

        self.outline_update = true;
        self.overall_texture_update = true;
        false
    }

    /// Inserts the [`Id`] of a [`Brush`] with sprite in the selected sprites set.
    /// # Panics
    /// Panics if the [`Brush`] has no sprite.
    #[inline]
    fn insert_selected_sprite(&mut self, identifier: Id)
    {
        let brush = self.brushes.get(&identifier).unwrap();
        let texture = brush.texture_settings().unwrap();

        assert!(texture.sprite(), "Brush has no sprite.");

        let name = texture.name();

        match self.selected_sprites.get_mut(name)
        {
            Some(ids) => ids.asserted_insert(brush.id()),
            None =>
            {
                self.selected_sprites
                    .asserted_insert((name.to_owned(), hv_hash_set![brush.id()]));
            }
        };
    }

    /// Removes the [`Id`] of a [`Brush`] with sprite from the selected sprites set.
    /// # Panics
    /// Panics if the [`Brush`] has a sprite.
    #[inline]
    fn remove_selected_sprite(&mut self, identifier: Id)
    {
        let brush = self.brushes.get(&identifier).unwrap();
        assert!(!brush.has_sprite(), "Brush has a sprite.");

        self.selected_sprites
            .get_mut(brush.texture_settings().unwrap().name())
            .unwrap()
            .asserted_remove(&identifier);
    }

    /// Removes the texture from the [`Brush`] with [`Id`] `identifier`, and returns its
    /// [`TextureSettings`].
    #[inline]
    pub fn remove_texture(&mut self, quad_trees: &mut Trees, identifier: Id) -> TextureSettings
    {
        assert!(self.is_selected(identifier), "Brush is not selected.");

        let tex = self.brush_mut(quad_trees, identifier).remove_texture();

        self.textured.asserted_remove(&identifier);
        self.selected_textured.asserted_remove(&identifier);

        if tex.sprite()
        {
            self.selected_sprites
                .get_mut(tex.name())
                .unwrap()
                .asserted_remove(&identifier);
        }

        tex
    }

    /// Selects the entities contained in `iter`.
    #[inline]
    fn select_cluster<'a, I: Iterator<Item = &'a Id> + Clone>(
        &mut self,
        edits_history: &mut EditsHistory,
        iter: I
    )
    {
        for id in iter.clone()
        {
            self.insert_entity_selection(*id);
        }

        edits_history.entity_selection_cluster(iter);
    }

    /// Deselects the entities contained in `iter`.
    #[inline]
    fn deselect_cluster<'a, I: Iterator<Item = &'a Id> + Clone>(
        &mut self,
        edits_history: &mut EditsHistory,
        iter: I
    )
    {
        for id in iter.clone()
        {
            self.remove_entity_selection(*id);
        }

        edits_history.entity_deselection_cluster(iter);
    }

    /// Selects all existing entities and updates the [`EditsHistory`].
    #[inline]
    fn select_all_entities(
        &mut self,
        edits_history: &mut EditsHistory,
        auxiliary: &mut AuxiliaryIds
    )
    {
        if self.selected_brushes_amount() == self.brushes.len() &&
            self.selected_things_amount() == self.things.len()
        {
            return;
        }

        edits_history.entity_deselection_cluster(self.selected_entities_ids());

        self.selected_brushes.replace_values(self.brushes.keys());
        self.selected_things.replace_values(self.things.keys());
        self.selected_moving.replace_values(&self.moving);
        self.selected_possible_moving.replace_values(&self.possible_moving);
        self.selected_textured.replace_values(&self.textured);
        self.selected_sprites.clear();

        auxiliary.replace_values(
            self.selected_textured
                .iter()
                .filter(|id| self.brush(**id).texture_settings().unwrap().sprite())
        );

        for id in &*auxiliary
        {
            self.insert_selected_sprite(*id);
        }

        self.overall_texture_update = true;
        self.overall_collision_update = true;
        self.overall_brushes_properties_update = PropertyUpdate::Total;
        self.overall_things_info_update = true;
        self.overall_things_properties_update = PropertyUpdate::Total;

        edits_history.entity_selection_cluster(self.selected_entities_ids());
    }

    //==============================================================
    // Brushes

    /// Returns a reference to the [`Brush`] with [`Id`] `identifier`.
    /// # Panics
    /// Panics if the [`Brush`] does not exist.
    #[inline]
    pub fn brush(&self, identifier: Id) -> &Brush
    {
        self.brushes
            .get(&identifier)
            .unwrap_or_else(|| panic!("Failed brush() call for id {identifier:?}"))
    }

    /// Returns a [`BrushMut`] wrapping the [`Brush`] with [`Id`] `identifier`.
    /// # Panics
    /// Panics if the [`Brush`] does not exist.
    #[inline]
    pub fn brush_mut<'a>(&'a mut self, quad_trees: &'a mut Trees, identifier: Id) -> BrushMut<'a>
    {
        BrushMut::new(self, quad_trees, identifier)
    }

    /// Returns a [`Brushes`] wrapping the existing [`Brush`]es.
    #[inline]
    const fn brushes(&self) -> Brushes { Brushes(&self.brushes) }

    /// Disanchors the [`Brush`] with [`Id`] `anchor_id` to the one with [`Id`] `owner_id`.
    #[inline]
    fn anchor(&mut self, quad_trees: &mut Trees, owner_id: Id, anchor_id: Id)
    {
        _ = self.remove_anchors_hull(quad_trees, owner_id);

        let [owner, anchor] = self.brushes.get_many_mut([&owner_id, &anchor_id]).unwrap();
        owner.anchor(anchor);
        self.possible_moving.asserted_remove(&anchor_id);
        self.selected_possible_moving.asserted_remove(&anchor_id);

        assert!(self.insert_anchors_hull(quad_trees, owner_id), "Could not insert anchor.");
    }

    /// Disanchors the [`Brush`] with [`Id`] `anchor_id` from the one with [`Id`] `owner_id`.
    #[inline]
    pub fn disanchor(&mut self, quad_trees: &mut Trees, owner_id: Id, anchor_id: Id)
    {
        assert!(
            self.remove_anchors_hull(quad_trees, owner_id),
            "Could not remove hull from quad trees."
        );

        let [owner, anchor] = self.brushes.get_many_mut([&owner_id, &anchor_id]).unwrap();
        owner.disanchor(anchor);
        self.possible_moving.asserted_insert(anchor_id);
        self.selected_possible_moving.asserted_insert(anchor_id);

        _ = self.insert_anchors_hull(quad_trees, owner_id);
    }

    /// Selects the [`Id`]s of the [`Brush`]es anchored to the ones with [`Id`]s contained in
    /// `identifiers`.
    #[inline]
    fn select_anchored_brushes(
        &mut self,
        edits_history: &mut EditsHistory,
        auxiliary: &mut AuxiliaryIds,
        identifiers: impl IntoIterator<Item = Id>
    )
    {
        auxiliary.store_anchored_ids(identifiers.into_iter().map(|id| self.brush(id)));
        auxiliary.retain(|id| !self.is_selected(*id));
        self.select_cluster(edits_history, auxiliary.iter());
    }

    /// Selects the [`Brush`]es anchored to the selected ones.
    #[inline]
    fn select_anchored_brushes_of_selected_brushes(
        &mut self,
        edits_history: &mut EditsHistory,
        auxiliary: &mut AuxiliaryIds
    )
    {
        auxiliary.store_anchored_ids(self.selected_brushes.iter().map(|id| self.brush(*id)));
        auxiliary.retain(|id| !self.is_selected(*id));
        self.select_cluster(edits_history, auxiliary.iter());
    }

    /// Adds a [`Brush`] to the map.
    /// # Panics
    /// Panics if the [`Brush`] has anchored [`Brush`]es but the [`Hull`] describing the anchors
    /// area could not be retrieved.
    #[inline]
    fn insert_brush(&mut self, quad_trees: &mut Trees, brush: Brush, selected: bool)
    {
        let id = brush.id();
        quad_trees.insert_brush_hull(&brush);
        self.outline_update = true;

        if brush.has_selected_vertexes()
        {
            self.selected_vertexes_update.asserted_insert(id);
        }

        if brush.possible_moving()
        {
            self.possible_moving.asserted_insert(id);
        }
        else if brush.has_path()
        {
            self.overall_node_update = true;
            self.moving.asserted_insert(id);
            quad_trees.insert_path_hull(&brush);
        }

        if brush.has_texture()
        {
            self.overall_texture_update = true;
            self.textured.asserted_insert(id);
        }

        let anchored = brush.anchored();
        let has_anchors = brush.has_anchors();
        let has_sprite = brush.has_sprite();

        if has_anchors
        {
            for id in brush.anchors_iter().unwrap()
            {
                self.brush_mut(quad_trees, *id).attach(brush.id());
                self.possible_moving.asserted_remove(id);
                self.selected_possible_moving.remove(id);
            }
        }
        else if let Some(id) = anchored
        {
            self.brush_mut(quad_trees, id).insert_anchor(&brush);
        }

        self.brushes.asserted_insert((id, brush));

        if selected
        {
            self.insert_entity_selection(id);
        }

        if has_anchors
        {
            assert!(self.insert_anchors_hull(quad_trees, id), "Brush has no anchors.");
        }
        else if let Some(id) = anchored
        {
            _ = self.remove_anchors_hull(quad_trees, id);
            _ = self.insert_anchors_hull(quad_trees, id);
        }

        if has_sprite
        {
            quad_trees.insert_sprite_hull(self.brush(id));
        }
    }

    /// Removes the [`Brush`] with [`Id`] `identifier` from the map and returns it.
    /// # Panics
    /// Panics if there are discrepancies between the [`Brush`] properties and the stored
    /// informations.
    #[inline]
    fn remove_brush(&mut self, quad_trees: &mut Trees, identifier: Id, selected: bool) -> Brush
    {
        self.outline_update = true;
        self.error_highlight.check_entity_error_removal(identifier);

        if selected
        {
            self.remove_entity_selection(identifier);
        }
        else
        {
            assert!(!self.is_selected(identifier), "Brush is stored as selected.");
        }

        let has_anchors = self.brush(identifier).has_anchors();

        if has_anchors
        {
            _ = self.remove_anchors_hull(quad_trees, identifier);
        }
        else
        {
            assert!(
                !self.brushes_with_anchors.contains_key(&identifier),
                "Brush is stored as having anchors."
            );
        }

        let brush = self.brushes.remove(&identifier).unwrap();
        quad_trees.remove_brush_hull(&brush);

        if brush.has_selected_vertexes()
        {
            self.selected_vertexes_update.insert(identifier);
        }

        if brush.has_path()
        {
            self.overall_node_update = true;
            quad_trees.remove_path_hull(&brush, &brush.path_hull().unwrap());
            self.moving.asserted_remove(&identifier);
        }
        else
        {
            assert!(!self.moving.contains(&identifier), "Brush is stored as moving.");

            if brush.possible_moving()
            {
                self.possible_moving.asserted_remove(&identifier);
            }
        }

        if has_anchors
        {
            for id in brush.anchors_iter().unwrap()
            {
                self.brush_mut(quad_trees, *id).detach();
                self.possible_moving.asserted_insert(*id);

                if self.is_selected(*id)
                {
                    self.selected_possible_moving.asserted_insert(*id);
                }
            }
        }
        else if let Some(id) = brush.anchored()
        {
            self.brush_mut(quad_trees, id).remove_anchor(&brush);
            self.replace_anchors_hull(quad_trees, id);
        }

        if brush.has_texture()
        {
            self.overall_texture_update = true;
            self.textured.asserted_remove(&identifier);
        }

        if brush.has_sprite()
        {
            quad_trees.remove_sprite_hull(&brush, &brush.sprite_and_anchor_hull().unwrap());
        }

        brush
    }

    /// Returns a new unique [`Id`].
    #[inline]
    #[must_use]
    fn new_id(&mut self) -> Id { self.id_generator.new_id() }

    /// Spawns a [`Brush`] in the map and returns its [`Id`].
    #[inline]
    pub fn spawn_brush<'a>(
        &mut self,
        quad_trees: &mut Trees,
        polygon: impl Into<Cow<'a, ConvexPolygon>>,
        edits_history: &mut EditsHistory,
        properties: Properties
    ) -> Id
    {
        let id = self.new_id();

        let brush = Brush::from_polygon(polygon, id, properties);

        edits_history.brush_spawn(id, true);
        self.insert_brush(quad_trees, brush, true);

        id
    }

    /// Despawns the [`Brush`] with [`Id`] `identifier` from the map.
    #[inline]
    fn despawn_brush(
        &mut self,
        quad_trees: &mut Trees,
        identifier: Id,
        edits_history: &mut EditsHistory,
        selected: bool
    )
    {
        let brush = self.remove_brush(quad_trees, identifier, selected);
        edits_history.brush_despawn(brush, selected);
    }

    /// Despawns all selected [`Brush`]es.
    #[inline]
    fn despawn_selected_brush(
        &mut self,
        quad_trees: &mut Trees,
        identifier: Id,
        edits_history: &mut EditsHistory
    )
    {
        self.despawn_brush(quad_trees, identifier, edits_history, true);
    }

    /// Sets the texture of the [`Brush`] with [`Id`] `identifier`.
    /// Returns the [`TextureMetadata`] of the replaced texture, if any.
    /// # Panics
    /// Panics if the [`Brush`] is not selected.
    #[inline]
    #[must_use]
    pub fn set_texture(
        &mut self,
        drawing_resources: &DrawingResources,
        quad_trees: &mut Trees,
        identifier: Id,
        texture: &str
    ) -> TextureSetResult
    {
        assert!(self.is_selected(identifier), "Brush is not selected.");

        let (sprite, result) = {
            let mut brush = self.brush_mut(quad_trees, identifier);
            (brush.has_sprite(), brush.set_texture(drawing_resources, texture))
        };

        match &result
        {
            TextureSetResult::Changed(prev) if sprite =>
            {
                self.selected_sprites
                    .get_mut(prev)
                    .unwrap()
                    .asserted_remove(&identifier);

                match self.selected_sprites.get_mut(texture)
                {
                    Some(ids) => ids.asserted_insert(identifier),
                    None =>
                    {
                        self.selected_sprites
                            .asserted_insert((texture.to_owned(), hv_hash_set![identifier]));
                    }
                };
            },
            TextureSetResult::Unchanged | TextureSetResult::Changed(_) => (),
            TextureSetResult::Set =>
            {
                self.textured.asserted_insert(identifier);
                self.selected_textured.asserted_insert(identifier);
            }
        }

        result
    }

    /// Sets the [`Path`] of the entity with [`Id`] `identifier` to `path` and updates the edits
    /// history.
    #[inline]
    pub fn create_path(
        &mut self,
        quad_trees: &mut Trees,
        identifier: Id,
        path: Path,
        edits_history: &mut EditsHistory
    )
    {
        assert!(self.is_selected(identifier), "Entity is not selected.");

        self.set_path(quad_trees, identifier, path);
        edits_history.path_creation(identifier);
    }

    /// Sets the [`Path`] of the entity with [`Id`] `identifier` to `path`.
    #[inline]
    pub fn set_path(&mut self, quad_trees: &mut Trees, identifier: Id, path: Path)
    {
        self.moving_mut(quad_trees, identifier).set_path(path);

        self.moving.asserted_insert(identifier);
        self.selected_moving.asserted_insert(identifier);
        self.possible_moving.asserted_remove(&identifier);
        self.selected_possible_moving.asserted_remove(&identifier);
    }

    /// Removes the [`Path`] of the selected entity.
    #[inline]
    fn remove_selected_path(
        &mut self,
        quad_trees: &mut Trees,
        identifier: Id,
        edits_history: &mut EditsHistory
    )
    {
        assert!(self.is_selected(identifier), "Entity is not selected.");

        self.overall_node_update = true;

        self.moving.asserted_remove(&identifier);
        self.selected_moving.asserted_remove(&identifier);
        self.possible_moving.asserted_insert(identifier);
        self.selected_possible_moving.asserted_insert(identifier);

        edits_history
            .path_deletion(identifier, self.moving_mut(quad_trees, identifier).take_path());
    }

    /// Replaces the [`Path`] of the selected entity with `path`.
    #[inline]
    fn replace_selected_path(
        &mut self,
        quad_trees: &mut Trees,
        identifier: Id,
        edits_history: &mut EditsHistory,
        path: Path
    )
    {
        self.remove_selected_path(quad_trees, identifier, edits_history);
        self.create_path(quad_trees, identifier, path, edits_history);
    }

    /// Replaces in the quad trees the [`Hull`] of the anchors of the [`Brush`] with [`Id`]
    /// `identifier`.
    /// # Panics
    /// Panics if the anchors [`Hull`] was not already inserted.
    #[inline]
    fn replace_anchors_hull(&mut self, quad_trees: &mut Trees, owner_id: Id)
    {
        assert!(
            self.remove_anchors_hull(quad_trees, owner_id),
            "The hull of the anchor was not inserted."
        );
        _ = self.insert_anchors_hull(quad_trees, owner_id);
    }

    /// Inserts in the quad trees the [`Hull`] of the anchors of the [`Brush`] with [`Id`], and
    /// returns whever the procedure was successful. `identifier`.
    #[inline]
    #[must_use]
    fn insert_anchors_hull(&mut self, quad_trees: &mut Trees, owner_id: Id) -> bool
    {
        let hull = return_if_none!(self.brush(owner_id).anchors_hull(self.brushes()), false);
        self.brushes_with_anchors.asserted_insert((owner_id, hull));
        quad_trees.insert_anchor_hull(owner_id, &hull);
        true
    }

    /// Removes from the quad trees the [`Hull`] of the anchors of the [`Brush`] with [`Id`]
    /// `identifier`, and returns whever the procedure was successful.
    #[inline]
    #[must_use]
    fn remove_anchors_hull(&mut self, quad_trees: &mut Trees, owner_id: Id) -> bool
    {
        let hull = return_if_none!(self.brushes_with_anchors.remove(&owner_id), false);
        quad_trees.remove_anchor_hull(owner_id, &hull);
        true
    }

    //==============================================================
    // Things

    /// Whever `identifier` belongs to a [`ThingInstance`].
    #[inline]
    #[must_use]
    pub fn is_thing(&self, identifier: Id) -> bool { self.things.contains_key(&identifier) }

    /// Returns the amount of selected [`ThingInstance`]s.
    #[inline]
    pub fn selected_things_amount(&self) -> usize { self.selected_things.len() }

    /// Returns a reference to the [`ThingInstance`] with [`Id`] `identifier`.
    #[inline]
    pub fn thing(&self, identifier: Id) -> &ThingInstance
    {
        self.things
            .get(&identifier)
            .unwrap_or_else(|| panic!("Failed thing() call for id {identifier:?}"))
    }

    /// Returns a [`ThingMut`] wrapper to the [`ThingInstance`] with [`Id`] `identifier`.
    #[inline]
    pub fn thing_mut<'a>(&'a mut self, quad_trees: &'a mut Trees, identifier: Id) -> ThingMut<'a>
    {
        ThingMut::new(self, quad_trees, identifier)
    }

    /// Inserts a `thing` in the map.
    #[inline]
    pub fn insert_thing(&mut self, thing: ThingInstance, quad_trees: &mut Trees, selected: bool)
    {
        self.overall_things_info_update = true;
        self.overall_things_properties_update = PropertyUpdate::Total;

        let id = thing.id();

        quad_trees.insert_thing_hull(&thing);

        if thing.has_path()
        {
            self.moving.asserted_insert(id);

            quad_trees.insert_path_hull(&thing);
        }
        else
        {
            self.possible_moving.asserted_insert(id);
        }

        if selected
        {
            self.selected_moving.asserted_insert(id);
            self.selected_things.asserted_insert(id);
        }
        else
        {
            self.selected_possible_moving.asserted_insert(id);
        }

        self.things.asserted_insert((id, thing));
    }

    /// Removes a [`ThingInstance`] from the map and returns it.
    #[inline]
    pub fn remove_thing(&mut self, quad_trees: &mut Trees, identifier: Id) -> ThingInstance
    {
        self.overall_things_info_update = true;
        self.overall_things_properties_update = PropertyUpdate::Total;

        self.error_highlight.check_entity_error_removal(identifier);

        quad_trees.remove_thing_hull(self.things.get(&identifier).unwrap());
        let thing = self.things.asserted_remove(&identifier);
        self.selected_things.asserted_remove(&identifier);

        if thing.has_path()
        {
            self.moving.asserted_remove(&identifier);
            self.selected_moving.asserted_remove(&identifier);
            quad_trees.remove_path_hull(&thing, &thing.path_hull().unwrap());
        }
        else
        {
            self.possible_moving.asserted_remove(&identifier);
            self.selected_possible_moving.asserted_remove(&identifier);
        }

        thing
    }

    /// Spawns `thing` into the map.
    #[inline]
    pub fn spawn_thing(
        &mut self,
        thing: ThingInstance,
        quad_trees: &mut Trees,
        edits_history: &mut EditsHistory
    )
    {
        edits_history.thing_spawn(thing.id(), thing.data().clone());
        self.insert_thing(thing, quad_trees, true);
    }

    /// Draws `thing` into the map.
    #[inline]
    pub fn draw_thing(
        &mut self,
        thing: ThingInstance,
        quad_trees: &mut Trees,
        edits_history: &mut EditsHistory
    )
    {
        edits_history.thing_draw(thing.id(), thing.data().clone());
        self.insert_thing(thing, quad_trees, true);
    }

    /// Despawns the [`ThingInstance`] with [`Id`] `identifier`.
    #[inline]
    pub fn despawn_thing(
        &mut self,
        quad_trees: &mut Trees,
        edits_history: &mut EditsHistory,
        identifier: Id
    )
    {
        let thing = self.remove_thing(quad_trees, identifier);
        edits_history.thing_despawn(identifier, thing.data().clone());
    }

    //==============================================================
    // Paths

    /// Returns a reference to the entity with id `identifier` as a trait object which implements
    /// the [`Moving`] trait.
    #[inline]
    pub fn moving(&self, identifier: Id) -> &dyn Moving
    {
        if self.is_thing(identifier)
        {
            return self.thing(identifier);
        }

        self.brush(identifier)
    }

    /// Returns a [`MovingMut`] wrapping the entity with id `identifier`.
    #[inline]
    pub fn moving_mut<'a>(&'a mut self, quad_trees: &'a mut Trees, identifier: Id)
        -> MovingMut<'a>
    {
        MovingMut::new(self, quad_trees, identifier)
    }
}

//=======================================================================//

/// The manager of all entities placed on the map.
pub(in crate::map::editor::state) struct EntitiesManager
{
    /// The core of the manager.
    innards:         Innards,
    /// The [`QuadTree`]s used for spacial partitioning.
    quad_trees:      Trees,
    /// The auxiliary container used to avoid using unsafe code in certain procedures.
    auxiliary:       AuxiliaryIds,
    /// Vector to help in the despawn of the selected [`Brush`]es.
    brushes_despawn: HvVec<Id>
}

impl EntitiesManager
{
    /// Returns a new [`EntitiesManager`]
    #[inline]
    #[must_use]
    pub fn new() -> Self
    {
        Self {
            innards:         Innards::new(),
            quad_trees:      Trees::new(),
            auxiliary:       AuxiliaryIds::new(),
            brushes_despawn: hv_vec![]
        }
    }

    /// Returns a new [`EntitiesManager`] along with the [`MapHeader`] read from `file` if the read
    /// process was successful.
    #[inline]
    pub fn from_file(
        header: &MapHeader,
        file: &mut BufReader<File>,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        default_properties: &mut AllDefaultProperties
    ) -> Result<Self, &'static str>
    {
        let mut manager = Self::new();

        match manager.innards.load(
            header,
            file,
            things_catalog,
            drawing_resources,
            default_properties,
            &mut manager.quad_trees
        )
        {
            Ok(()) => Ok(manager),
            Err(err) => Err(err)
        }
    }

    //==============================================================
    // General

    /// Whever the entities properties have been refactored on file load.
    #[inline(always)]
    #[must_use]
    pub const fn refactored_properties(&self) -> bool { self.innards.refactored_properties }

    /// Turns off the refactored properties flag.
    #[inline]
    pub fn reset_refactored_properties(&mut self) { self.innards.refactored_properties = false; }

    /// Whever an entity with [`Id`] `identifier` exists.
    #[inline]
    #[must_use]
    pub fn entity_exists(&self, identifier: Id) -> bool
    {
        self.innards.brushes.get(&identifier).is_some() ||
            self.innards.things.get(&identifier).is_some()
    }

    /// Returns the amount of entities placed on the map.
    #[inline]
    #[must_use]
    pub fn entities_amount(&self) -> usize { self.brushes_amount() + self.things_amount() }

    /// Returns a reference to the [`Entity`] trait object with [`Id`] `identifier`.
    #[inline]
    #[must_use]
    pub fn entity(&self, identifier: Id) -> &dyn Entity { self.innards.entity(identifier) }

    /// Schedule a tool outline update.
    #[inline]
    pub fn schedule_outline_update(&mut self) { self.innards.outline_update = true; }

    /// Updates certain tool and UI properties.
    #[inline]
    pub fn update_tool_and_overall_values(
        &mut self,
        drawing_resources: &DrawingResources,
        core: &mut Core,
        ui: &mut Ui,
        grid: Grid,
        settings: &mut ToolsSettings
    )
    {
        if std::mem::replace(&mut self.innards.outline_update, false)
        {
            core.update_outline(self, grid, settings);
        }

        if !self.innards.selected_vertexes_update.is_empty()
        {
            core.update_selected_vertexes(self, self.innards.selected_vertexes_update.iter());
            self.innards.selected_vertexes_update.clear();
        }

        if std::mem::take(&mut self.innards.overall_texture_update)
        {
            ui.update_overall_texture(drawing_resources, self);
        }

        if std::mem::take(&mut self.innards.overall_node_update)
        {
            core.update_overall_node(self);
        }

        if std::mem::take(&mut self.innards.overall_collision_update)
        {
            ui.update_overall_brushes_collision(self);
        }

        match std::mem::take(&mut self.innards.overall_brushes_properties_update)
        {
            PropertyUpdate::None => (),
            PropertyUpdate::Total => ui.update_overall_total_brush_properties(self),
            PropertyUpdate::Single(key) => ui.update_overall_brushes_property(self, &key)
        };

        if std::mem::take(&mut self.innards.overall_things_info_update)
        {
            ui.update_overall_things_info(self);
        }

        match std::mem::take(&mut self.innards.overall_things_properties_update)
        {
            PropertyUpdate::None => (),
            PropertyUpdate::Total => ui.update_overall_total_things_properties(self),
            PropertyUpdate::Single(key) => ui.update_overall_things_property(self, &key)
        };
    }

    /// Executes `f` and stores the error returned if any.
    #[inline]
    #[must_use]
    pub fn test_operation_validity<F>(&mut self, mut f: F) -> bool
    where
        F: FnMut(&mut Self) -> Option<Id>
    {
        let error = return_if_none!(f(self), true);
        self.innards.error_highlight.set_error(error);
        false
    }

    /// Schedules the update of the overall collision of the [`Brush`]es.
    #[inline]
    pub fn schedule_overall_collision_update(&mut self)
    {
        self.innards.overall_collision_update = true;
    }

    /// Schedules the update of the overall [`Brush`]s property with key `k` value.
    #[inline]
    pub fn schedule_overall_brushes_property_update(&mut self, k: &str)
    {
        self.innards.overall_brushes_properties_update = PropertyUpdate::Single(k.to_string());
    }

    /// Schedules the update of the overall [`ThingInstance`]s infos.
    #[inline]
    pub fn schedule_overall_things_info_update(&mut self)
    {
        self.innards.overall_things_info_update = true;
    }

    /// Schedules the update of the overall [`ThingInstance`]s property with key `k` value.
    #[inline]
    pub fn schedule_overall_things_property_update(&mut self, k: &str)
    {
        self.innards.overall_things_properties_update = PropertyUpdate::Single(k.to_string());
    }

    /// Schedules the update of the overall [`Path`]s node values.
    #[inline]
    pub fn schedule_overall_node_update(&mut self) { self.innards.overall_node_update = true; }

    //==============================================================
    // Selection

    /// Whever there are any currently selected entities.
    #[inline]
    #[must_use]
    pub fn any_selected_entities(&self) -> bool
    {
        self.any_selected_brushes() || self.any_selected_things()
    }

    /// Returns an iterator to the [`Entity`] trait objects in the map.
    #[inline]
    pub fn selected_entities(&self) -> impl Iterator<Item = &dyn Entity>
    {
        self.selected_brushes_ids()
            .chain(self.selected_things_ids())
            .map(|id| self.entity(*id))
    }

    /// Whever the entity with [`Id`] `identifier` is selected.
    #[inline]
    #[must_use]
    pub fn is_selected(&self, identifier: Id) -> bool { self.innards.is_selected(identifier) }

    /// Selects the entity with [`Id`] `identifier`.
    #[inline]
    pub fn select_entity(
        &mut self,
        identifier: Id,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory
    )
    {
        let thing = self.insert_entity_selection(identifier);
        edits_history.entity_selection(identifier);

        if thing || !inputs.ctrl_pressed()
        {
            return;
        }

        self.innards
            .select_anchored_brushes(edits_history, &mut self.auxiliary, Some(identifier));
    }

    /// Deselects the entity with [`Id`] `identifier`.
    #[inline]
    pub fn deselect_entity(
        &mut self,
        identifier: Id,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory
    )
    {
        let thing = self.remove_entity_selection(identifier);
        edits_history.entity_deselection(identifier);

        if thing || !inputs.ctrl_pressed()
        {
            return;
        }

        self.deselect_anchored_brushes(edits_history, Some(identifier));
    }

    /// Updates the value related to entity selection for the entity identifier. Returns true if
    /// entity is a [`ThingInstance`].
    #[inline]
    pub fn insert_entity_selection(&mut self, identifier: Id) -> bool
    {
        self.innards.insert_entity_selection(identifier)
    }

    /// Updates the value related to entity deselection for the entity identifier. Returns true if
    /// entity is a [`ThingInstance`].
    #[inline]
    pub fn remove_entity_selection(&mut self, identifier: Id) -> bool
    {
        self.innards.remove_entity_selection(identifier)
    }

    /// Deselects all selected entities.
    #[inline]
    pub fn deselect_selected_entities(&mut self, edits_history: &mut EditsHistory)
    {
        self.auxiliary.replace_values(
            self.innards
                .selected_brushes
                .iter()
                .chain(&self.innards.selected_things)
        );

        for id in &self.auxiliary
        {
            self.innards.remove_entity_selection(*id);
        }

        edits_history.entity_deselection_cluster(self.auxiliary.iter());
    }

    /// Selects all entities.
    #[inline]
    pub fn select_all_entities(&mut self, edits_history: &mut EditsHistory)
    {
        self.innards.select_all_entities(edits_history, &mut self.auxiliary);
    }

    /// Despawns the selected entities.
    #[inline]
    pub fn despawn_selected_entities(&mut self, edits_history: &mut EditsHistory)
    {
        self.despawn_selected_brushes(edits_history);

        self.auxiliary.replace_values(self.innards.selected_things.iter());

        for id in &self.auxiliary
        {
            self.innards.despawn_thing(&mut self.quad_trees, edits_history, *id);
        }
    }

    //==============================================================
    // Brushes

    /// Returns the amount of [`Brush`]es.
    #[inline]
    #[must_use]
    pub fn brushes_amount(&self) -> usize { self.innards.brushes.len() }

    /// Returns the amount of selected [`Brush`]es.
    #[inline]
    #[must_use]
    pub fn selected_brushes_amount(&self) -> usize { self.innards.selected_brushes_amount() }

    /// Whever there are any currently selected [`Brush`]es.
    #[inline]
    #[must_use]
    pub fn any_selected_brushes(&self) -> bool { self.selected_brushes_amount() != 0 }

    /// Returns a reference to the [`Brush`] with [`Id`] identifier.
    #[inline]
    pub fn brush(&self, identifier: Id) -> &Brush { self.innards.brush(identifier) }

    /// Returns an iterator to the [`Id`]s of the selected [`Brush`]es.
    #[inline]
    pub fn selected_brushes_ids(&self) -> impl ExactSizeIterator<Item = &Id> + Clone
    {
        self.innards.selected_brushes.iter()
    }

    /// Returns a [`BrushesIter`] created from `ids`.
    #[inline]
    const fn brushes_iter<'a>(&'a self, ids: Ref<'a, QuadTreeIds>) -> BrushesIter<'a>
    {
        BrushesIter::new(self, ids)
    }

    /// Returns a [`SelectedBrushesIter`] created from `ids`
    #[inline]
    const fn selected_brushes_iter<'a>(
        &'a self,
        ids: Ref<'a, QuadTreeIds>
    ) -> SelectedBrushesIter<'a>
    {
        SelectedBrushesIter::new(self, ids)
    }

    /// Returns a [`Brushes`] instance.
    #[inline]
    pub const fn brushes(&self) -> Brushes { self.innards.brushes() }

    /// Returns an array of references to `N` [`Brush`]es.
    #[inline]
    pub fn many_brushes<const N: usize>(&self, identifiers: [Id; N]) -> [&Brush; N]
    {
        std::array::from_fn(|i| self.brush(identifiers[i]))
    }

    /// Returns a [`BrushMut`] wrapping the [`Brush`] with [`Id`] `identifier`.
    #[inline]
    pub fn brush_mut(&mut self, identifier: Id) -> BrushMut<'_>
    {
        BrushMut::new(&mut self.innards, &mut self.quad_trees, identifier)
    }

    /// Returns an iterator to the non selected [`Brush`]es.
    #[inline]
    pub fn non_selected_brushes(&mut self) -> impl Iterator<Item = &Brush>
    {
        self.innards
            .brushes
            .values()
            .filter(|brush| !self.is_selected(brush.id()))
    }

    /// Returns an iterator to the selected [`Brush`]es.
    #[inline]
    pub fn selected_brushes(&self) -> impl Iterator<Item = &Brush>
    {
        self.selected_brushes_ids().map(|id| self.brush(*id))
    }

    /// Returns an iterator to [`BrushMut`] wrapping the selected [`Brush`]es.
    #[inline]
    pub fn selected_brushes_mut(&mut self) -> impl Iterator<Item = BrushMut<'_>>
    {
        self.auxiliary.replace_values(&self.innards.selected_brushes);
        SelectedBrushesMut::new(&mut self.innards, &mut self.quad_trees, &self.auxiliary)
    }

    /// Returns a [`BrushesIter`] that returns the [`Brush`]es near `cursor_pos`.
    /// If `camera_scale` contains a value it wraps [`Brush`]es within the cursor highlight.
    #[inline]
    pub fn brushes_at_pos(&self, cursor_pos: Vec2, camera_scale: Option<f32>) -> BrushesIter<'_>
    {
        self.brushes_iter(self.quad_trees.brushes_at_pos(cursor_pos, camera_scale))
    }

    /// Returns an iterator to the visible [`Brush`]es.
    #[inline]
    pub fn visible_brushes(&self, window: &Window, camera: &Transform) -> BrushesIter<'_>
    {
        self.brushes_iter(self.quad_trees.visible_brushes(camera, window))
    }

    /// Returns a [`SelectedBrushesIter`] that returns the selected [`Brush`]es near `cursor_pos`.
    /// If `camera_scale` contains a value it wraps the selected [`Brush`]es within the cursor
    /// highlight.
    #[inline]
    pub fn selected_brushes_at_pos(
        &self,
        cursor_pos: Vec2,
        camera_scale: impl Into<Option<f32>>
    ) -> SelectedBrushesIter<'_>
    {
        self.selected_brushes_iter(self.quad_trees.brushes_at_pos(cursor_pos, camera_scale.into()))
    }

    /// Returns an iterator to [`BrushMut`]s wrapping the selected [`Brush`]es near `cursor_pos`.
    /// If `camera_scale` contains a value it wraps the selected [`Brush`]es within the cursor
    /// highlight.
    #[inline]
    pub fn selected_brushes_mut_at_pos(
        &mut self,
        cursor_pos: Vec2,
        camera_scale: impl Into<Option<f32>>
    ) -> impl Iterator<Item = BrushMut<'_>>
    {
        self.auxiliary.replace_values(
            self.quad_trees
                .brushes_at_pos(cursor_pos, camera_scale.into())
                .ids()
                .filter(|id| self.innards.selected_brushes.contains(*id))
        );

        SelectedBrushesMut::new(&mut self.innards, &mut self.quad_trees, &self.auxiliary)
    }

    /// Returns an [`IdsInRange`] returning the [`Id`]s of [`Brush`]es intersecting `range`.
    #[inline]
    pub fn brushes_in_range(&self, range: &Hull) -> IdsInRange<'_>
    {
        IdsInRange::new(self.quad_trees.brushes_in_range(range))
    }

    /// Selects all entities that are fully contained in `range`.
    #[inline]
    pub fn select_entities_in_range(
        &mut self,
        range: &Hull,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        settings: &ToolsSettings
    )
    {
        /// Executes the ranged selection.
        macro_rules! select {
            ($func:ident $(, $ctrl_pressed:ident)?) => {{
                let in_range = self.quad_trees.$func(range);

                self.auxiliary
                    .replace_values(in_range.iter().filter_map(|(id, hull)| {
                        (!self.innards.is_selected(*id) && range.contains_hull(hull)).then_some(id)
                    }));

                self.innards.select_cluster(edits_history, self.auxiliary.iter());

                $(
                    if inputs.$ctrl_pressed()
                    {
                        self.innards.select_anchored_brushes(
                            edits_history,
                            &mut self.auxiliary,
                            in_range
                                .iter()
                                .filter_map(|(id, hull)| range.contains_hull(hull).then_some(id))
                                .copied()
                        );
                    }
                )?
            }};
        }

        match settings.target_switch()
        {
            TargetSwitch::Entity =>
            {
                select!(brushes_in_range, ctrl_pressed);
                select!(things_in_range);
            },
            TargetSwitch::Both =>
            {
                select!(brushes_in_range, ctrl_pressed);
                select!(things_in_range);
                select!(sprites_in_range, ctrl_pressed);
            },
            TargetSwitch::Texture => select!(sprites_in_range, ctrl_pressed)
        };
    }

    /// Exclusively selects all entities that are fully within `range`.
    #[inline]
    pub fn exclusively_select_entities_in_range(
        &mut self,
        range: &Hull,
        inputs: &InputsPresses,
        edits_history: &mut EditsHistory,
        settings: &ToolsSettings
    )
    {
        /// Executes the ranged selection.
        macro_rules! select_and_deselect {
            ($selected_entities:expr, $in_range:expr) => {
                let in_range = $in_range;
                self.auxiliary.replace_values(&$selected_entities);

                self.innards.deselect_cluster(
                    edits_history,
                    self.auxiliary.iter().filter(|id| !in_range.contains(**id))
                );

                self.innards.select_cluster(
                    edits_history,
                    in_range.ids().filter(|id| !self.auxiliary.contains(**id))
                );
            };
        }

        match settings.target_switch()
        {
            TargetSwitch::Entity =>
            {
                select_and_deselect!(
                    self.innards.selected_things,
                    &self.quad_trees.things_in_range(range)
                );

                select_and_deselect!(
                    self.innards.selected_brushes,
                    &self.quad_trees.brushes_in_range(range)
                );
            },
            TargetSwitch::Both =>
            {
                select_and_deselect!(
                    self.innards.selected_things,
                    &self.quad_trees.things_in_range(range)
                );

                let brushes_in_range = self.quad_trees.brushes_in_range(range);
                let sprites_in_range = self.quad_trees.sprites_in_range(range);

                self.innards.deselect_cluster(
                    edits_history,
                    self.auxiliary.iter().filter(|id| {
                        !brushes_in_range.contains(**id) && !sprites_in_range.contains(**id)
                    })
                );

                self.innards.select_cluster(
                    edits_history,
                    brushes_in_range
                        .ids()
                        .filter(|id| !sprites_in_range.contains(**id))
                        .chain(sprites_in_range.ids())
                        .filter(|id| !self.auxiliary.contains(**id))
                );
            },
            TargetSwitch::Texture =>
            {
                select_and_deselect!(
                    self.innards.selected_brushes,
                    &self.quad_trees.sprites_in_range(range)
                );
            }
        };

        if !inputs.ctrl_pressed()
        {
            return;
        }

        self.innards
            .select_anchored_brushes_of_selected_brushes(edits_history, &mut self.auxiliary);
    }

    /// Stores the [`Id`]s of the [`Brush`]es anchored to the ones with [`Id`]s returned by
    /// `identifiers`.
    #[inline]
    fn store_anchored_ids(&mut self, identifiers: impl IntoIterator<Item = Id>)
    {
        self.auxiliary
            .store_anchored_ids(identifiers.into_iter().map(|id| self.innards.brush(id)));
    }

    /// Deselects the [`Id`]s of the [`Brush`]es anchored to the ones with [`Id`]s returned by
    /// `identifiers`.
    #[inline]
    fn deselect_anchored_brushes(
        &mut self,
        edits_history: &mut EditsHistory,
        identifiers: impl IntoIterator<Item = Id>
    )
    {
        self.store_anchored_ids(identifiers);
        self.auxiliary.retain(|id| self.innards.is_selected(*id));
        self.innards.deselect_cluster(edits_history, self.auxiliary.iter());
    }

    /// Returns the center of the rectangle encompassing the [`Brush`]es with [`Id`]s returned by
    /// `ids`, if any.
    #[inline]
    #[must_use]
    pub fn brushes_center(&self, ids: impl ExactSizeIterator<Item = Id>) -> Option<Vec2>
    {
        if ids.len() == 0
        {
            return None;
        }

        let mut vx_count = 0f32;

        let center = ids.fold(Vec2::ZERO, |acc, id| {
            let center = self.brush(id).center();
            vx_count += 1f32;
            Vec2::new(acc.x + center.x, acc.y + center.y)
        });

        Some(center / vx_count)
    }

    /// Returns the center of the rectangle encompassing the selected [`Brush`]es, if any.
    #[inline]
    #[must_use]
    pub fn selected_brushes_center(&self) -> Option<Vec2>
    {
        self.brushes_center(self.selected_brushes_ids().copied())
    }

    /// Returns the center of the rectangle encompassing the selected textured [`Brush`]es, if any.
    #[inline]
    #[must_use]
    pub fn selected_textured_brushes_center(&self) -> Option<Vec2>
    {
        self.brushes_center(self.selected_textured_ids().copied())
    }

    /// Returns the [`Hull`] describing the rectangle encompassing all selected [`Brush`]es, if any.
    #[inline]
    #[must_use]
    pub fn selected_brushes_hull(&self) -> Option<Hull>
    {
        Hull::from_hulls_iter(self.selected_brushes_ids().map(|id| self.brush(*id).hull()))
    }

    /// Returns the [`Hull`] describing the rectangle encompassing all selected textured
    /// [`Brush`]es, if any.
    #[inline]
    #[must_use]
    pub fn selected_textured_brushes_hull(&self) -> Option<Hull>
    {
        Hull::from_hulls_iter(
            self.selected_textured_brushes()
                .map(|brush| brush.sprite_hull().unwrap_or(brush.hull()))
        )
    }

    /// Returns the [`Hull`] describing the rectangle encompassing all selected entities, if any.
    #[inline]
    #[must_use]
    pub fn selected_entities_hull(&self) -> Option<Hull>
    {
        Hull::from_hulls_iter(
            self.selected_brushes()
                .map(Brush::global_hull)
                .chain(self.selected_things().map(EntityHull::hull))
        )
    }

    /// Returns an iterator to all the selected [`Brush`]es with sprites.
    #[inline]
    pub fn selected_brushes_with_sprites(&mut self) -> impl Iterator<Item = &Brush>
    {
        /// The iterator to the brushes.
        struct Iter<'a>
        {
            /// All the identifiers.
            iter:     hashbrown::hash_map::Values<'a, String, Ids>,
            /// Identifiers of the [`Brush`]es with same sprite.
            sub_iter: hashbrown::hash_set::Iter<'a, Id>
        }

        impl<'a> Iterator for Iter<'a>
        {
            type Item = &'a Id;

            #[inline]
            #[must_use]
            fn next(&mut self) -> Option<Self::Item>
            {
                match self.sub_iter.next()
                {
                    id @ Some(_) => id,
                    None =>
                    {
                        self.sub_iter = self.iter.next()?.iter();
                        self.sub_iter.next()
                    }
                }
            }
        }

        let mut iter = self.innards.selected_sprites.values();
        let sub_iter = iter.next_value().iter();

        Iter { iter, sub_iter }.map(|id| self.brush(*id))
    }

    /// Returns an iterator to the [`BrushMut`]s wrapping the selected [`Brush`]es with sprites.
    #[inline]
    pub fn selected_brushes_with_sprite_mut(&mut self) -> impl Iterator<Item = BrushMut>
    {
        self.auxiliary.clear();

        for set in self.innards.selected_sprites.values()
        {
            self.auxiliary.0.extend(set);
        }

        SelectedBrushesMut::new(&mut self.innards, &mut self.quad_trees, &self.auxiliary)
    }

    /// Returns an iterator to the [`BrushMut`]s wrapping the selected [`Brush`]es with sprite
    /// `texture`.
    #[inline]
    pub fn selected_brushes_with_texture_sprite_mut(
        &mut self,
        texture: &str
    ) -> Option<impl Iterator<Item = BrushMut>>
    {
        self.auxiliary
            .replace_values(self.innards.selected_sprites.get(texture)?);
        SelectedBrushesMut::new(&mut self.innards, &mut self.quad_trees, &self.auxiliary).into()
    }

    /// Spawns a [`Brush`] generated from `polygon` and returns its [`Id`].
    #[inline]
    pub fn spawn_brush<'d>(
        &mut self,
        polygon: impl Into<Cow<'d, ConvexPolygon>>,
        edits_history: &mut EditsHistory,
        properties: Properties
    ) -> Id
    {
        self.innards
            .spawn_brush(&mut self.quad_trees, polygon, edits_history, properties)
    }

    /// Spawns a [`Brush`] generated from the arguments.
    #[inline]
    pub fn spawn_brush_from_parts(&mut self, identifier: Id, data: BrushData, selected: bool)
    {
        let brush = Brush::from_parts(data, identifier);
        self.innards.insert_brush(&mut self.quad_trees, brush, selected);
    }

    /// Spawns the entities created from `data`.
    #[inline]
    #[must_use]
    pub fn spawn_pasted_entity(
        &mut self,
        drawing_resources: &DrawingResources,
        edits_history: &mut EditsHistory,
        data: ClipboardData,
        delta: Vec2
    ) -> Id
    {
        self.innards.spawn_pasted_entity(
            drawing_resources,
            edits_history,
            &mut self.quad_trees,
            data,
            delta
        )
    }

    /// Spawns the [`Brush`]es created from `polygons`.
    #[inline]
    pub fn spawn_brushes(
        &mut self,
        mut polygons: impl ExactSizeIterator<Item = ConvexPolygon>,
        edits_history: &mut EditsHistory,
        properties: Properties
    )
    {
        for _ in 0..polygons.len() - 1
        {
            self.spawn_brush(polygons.next_value(), edits_history, properties.clone());
        }

        self.spawn_brush(polygons.next_value(), edits_history, properties);
    }

    /// Spawns a [`Brush`] created with a draw tool.
    #[inline]
    pub fn spawn_drawn_brush(
        &mut self,
        polygon: ConvexPolygon,
        drawn_brushes: &mut Ids,
        edits_history: &mut EditsHistory,
        default_properties: &DefaultProperties
    )
    {
        let id = self.innards.id_generator.new_id();

        let brush = Brush::from_polygon(polygon, id, default_properties.instance());

        edits_history.brush_draw(id);
        drawn_brushes.asserted_insert(id);

        self.innards.insert_brush(&mut self.quad_trees, brush, true);
    }

    /// Removes the [`Brush`] with [`Id`] `identifier` from the internal data structures.
    #[inline]
    fn remove_brush(&mut self, identifier: Id, selected: bool) -> Brush
    {
        self.innards.remove_brush(&mut self.quad_trees, identifier, selected)
    }

    /// Despawns the [`Brush`]es created with a draw tool whose [`Id`]s are contained in
    /// `drawn_brushes`.
    #[inline]
    pub fn despawn_drawn_brushes(
        &mut self,
        drawn_brushes: &mut Ids,
        edits_history: &mut EditsHistory
    )
    {
        for id in &*drawn_brushes
        {
            edits_history.drawn_brush_despawn(self.remove_brush(*id, true));
        }

        drawn_brushes.clear();
    }

    /// Despawns the [`Brush`] with [`Id`] `identifier`.
    #[inline]
    pub fn despawn_brush(
        &mut self,
        identifier: Id,
        edits_history: &mut EditsHistory,
        selected: bool
    )
    {
        self.innards
            .despawn_brush(&mut self.quad_trees, identifier, edits_history, selected);
    }

    /// Despawns the [`Brush`] with [`Id`] `identifier` and returns its parts.
    #[inline]
    pub fn despawn_brush_into_parts(&mut self, identifier: Id, selected: bool) -> BrushData
    {
        self.remove_brush(identifier, selected).into_parts().0
    }

    /// Despawns the [`Brush`] with [`Id`] `identifier`.
    #[inline]
    pub fn despawn_selected_brush(&mut self, identifier: Id, edits_history: &mut EditsHistory)
    {
        self.despawn_brush(identifier, edits_history, true);
    }

    /// Despawns the selected [`Brush`]es.
    #[inline]
    pub fn despawn_selected_brushes(&mut self, edits_history: &mut EditsHistory)
    {
        self.brushes_despawn
            .replace_values(self.innards.selected_brushes_ids().copied());

        self.brushes_despawn.sort_by(|a, b| {
            self.innards
                .brush(*a)
                .has_anchors()
                .cmp(&self.innards.brush(*b).has_anchors())
                .reverse()
        });

        for id in &self.brushes_despawn
        {
            self.innards
                .despawn_selected_brush(&mut self.quad_trees, *id, edits_history);
        }
    }

    /// Replaces the selected [`Brush`]es with the ones generated by `polygons`.
    #[inline]
    pub fn replace_selected_brushes(
        &mut self,
        polygons: impl ExactSizeIterator<Item = ConvexPolygon>,
        edits_history: &mut EditsHistory,
        properties: Properties
    )
    {
        self.despawn_selected_brushes(edits_history);
        self.spawn_brushes(polygons, edits_history, properties);
    }

    /// Duplicates the selected entities crating copies displaced by `delta`.
    #[inline]
    #[must_use]
    pub fn duplicate_selected_entities(
        &mut self,
        drawing_resources: &DrawingResources,
        edits_history: &mut EditsHistory,
        delta: Vec2
    ) -> bool
    {
        let valid = self.test_operation_validity(|manager| {
            manager
                .selected_entities()
                .find_map(|entity| (entity.hull() + delta).out_of_bounds().then_some(entity.id()))
        });

        if !valid
        {
            return false;
        }

        self.auxiliary.replace_values(
            self.innards
                .selected_brushes
                .iter()
                .chain(&self.innards.selected_things)
        );
        self.deselect_selected_entities(edits_history);

        for id in &self.auxiliary
        {
            let data = self.innards.entity(*id).copy_to_clipboard();

            _ = self.innards.spawn_pasted_entity(
                drawing_resources,
                edits_history,
                &mut self.quad_trees,
                data,
                delta
            );
        }

        true
    }

    /// Makes the Brush with [`Id`] `identifier` moving.
    #[inline]
    pub fn create_path(&mut self, identifier: Id, path: Path, edits_history: &mut EditsHistory)
    {
        self.innards
            .create_path(&mut self.quad_trees, identifier, path, edits_history);
    }

    /// Gives the [`Brush`] with [`Id`] `identifier` a [`Motor`].
    #[inline]
    pub fn set_path(&mut self, identifier: Id, path: Path)
    {
        self.innards.set_path(&mut self.quad_trees, identifier, path);
    }

    /// Removes the [`Path`] from the [`Brush`] with [`Id`] `identifier`.
    #[inline]
    pub fn remove_path(&mut self, identifier: Id) -> Path
    {
        self.innards.selected_moving.remove(&identifier);
        self.innards.moving.asserted_remove(&identifier);

        self.innards.possible_moving.asserted_insert(identifier);

        if self.is_selected(identifier)
        {
            self.innards.overall_node_update = true;
            self.innards.selected_possible_moving.asserted_insert(identifier);
        }

        self.moving_mut(identifier).take_path()
    }

    /// Removes the [`Motor`] from the selected [`Brush`] with [`Id`] `identifier`.
    #[inline]
    pub fn remove_selected_path(&mut self, identifier: Id, edits_history: &mut EditsHistory)
    {
        self.innards
            .remove_selected_path(&mut self.quad_trees, identifier, edits_history);
    }

    /// Replaces the [`Motor`] of the selected [`Brush`] with [`Id`] `identifier` with the one
    /// generated from `path`.
    #[inline]
    pub fn replace_selected_path(
        &mut self,
        identifier: Id,
        edits_history: &mut EditsHistory,
        path: Path
    )
    {
        self.innards
            .replace_selected_path(&mut self.quad_trees, identifier, edits_history, path);
    }

    /// Removes the [`Motor`]s from the selected [`Brush`]es.
    #[inline]
    pub fn remove_selected_paths(&mut self, edits_history: &mut EditsHistory)
    {
        self.auxiliary.replace_values(&self.innards.selected_moving);

        for id in &self.auxiliary
        {
            self.innards
                .remove_selected_path(&mut self.quad_trees, *id, edits_history);
        }
    }

    /// Returns a [`BrushesIter`] returning the visible anchors.
    #[inline]
    pub fn visible_anchors(&self, window: &Window, camera: &Transform) -> BrushesIter<'_>
    {
        self.brushes_iter(self.quad_trees.visible_anchors(camera, window))
    }

    /// Returns the amount of selected textured [`Brush`]es.
    #[inline]
    pub fn selected_textured_amount(&self) -> usize { self.innards.selected_textured.len() }

    /// Returns the amount of textured [`Brush`]es.
    #[inline]
    pub fn textured_amount(&self) -> usize { self.innards.textured.len() }

    /// Returns the amount of selected [`Brush`]es with sprites.
    #[inline]
    pub fn selected_sprites_amount(&self) -> usize { self.innards.selected_sprites.len() }

    /// Returns an iterator to the [`Id`]s of the selected textured [`Brush`]es.
    #[inline]
    pub fn selected_textured_ids(&self) -> impl ExactSizeIterator<Item = &Id>
    {
        self.innards.selected_textured.iter()
    }

    /// Returns an iterator to the selected textured [`Brush`]es.
    #[inline]
    pub fn selected_textured_brushes(&self) -> impl Iterator<Item = &Brush>
    {
        self.selected_textured_ids().map(|id| self.brush(*id))
    }

    /// Returns an iterator to the [`BrushMut`] wrapping the selected textured [`Brush`]es.
    #[inline]
    pub fn selected_textured_brushes_mut(&mut self) -> impl Iterator<Item = BrushMut>
    {
        self.auxiliary.replace_values(&self.innards.selected_textured);
        SelectedBrushesMut::new(&mut self.innards, &mut self.quad_trees, &self.auxiliary)
    }

    /// Returns a [`BrushesIter`] returning the [`Brush`]es with sprites at the position
    /// `cursor_pos`.
    #[inline]
    pub fn sprites_at_pos(&self, cursor_pos: Vec2) -> BrushesIter<'_>
    {
        self.brushes_iter(self.quad_trees.sprites_at_pos(cursor_pos))
    }

    /// Returns the visible [`Brush`]es with sprites.
    #[inline]
    pub fn visible_sprites(&self, window: &Window, camera: &Transform) -> BrushesIter<'_>
    {
        self.brushes_iter(self.quad_trees.visible_sprites(camera, window))
    }

    /// Anchors the [`Brush`] with [`Id`] `anchor_id` to the one with [`Id`] `owner_id`.
    #[inline]
    pub fn anchor(&mut self, owner_id: Id, anchor_id: Id)
    {
        self.innards.anchor(&mut self.quad_trees, owner_id, anchor_id);
    }

    /// Disanchors the [`Brush`] with [`Id`] `anchor_id` from the one with [`Id`] `owner_id`.
    #[inline]
    pub fn disanchor(&mut self, owner_id: Id, anchor_id: Id)
    {
        self.innards.disanchor(&mut self.quad_trees, owner_id, anchor_id);
    }

    /// Sets the texture of the [`Brush`] with [`Id`] identifier.
    /// Returns the name of the replaced texture, if any.
    #[inline]
    #[must_use]
    pub fn set_texture(
        &mut self,
        drawing_resources: &DrawingResources,
        identifier: Id,
        texture: &str
    ) -> TextureSetResult
    {
        self.innards
            .set_texture(drawing_resources, &mut self.quad_trees, identifier, texture)
    }

    /// Removes the texture from the [`Brush`] with [`Id`] identifier, and returns its
    /// [`TextureSettings`].
    #[inline]
    pub fn remove_texture(&mut self, identifier: Id) -> TextureSettings
    {
        self.innards.remove_texture(&mut self.quad_trees, identifier)
    }

    /// Set the [`TextureSettings`] of the [`Brush`] with [`Id`] `identifier`.
    #[inline]
    pub fn set_texture_settings(&mut self, identifier: Id, texture: TextureSettings)
    {
        assert!(self.is_selected(identifier), "Brush is not selected.");

        let sprite = texture.sprite();

        self.brush_mut(identifier).set_texture_settings(texture);
        self.innards.textured.asserted_insert(identifier);
        self.innards.selected_textured.asserted_insert(identifier);

        if sprite
        {
            self.innards.insert_selected_sprite(identifier);
        }
    }

    /// Sets the texture of the selected [`Brush`]es and returns a [`TextureResult`] describing the
    /// result of the procedure.
    #[inline]
    pub fn set_selected_brushes_texture(
        &mut self,
        drawing_resources: &DrawingResources,
        edits_history: &mut EditsHistory,
        texture: &str
    ) -> TextureResult
    {
        let valid = self.test_operation_validity(|manager| {
            manager.selected_brushes_with_sprite_mut().find_map(|mut brush| {
                (!brush.check_texture_change(drawing_resources, texture)).then_some(brush.id)
            })
        });

        if !valid
        {
            return TextureResult::Invalid;
        }

        let mut sprite = false;
        self.auxiliary.replace_values(&self.innards.selected_brushes);
        let mut iter = self.auxiliary.iter();

        for id in iter.by_ref()
        {
            let brush = self.brush(*id);

            if let Some(n) = brush.texture_settings().map(TextureInterface::name)
            {
                if n == texture
                {
                    continue;
                }
            }

            let has_sprite = brush.has_sprite();

            match self
                .innards
                .set_texture(drawing_resources, &mut self.quad_trees, *id, texture)
            {
                TextureSetResult::Unchanged => continue,
                TextureSetResult::Changed(prev) => edits_history.texture(*id, prev.into()),
                TextureSetResult::Set => edits_history.texture(*id, None)
            };

            if has_sprite
            {
                sprite = true;
                break;
            }
        }

        edits_history.texture_cluster(iter.filter_map(|id| {
            match self
                .innards
                .set_texture(drawing_resources, &mut self.quad_trees, *id, texture)
            {
                TextureSetResult::Unchanged => None,
                TextureSetResult::Changed(prev) => (*id, prev.into()).into(),
                TextureSetResult::Set => (*id, None).into()
            }
        }));

        if sprite
        {
            return TextureResult::ValidRefreshOutline;
        }

        TextureResult::Valid
    }

    /// Removes the textures from the selected [`Brush`]es.
    #[inline]
    pub fn remove_selected_textures(&mut self, edits_history: &mut EditsHistory)
    {
        self.auxiliary.replace_values(&self.innards.selected_textured);

        edits_history.texture_removal_cluster(
            self.auxiliary
                .iter()
                .map(|id| (*id, self.innards.remove_texture(&mut self.quad_trees, *id)))
        );

        self.innards.selected_textured.clear();
    }

    /// Sets whever the texture of the selected [`Brush`]es should be rendered as a sprite or not.
    #[inline]
    pub fn set_sprite(
        &mut self,
        drawing_resources: &DrawingResources,
        edits_history: &mut EditsHistory,
        value: bool
    )
    {
        if value
        {
            let valid = self.test_operation_validity(|manager| {
                manager.selected_textured_brushes_mut().find_map(|mut brush| {
                    (!brush.check_texture_sprite(drawing_resources, value)).then_some(brush.id())
                })
            });

            if !valid
            {
                return;
            }
        }

        self.auxiliary.replace_values(self.innards.selected_brushes_ids());

        /// Sets the sprite value.
        macro_rules! set {
            ($func:ident) => {
                for id in &self.auxiliary
                {
                    {
                        let mut brush = self.innards.brush_mut(&mut self.quad_trees, *id);
                        let (value, o_x, o_y) =
                            continue_if_none!(brush.set_texture_sprite(drawing_resources, value));
                        edits_history.sprite(brush.id(), value, o_x, o_y);
                    }

                    self.innards.$func(*id);
                }
            };
        }

        if value
        {
            set!(insert_selected_sprite);
            return;
        }

        set!(remove_selected_sprite);
    }

    /// Sets whever the texture of the selected [`Brush`] with [`Id`] `identifier` should be
    /// rendered as a sprite or not. Returns the previous sprite rendering parameters.
    #[inline]
    pub fn set_single_sprite(
        &mut self,
        drawing_resources: &DrawingResources,
        identifier: Id,
        value: bool
    ) -> (Sprite, f32, f32)
    {
        let out = self
            .brush_mut(identifier)
            .set_texture_sprite(drawing_resources, value)
            .unwrap();

        if value
        {
            self.innards.insert_selected_sprite(identifier);
        }
        else
        {
            self.innards.remove_selected_sprite(identifier);
        }

        out
    }

    /// Completes the texture reload.
    #[inline]
    pub fn finish_textures_reload(&mut self, drawing_resources: &DrawingResources)
    {
        self.auxiliary.replace_values(&self.innards.textured);

        for id in &self.auxiliary
        {
            let mut brush = self.innards.brush_mut(&mut self.quad_trees, *id);
            let name = {
                let settings = brush.texture_settings().unwrap();

                if !settings.sprite()
                {
                    continue;
                }

                continue_if_none!(drawing_resources.texture(settings.name())).name()
            };

            if !brush.check_texture_change(drawing_resources, name)
            {
                _ = brush.set_texture(drawing_resources, "error");
            }
        }
    }

    //==============================================================
    // Things

    /// Whever `identifier` belongs to a [`ThingInstance`].
    #[inline]
    #[must_use]
    pub fn is_thing(&self, identifier: Id) -> bool { self.innards.is_thing(identifier) }

    /// Returns a reference to the [`ThingInstance`] with [`Id`] `identifier`.
    #[inline]
    pub fn thing(&self, identifier: Id) -> &ThingInstance { self.innards.thing(identifier) }

    /// Returns a [`ThingMut`] wrapper to the [`ThingInstance`] with [`Id`] `identifier`.
    #[inline]
    pub fn thing_mut(&mut self, identifier: Id) -> ThingMut<'_>
    {
        self.innards.thing_mut(&mut self.quad_trees, identifier)
    }

    /// Returns the amount of [`ThingInstance`] in the map.
    #[inline]
    #[must_use]
    pub fn things_amount(&self) -> usize { self.innards.things.len() }

    /// Returns an iterator to all [`ThingInstance`]s in the map.
    #[inline]
    pub fn things(&self) -> impl Iterator<Item = &ThingInstance> { self.innards.things.values() }

    /// Returns the amount of [`ThingInstance`]s.
    #[inline]
    pub fn selected_things_amount(&self) -> usize { self.innards.selected_things_amount() }

    /// Whever any [`ThingInstance`] is currently selected.
    #[inline]
    #[must_use]
    pub fn any_selected_things(&self) -> bool { self.selected_things_amount() != 0 }

    /// Returns the [`Id`]s of the selected [`ThingInstance`]s.
    #[inline]
    pub fn selected_things_ids(&self) -> impl Iterator<Item = &Id>
    {
        self.innards.selected_things.iter()
    }

    /// Returns an iterator to the selected [`ThingInstance`]s.
    #[inline]
    pub fn selected_things(&self) -> impl Iterator<Item = &ThingInstance>
    {
        self.selected_things_ids().map(|id| self.thing(*id))
    }

    /// Returns an iterator to the [`ThingMut`]s wrapping the selected [`ThingInstance`]s.
    #[inline]
    pub fn selected_things_mut(&mut self) -> impl Iterator<Item = ThingMut<'_>>
    {
        self.auxiliary.replace_values(&self.innards.selected_things);
        SelectedThingsMut::new(&mut self.innards, &mut self.quad_trees, &self.auxiliary)
    }

    /// Spawns a new [`ThingInstance`] with id [`identifier`].
    #[inline]
    pub fn spawn_thing_from_parts(&mut self, identifier: Id, data: ThingInstanceData)
    {
        self.innards.insert_thing(
            ThingInstance::from_parts(identifier, data),
            &mut self.quad_trees,
            true
        );
    }

    /// Spawns a selected [`ThingInstance`] from the selected [`Thing`]. Returns its [`Id`].
    #[inline]
    pub fn spawn_selected_thing(
        &mut self,
        bundle: &ToolUpdateBundle,
        edits_history: &mut EditsHistory,
        settings: &mut ToolsSettings
    ) -> Id
    {
        let id = self.innards.new_id();

        self.innards.draw_thing(
            bundle.things_catalog.thing_instance(
                id,
                bundle.things_catalog.selected_thing().id(),
                settings.thing_pivot.spawn_pos(
                    bundle.things_catalog.selected_thing(),
                    bundle.cursor.world_snapped()
                ),
                bundle.things_default_properties
            ),
            &mut self.quad_trees,
            edits_history
        );

        id
    }

    /// Despawns the drawn [`ThingInstance`]s with [`Id`]s contained in `drawn_things`.
    #[inline]
    pub fn despawn_drawn_things(&mut self, drawn_things: &mut Ids, edits_history: &mut EditsHistory)
    {
        for id in &*drawn_things
        {
            let thing = self.innards.remove_thing(&mut self.quad_trees, *id);
            edits_history.drawn_thing_despawn(*id, thing.data().clone());
        }

        drawn_things.clear();
    }

    /// Returns a [`ThingsIter`] returning the [`ThingInstance`]s near `cursor_pos`. If
    /// `camera_scale` contains a value it returns the ones inside the cursor highlight.
    #[inline]
    pub fn things_at_pos(
        &self,
        cursor_pos: Vec2,
        camera_scale: impl Into<Option<f32>>
    ) -> ThingsIter<'_>
    {
        ThingsIter::new(self, self.quad_trees.things_at_pos(cursor_pos, camera_scale.into()))
    }

    /// Returns a [`SelectedThingsIter`] returning the selected [`ThingInstance`]s at the cursor
    /// pos, or near it if `camera_scale` contains a value.
    #[inline]
    pub fn selected_things_at_pos(
        &self,
        cursor_pos: Vec2,
        camera_scale: impl Into<Option<f32>>
    ) -> SelectedThingsIter<'_>
    {
        SelectedThingsIter::new(
            self,
            self.quad_trees.things_at_pos(cursor_pos, camera_scale.into())
        )
    }

    /// Returns a [`ThingsIter`] to the visible [`ThingInstance`]s.
    #[inline]
    pub fn visible_things(&self, window: &Window, camera: &Transform) -> ThingsIter<'_>
    {
        ThingsIter::new(self, self.quad_trees.visible_things(camera, window))
    }

    /// Remove the [`ThingInstance`] with [`Id`] `identifier` from the map.
    #[inline]
    pub fn remove_thing(&mut self, identifier: Id) -> ThingInstance
    {
        self.innards.remove_thing(&mut self.quad_trees, identifier)
    }

    /// Concludes the texture reloading process.
    #[inline]
    pub fn finish_things_reload(&mut self, things_catalog: &ThingsCatalog)
    {
        self.auxiliary.replace_values(self.innards.things.keys());
        let error = things_catalog.error();

        for id in &self.auxiliary
        {
            let mut instance = self.innards.thing_mut(&mut self.quad_trees, *id);
            let thing = continue_if_none!(things_catalog.thing(instance.thing()));

            if !instance.check_thing_change(thing)
            {
                _ = instance.set_thing(error);
            }
        }
    }

    //==============================================================
    // Moving

    /// Whever the [`Brush`] with [`Id`] `identifier` is moving.
    #[inline]
    #[must_use]
    pub fn is_moving(&self, identifier: Id) -> bool { self.innards.moving.contains(&identifier) }

    /// Whever the [`Brush`] with [`Id`] `identifier` is moving and selected.
    #[inline]
    #[must_use]
    pub fn is_selected_moving(&self, identifier: Id) -> bool
    {
        self.innards.selected_moving.contains(&identifier)
    }

    /// Whever there are any entities that don't have a [`Path`] but could have one.
    #[inline]
    #[must_use]
    pub fn any_selected_possible_moving(&self) -> bool
    {
        !self.innards.selected_possible_moving.is_empty()
    }

    /// Returns an iterator to the [`Id`]s of the selected moving [`Brush`]es.
    #[inline]
    pub fn selected_moving_ids(&self) -> impl Iterator<Item = &Id>
    {
        self.innards.selected_moving.iter()
    }

    /// Returns the amount of selected moving [`Brush`]es.
    #[inline]
    #[must_use]
    pub fn selected_moving_amount(&self) -> usize { self.innards.selected_moving.len() }

    /// Returns an iterator to the moving selected [`Brush`]es.
    #[inline]
    pub fn selected_moving(&self) -> impl Iterator<Item = &dyn Moving>
    {
        self.innards.selected_moving.iter().map(|id| self.moving(*id))
    }

    /// Returns an iterator to the moving selected [`Brush`]es wrapped in [`BrushMut`]s.
    #[inline]
    pub fn selected_movings_mut(&mut self) -> impl Iterator<Item = MovingMut<'_>>
    {
        self.auxiliary.replace_values(&self.innards.selected_moving);
        SelectedMovingsMut::new(&mut self.innards, &mut self.quad_trees, &self.auxiliary)
    }

    /// Returns all the [`MovementSimulator`] of the entities with a [`Path`].
    #[inline]
    pub fn movement_simulators(&self) -> HvVec<MovementSimulator>
    {
        hv_vec![collect; self.innards.moving.iter()
            .map(|id| self.moving(*id).movement_simulator())
        ]
    }

    /// Returns a vector containing the [`MovingSimulator`]s of the moving [`Brush`]es for the map
    /// preview.
    #[inline]
    pub fn selected_movement_simulators(&self) -> HvVec<MovementSimulator>
    {
        hv_vec![collect; self
            .selected_moving_ids()
            .map(|id| self.moving(*id).movement_simulator())
        ]
    }

    /// Returns a reference to the entity with id `identifier` as a trait object which implements
    /// the [`Moving`] trait.
    #[inline]
    pub fn moving(&self, identifier: Id) -> &dyn Moving { self.innards.moving(identifier) }

    /// Returns a [`MovingMut`] wrapping the entity with id `identifier`.
    #[inline]
    pub fn moving_mut(&mut self, identifier: Id) -> MovingMut<'_>
    {
        self.innards.moving_mut(&mut self.quad_trees, identifier)
    }

    /// Returns a [`SelectedMovingsIter`] returning an iterator to the selected entities with
    /// [`Path`]s.
    #[inline]
    pub fn selected_movings_at_pos(
        &self,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> SelectedMovingsIter<'_>
    {
        SelectedMovingsIter::new(self, self.quad_trees.paths_at_pos(cursor_pos, camera_scale))
    }

    /// Returns a [`MovingsIter`] returning an iterator to the entities with visible [`Path`]s.
    #[inline]
    pub fn visible_paths(&self, window: &Window, camera: &Transform) -> MovingsIter<'_>
    {
        MovingsIter::new(self, self.quad_trees.visible_paths(camera, window))
    }

    //==============================================================
    // Draw

    /// Returns the [`Animators`] for the map preview.
    #[inline]
    pub fn texture_animators(&self, drawing_resources: &DrawingResources) -> Animators
    {
        Animators::new(drawing_resources, self.innards.textured.iter().map(|id| self.brush(*id)))
    }

    /// Draws the UI error highlight.
    #[inline]
    pub fn draw_error_highlight(&mut self, bundle: &mut DrawBundle)
    {
        let error = return_if_none!(self.innards.error_highlight.draw(bundle));

        if self.innards.is_thing(error)
        {
            bundle.drawer.polygon_with_solid_color(
                self.thing(error).hull().rectangle().into_iter(),
                Color::ErrorHighlight
            );
            return;
        }

        self.brush(error)
            .draw_wih_solid_color(&mut bundle.drawer, Color::ErrorHighlight);
    }

    #[cfg(feature = "debug")]
    /// Draws the quad tree debug lines.
    #[inline]
    pub fn draw_debug_lines(
        &self,
        gizmos: &mut bevy::prelude::Gizmos,
        viewport: &Hull,
        camera_scale: f32
    )
    {
        self.quad_trees.draw(gizmos, viewport, camera_scale);
    }
}

//=======================================================================//

/// A wrapper for all the [`Brush`]es in the map.
#[derive(Clone, Copy)]
pub(in crate::map) struct Brushes<'a>(&'a HvHashMap<Id, Brush>);

impl<'a> Brushes<'a>
{
    /// Returns the [`Brush`] with [`Id`] `identifier`.
    /// # Panics
    /// Panics if the [`Brush`] does not exist.
    #[inline]
    pub fn get(&self, identifier: Id) -> &Brush { self.0.get(&identifier).unwrap() }

    /// Returns an iterator to the [`Brush`]es.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &Brush> { self.0.values() }
}

//=======================================================================//

/// A wrapper for a [`Brush`] that automatically updates certain [`EntitiesManager`] values when
/// it's dropped.
#[must_use]
pub(in crate::map) struct BrushMut<'a>
{
    /// A mutable reference to the [`EntitiesManager`] core.
    manager:           &'a mut Innards,
    /// A mutable reference to the [`QuadTree`]s.
    quad_trees:        &'a mut Trees,
    /// The [`Id`] of the [`Brush`].
    id:                Id,
    /// The [`Hull`] of the [`Brush`] at the moment the struct was created.
    hull:              Hull,
    /// The center of the [`Brush`] at the moment the struct was created.
    center:            Vec2,
    /// The [`Hull`] of the [`Path`] of the [`Brush`], if any, at the moment the struct was
    /// created.
    path_hull:         Option<Hull>,
    /// The [`Hull`] of the sprite and sprite highlight of the [`Brush`], if any, at the moment the
    /// struct was created.
    sprite_hull:       Option<Hull>,
    /// The amount of selected vertexes of the [`Brush`] at the moment the struct was created.
    selected_vertexes: bool
}

impl<'a> Deref for BrushMut<'a>
{
    type Target = Brush;

    #[inline]
    #[must_use]
    fn deref(&self) -> &Self::Target { self.manager.brush(self.id) }
}

impl<'a> DerefMut for BrushMut<'a>
{
    #[inline]
    #[must_use]
    fn deref_mut(&mut self) -> &mut Self::Target { self.manager.brushes.get_mut(&self.id).unwrap() }
}

impl<'a> Drop for BrushMut<'a>
{
    #[inline]
    fn drop(&mut self)
    {
        let brush = unsafe {
            std::ptr::from_mut(self.manager.brushes.get_mut(&self.id).unwrap())
                .as_mut()
                .unwrap()
        };

        /// Updates the quad trees storing the brush's hulls.
        macro_rules! update_quad_tree {
            ($(($name:ident, $func:ident $(, $schedule:block)?)),+) => { paste::paste! { $(
                match (&self.[< $name _hull >], brush.$func())
                {
                    (None, None) => (),
                    (None, Some(_)) =>
                    {
                        self.quad_trees.[< insert_ $name _hull >](brush);
                    },
                    (Some(hull), None) =>
                    {
                        self.quad_trees.[< remove_ $name _hull >](brush, hull);
                    },
                    (Some(prev_hull), Some(hull)) =>
                    {
                        if !prev_hull.around_equal_narrow(&hull)
                        {
                            $($schedule)?
                            self.quad_trees.[< replace_ $name _hull >](brush, &hull, prev_hull);
                        }
                    }
                };
            )+ }};
        }

        let new_hull = brush.hull();

        if !self.hull.around_equal_narrow(&new_hull)
        {
            self.manager.outline_update = true;
            self.quad_trees.replace_brush_hull(self.id, &new_hull, &self.hull);
        }

        // Has or had selected vertexes and now it doesn't.
        if brush.has_selected_vertexes() || self.selected_vertexes
        {
            self.manager.selected_vertexes_update.insert(self.id);
        }

        if !self.center.around_equal_narrow(&brush.center())
        {
            if brush.has_anchors()
            {
                self.manager.replace_anchors_hull(self.quad_trees, self.id);
            }
            else if let Some(id) = brush.anchored()
            {
                self.manager.replace_anchors_hull(self.quad_trees, id);
            }
        }

        update_quad_tree!(
            (path, path_hull),
            (sprite, sprite_and_anchor_hull, {
                self.manager.outline_update = true;
            })
        );

        if brush.was_texture_edited()
        {
            self.manager.overall_texture_update = true;
        }
    }
}

impl<'a> EntityId for BrushMut<'a>
{
    #[inline]
    fn id(&self) -> Id { self.deref().id() }

    #[inline]
    fn id_as_ref(&self) -> &Id { self.deref().id_as_ref() }
}

impl<'a> BrushMut<'a>
{
    /// Generates a new [`BrushMut`].
    #[inline]
    fn new(manager: &'a mut Innards, quad_trees: &'a mut Trees, identifier: Id) -> Self
    {
        let brush = manager.brush(identifier);
        let hull = brush.hull();
        let center = brush.center();
        let path_hull = brush.path_hull();
        let sprite_hull = brush.sprite_and_anchor_hull();
        let selected_vertexes = brush.has_selected_vertexes();

        Self {
            manager,
            quad_trees,
            id: identifier,
            hull,
            center,
            path_hull,
            sprite_hull,
            selected_vertexes
        }
    }
}

//=======================================================================//

/// A wrapper for a [`ThingInstance`] that automatically updates certain [`EntitiesManager`] values
/// when it's dropped.
#[must_use]
pub(in crate::map) struct ThingMut<'a>
{
    /// A mutable reference to the core of the [`EntitiesManager`].
    manager:    &'a mut Innards,
    /// A mutable reference to the [`QuadTree`]s.
    quad_trees: &'a mut Trees,
    /// The [`Id`] of the [`ThingInstance`].
    id:         Id,
    /// The [`Hull`] of the [`ThingInstance`] at the moment the struct was created.
    hull:       Hull,
    /// The [`Hull`] of the [`Path`] of the [`ThingInstance`] at the moment the struct was created,
    /// if any.
    path_hull:  Option<Hull>
}

impl<'a> Deref for ThingMut<'a>
{
    type Target = ThingInstance;

    #[inline]
    #[must_use]
    fn deref(&self) -> &Self::Target { self.manager.thing(self.id) }
}

impl<'a> DerefMut for ThingMut<'a>
{
    #[inline]
    #[must_use]
    fn deref_mut(&mut self) -> &mut Self::Target { self.manager.things.get_mut(&self.id).unwrap() }
}

impl<'a> Drop for ThingMut<'a>
{
    #[inline]
    fn drop(&mut self)
    {
        let thing = self.manager.things.get_mut(&self.id).unwrap();
        self.quad_trees.replace_thing_hull(thing, &self.hull);

        match (&self.path_hull, thing.path_hull())
        {
            (None, None) => (),
            (None, Some(_)) => self.quad_trees.insert_path_hull(thing),
            (Some(hull), None) => self.quad_trees.remove_path_hull(thing, hull),
            (Some(previous_hull), Some(current_hull)) =>
            {
                if !current_hull.around_equal_narrow(previous_hull)
                {
                    self.quad_trees.replace_path_hull(thing, &current_hull, previous_hull);
                }
            }
        };
    }
}

impl<'a> EntityId for ThingMut<'a>
{
    #[inline]
    fn id(&self) -> Id { self.deref().id() }

    #[inline]
    fn id_as_ref(&self) -> &Id { self.deref().id_as_ref() }
}

impl<'a> ThingMut<'a>
{
    /// Returns a new [`ThingMut`].
    #[inline]
    fn new(manager: &'a mut Innards, quad_trees: &'a mut Trees, identifier: Id) -> Self
    {
        let thing = manager.thing(identifier);
        let hull = thing.hull();
        let path_hull = thing.path_hull();

        Self {
            manager,
            quad_trees,
            id: identifier,
            hull,
            path_hull
        }
    }
}

//=======================================================================//

/// A wrapper for an entity that implements the [`EditPath`] trait.
#[must_use]
pub(in crate::map) enum MovingMut<'a>
{
    /// A [`Brush`].
    Brush(BrushMut<'a>),
    /// A [`ThingInstance`].
    Thing(ThingMut<'a>)
}

impl<'a> Deref for MovingMut<'a>
{
    type Target = dyn EditPath;

    #[inline]
    #[must_use]
    fn deref(&self) -> &Self::Target
    {
        match self
        {
            MovingMut::Brush(e) => &**e as &dyn EditPath,
            MovingMut::Thing(e) => &**e as &dyn EditPath
        }
    }
}

impl<'a> DerefMut for MovingMut<'a>
{
    #[inline]
    #[must_use]
    fn deref_mut(&mut self) -> &mut Self::Target
    {
        match self
        {
            MovingMut::Brush(e) => &mut **e as &mut dyn EditPath,
            MovingMut::Thing(e) => &mut **e as &mut dyn EditPath
        }
    }
}

impl<'a> EntityId for MovingMut<'a>
{
    #[inline]
    fn id(&self) -> Id { *self.id_as_ref() }

    #[inline]
    fn id_as_ref(&self) -> &Id
    {
        match self
        {
            MovingMut::Brush(e) => e.id_as_ref(),
            MovingMut::Thing(e) => e.id_as_ref()
        }
    }
}

impl<'a> EntityCenter for MovingMut<'a>
{
    #[inline]
    fn center(&self) -> Vec2
    {
        match self
        {
            MovingMut::Brush(e) => e.center(),
            MovingMut::Thing(e) => e.center()
        }
    }
}

impl<'a> MovingMut<'a>
{
    /// Returns a new [`MovingMut`].
    #[inline]
    pub(in crate::map::editor::state::manager) fn new(
        manager: &'a mut Innards,
        quad_trees: &'a mut Trees,
        identifier: Id
    ) -> Self
    {
        if manager.is_thing(identifier)
        {
            return Self::Thing(ThingMut::new(manager, quad_trees, identifier));
        }

        Self::Brush(BrushMut::new(manager, quad_trees, identifier))
    }
}
