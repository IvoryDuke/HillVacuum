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

use bevy::{transform::components::Transform, window::Window};
use glam::Vec2;
use hill_vacuum_shared::{continue_if_none, return_if_none, NextValue};
use quad_tree::InsertResult;

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
    clipboard::{Clipboard, ClipboardData, CopyToClipboard},
    core::Core,
    editor_state::ToolsSettings,
    edits_history::EditsHistory,
    grid::Grid,
    inputs_presses::InputsPresses,
    ui::Ui
};
use crate::{
    map::{
        brush::{
            convex_polygon::{ConvexPolygon, TextureSetResult},
            Brush,
            BrushData
        },
        drawer::{
            animation::Animator,
            color::Color,
            drawers::EditDrawer,
            drawing_resources::DrawingResources,
            texture::{TextureInterface, TextureInterfaceExtra, TextureSettings, TextureSpriteSet}
        },
        editor::{
            state::{editor_state::TargetSwitch, manager::quad_tree::QuadTreeIds},
            AllDefaultProperties,
            StateUpdateBundle,
            ToolUpdateBundle
        },
        hv_vec,
        path::{EditPath, MovementSimulator, Moving, Path},
        properties::{
            read_default_properties,
            BrushProperties,
            DefaultBrushProperties,
            DefaultThingProperties,
            EngineDefaultProperties,
            PropertiesRefactor
        },
        thing::{catalog::ThingsCatalog, ThingInstance, ThingInstanceData, ThingInterface},
        AssertedInsertRemove,
        FileStructure,
        HvHashMap,
        MapHeader,
        OutOfBounds,
        Viewer
    },
    utils::{
        collections::{hv_hash_map, hv_hash_set, Ids},
        hull::Hull,
        identifiers::{EntityCenter, EntityId, Id, IdGenerator},
        math::AroundEqual,
        misc::{Blinker, ReplaceValues, TakeValue}
    },
    warning_message,
    HvHashSet,
    HvVec
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

pub(in crate::map::editor::state) enum Entity<'a>
{
    Brush(&'a Brush),
    Thing(&'a ThingInstance)
}

impl EntityId for Entity<'_>
{
    #[inline]
    fn id(&self) -> Id { *self.id_as_ref() }

    #[inline]
    fn id_as_ref(&self) -> &Id
    {
        match self
        {
            Entity::Brush(brush) => brush.id_as_ref(),
            Entity::Thing(thing) => thing.id_as_ref()
        }
    }
}

impl DrawHeight for Entity<'_>
{
    #[inline]
    fn draw_height(&self) -> Option<f32>
    {
        match self
        {
            Entity::Brush(brush) => brush.draw_height(),
            Entity::Thing(thing) => DrawHeight::draw_height(*thing)
        }
    }
}

impl CopyToClipboard for Entity<'_>
{
    #[inline]
    fn copy_to_clipboard(&self) -> ClipboardData
    {
        match self
        {
            Entity::Brush(brush) => brush.copy_to_clipboard(),
            Entity::Thing(thing) => thing.copy_to_clipboard()
        }
    }
}

impl Entity<'_>
{
    #[inline]
    fn hull(
        &self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid
    ) -> Hull
    {
        match self
        {
            Entity::Brush(brush) => brush.hull(drawing_resources, grid),
            Entity::Thing(thing) => thing.hull(things_catalog)
        }
    }
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
// STRUCTS
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

    /// Whether `self` contains `id`.
    #[inline]
    #[must_use]
    fn contains(&self, id: Id) -> bool { self.0.contains(&id) }

    /// Returns an iterator to the contained elements.
    #[inline]
    pub fn iter(&self) -> hashbrown::hash_set::Iter<Id> { self.0.iter() }

    /// Pushes the [`Id`]s of the attached brushes of `brushes`.
    #[inline]
    fn store_attached_ids<'a>(&mut self, brushes: impl Iterator<Item = &'a Brush>)
    {
        self.0.clear();

        for brush in brushes
        {
            self.0.extend(continue_if_none!(brush.attachments_iter()));
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

#[must_use]
struct SelectedSprites(HvHashMap<String, Ids>, usize);

impl Default for SelectedSprites
{
    #[inline]
    fn default() -> Self { Self(hv_hash_map![capacity; 10], 0) }
}

impl SelectedSprites
{
    #[inline]
    #[must_use]
    const fn len(&self) -> usize { self.1 }

    #[inline]
    #[must_use]
    fn get(&self, texture: &str) -> Option<&Ids> { self.0.get(texture) }

    #[inline]
    fn insert(&mut self, brush: &Brush)
    {
        let texture = brush.texture_settings().unwrap();
        assert!(texture.sprite(), "Brush has no sprite.");
        let texture = texture.name();

        match self.0.get_mut(texture)
        {
            Some(ids) => ids.asserted_insert(brush.id()),
            None =>
            {
                self.0.asserted_insert((texture.to_owned(), hv_hash_set![brush.id()]));
            }
        };

        self.1 += 1;
    }

    #[inline]
    fn remove(&mut self, brush: &Brush)
    {
        self.0
            .get_mut(brush.texture_settings().unwrap().name())
            .unwrap()
            .asserted_remove(brush.id_as_ref());
        self.1 -= 1;
    }

    #[inline]
    fn remove_texture(&mut self, identifier: Id, texture: &TextureSettings)
    {
        self.0.get_mut(texture.name()).unwrap().asserted_remove(&identifier);
        self.1 -= 1;
    }

    #[inline]
    fn replace(&mut self, brush: &Brush, prev_texture: &str)
    {
        self.0
            .get_mut(prev_texture)
            .unwrap()
            .asserted_remove(brush.id_as_ref());

        self.insert(brush);
    }

    #[inline]
    fn clear(&mut self)
    {
        for ids in self.0.values_mut()
        {
            ids.clear();
        }

        self.1 = 0;
    }

    #[inline]
    fn values(&self) -> hashbrown::hash_map::Values<String, Ids> { self.0.values() }
}

//=======================================================================//

/// The error drawer.
#[must_use]
struct ErrorHighlight
{
    /// The latest occurred error.
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

    /// Check whether the stored error concerns the entity `error` and removes it if that is the
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
    fn draw(&mut self, delta_time: f32) -> Option<Id>
    {
        if self.blinks == 0
        {
            return None;
        }

        let prev = self.blinker.on();
        let cur = self.blinker.update(delta_time);

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
pub(in crate::map) struct Animators
{
    brushes: HvHashMap<Id, Animator>,
    things:  HvHashMap<String, Animator>
}

impl Animators
{
    /// Returns a new [`Animators`].
    #[inline]
    pub(in crate::map::editor::state) fn new(
        bundle: &StateUpdateBundle,
        manager: &EntitiesManager
    ) -> Self
    {
        let StateUpdateBundle {
            drawing_resources,
            things_catalog,
            ..
        } = bundle;

        let previews = hv_hash_set![collect; manager.things().filter_map(|thing| {
            let texture = things_catalog.texture(thing.thing_id());
            drawing_resources.is_animated(texture).then_some(texture)
        })];

        Self {
            brushes: hv_hash_map![collect; manager.innards.textured.iter().filter_map(|id| {
                manager.brush(*id).animator(drawing_resources).map(|anim| (*id, anim))
            })],
            things:  hv_hash_map![collect; previews.into_iter().map(|texture| {
                (texture.to_string(), Animator::new(drawing_resources.texture(texture).unwrap().animation()).unwrap())
            })]
        }
    }

    /// Returns a reference to the [`Animator`] associated with the [`Brush`] with [`Id`]
    /// `identifier`, if any.
    #[inline]
    pub fn get_brush_animator(&self, identifier: Id) -> Option<&Animator>
    {
        self.brushes.get(&identifier)
    }

    #[inline]
    pub fn get_thing_animator(&self, texture: &str) -> Option<&Animator>
    {
        self.things.get(texture)
    }

    /// Updates the contained [`Animator`]s based of the time that has passed since the last update.
    #[inline]
    pub(in crate::map::editor::state) fn update(&mut self, bundle: &ToolUpdateBundle)
    {
        for (id, a) in &mut self.brushes
        {
            match a
            {
                Animator::List(a) =>
                {
                    a.update(
                        bundle
                            .manager
                            .brush(*id)
                            .texture_settings()
                            .unwrap()
                            .overall_animation(bundle.drawing_resources)
                            .get_list_animation(),
                        bundle.delta_time
                    );
                },
                Animator::Atlas(a) =>
                {
                    a.update(
                        bundle
                            .manager
                            .brush(*id)
                            .texture_settings()
                            .unwrap()
                            .overall_animation(bundle.drawing_resources)
                            .get_atlas_animation(),
                        bundle.delta_time
                    );
                }
            };
        }

        for (name, a) in &mut self.things
        {
            match a
            {
                Animator::List(a) =>
                {
                    a.update(
                        bundle
                            .drawing_resources
                            .texture_or_error(name)
                            .animation()
                            .get_list_animation(),
                        bundle.delta_time
                    );
                },
                Animator::Atlas(a) =>
                {
                    a.update(
                        bundle
                            .drawing_resources
                            .texture_or_error(name)
                            .animation()
                            .get_atlas_animation(),
                        bundle.delta_time
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
    /// All the brushes on the map.
    brushes: HvHashMap<Id, Brush>,
    /// All the [`Thing`]s on the map.
    things: HvHashMap<Id, ThingInstance>,
    /// The currently selected brushes.
    selected_brushes: Ids,
    /// The currently selected [`Thing`]s.
    selected_things: Ids,
    /// The [`Id`]s of all the moving brushes.
    moving: Ids,
    /// The [`Id`]s of the selected moving brushes.
    selected_moving: Ids,
    /// The [`Id`]s of the entities that do not have a [`Path`] but could have one.
    possible_moving: Ids,
    /// The [`Id`]s of the selected entities that do not have a [`Path`] but could have one.
    selected_possible_moving: Ids,
    /// The [`Id`]s of the textured moving brushes.
    textured: Ids,
    /// The [`Id`]s of the selected textured brushes.
    selected_textured: Ids,
    /// The [`Id`]s of the selected brushes with associated sprites.
    selected_sprites: SelectedSprites,
    /// The [`Id`]s of the moving brushes with attachments.
    brushes_with_attachments: HvHashMap<Id, Hull>,
    /// The generator of the [`Id`]s of the new entities.
    id_generator: IdGenerator,
    /// The error drawer.
    error_highlight: ErrorHighlight,
    /// Whether the tool outline should be updated.
    outline_update: bool,
    /// The [`Id`]s of the brushes whose amount of selected vertexes changed, necessary for the
    /// update of the vertex and side tools.
    selected_vertexes_update: Ids,
    /// Whether the texture displayed in the texture editor should be updated.
    overall_texture_update: bool,
    /// Whether the info displayed in the platform tool's node editor should be updated.
    overall_node_update: bool,
    /// Whether the overall value of the selected brushes' collision should be updated.
    overall_collision_update: bool,
    /// Whether the overall properties of the brushes should be updated.
    overall_brushes_properties_update: PropertyUpdate,
    /// Whether the overall value of the draw height of the selected [`Thing`]s should be updated.
    overall_things_info_update: bool,
    /// Whether the overall properties of the [`ThingInstance`]s should be updated.
    overall_things_properties_update: PropertyUpdate,
    /// Whether the properties where refactored after loading a map file.
    loaded_file_modified: bool
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
            selected_sprites: SelectedSprites::default(),
            brushes_with_attachments: hv_hash_map![],
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
            loaded_file_modified: false
        }
    }

    /// Reads the brushes and [`Thing`]s from `file`.
    /// Returns an error if it occurred.
    #[inline]
    pub(in crate::map::editor::state) fn load<I: Iterator<Item = FileStructure>>(
        &mut self,
        header: &MapHeader,
        file: &mut BufReader<File>,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid,
        default_properties: &AllDefaultProperties,
        steps: &mut I,
        quad_trees: &mut Trees
    ) -> Result<(Option<DefaultBrushProperties>, Option<DefaultThingProperties>), &'static str>
    {
        use crate::map::{brush::BrushViewer, thing::ThingViewer};

        macro_rules! removed_message {
            ($entities:literal) => {
                warning_message(concat!(
                    "Some ",
                    $entities,
                    " did not fit within the boundaries of the map and have therefore been \
                     removed. Be careful before saving the file."
                ));
            };
        }

        /// Stores in `map_default_properties` the desired properties and returns a
        /// [`PropertiesRefactor`] if the default and file properties do not match.
        #[inline]
        fn mismatching_properties<'a, E: EngineDefaultProperties>(
            engine_default_properties: &'a E,
            file_default_properties: &E::Inner,
            entity: &str
        ) -> Option<PropertiesRefactor<'a, E>>
        {
            if engine_default_properties.eq(file_default_properties)
            {
                return None;
            }

            let description = format!(
                "The engine default {entity} properties are different from the ones stored in the \
                 map file.\nIf you decide to use the engine defined ones, all values currently \
                 contained in the {entity} that do not match will be removed, and the missing \
                 ones will be inserted.\n- Press YES to use the engine properties;\n- Press NO to \
                 use the map file properties.\n\nHere are the two property \
                 lists:\n\nENGINE:\n{engine_default_properties}\n\nMAP:\n{file_default_properties}"
            );

            match rfd::MessageDialog::new()
                .set_level(rfd::MessageLevel::Warning)
                .set_title("WARNING")
                .set_description(&description)
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
            {
                rfd::MessageDialogResult::Yes =>
                {
                    let refactor =
                        engine_default_properties.generate_refactor(file_default_properties);
                    refactor.into()
                },
                rfd::MessageDialogResult::No => None,
                _ => unreachable!()
            }
        }

        let mut max_id = Id::ZERO;
        let mut brushes = hv_vec![];
        let mut with_attachments = hv_vec![];

        steps.next_value().assert(FileStructure::Properties);
        let (file_default_brush_properties, file_default_thing_properties) =
            read_default_properties(file)?;

        steps.next_value().assert(FileStructure::Brushes);
        let b_refactor = mismatching_properties(
            default_properties.engine_brushes,
            &file_default_brush_properties,
            "brushes"
        );
        let mut brushes_removed = false;

        for _ in 0..header.brushes
        {
            let mut brush = Brush::from_viewer(
                ciborium::from_reader::<BrushViewer, _>(&mut *file)
                    .map_err(|_| "Error reading brushes")?
            );

            if brush.hull(drawing_resources, grid).out_of_bounds()
            {
                brushes_removed = true;
                continue;
            }

            max_id = max_id.max(brush.id());

            if brush.has_attachments()
            {
                with_attachments.push(brush);
                continue;
            }

            if brush.attached().is_some()
            {
                _ = brush.take_mover();
            }

            brushes.push(brush);
        }

        if brushes_removed
        {
            removed_message!("brushes");
        }

        if let Some(refactor) = &b_refactor
        {
            for brush in &mut brushes
            {
                brush.refactor_properties(refactor);
            }
        }

        steps.next_value().assert(FileStructure::Things);
        let mut things = hv_vec![];
        let t_refactor = mismatching_properties(
            default_properties.engine_things,
            &file_default_thing_properties,
            "things"
        );
        let mut things_removed = false;

        for _ in 0..header.things
        {
            let thing = ThingInstance::from_viewer(
                ciborium::from_reader::<ThingViewer, _>(&mut *file)
                    .map_err(|_| "Error reading things")?
            );

            if thing.hull(things_catalog).out_of_bounds()
            {
                things_removed = true;
                continue;
            }

            max_id = max_id.max(thing.id());
            things.push(thing);
        }

        if things_removed
        {
            removed_message!("things");
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
            self.insert_brush(drawing_resources, grid, quad_trees, brush, false);
        }

        for brush in with_attachments
        {
            self.insert_brush(drawing_resources, grid, quad_trees, brush, false);
        }

        for thing in things
        {
            self.insert_thing(things_catalog, thing, quad_trees, false);
        }

        self.id_generator.reset(max_id);
        _ = self.id_generator.new_id();
        self.loaded_file_modified =
            b_refactor.is_some() || t_refactor.is_some() || brushes_removed || things_removed;
        
        let brushes = match b_refactor
        {
            Some(_) => None,
            None => file_default_brush_properties.into()
        };

        let things = match t_refactor
        {
            Some(_) => None,
            None => file_default_thing_properties.into()
        };

        Ok((brushes, things))
    }

    //==============================================================
    // General

    /// Whether `identifier` is a selected entity.
    #[inline]
    #[must_use]
    fn is_selected(&self, identifier: Id) -> bool
    {
        self.selected_brushes.contains(&identifier) || self.selected_things.contains(&identifier)
    }

    /// Whether `identifier` belongs to an entity that exists.
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
    pub(in crate::map::editor::state) fn entity(&self, identifier: Id) -> Entity
    {
        self.brushes
            .get(&identifier)
            .map(Entity::Brush)
            .or(self.things.get(&identifier).map(Entity::Thing))
            .unwrap()
    }

    /// Spawns an entity pasted from the [`Clipboard`].
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn spawn_pasted_entity(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        edits_history: &mut EditsHistory,
        grid: &Grid,
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
                brush.move_by_delta(delta, true);
                edits_history.brush_spawn(brush.id(), true);
                self.insert_brush(drawing_resources, grid, quad_trees, brush, true);
            },
            ClipboardData::Thing(data, _) =>
            {
                let mut thing = ThingInstance::from_parts(id, data);
                thing.move_by_delta(delta);
                self.spawn_thing(things_catalog, thing, quad_trees, edits_history);
            }
        };

        id
    }

    //==============================================================
    // Selected entities

    /// Returns the amount of selected brushes.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn selected_brushes_amount(&self) -> usize
    {
        self.selected_brushes.len()
    }

    /// Returns the [`Id`]s of the selected brushes.
    #[inline]
    pub(in crate::map::editor::state) fn selected_brushes_ids(
        &self
    ) -> impl ExactSizeIterator<Item = &Id> + Clone
    {
        self.selected_brushes.iter()
    }

    /// Returns the [`Id`]s of the selected brushes and [`Thing`]s.
    #[inline]
    pub(in crate::map::editor::state) fn selected_entities_ids(&self) -> impl Iterator<Item = &Id>
    {
        self.selected_brushes_ids().chain(&self.selected_things)
    }

    /// Updates the value related to entity selection for the entity `identifier`.
    /// Returns true if entity is a [`ThingInstance`].
    /// # Panics
    /// Panics if the entity does not exist, or it belongs to a textured brush and it is not
    /// part of the set of textured brushes [`Id`]s.
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
            self.selected_sprites.remove(self.brushes.get(&identifier).unwrap());
        }

        self.outline_update = true;
        self.overall_texture_update = true;
        false
    }

    /// Inserts the [`Id`] of a brush with sprite in the selected sprites set.
    /// # Panics
    /// Panics if the brush has no sprite.
    #[inline]
    fn insert_selected_sprite(&mut self, identifier: Id)
    {
        self.selected_sprites.insert(self.brushes.get(&identifier).unwrap());
    }

    /// Removes the [`Id`] of a brush with sprite from the selected sprites set.
    /// # Panics
    /// Panics if the brush has a sprite.
    #[inline]
    fn remove_selected_sprite(&mut self, identifier: Id)
    {
        self.selected_sprites.remove(self.brushes.get(&identifier).unwrap());
    }

    /// Removes the texture from the brush with [`Id`] `identifier`, and returns its
    /// [`TextureSettings`].
    #[inline]
    pub(in crate::map::editor::state) fn remove_texture(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        quad_trees: &mut Trees,
        identifier: Id
    ) -> TextureSettings
    {
        assert!(self.is_selected(identifier), "Brush is not selected.");
        let texture = self
            .brush_mut(drawing_resources, grid, quad_trees, identifier)
            .remove_texture();

        if texture.sprite()
        {
            self.selected_sprites.remove_texture(identifier, &texture);
        }

        self.textured.asserted_remove(&identifier);
        self.selected_textured.asserted_remove(&identifier);

        texture
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

    /// Returns a reference to the brush with [`Id`] `identifier`.
    /// # Panics
    /// Panics if the brush does not exist.
    #[inline]
    pub(in crate::map::editor::state) fn brush(&self, identifier: Id) -> &Brush
    {
        self.brushes
            .get(&identifier)
            .unwrap_or_else(|| panic!("Failed brush() call for id {identifier:?}"))
    }

    /// Returns a [`BrushMut`] wrapping the brush with [`Id`] `identifier`.
    /// # Panics
    /// Panics if the brush does not exist.
    #[inline]
    pub(in crate::map::editor::state) fn brush_mut<'a>(
        &'a mut self,
        drawing_resources: &'a DrawingResources,
        grid: &'a Grid,
        quad_trees: &'a mut Trees,
        identifier: Id
    ) -> BrushMut<'a>
    {
        BrushMut::new(drawing_resources, self, grid, quad_trees, identifier)
    }

    /// Returns a [`Brushes`] wrapping the existing brushes.
    #[inline]
    const fn brushes(&self) -> Brushes { Brushes(&self.brushes) }

    /// Attaches the brush with [`Id`] `attachment` to the one with [`Id`] `owner`.
    #[inline]
    fn attach(&mut self, quad_trees: &mut Trees, owner: Id, attachment: Id)
    {
        _ = self.remove_anchors_hull(quad_trees, owner);

        let [o_brush, a_brush] = self.brushes.get_many_mut([&owner, &attachment]).unwrap();
        o_brush.attach_brush(a_brush);
        self.possible_moving.asserted_remove(&attachment);
        self.selected_possible_moving.asserted_remove(&attachment);

        assert!(self.insert_anchors_hull(quad_trees, owner), "Could not insert attachment.");
    }

    /// Detaches the brush with [`Id`] `attachment` from the one with [`Id`] `owner`.
    #[inline]
    pub(in crate::map::editor::state) fn detach(
        &mut self,
        quad_trees: &mut Trees,
        owner: Id,
        attachment: Id
    )
    {
        assert!(
            self.remove_anchors_hull(quad_trees, owner),
            "Could not remove hull from quad trees."
        );

        let [o_brush, a_brush] = self.brushes.get_many_mut([&owner, &attachment]).unwrap();
        o_brush.detach_brush(a_brush);
        self.possible_moving.asserted_insert(attachment);
        self.selected_possible_moving.asserted_insert(attachment);

        _ = self.insert_anchors_hull(quad_trees, owner);
    }

    /// Selects the [`Id`]s of the brushes attached to the ones with [`Id`]s contained in
    /// `identifiers`.
    #[inline]
    fn select_attached_brushes(
        &mut self,
        edits_history: &mut EditsHistory,
        auxiliary: &mut AuxiliaryIds,
        identifiers: impl IntoIterator<Item = Id>
    )
    {
        auxiliary.store_attached_ids(identifiers.into_iter().map(|id| self.brush(id)));
        auxiliary.retain(|id| !self.is_selected(*id));
        self.select_cluster(edits_history, auxiliary.iter());
    }

    /// Selects the brushes attached to the selected ones.
    #[inline]
    fn select_attached_brushes_of_selected_brushes(
        &mut self,
        edits_history: &mut EditsHistory,
        auxiliary: &mut AuxiliaryIds
    )
    {
        auxiliary.store_attached_ids(self.selected_brushes.iter().map(|id| self.brush(*id)));
        auxiliary.retain(|id| !self.is_selected(*id));
        self.select_cluster(edits_history, auxiliary.iter());
    }

    /// Adds a brush to the map.
    /// # Panics
    /// Panics if the brush has attached brushes but the [`Hull`] describing the attachments
    /// area could not be retrieved.
    #[inline]
    fn insert_brush(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        quad_trees: &mut Trees,
        brush: Brush,
        selected: bool
    )
    {
        let id = brush.id();
        assert!(
            quad_trees.insert_brush_hull(&brush).inserted(),
            "Brush hull was already in the quad tree."
        );
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
            assert!(
                quad_trees.insert_path_hull(&brush).inserted(),
                "Brush path hull was already in the quad tree."
            );
        }

        if brush.has_texture()
        {
            self.overall_texture_update = true;
            self.textured.asserted_insert(id);
        }

        let attached = brush.attached();
        let has_attachments = brush.has_attachments();
        let has_sprite = brush.has_sprite();

        if has_attachments
        {
            for id in brush.attachments_iter().unwrap()
            {
                self.brush_mut(drawing_resources, grid, quad_trees, *id)
                    .attach(brush.id());
                self.possible_moving.asserted_remove(id);
                self.selected_possible_moving.remove(id);
            }
        }
        else if let Some(id) = attached
        {
            self.brush_mut(drawing_resources, grid, quad_trees, id)
                .insert_attachment(&brush);
        }

        self.brushes.asserted_insert((id, brush));

        if selected
        {
            self.insert_entity_selection(id);
        }

        if has_attachments
        {
            assert!(self.insert_anchors_hull(quad_trees, id), "Brush has no attachments.");
        }
        else if let Some(id) = attached
        {
            _ = self.remove_anchors_hull(quad_trees, id);
            _ = self.insert_anchors_hull(quad_trees, id);
        }

        if has_sprite
        {
            assert!(
                quad_trees
                    .insert_sprite_hull(drawing_resources, grid, self.brush(id))
                    .inserted(),
                "Sprite hull was already in the quad tree."
            );
        }
    }

    /// Removes the brush with [`Id`] `identifier` from the map and returns it.
    /// # Panics
    /// Panics if there are discrepancies between the brush properties and the stored
    /// information.
    #[inline]
    fn remove_brush(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        quad_trees: &mut Trees,
        identifier: Id
    ) -> (Brush, bool)
    {
        self.outline_update = true;
        self.error_highlight.check_entity_error_removal(identifier);
        let selected = self.is_selected(identifier);

        if selected
        {
            self.remove_entity_selection(identifier);
        }

        let has_attachments = self.brush(identifier).has_attachments();

        if has_attachments
        {
            _ = self.remove_anchors_hull(quad_trees, identifier);
        }
        else
        {
            assert!(
                !self.brushes_with_attachments.contains_key(&identifier),
                "Brush is stored as having attachments."
            );
        }

        let brush = self.brushes.remove(&identifier).unwrap();
        assert!(quad_trees.remove_brush_hull(&brush), "Brush hull was not in the quad tree.");

        if brush.has_selected_vertexes()
        {
            self.selected_vertexes_update.insert(identifier);
        }

        if brush.has_path()
        {
            self.overall_node_update = true;
            assert!(
                quad_trees.remove_path_hull(&brush),
                "Brush path hull was not in the quad tree."
            );
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

        if has_attachments
        {
            for id in brush.attachments_iter().unwrap()
            {
                self.brush_mut(drawing_resources, grid, quad_trees, *id).detach();
                self.possible_moving.asserted_insert(*id);

                if self.is_selected(*id)
                {
                    self.selected_possible_moving.asserted_insert(*id);
                }
            }
        }
        else if let Some(id) = brush.attached()
        {
            self.brush_mut(drawing_resources, grid, quad_trees, id)
                .remove_attachment(&brush);
            self.replace_anchors_hull(quad_trees, id);
        }

        if brush.has_texture()
        {
            self.overall_texture_update = true;
            self.textured.asserted_remove(&identifier);
        }

        if brush.has_sprite()
        {
            assert!(quad_trees.remove_sprite_hull(&brush), "Sprite hull was not in the quad tree.");
        }

        (brush, selected)
    }

    /// Returns a new unique [`Id`].
    #[inline]
    #[must_use]
    fn new_id(&mut self) -> Id { self.id_generator.new_id() }

    /// Spawns a brush in the map and returns its [`Id`].
    #[inline]
    pub(in crate::map::editor::state) fn spawn_brush<'a>(
        &mut self,
        drawing_resources: &DrawingResources,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        quad_trees: &mut Trees,
        polygon: impl Into<Cow<'a, ConvexPolygon>>,
        properties: BrushProperties
    ) -> Id
    {
        let id = self.new_id();

        let brush = Brush::from_polygon(polygon, id, properties);

        edits_history.brush_spawn(id, true);
        self.insert_brush(drawing_resources, grid, quad_trees, brush, true);

        id
    }

    /// Despawns the brush with [`Id`] `identifier` from the map.
    #[inline]
    fn despawn_brush(
        &mut self,
        drawing_resources: &DrawingResources,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        quad_trees: &mut Trees,
        identifier: Id
    )
    {
        let (brush, selected) = self.remove_brush(drawing_resources, grid, quad_trees, identifier);
        edits_history.brush_despawn(brush, selected);
    }

    /// Despawns all selected brushes.
    #[inline]
    fn despawn_selected_brush(
        &mut self,
        drawing_resources: &DrawingResources,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        quad_trees: &mut Trees,
        identifier: Id
    )
    {
        self.despawn_brush(drawing_resources, edits_history, grid, quad_trees, identifier);
    }

    /// Sets the texture of the brush with [`Id`] `identifier`.
    /// Returns the [`TextureMetadata`] of the replaced texture, if any.
    /// # Panics
    /// Panics if the brush is not selected.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn set_texture(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        quad_trees: &mut Trees,
        identifier: Id,
        texture: &str
    ) -> TextureSetResult
    {
        assert!(self.is_selected(identifier), "Brush is not selected.");

        let (sprite, result) = {
            let mut brush = self.brush_mut(drawing_resources, grid, quad_trees, identifier);
            (brush.has_sprite(), brush.set_texture(drawing_resources, texture))
        };

        match &result
        {
            TextureSetResult::Changed(prev) if sprite =>
            {
                self.selected_sprites
                    .replace(self.brushes.get(&identifier).unwrap(), prev);
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
    pub(in crate::map::editor::state) fn create_path(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        quad_trees: &mut Trees,
        identifier: Id,
        path: Path
    )
    {
        assert!(self.is_selected(identifier), "Entity is not selected.");

        self.set_path(drawing_resources, things_catalog, grid, quad_trees, identifier, path);
        edits_history.path_creation(identifier);
    }

    /// Sets the [`Path`] of the entity with [`Id`] `identifier` to `path`.
    #[inline]
    pub(in crate::map::editor::state) fn set_path(
        &mut self,
        resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid,
        quad_trees: &mut Trees,
        identifier: Id,
        path: Path
    )
    {
        self.moving_mut(resources, things_catalog, grid, quad_trees, identifier)
            .set_path(path);

        self.moving.asserted_insert(identifier);
        self.selected_moving.asserted_insert(identifier);
        self.possible_moving.asserted_remove(&identifier);
        self.selected_possible_moving.asserted_remove(&identifier);
    }

    /// Removes the [`Path`] of the selected entity.
    #[inline]
    fn remove_selected_path(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        quad_trees: &mut Trees,
        identifier: Id
    )
    {
        assert!(self.is_selected(identifier), "Entity is not selected.");

        self.overall_node_update = true;

        self.moving.asserted_remove(&identifier);
        self.selected_moving.asserted_remove(&identifier);
        self.possible_moving.asserted_insert(identifier);
        self.selected_possible_moving.asserted_insert(identifier);

        edits_history.path_deletion(
            identifier,
            self.moving_mut(drawing_resources, things_catalog, grid, quad_trees, identifier)
                .take_path()
        );
    }

    /// Replaces the [`Path`] of the selected entity with `path`.
    #[inline]
    fn replace_selected_path(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        quad_trees: &mut Trees,
        identifier: Id,
        path: Path
    )
    {
        self.remove_selected_path(
            drawing_resources,
            things_catalog,
            edits_history,
            grid,
            quad_trees,
            identifier
        );
        self.create_path(
            drawing_resources,
            things_catalog,
            edits_history,
            grid,
            quad_trees,
            identifier,
            path
        );
    }

    /// Replaces in the quad trees the [`Hull`] of the attachments of the brush with [`Id`]
    /// `identifier`.
    /// # Panics
    /// Panics if the attachments [`Hull`] was not already inserted.
    #[inline]
    fn replace_anchors_hull(&mut self, quad_trees: &mut Trees, owner: Id)
    {
        assert!(
            self.remove_anchors_hull(quad_trees, owner),
            "The hull of the anchor was not inserted."
        );
        _ = self.insert_anchors_hull(quad_trees, owner);
    }

    /// Inserts in the quad trees the [`Hull`] of the attachments of the brush with [`Id`], and
    /// returns whether the procedure was successful. `identifier`.
    #[inline]
    #[must_use]
    fn insert_anchors_hull(&mut self, quad_trees: &mut Trees, owner: Id) -> bool
    {
        let hull =
            return_if_none!(self.brush(owner).attachments_anchors_hull(self.brushes()), false);
        self.brushes_with_attachments.asserted_insert((owner, hull));
        assert!(
            quad_trees.insert_anchor_hull(self.brush(owner), &hull).inserted(),
            "Brush anchor hull was already in the quad tree."
        );
        true
    }

    /// Removes from the quad trees the [`Hull`] of the attachments of the brush with [`Id`]
    /// `identifier`, and returns whether the procedure was successful.
    #[inline]
    #[must_use]
    fn remove_anchors_hull(&mut self, quad_trees: &mut Trees, owner: Id) -> bool
    {
        _ = return_if_none!(self.brushes_with_attachments.remove(&owner), false);
        assert!(
            quad_trees.remove_anchor_hull(self.brush(owner)),
            "Brush attachments hull was not in the quad tree."
        );
        true
    }

    //==============================================================
    // Things

    /// Whether `identifier` belongs to a [`ThingInstance`].
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn is_thing(&self, identifier: Id) -> bool
    {
        self.things.contains_key(&identifier)
    }

    /// Returns the amount of selected [`ThingInstance`]s.
    #[inline]
    pub(in crate::map::editor::state) fn selected_things_amount(&self) -> usize
    {
        self.selected_things.len()
    }

    /// Returns a reference to the [`ThingInstance`] with [`Id`] `identifier`.
    #[inline]
    pub(in crate::map::editor::state) fn thing(&self, identifier: Id) -> &ThingInstance
    {
        self.things
            .get(&identifier)
            .unwrap_or_else(|| panic!("Failed thing() call for id {identifier:?}"))
    }

    /// Returns a [`ThingMut`] wrapper to the [`ThingInstance`] with [`Id`] `identifier`.
    #[inline]
    pub(in crate::map::editor::state) fn thing_mut<'a>(
        &'a mut self,
        things_catalog: &'a ThingsCatalog,
        quad_trees: &'a mut Trees,
        identifier: Id
    ) -> ThingMut<'a>
    {
        ThingMut::new(things_catalog, self, quad_trees, identifier)
    }

    /// Inserts a `thing` in the map.
    #[inline]
    pub(in crate::map::editor::state) fn insert_thing(
        &mut self,
        things_catalog: &ThingsCatalog,
        thing: ThingInstance,
        quad_trees: &mut Trees,
        selected: bool
    )
    {
        self.overall_things_info_update = true;
        self.overall_things_properties_update = PropertyUpdate::Total;

        let id = thing.id();

        assert!(
            quad_trees.insert_thing_hull(things_catalog, &thing).inserted(),
            "Thing hull was already in the quad tree."
        );

        if thing.has_path()
        {
            self.moving.asserted_insert(id);
            assert!(
                quad_trees.insert_path_hull(&thing).inserted(),
                "Thing path hull was already in the quad tree."
            );
        }
        else
        {
            self.possible_moving.asserted_insert(id);
        }

        if selected
        {
            if thing.has_path()
            {
                self.selected_moving.asserted_insert(id);
            }
            else
            {
                self.selected_possible_moving.asserted_insert(id);
            }

            self.selected_things.asserted_insert(id);
        }

        self.things.asserted_insert((id, thing));
    }

    /// Removes a [`ThingInstance`] from the map and returns it.
    #[inline]
    pub(in crate::map::editor::state) fn remove_thing(
        &mut self,
        quad_trees: &mut Trees,
        identifier: Id
    ) -> ThingInstance
    {
        self.overall_things_info_update = true;
        self.overall_things_properties_update = PropertyUpdate::Total;

        self.error_highlight.check_entity_error_removal(identifier);

        assert!(
            quad_trees.remove_thing_hull(self.things.get(&identifier).unwrap()),
            "Thing hull was not in the quad tree."
        );
        let thing = self.things.asserted_remove(&identifier);
        self.selected_things.asserted_remove(&identifier);

        if thing.has_path()
        {
            self.moving.asserted_remove(&identifier);
            self.selected_moving.asserted_remove(&identifier);
            assert!(
                quad_trees.remove_path_hull(&thing),
                "Thing path hull was not in the quad tree."
            );
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
    pub(in crate::map::editor::state) fn spawn_thing(
        &mut self,
        things_catalog: &ThingsCatalog,
        thing: ThingInstance,
        quad_trees: &mut Trees,
        edits_history: &mut EditsHistory
    )
    {
        edits_history.thing_spawn(thing.id(), thing.data().clone());
        self.insert_thing(things_catalog, thing, quad_trees, true);
    }

    /// Draws `thing` into the map.
    #[inline]
    pub(in crate::map::editor::state) fn draw_thing(
        &mut self,
        things_catalog: &ThingsCatalog,
        thing: ThingInstance,
        quad_trees: &mut Trees,
        edits_history: &mut EditsHistory
    )
    {
        edits_history.thing_draw(thing.id(), thing.data().clone());
        self.insert_thing(things_catalog, thing, quad_trees, true);
    }

    /// Despawns the [`ThingInstance`] with [`Id`] `identifier`.
    #[inline]
    pub(in crate::map::editor::state) fn despawn_thing(
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
    pub(in crate::map::editor::state) fn moving(&self, identifier: Id) -> &dyn Moving
    {
        if self.is_thing(identifier)
        {
            return self.thing(identifier);
        }

        self.brush(identifier)
    }

    /// Returns a [`MovingMut`] wrapping the entity with id `identifier`.
    #[inline]
    pub(in crate::map::editor::state) fn moving_mut<'a>(
        &'a mut self,
        resources: &'a DrawingResources,
        things_catalog: &'a ThingsCatalog,
        grid: &'a Grid,
        quad_trees: &'a mut Trees,
        identifier: Id
    ) -> MovingMut<'a>
    {
        MovingMut::new(resources, things_catalog, self, grid, quad_trees, identifier)
    }
}

//=======================================================================//

/// The manager of all entities placed on the map.
pub(in crate::map::editor) struct EntitiesManager
{
    /// The core of the manager.
    innards:         Innards,
    /// The [`QuadTree`]s used for spacial partitioning.
    quad_trees:      Trees,
    /// The auxiliary container used to avoid using unsafe code in certain procedures.
    auxiliary:       AuxiliaryIds,
    /// Vector to help in the despawn of the selected brushes.
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
    pub(in crate::map::editor::state) fn from_file<I: Iterator<Item = FileStructure>>(
        header: &MapHeader,
        file: &mut BufReader<File>,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid,
        default_properties: &AllDefaultProperties,
        steps: &mut I
    ) -> Result<(Self, Option<DefaultBrushProperties>, Option<DefaultThingProperties>), &'static str>
    {
        let mut manager = Self::new();

        match manager.innards.load(
            header,
            file,
            drawing_resources,
            things_catalog,
            grid,
            default_properties,
            steps,
            &mut manager.quad_trees
        )
        {
            Ok(value) => Ok((manager, value.0, value.1)),
            Err(err) => Err(err)
        }
    }

    //==============================================================
    // General

    /// Whether the entities properties have been refactored on file load.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) const fn loaded_file_modified(&self) -> bool
    {
        self.innards.loaded_file_modified
    }

    /// Turns off the refactored properties flag.
    #[inline]
    pub(in crate::map::editor::state) fn reset_loaded_file_modified(&mut self)
    {
        self.innards.loaded_file_modified = false;
    }

    /// Whether an entity with [`Id`] `identifier` exists.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn entity_exists(&self, identifier: Id) -> bool
    {
        self.innards.brushes.get(&identifier).is_some() ||
            self.innards.things.get(&identifier).is_some()
    }

    /// Returns the amount of entities placed on the map.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn entities_amount(&self) -> usize
    {
        self.brushes_amount() + self.things_amount()
    }

    /// Returns a reference to the [`Entity`] trait object with [`Id`] `identifier`.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn entity(&self, identifier: Id) -> Entity
    {
        self.innards.entity(identifier)
    }

    /// Schedule a tool outline update.
    #[inline]
    pub(in crate::map::editor::state) fn schedule_outline_update(&mut self)
    {
        self.innards.outline_update = true;
    }

    /// Updates certain tool and UI properties.
    #[inline]
    pub(in crate::map::editor::state) fn update_tool_and_overall_values(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        core: &mut Core,
        ui: &mut Ui,
        grid: &Grid,
        settings: &mut ToolsSettings
    )
    {
        if self.innards.outline_update.take_value()
        {
            core.update_outline(drawing_resources, things_catalog, self, grid, settings);
        }

        if !self.innards.selected_vertexes_update.is_empty()
        {
            core.update_selected_vertexes(self, self.innards.selected_vertexes_update.iter());
            self.innards.selected_vertexes_update.clear();
        }

        if self.innards.overall_texture_update.take_value()
        {
            ui.update_overall_texture(drawing_resources, self);
        }

        if self.innards.overall_node_update.take_value()
        {
            core.update_overall_node(self);
        }

        match self.innards.overall_brushes_properties_update.take_value()
        {
            PropertyUpdate::None => (),
            PropertyUpdate::Total => ui.update_overall_total_brush_properties(self),
            PropertyUpdate::Single(key) => ui.update_overall_brushes_property(self, &key)
        };

        if self.innards.overall_things_info_update.take_value()
        {
            ui.update_overall_things_info(self);
        }

        match self.innards.overall_things_properties_update.take_value()
        {
            PropertyUpdate::None => (),
            PropertyUpdate::Total => ui.update_overall_total_things_properties(self),
            PropertyUpdate::Single(key) => ui.update_overall_things_property(self, &key)
        };
    }

    /// Executes `f` and stores the error returned if any.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn test_operation_validity<F>(&mut self, f: F) -> bool
    where
        F: FnOnce(&mut Self) -> Option<Id>
    {
        let error = return_if_none!(f(self), true);
        self.innards.error_highlight.set_error(error);
        false
    }

    /// Schedules the update of the overall brushs property with key `k` value.
    #[inline]
    pub(in crate::map::editor::state) fn schedule_overall_brushes_property_update(
        &mut self,
        k: &str
    )
    {
        self.innards.overall_brushes_properties_update = PropertyUpdate::Single(k.to_string());
    }

    /// Schedules the update of the overall [`ThingInstance`]s property with key `k` value.
    #[inline]
    pub(in crate::map::editor::state) fn schedule_overall_things_property_update(&mut self, k: &str)
    {
        self.innards.overall_things_properties_update = PropertyUpdate::Single(k.to_string());
    }

    /// Schedules the update of the overall [`Path`]s node values.
    #[inline]
    pub(in crate::map::editor::state) fn schedule_overall_node_update(&mut self)
    {
        self.innards.overall_node_update = true;
    }

    //==============================================================
    // Selection

    /// Whether there are any currently selected entities.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn any_selected_entities(&self) -> bool
    {
        self.any_selected_brushes() || self.any_selected_things()
    }

    /// Returns an iterator to the [`Entity`] trait objects in the map.
    #[inline]
    pub(in crate::map::editor::state) fn selected_entities(&self) -> impl Iterator<Item = Entity>
    {
        self.selected_brushes_ids()
            .chain(self.selected_things_ids())
            .map(|id| self.entity(*id))
    }

    /// Whether the entity with [`Id`] `identifier` is selected.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn is_selected(&self, identifier: Id) -> bool
    {
        self.innards.is_selected(identifier)
    }

    /// Selects the entity with [`Id`] `identifier`.
    #[inline]
    pub(in crate::map::editor::state) fn select_entity(
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

        self.select_attached_brushes(identifier, edits_history);
    }

    #[inline]
    pub(in crate::map::editor::state) fn select_attached_brushes(
        &mut self,
        identifier: Id,
        edits_history: &mut EditsHistory
    )
    {
        assert!(self.is_selected(identifier), "Brush is not selected.");
        self.innards
            .select_attached_brushes(edits_history, &mut self.auxiliary, Some(identifier));
    }

    /// Deselects the entity with [`Id`] `identifier`.
    #[inline]
    pub(in crate::map::editor::state) fn deselect_entity(
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

        self.deselect_attached_brushes(edits_history, Some(identifier));
    }

    /// Updates the value related to entity selection for the entity identifier. Returns true if
    /// entity is a [`ThingInstance`].
    #[inline]
    pub(in crate::map::editor::state) fn insert_entity_selection(&mut self, identifier: Id)
        -> bool
    {
        self.innards.insert_entity_selection(identifier)
    }

    /// Updates the value related to entity deselection for the entity identifier. Returns true if
    /// entity is a [`ThingInstance`].
    #[inline]
    pub(in crate::map::editor::state) fn remove_entity_selection(&mut self, identifier: Id)
        -> bool
    {
        self.innards.remove_entity_selection(identifier)
    }

    /// Deselects all selected entities.
    #[inline]
    pub(in crate::map::editor::state) fn deselect_selected_entities(
        &mut self,
        edits_history: &mut EditsHistory
    )
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
    pub(in crate::map::editor::state) fn select_all_entities(
        &mut self,
        edits_history: &mut EditsHistory
    )
    {
        self.innards.select_all_entities(edits_history, &mut self.auxiliary);
    }

    /// Despawns the selected entities.
    #[inline]
    pub(in crate::map::editor::state) fn despawn_selected_entities(
        &mut self,
        drawing_resources: &DrawingResources,
        edits_history: &mut EditsHistory,
        grid: &Grid
    )
    {
        self.despawn_selected_brushes(drawing_resources, edits_history, grid);

        self.auxiliary.replace_values(self.innards.selected_things.iter());

        for id in &self.auxiliary
        {
            self.innards.despawn_thing(&mut self.quad_trees, edits_history, *id);
        }
    }

    //==============================================================
    // Brushes

    /// Returns the amount of brushes.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn brushes_amount(&self) -> usize
    {
        self.innards.brushes.len()
    }

    /// Returns the amount of selected brushes.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn selected_brushes_amount(&self) -> usize
    {
        self.innards.selected_brushes_amount()
    }

    /// Whether there are any currently selected brushes.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn any_selected_brushes(&self) -> bool
    {
        self.selected_brushes_amount() != 0
    }

    /// Returns a reference to the brush with [`Id`] identifier.
    #[inline]
    pub(in crate::map::editor::state) fn brush(&self, identifier: Id) -> &Brush
    {
        self.innards.brush(identifier)
    }

    /// Returns an iterator to the [`Id`]s of the selected brushes.
    #[inline]
    pub(in crate::map::editor::state) fn selected_brushes_ids(
        &self
    ) -> impl ExactSizeIterator<Item = &Id> + Clone
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
    pub(in crate::map::editor::state) const fn brushes(&self) -> Brushes { self.innards.brushes() }

    /// Returns a [`BrushMut`] wrapping the brush with [`Id`] `identifier`.
    #[inline]
    pub(in crate::map::editor::state) fn brush_mut<'a>(
        &'a mut self,
        drawing_resources: &'a DrawingResources,
        grid: &'a Grid,
        identifier: Id
    ) -> BrushMut<'a>
    {
        BrushMut::new(drawing_resources, &mut self.innards, grid, &mut self.quad_trees, identifier)
    }

    /// Returns an iterator to the non selected brushes.
    #[inline]
    pub(in crate::map::editor::state) fn non_selected_brushes(
        &mut self
    ) -> impl Iterator<Item = &Brush>
    {
        self.innards
            .brushes
            .values()
            .filter(|brush| !self.is_selected(brush.id()))
    }

    /// Returns an iterator to the selected brushes.
    #[inline]
    pub(in crate::map::editor::state) fn selected_brushes(&self) -> impl Iterator<Item = &Brush>
    {
        self.selected_brushes_ids().map(|id| self.brush(*id))
    }

    /// Returns an iterator to [`BrushMut`] wrapping the selected brushes.
    #[inline]
    pub(in crate::map::editor::state) fn selected_brushes_mut<'a>(
        &'a mut self,
        drawing_resources: &'a DrawingResources,
        grid: &'a Grid
    ) -> impl Iterator<Item = BrushMut<'a>>
    {
        self.auxiliary.replace_values(&self.innards.selected_brushes);
        SelectedBrushesMut::new(
            drawing_resources,
            &mut self.innards,
            grid,
            &mut self.quad_trees,
            &self.auxiliary
        )
    }

    /// Returns a [`BrushesIter`] that returns the brushes near `cursor_pos`.
    /// If `camera_scale` contains a value it wraps brushes within the cursor highlight.
    #[inline]
    pub(in crate::map::editor::state) fn brushes_at_pos(
        &self,
        cursor_pos: Vec2,
        camera_scale: Option<f32>
    ) -> BrushesIter<'_>
    {
        self.brushes_iter(self.quad_trees.brushes_at_pos(cursor_pos, camera_scale))
    }

    /// Returns an iterator to the visible brushes.
    #[inline]
    pub(in crate::map::editor::state) fn visible_brushes(
        &self,
        window: &Window,
        camera: &Transform,
        grid: &Grid
    ) -> BrushesIter<'_>
    {
        self.brushes_iter(self.quad_trees.visible_brushes(camera, window, grid))
    }

    /// Returns a [`SelectedBrushesIter`] that returns the selected brushes near `cursor_pos`.
    /// If `camera_scale` contains a value it wraps the selected brushes within the cursor
    /// highlight.
    #[inline]
    pub(in crate::map::editor::state) fn selected_brushes_at_pos(
        &self,
        cursor_pos: Vec2,
        camera_scale: impl Into<Option<f32>>
    ) -> SelectedBrushesIter<'_>
    {
        self.selected_brushes_iter(self.quad_trees.brushes_at_pos(cursor_pos, camera_scale.into()))
    }

    /// Returns an iterator to [`BrushMut`]s wrapping the selected brushes near `cursor_pos`.
    /// If `camera_scale` contains a value it wraps the selected brushes within the cursor
    /// highlight.
    #[inline]
    pub(in crate::map::editor::state) fn selected_brushes_mut_at_pos<'a>(
        &'a mut self,
        drawing_resources: &'a DrawingResources,
        grid: &'a Grid,
        cursor_pos: Vec2,
        camera_scale: impl Into<Option<f32>>
    ) -> impl Iterator<Item = BrushMut<'a>>
    {
        self.auxiliary.replace_values(
            self.quad_trees
                .brushes_at_pos(cursor_pos, camera_scale.into())
                .ids()
                .filter(|id| self.innards.selected_brushes.contains(*id))
        );

        SelectedBrushesMut::new(
            drawing_resources,
            &mut self.innards,
            grid,
            &mut self.quad_trees,
            &self.auxiliary
        )
    }

    /// Returns an [`IdsInRange`] returning the [`Id`]s of brushes intersecting `range`.
    #[inline]
    pub(in crate::map::editor::state) fn brushes_in_range(&self, range: &Hull) -> IdsInRange<'_>
    {
        IdsInRange::new(self.quad_trees.brushes_in_range(range))
    }

    /// Selects all entities that are fully contained in `range`.
    #[inline]
    pub(in crate::map::editor::state) fn select_entities_in_range(
        &mut self,
        range: &Hull,
        edits_history: &mut EditsHistory,
        inputs: &InputsPresses,
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
                        self.innards.select_attached_brushes(
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
    pub(in crate::map::editor::state) fn exclusively_select_entities_in_range(
        &mut self,
        range: &Hull,
        edits_history: &mut EditsHistory,
        inputs: &InputsPresses,
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
            .select_attached_brushes_of_selected_brushes(edits_history, &mut self.auxiliary);
    }

    /// Stores the [`Id`]s of the brushes attached to the ones with [`Id`]s returned by
    /// `identifiers`.
    #[inline]
    fn store_attached_ids(&mut self, identifiers: impl IntoIterator<Item = Id>)
    {
        self.auxiliary
            .store_attached_ids(identifiers.into_iter().map(|id| self.innards.brush(id)));
    }

    /// Deselects the [`Id`]s of the brushes attached to the ones with [`Id`]s returned by
    /// `identifiers`.
    #[inline]
    fn deselect_attached_brushes(
        &mut self,
        edits_history: &mut EditsHistory,
        identifiers: impl IntoIterator<Item = Id>
    )
    {
        self.store_attached_ids(identifiers);
        self.auxiliary.retain(|id| self.innards.is_selected(*id));
        self.innards.deselect_cluster(edits_history, self.auxiliary.iter());
    }

    /// Returns the center of the rectangle encompassing the brushes with [`Id`]s returned by
    /// `ids`, if any.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn brushes_center(
        &self,
        ids: impl ExactSizeIterator<Item = Id>
    ) -> Option<Vec2>
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

    /// Returns the center of the rectangle encompassing the selected brushes, if any.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn selected_brushes_center(&self) -> Option<Vec2>
    {
        self.brushes_center(self.selected_brushes_ids().copied())
    }

    /// Returns the center of the rectangle encompassing the selected textured brushes, if any.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn selected_textured_brushes_center(&self) -> Option<Vec2>
    {
        self.brushes_center(self.selected_textured_ids().copied())
    }

    /// Returns the [`Hull`] describing the rectangle encompassing all selected brushes, if any.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn selected_brushes_polygon_hull(&self) -> Option<Hull>
    {
        Hull::from_hulls_iter(self.selected_brushes_ids().map(|id| self.brush(*id).polygon_hull()))
    }

    /// Returns an iterator to all the selected brushes with sprites.
    #[inline]
    pub(in crate::map::editor::state) fn selected_brushes_with_sprites(
        &mut self
    ) -> Option<impl Iterator<Item = &Brush>>
    {
        /// The iterator to the brushes.
        struct Iter<'a>
        {
            /// All the identifiers.
            iter:     hashbrown::hash_map::Values<'a, String, Ids>,
            /// Identifiers of the brushes with same sprite.
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

        if self.innards.selected_sprites.len() == 0
        {
            return None;
        }

        let mut iter = self.innards.selected_sprites.values();
        let sub_iter = iter.next_value().iter();

        Iter { iter, sub_iter }.map(|id| self.brush(*id)).into()
    }

    /// Returns an iterator to the [`BrushMut`]s wrapping the selected brushes with sprites.
    #[inline]
    pub(in crate::map::editor::state) fn selected_brushes_with_sprite_mut<'a>(
        &'a mut self,
        drawing_resources: &'a DrawingResources,
        grid: &'a Grid
    ) -> impl Iterator<Item = BrushMut<'a>>
    {
        self.auxiliary.clear();

        for set in self.innards.selected_sprites.0.values()
        {
            self.auxiliary.0.extend(set);
        }

        SelectedBrushesMut::new(
            drawing_resources,
            &mut self.innards,
            grid,
            &mut self.quad_trees,
            &self.auxiliary
        )
    }

    /// Returns an iterator to the [`BrushMut`]s wrapping the selected brushes with sprite
    /// `texture`.
    #[inline]
    pub(in crate::map::editor::state) fn selected_brushes_with_sprites_mut<'a>(
        &'a mut self,
        drawing_resources: &'a DrawingResources,
        grid: &'a Grid,
        texture: &str
    ) -> Option<impl Iterator<Item = BrushMut<'a>>>
    {
        self.auxiliary
            .replace_values(self.innards.selected_sprites.get(texture)?);
        SelectedBrushesMut::new(
            drawing_resources,
            &mut self.innards,
            grid,
            &mut self.quad_trees,
            &self.auxiliary
        )
        .into()
    }

    /// Spawns a brush generated from `polygon` and returns its [`Id`].
    #[inline]
    pub(in crate::map::editor::state) fn spawn_brush<'d>(
        &mut self,
        drawing_resources: &DrawingResources,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        polygon: impl Into<Cow<'d, ConvexPolygon>>,
        properties: BrushProperties
    ) -> Id
    {
        self.innards.spawn_brush(
            drawing_resources,
            edits_history,
            grid,
            &mut self.quad_trees,
            polygon,
            properties
        )
    }

    /// Spawns a brush generated from the arguments.
    #[inline]
    pub(in crate::map::editor::state) fn spawn_brush_from_parts(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        identifier: Id,
        data: BrushData,
        selected: bool
    )
    {
        let brush = Brush::from_parts(data, identifier);
        self.innards
            .insert_brush(drawing_resources, grid, &mut self.quad_trees, brush, selected);
    }

    /// Spawns the entities created from `data`.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn spawn_pasted_entity(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        data: ClipboardData,
        delta: Vec2
    ) -> Id
    {
        self.innards.spawn_pasted_entity(
            drawing_resources,
            things_catalog,
            edits_history,
            grid,
            &mut self.quad_trees,
            data,
            delta
        )
    }

    /// Spawns the brushes created from `polygons`.
    #[inline]
    pub(in crate::map::editor::state) fn spawn_brushes(
        &mut self,
        drawing_resources: &DrawingResources,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        mut polygons: impl ExactSizeIterator<Item = ConvexPolygon>,
        properties: BrushProperties
    )
    {
        for _ in 0..polygons.len() - 1
        {
            self.spawn_brush(
                drawing_resources,
                edits_history,
                grid,
                polygons.next_value(),
                properties.clone()
            );
        }

        self.spawn_brush(drawing_resources, edits_history, grid, polygons.next_value(), properties);
    }

    #[inline]
    pub(in crate::map::editor::state) fn replace_brush_with_partition<F>(
        &mut self,
        drawing_resources: &DrawingResources,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        others: impl ExactSizeIterator<Item = ConvexPolygon>,
        identifier: Id,
        f: F
    ) -> impl Iterator<Item = Id>
    where
        F: FnOnce(&mut Brush) -> ConvexPolygon
    {
        #[must_use]
        enum PathStatus
        {
            None,
            OwnsOrPath,
            Anchored(Id)
        }

        #[inline]
        fn path_status(brush: &Brush) -> PathStatus
        {
            if brush.has_path() || brush.has_attachments()
            {
                PathStatus::OwnsOrPath
            }
            else if let Some(id) = brush.attached()
            {
                PathStatus::Anchored(id)
            }
            else
            {
                PathStatus::None
            }
        }

        #[inline]
        fn spawn_brushes_with_ids(
            manager: &mut EntitiesManager,
            drawing_resources: &DrawingResources,
            edits_history: &mut EditsHistory,
            grid: &Grid,
            mut polygons: impl ExactSizeIterator<Item = ConvexPolygon>,
            properties: BrushProperties
        ) -> HvHashSet<Id>
        {
            let mut ids = hv_hash_set![];

            if polygons.len() == 0
            {
                return ids;
            }

            for _ in 0..polygons.len() - 1
            {
                ids.asserted_insert(manager.spawn_brush(
                    drawing_resources,
                    edits_history,
                    grid,
                    polygons.next_value(),
                    properties.clone()
                ));
            }

            ids.asserted_insert(manager.spawn_brush(
                drawing_resources,
                edits_history,
                grid,
                polygons.next_value(),
                properties
            ));

            ids
        }

        let (properties, path_status) = {
            let mut brush = self.brush_mut(drawing_resources, grid, identifier);
            edits_history.polygon_edit(identifier, f(&mut brush));
            (brush.properties(), path_status(&brush))
        };

        match path_status
        {
            PathStatus::None =>
            {
                spawn_brushes_with_ids(
                    self,
                    drawing_resources,
                    edits_history,
                    grid,
                    others,
                    properties
                )
            },
            PathStatus::OwnsOrPath =>
            {
                let ids = spawn_brushes_with_ids(
                    self,
                    drawing_resources,
                    edits_history,
                    grid,
                    others,
                    properties
                );

                for id in &ids
                {
                    self.attach(identifier, *id);
                }

                ids
            },
            PathStatus::Anchored(owner) =>
            {
                let ids = spawn_brushes_with_ids(
                    self,
                    drawing_resources,
                    edits_history,
                    grid,
                    others,
                    properties
                );

                for id in &ids
                {
                    self.attach(owner, *id);
                }

                ids
            }
        }
        .into_iter()
    }

    /// Spawns a brush created with a draw tool.
    #[inline]
    pub(in crate::map::editor::state) fn spawn_drawn_brush(
        &mut self,
        drawing_resources: &DrawingResources,
        default_properties: &DefaultBrushProperties,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        polygon: ConvexPolygon,
        drawn_brushes: &mut Ids
    )
    {
        let id = self.innards.id_generator.new_id();

        let brush = Brush::from_polygon(polygon, id, default_properties.instance());

        edits_history.brush_draw(id);
        drawn_brushes.asserted_insert(id);

        self.innards
            .insert_brush(drawing_resources, grid, &mut self.quad_trees, brush, true);
    }

    /// Removes the brush with [`Id`] `identifier` from the internal data structures.
    #[inline]
    fn remove_brush(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        identifier: Id
    ) -> Brush
    {
        self.innards
            .remove_brush(drawing_resources, grid, &mut self.quad_trees, identifier)
            .0
    }

    /// Despawns the brushes created with a draw tool whose [`Id`]s are contained in
    /// `drawn_brushes`.
    #[inline]
    pub(in crate::map::editor::state) fn despawn_drawn_brushes(
        &mut self,
        drawing_resources: &DrawingResources,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        drawn_brushes: &mut Ids
    )
    {
        for id in &*drawn_brushes
        {
            edits_history.drawn_brush_despawn(self.remove_brush(drawing_resources, grid, *id));
        }

        drawn_brushes.clear();
    }

    /// Despawns the brush with [`Id`] `identifier`.
    #[inline]
    pub(in crate::map::editor::state) fn despawn_brush(
        &mut self,
        drawing_resources: &DrawingResources,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        identifier: Id
    )
    {
        self.innards.despawn_brush(
            drawing_resources,
            edits_history,
            grid,
            &mut self.quad_trees,
            identifier
        );
    }

    /// Despawns the brush with [`Id`] `identifier` and returns its parts.
    #[inline]
    pub(in crate::map::editor::state) fn despawn_brush_into_parts(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        identifier: Id
    ) -> BrushData
    {
        self.remove_brush(drawing_resources, grid, identifier).into_parts().0
    }

    /// Despawns the selected brushes.
    #[inline]
    pub(in crate::map::editor::state) fn despawn_selected_brushes(
        &mut self,
        drawing_resources: &DrawingResources,
        edits_history: &mut EditsHistory,
        grid: &Grid
    )
    {
        self.brushes_despawn
            .replace_values(self.innards.selected_brushes_ids().copied());

        self.brushes_despawn.sort_by(|a, b| {
            self.innards
                .brush(*a)
                .has_attachments()
                .cmp(&self.innards.brush(*b).has_attachments())
                .reverse()
        });

        for id in &self.brushes_despawn
        {
            self.innards.despawn_selected_brush(
                drawing_resources,
                edits_history,
                grid,
                &mut self.quad_trees,
                *id
            );
        }
    }

    /// Replaces the selected brushes with the ones generated by `polygons`.
    #[inline]
    pub(in crate::map::editor::state) fn replace_selected_brushes(
        &mut self,
        drawing_resources: &DrawingResources,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        polygons: impl ExactSizeIterator<Item = ConvexPolygon>,
        properties: BrushProperties
    )
    {
        self.despawn_selected_brushes(drawing_resources, edits_history, grid);
        self.spawn_brushes(drawing_resources, edits_history, grid, polygons, properties);
    }

    /// Duplicates the selected entities crating copies displaced by `delta`.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn duplicate_selected_entities(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        clipboard: &mut Clipboard,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        delta: Vec2
    ) -> bool
    {
        let valid = self.test_operation_validity(|manager| {
            manager.selected_entities().find_map(|entity| {
                (entity.hull(drawing_resources, things_catalog, grid) + delta)
                    .out_of_bounds()
                    .then_some(entity.id())
            })
        });

        if !valid
        {
            return false;
        }

        clipboard.duplicate(drawing_resources, things_catalog, self, edits_history, grid, delta);
        true
    }

    /// Makes the Brush with [`Id`] `identifier` moving.
    #[inline]
    pub(in crate::map::editor::state) fn create_path(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        identifier: Id,
        path: Path
    )
    {
        self.innards.create_path(
            drawing_resources,
            things_catalog,
            edits_history,
            grid,
            &mut self.quad_trees,
            identifier,
            path
        );
    }

    /// Gives the brush with [`Id`] `identifier` a [`Path`].
    #[inline]
    pub(in crate::map::editor::state) fn set_path(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid,
        identifier: Id,
        path: Path
    )
    {
        self.innards.set_path(
            drawing_resources,
            things_catalog,
            grid,
            &mut self.quad_trees,
            identifier,
            path
        );
    }

    /// Removes the [`Path`] from the brush with [`Id`] `identifier`.
    #[inline]
    pub(in crate::map::editor::state) fn remove_path(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid,
        identifier: Id
    ) -> Path
    {
        self.innards.selected_moving.remove(&identifier);
        self.innards.moving.asserted_remove(&identifier);

        self.innards.possible_moving.asserted_insert(identifier);

        if self.is_selected(identifier)
        {
            self.innards.overall_node_update = true;
            self.innards.selected_possible_moving.asserted_insert(identifier);
        }

        self.moving_mut(drawing_resources, things_catalog, grid, identifier)
            .take_path()
    }

    /// Removes the [`Path`] from the selected brush with [`Id`] `identifier`.
    #[inline]
    pub(in crate::map::editor::state) fn remove_selected_path(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        identifier: Id
    )
    {
        self.innards.remove_selected_path(
            drawing_resources,
            things_catalog,
            edits_history,
            grid,
            &mut self.quad_trees,
            identifier
        );
    }

    /// Replaces the [`Path`] of the selected brush with [`Id`] `identifier` with the one
    /// generated from `path`.
    #[inline]
    pub(in crate::map::editor::state) fn replace_selected_path(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        identifier: Id,
        path: Path
    )
    {
        self.innards.replace_selected_path(
            drawing_resources,
            things_catalog,
            edits_history,
            grid,
            &mut self.quad_trees,
            identifier,
            path
        );
    }

    /// Removes the [`Path`]s from the selected brushes.
    #[inline]
    pub(in crate::map::editor::state) fn remove_selected_paths(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        edits_history: &mut EditsHistory,
        grid: &Grid
    )
    {
        self.auxiliary.replace_values(&self.innards.selected_moving);

        for id in &self.auxiliary
        {
            self.innards.remove_selected_path(
                drawing_resources,
                things_catalog,
                edits_history,
                grid,
                &mut self.quad_trees,
                *id
            );
        }
    }

    /// Returns a [`BrushesIter`] returning the visible attachments.
    #[inline]
    pub(in crate::map::editor::state) fn visible_anchors(
        &self,
        window: &Window,
        camera: &Transform,
        grid: &Grid
    ) -> BrushesIter<'_>
    {
        self.brushes_iter(self.quad_trees.visible_anchors(camera, window, grid))
    }

    /// Returns the amount of selected textured brushes.
    #[inline]
    pub(in crate::map::editor::state) fn selected_textured_amount(&self) -> usize
    {
        self.innards.selected_textured.len()
    }

    /// Returns the amount of textured brushes.
    #[inline]
    pub(in crate::map::editor::state) fn textured_amount(&self) -> usize
    {
        self.innards.textured.len()
    }

    /// Returns the amount of selected brushes with sprites.
    #[inline]
    pub(in crate::map::editor::state) const fn selected_sprites_amount(&self) -> usize
    {
        self.innards.selected_sprites.len()
    }

    /// Returns an iterator to the [`Id`]s of the selected textured brushes.
    #[inline]
    pub(in crate::map::editor::state) fn selected_textured_ids(
        &self
    ) -> impl ExactSizeIterator<Item = &Id>
    {
        self.innards.selected_textured.iter()
    }

    /// Returns an iterator to the selected textured brushes.
    #[inline]
    pub(in crate::map::editor::state) fn selected_textured_brushes(
        &self
    ) -> impl Iterator<Item = &Brush>
    {
        self.selected_textured_ids().map(|id| self.brush(*id))
    }

    /// Returns an iterator to the [`BrushMut`] wrapping the selected textured brushes.
    #[inline]
    pub(in crate::map::editor::state) fn selected_textured_brushes_mut<'a>(
        &'a mut self,
        drawing_resources: &'a DrawingResources,
        grid: &'a Grid
    ) -> impl Iterator<Item = BrushMut<'a>>
    {
        self.auxiliary.replace_values(&self.innards.selected_textured);
        SelectedBrushesMut::new(
            drawing_resources,
            &mut self.innards,
            grid,
            &mut self.quad_trees,
            &self.auxiliary
        )
    }

    /// Returns a [`BrushesIter`] returning the brushes with sprites at the position
    /// `cursor_pos`.
    #[inline]
    pub(in crate::map::editor::state) fn sprites_at_pos(&self, cursor_pos: Vec2)
        -> BrushesIter<'_>
    {
        self.brushes_iter(self.quad_trees.sprites_at_pos(cursor_pos))
    }

    /// Returns the visible brushes with sprites.
    #[inline]
    pub(in crate::map::editor::state) fn visible_sprites(
        &self,
        window: &Window,
        camera: &Transform,
        grid: &Grid
    ) -> BrushesIter<'_>
    {
        self.brushes_iter(self.quad_trees.visible_sprites(camera, window, grid))
    }

    #[inline]
    pub(in crate::map::editor::state) fn rebuild_sprite_quad_tree(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid
    )
    {
        self.quad_trees.rebuild_sprite_quad_tree(
            drawing_resources,
            grid,
            self.innards.textured.iter().filter_map(|id| {
                let brush = self.innards.brush(*id);
                brush.has_sprite().then_some(brush)
            })
        );
    }

    /// Anchors the brush with [`Id`] `attachment` to the one with [`Id`] `owner`.
    #[inline]
    pub(in crate::map::editor::state) fn attach(&mut self, owner: Id, attachment: Id)
    {
        self.innards.attach(&mut self.quad_trees, owner, attachment);
    }

    /// Detaches the brush with [`Id`] `attachment` from the one with [`Id`] `owner`.
    #[inline]
    pub(in crate::map::editor::state) fn detach(&mut self, owner: Id, attachment: Id)
    {
        self.innards.detach(&mut self.quad_trees, owner, attachment);
    }

    /// Sets the texture of the brush with [`Id`] identifier.
    /// Returns the name of the replaced texture, if any.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn set_texture(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        identifier: Id,
        texture: &str
    ) -> TextureSetResult
    {
        self.innards
            .set_texture(drawing_resources, grid, &mut self.quad_trees, identifier, texture)
    }

    /// Removes the texture from the brush with [`Id`] identifier, and returns its
    /// [`TextureSettings`].
    #[inline]
    pub(in crate::map::editor::state) fn remove_texture(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        identifier: Id
    ) -> TextureSettings
    {
        self.innards
            .remove_texture(drawing_resources, grid, &mut self.quad_trees, identifier)
    }

    /// Set the [`TextureSettings`] of the brush with [`Id`] `identifier`.
    #[inline]
    pub(in crate::map::editor::state) fn set_texture_settings(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        identifier: Id,
        texture: TextureSettings
    )
    {
        assert!(self.is_selected(identifier), "Brush is not selected.");

        let sprite = texture.sprite();

        self.brush_mut(drawing_resources, grid, identifier)
            .set_texture_settings(texture);
        self.innards.textured.asserted_insert(identifier);
        self.innards.selected_textured.asserted_insert(identifier);

        if sprite
        {
            self.innards.insert_selected_sprite(identifier);
        }
    }

    /// Sets the texture of the selected brushes and returns a [`TextureResult`] describing the
    /// result of the procedure.
    #[inline]
    pub(in crate::map::editor::state) fn set_selected_brushes_texture(
        &mut self,
        drawing_resources: &DrawingResources,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        texture: &str
    ) -> TextureResult
    {
        let valid = self.test_operation_validity(|manager| {
            manager
                .selected_brushes_with_sprite_mut(drawing_resources, grid)
                .find_map(|mut brush| {
                    (!brush.check_texture_change(drawing_resources, grid, texture))
                        .then_some(brush.id)
                })
        });

        if !valid
        {
            return TextureResult::Invalid;
        }

        let mut sprite = false;
        self.auxiliary.replace_values(&self.innards.selected_brushes);
        let mut iter = self.auxiliary.iter();

        for id in &mut iter
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

            match self.innards.set_texture(
                drawing_resources,
                grid,
                &mut self.quad_trees,
                *id,
                texture
            )
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
            match self.innards.set_texture(
                drawing_resources,
                grid,
                &mut self.quad_trees,
                *id,
                texture
            )
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

    /// Removes the textures from the selected brushes.
    #[inline]
    pub(in crate::map::editor::state) fn remove_selected_textures(
        &mut self,
        drawing_resources: &DrawingResources,
        edits_history: &mut EditsHistory,
        grid: &Grid
    )
    {
        self.auxiliary.replace_values(&self.innards.selected_textured);

        edits_history.texture_removal_cluster(self.auxiliary.iter().map(|id| {
            (
                *id,
                self.innards
                    .remove_texture(drawing_resources, grid, &mut self.quad_trees, *id)
            )
        }));

        self.innards.selected_textured.clear();
    }

    /// Sets whether the texture of the selected brushes should be rendered as a sprite or not.
    #[inline]
    pub(in crate::map::editor::state) fn set_sprite(
        &mut self,
        drawing_resources: &DrawingResources,
        edits_history: &mut EditsHistory,
        grid: &Grid,
        value: bool
    )
    {
        if value
        {
            let valid = self.test_operation_validity(|manager| {
                manager
                    .selected_textured_brushes_mut(drawing_resources, grid)
                    .find_map(|mut brush| {
                        (!brush.check_texture_sprite(drawing_resources, grid, value))
                            .then_some(brush.id())
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
                        let mut brush = self.innards.brush_mut(
                            drawing_resources,
                            grid,
                            &mut self.quad_trees,
                            *id
                        );
                        let value = continue_if_none!(brush.set_texture_sprite(value));
                        edits_history.sprite(brush.id(), value);
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

    /// Sets whether the texture of the selected brush with [`Id`] `identifier` should be
    /// rendered as a sprite or not. Returns the previous sprite rendering parameters.
    #[inline]
    pub(in crate::map::editor::state) fn undo_redo_texture_sprite(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        identifier: Id,
        value: &mut TextureSpriteSet
    )
    {
        let enabled = value.enabled();
        self.brush_mut(drawing_resources, grid, identifier)
            .undo_redo_texture_sprite(value);

        if enabled
        {
            self.innards.insert_selected_sprite(identifier);
        }
        else
        {
            self.innards.remove_selected_sprite(identifier);
        }
    }

    /// Completes the texture reload.
    #[inline]
    pub(in crate::map::editor::state) fn finish_textures_reload(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid
    )
    {
        let mut errors = hv_hash_set![];
        self.auxiliary.replace_values(&self.innards.textured);

        for id in &self.auxiliary
        {
            let mut brush =
                self.innards
                    .brush_mut(drawing_resources, grid, &mut self.quad_trees, *id);
            let name = {
                let settings = brush.texture_settings().unwrap();

                if !settings.sprite()
                {
                    continue;
                }

                drawing_resources.texture_or_error(settings.name()).name()
            };

            let valid = brush.check_texture_change(drawing_resources, grid, name);
            drop(brush);

            if !valid
            {
                errors.insert(name);
                _ = self
                    .innards
                    .remove_brush(drawing_resources, grid, &mut self.quad_trees, *id);
            }
        }

        if errors.is_empty()
        {
            return;
        }

        warning_message(&format!(
            "Some brushes did not fit within the boundaries of the map due to updated the size of \
             their sprites and have therefore been removed. Be careful before saving the \
             file.\nHere is the list of the textures of the sprites of the brushes that have been \
             removed:\n{errors:?}"
        ));
        self.innards.loaded_file_modified = true;
    }

    //==============================================================
    // Things

    /// Whether `identifier` belongs to a [`ThingInstance`].
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn is_thing(&self, identifier: Id) -> bool
    {
        self.innards.is_thing(identifier)
    }

    /// Returns a reference to the [`ThingInstance`] with [`Id`] `identifier`.
    #[inline]
    pub(in crate::map::editor::state) fn thing(&self, identifier: Id) -> &ThingInstance
    {
        self.innards.thing(identifier)
    }

    /// Returns a [`ThingMut`] wrapper to the [`ThingInstance`] with [`Id`] `identifier`.
    #[inline]
    pub(in crate::map::editor::state) fn thing_mut<'a>(
        &'a mut self,
        things_catalog: &'a ThingsCatalog,
        identifier: Id
    ) -> ThingMut<'a>
    {
        self.innards
            .thing_mut(things_catalog, &mut self.quad_trees, identifier)
    }

    /// Returns the amount of [`ThingInstance`] in the map.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn things_amount(&self) -> usize { self.innards.things.len() }

    /// Returns an iterator to all [`ThingInstance`]s in the map.
    #[inline]
    pub(in crate::map::editor::state) fn things(&self) -> impl Iterator<Item = &ThingInstance>
    {
        self.innards.things.values()
    }

    /// Returns the amount of [`ThingInstance`]s.
    #[inline]
    pub(in crate::map::editor::state) fn selected_things_amount(&self) -> usize
    {
        self.innards.selected_things_amount()
    }

    /// Whether any [`ThingInstance`] is currently selected.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn any_selected_things(&self) -> bool
    {
        self.selected_things_amount() != 0
    }

    /// Returns the [`Id`]s of the selected [`ThingInstance`]s.
    #[inline]
    pub(in crate::map::editor::state) fn selected_things_ids(&self) -> impl Iterator<Item = &Id>
    {
        self.innards.selected_things.iter()
    }

    /// Returns an iterator to the selected [`ThingInstance`]s.
    #[inline]
    pub(in crate::map::editor::state) fn selected_things(
        &self
    ) -> impl Iterator<Item = &ThingInstance>
    {
        self.selected_things_ids().map(|id| self.thing(*id))
    }

    /// Returns an iterator to the [`ThingMut`]s wrapping the selected [`ThingInstance`]s.
    #[inline]
    pub(in crate::map::editor::state) fn selected_things_mut<'a>(
        &'a mut self,
        things_catalog: &'a ThingsCatalog
    ) -> impl Iterator<Item = ThingMut<'a>>
    {
        self.auxiliary.replace_values(&self.innards.selected_things);
        SelectedThingsMut::new(
            things_catalog,
            &mut self.innards,
            &mut self.quad_trees,
            &self.auxiliary
        )
    }

    /// Spawns a new [`ThingInstance`] with id [`identifier`].
    #[inline]
    pub(in crate::map::editor::state) fn spawn_thing_from_parts(
        &mut self,
        things_catalog: &ThingsCatalog,
        identifier: Id,
        data: ThingInstanceData
    )
    {
        self.innards.insert_thing(
            things_catalog,
            ThingInstance::from_parts(identifier, data),
            &mut self.quad_trees,
            true
        );
    }

    /// Spawns a selected [`ThingInstance`] from the selected [`Thing`]. Returns its [`Id`].
    #[inline]
    pub(in crate::map::editor::state) fn spawn_selected_thing(
        &mut self,
        things_catalog: &ThingsCatalog,
        default_thing_properties: &DefaultThingProperties,
        edits_history: &mut EditsHistory,
        settings: &mut ToolsSettings,
        cursor_pos: Vec2
    ) -> Id
    {
        let id = self.innards.new_id();

        self.innards.draw_thing(
            things_catalog,
            ThingInstance::new(
                id,
                things_catalog.selected_thing().id(),
                settings
                    .thing_pivot
                    .spawn_pos(things_catalog.selected_thing(), cursor_pos),
                default_thing_properties
            ),
            &mut self.quad_trees,
            edits_history
        );

        id
    }

    /// Despawns the drawn [`ThingInstance`]s with [`Id`]s contained in `drawn_things`.
    #[inline]
    pub(in crate::map::editor::state) fn despawn_drawn_things(
        &mut self,
        edits_history: &mut EditsHistory,
        drawn_things: &mut Ids
    )
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
    pub(in crate::map::editor::state) fn things_at_pos(
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
    pub(in crate::map::editor::state) fn selected_things_at_pos(
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
    pub(in crate::map::editor::state) fn visible_things(
        &self,
        window: &Window,
        camera: &Transform,
        grid: &Grid
    ) -> ThingsIter<'_>
    {
        ThingsIter::new(self, self.quad_trees.visible_things(camera, window, grid))
    }

    /// Remove the [`ThingInstance`] with [`Id`] `identifier` from the map.
    #[inline]
    pub(in crate::map::editor::state) fn remove_thing(&mut self, identifier: Id) -> ThingInstance
    {
        self.innards.remove_thing(&mut self.quad_trees, identifier)
    }

    /// Concludes the texture reloading process.
    #[inline]
    pub(in crate::map::editor::state) fn finish_things_reload(
        &mut self,
        things_catalog: &ThingsCatalog
    )
    {
        let mut errors = hv_hash_set![];
        self.auxiliary.replace_values(self.innards.things.keys());

        for id in &self.auxiliary
        {
            let instance = self.innards.thing(*id);
            let valid = instance.check_thing_change(
                things_catalog,
                things_catalog.thing_or_error(instance.thing_id()).id()
            );

            if !valid
            {
                errors.insert(instance.thing_id());
                _ = self.innards.remove_thing(&mut self.quad_trees, *id);
            }
        }

        if errors.is_empty()
        {
            return;
        }

        warning_message(&format!(
            "Some things did not fit within the boundaries of the map due to the updated size of \
             their outline and have therefore been removed. Be careful before saving the \
             file.\nHere is the list of the ThingIds of the things that have been \
             removed:\n{errors:?}"
        ));
        self.innards.loaded_file_modified = true;
    }

    //==============================================================
    // Moving

    /// Whether the brush with [`Id`] `identifier` is moving.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn is_moving(&self, identifier: Id) -> bool
    {
        self.innards.moving.contains(&identifier)
    }

    /// Whether the brush with [`Id`] `identifier` is moving and selected.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn is_selected_moving(&self, identifier: Id) -> bool
    {
        self.innards.selected_moving.contains(&identifier)
    }

    /// Whether there are any entities that don't have a [`Path`] but could have one.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn any_selected_possible_moving(&self) -> bool
    {
        !self.innards.selected_possible_moving.is_empty()
    }

    /// Returns an iterator to the [`Id`]s of the selected moving brushes.
    #[inline]
    pub(in crate::map::editor::state) fn selected_moving_ids(&self) -> impl Iterator<Item = &Id>
    {
        self.innards.selected_moving.iter()
    }

    /// Returns the amount of selected moving brushes.
    #[inline]
    #[must_use]
    pub(in crate::map::editor::state) fn selected_moving_amount(&self) -> usize
    {
        self.innards.selected_moving.len()
    }

    /// Returns an iterator to the moving selected brushes.
    #[inline]
    pub(in crate::map::editor::state) fn selected_moving(&self)
        -> impl Iterator<Item = &dyn Moving>
    {
        self.innards.selected_moving.iter().map(|id| self.moving(*id))
    }

    /// Returns an iterator to the moving selected brushes wrapped in [`BrushMut`]s.
    #[inline]
    pub(in crate::map::editor::state) fn selected_movings_mut<'a>(
        &'a mut self,
        drawing_resources: &'a DrawingResources,
        things_catalog: &'a ThingsCatalog,
        grid: &'a Grid
    ) -> impl Iterator<Item = MovingMut<'a>>
    {
        self.auxiliary.replace_values(&self.innards.selected_moving);

        SelectedMovingsMut::new(
            drawing_resources,
            things_catalog,
            &mut self.innards,
            grid,
            &mut self.quad_trees,
            &self.auxiliary
        )
    }

    /// Returns all the [`MovementSimulator`] of the entities with a [`Path`].
    #[inline]
    pub(in crate::map::editor::state) fn movement_simulators(&self) -> HvVec<MovementSimulator>
    {
        hv_vec![collect; self.innards.moving.iter()
            .map(|id| self.moving(*id).movement_simulator())
        ]
    }

    /// Returns a vector containing the [`MovingSimulator`]s of the moving brushes for the map
    /// preview.
    #[inline]
    pub(in crate::map::editor::state) fn selected_movement_simulators(
        &self
    ) -> HvVec<MovementSimulator>
    {
        hv_vec![collect; self
            .selected_moving_ids()
            .map(|id| self.moving(*id).movement_simulator())
        ]
    }

    /// Returns a reference to the entity with id `identifier` as a trait object which implements
    /// the [`Moving`] trait.
    #[inline]
    pub(in crate::map::editor::state) fn moving(&self, identifier: Id) -> &dyn Moving
    {
        self.innards.moving(identifier)
    }

    /// Returns a [`MovingMut`] wrapping the entity with id `identifier`.
    #[inline]
    pub(in crate::map::editor::state) fn moving_mut<'a>(
        &'a mut self,
        drawing_resources: &'a DrawingResources,
        things_catalog: &'a ThingsCatalog,
        grid: &'a Grid,
        identifier: Id
    ) -> MovingMut<'a>
    {
        self.innards.moving_mut(
            drawing_resources,
            things_catalog,
            grid,
            &mut self.quad_trees,
            identifier
        )
    }

    /// Returns a [`SelectedMovingsIter`] returning an iterator to the selected entities with
    /// [`Path`]s.
    #[inline]
    pub(in crate::map::editor::state) fn selected_movings_at_pos(
        &self,
        cursor_pos: Vec2,
        camera_scale: f32
    ) -> SelectedMovingsIter<'_>
    {
        SelectedMovingsIter::new(self, self.quad_trees.paths_at_pos(cursor_pos, camera_scale))
    }

    /// Returns a [`MovingsIter`] returning an iterator to the entities with visible [`Path`]s.
    #[inline]
    pub(in crate::map::editor::state) fn visible_paths(
        &self,
        window: &Window,
        camera: &Transform,
        grid: &Grid
    ) -> MovingsIter<'_>
    {
        MovingsIter::new(self, self.quad_trees.visible_paths(camera, window, grid))
    }

    //==============================================================
    // Draw

    /// Returns the [`Animators`] for the map preview.
    #[inline]
    pub(in crate::map::editor::state) fn texture_animators(
        &self,
        bundle: &StateUpdateBundle
    ) -> Animators
    {
        Animators::new(bundle, self)
    }

    /// Draws the UI error highlight.
    #[inline]
    pub(in crate::map::editor::state) fn draw_error_highlight(
        &mut self,
        things_catalog: &ThingsCatalog,
        drawer: &mut EditDrawer,
        delta_time: f32
    )
    {
        let error = return_if_none!(self.innards.error_highlight.draw(delta_time));

        if self.innards.is_thing(error)
        {
            drawer.polygon_with_solid_color(
                self.thing(error).thing_hull(things_catalog).rectangle().into_iter(),
                Color::ErrorHighlight
            );
            return;
        }

        self.brush(error).draw_with_solid_color(drawer, Color::ErrorHighlight);
    }
}

//=======================================================================//

/// A wrapper for all the brushes in the map.
#[derive(Clone, Copy)]
pub(in crate::map) struct Brushes<'a>(&'a HvHashMap<Id, Brush>);

impl Brushes<'_>
{
    /// Returns the brush with [`Id`] `identifier`.
    /// # Panics
    /// Panics if the brush does not exist.
    #[inline]
    pub fn get(&self, identifier: Id) -> &Brush { self.0.get(&identifier).unwrap() }

    /// Returns an iterator to the brushes.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &Brush> { self.0.values() }
}

//=======================================================================//

/// A wrapper for a brush that automatically updates certain [`EntitiesManager`] values when
/// it's dropped.
#[must_use]
pub(in crate::map) struct BrushMut<'a>
{
    resources:         &'a DrawingResources,
    /// A mutable reference to the [`EntitiesManager`] core.
    manager:           &'a mut Innards,
    grid:              &'a Grid,
    /// A mutable reference to the [`QuadTree`]s.
    quad_trees:        &'a mut Trees,
    /// The [`Id`] of the brush.
    id:                Id,
    /// The center of the brush at the moment the struct was created.
    center:            Vec2,
    /// The amount of selected vertexes of the brush at the moment the struct was created.
    selected_vertexes: bool
}

impl Deref for BrushMut<'_>
{
    type Target = Brush;

    #[inline]
    #[must_use]
    fn deref(&self) -> &Self::Target { self.manager.brush(self.id) }
}

impl DerefMut for BrushMut<'_>
{
    #[inline]
    #[must_use]
    fn deref_mut(&mut self) -> &mut Self::Target { self.manager.brushes.get_mut(&self.id).unwrap() }
}

impl Drop for BrushMut<'_>
{
    #[inline]
    fn drop(&mut self)
    {
        let brush = unsafe {
            std::ptr::from_mut(self.manager.brushes.get_mut(&self.id).unwrap())
                .as_mut()
                .unwrap()
        };

        if matches!(self.quad_trees.insert_brush_hull(brush), InsertResult::Replaced)
        {
            self.manager.outline_update = true;
        }

        // Has or had selected vertexes and now it doesn't.
        if brush.has_selected_vertexes() || self.selected_vertexes
        {
            self.manager.selected_vertexes_update.insert(self.id);
        }

        if !self.center.around_equal_narrow(&brush.center())
        {
            if brush.has_attachments()
            {
                self.manager.replace_anchors_hull(self.quad_trees, self.id);
            }
            else if let Some(id) = brush.attached()
            {
                self.manager.replace_anchors_hull(self.quad_trees, id);
            }
        }

        if brush.has_path()
        {
            _ = self.quad_trees.insert_path_hull(brush);
        }
        else
        {
            _ = self.quad_trees.remove_path_hull(brush);
        }

        if brush.has_sprite()
        {
            if matches!(
                self.quad_trees.insert_sprite_hull(self.resources, self.grid, brush),
                InsertResult::Inserted | InsertResult::Replaced
            )
            {
                self.manager.outline_update = true;
            }
        }
        else if self.quad_trees.remove_sprite_hull(brush)
        {
            self.manager.outline_update = true;
        }

        if brush.was_texture_edited()
        {
            self.manager.overall_texture_update = true;
        }
    }
}

impl EntityId for BrushMut<'_>
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
    fn new(
        resources: &'a DrawingResources,
        manager: &'a mut Innards,
        grid: &'a Grid,
        quad_trees: &'a mut Trees,
        identifier: Id
    ) -> Self
    {
        let brush = manager.brush(identifier);
        let center = brush.center();
        let selected_vertexes = brush.has_selected_vertexes();

        Self {
            resources,
            manager,
            grid,
            quad_trees,
            id: identifier,
            center,
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
    things_catalog: &'a ThingsCatalog,
    /// A mutable reference to the core of the [`EntitiesManager`].
    manager:        &'a mut Innards,
    /// A mutable reference to the [`QuadTree`]s.
    quad_trees:     &'a mut Trees,
    /// The [`Id`] of the [`ThingInstance`].
    id:             Id
}

impl Deref for ThingMut<'_>
{
    type Target = ThingInstance;

    #[inline]
    #[must_use]
    fn deref(&self) -> &Self::Target { self.manager.thing(self.id) }
}

impl DerefMut for ThingMut<'_>
{
    #[inline]
    #[must_use]
    fn deref_mut(&mut self) -> &mut Self::Target { self.manager.things.get_mut(&self.id).unwrap() }
}

impl Drop for ThingMut<'_>
{
    #[inline]
    fn drop(&mut self)
    {
        let thing = self.manager.things.get_mut(&self.id).unwrap();
        _ = self.quad_trees.insert_thing_hull(self.things_catalog, thing);

        if thing.has_path()
        {
            _ = self.quad_trees.insert_path_hull(thing);
        }
        else
        {
            _ = self.quad_trees.remove_path_hull(thing);
        }
    }
}

impl EntityId for ThingMut<'_>
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
    fn new(
        things_catalog: &'a ThingsCatalog,
        manager: &'a mut Innards,
        quad_trees: &'a mut Trees,
        identifier: Id
    ) -> Self
    {
        Self {
            things_catalog,
            manager,
            quad_trees,
            id: identifier
        }
    }
}

//=======================================================================//

/// A wrapper for an entity that implements the [`EditPath`] trait.
#[must_use]
pub(in crate::map) enum MovingMut<'a>
{
    /// A brush.
    Brush(BrushMut<'a>),
    /// A [`ThingInstance`].
    Thing(ThingMut<'a>)
}

impl Deref for MovingMut<'_>
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

impl DerefMut for MovingMut<'_>
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

impl EntityId for MovingMut<'_>
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

impl EntityCenter for MovingMut<'_>
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
        resources: &'a DrawingResources,
        things_catalog: &'a ThingsCatalog,
        manager: &'a mut Innards,
        grid: &'a Grid,
        quad_trees: &'a mut Trees,
        identifier: Id
    ) -> Self
    {
        if manager.is_thing(identifier)
        {
            return Self::Thing(ThingMut::new(things_catalog, manager, quad_trees, identifier));
        }

        Self::Brush(BrushMut::new(resources, manager, grid, quad_trees, identifier))
    }
}
