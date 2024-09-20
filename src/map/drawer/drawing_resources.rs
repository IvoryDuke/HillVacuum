//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{
    fs::File,
    io::{BufReader, BufWriter},
    ops::{Deref, DerefMut}
};

use bevy::{
    asset::{AssetServer, Assets, Handle},
    ecs::{
        entity::Entity,
        query::With,
        system::{Commands, Query}
    },
    render::{
        mesh::{Indices, Mesh, PrimitiveTopology, VertexAttributeValues},
        render_asset::RenderAssetUsages,
        texture::Image,
        view::NoFrustumCulling
    },
    sprite::{ColorMaterial, MaterialMesh2dBundle, Mesh2dHandle},
    transform::components::Transform
};
use bevy_egui::{egui, EguiUserTextures};
use glam::{UVec2, Vec2};
use hill_vacuum_proc_macros::{meshes_indexes, str_array};
use hill_vacuum_shared::{continue_if_none, match_or_panic, return_if_none};

use super::{
    animation::{Animation, AtlasAnimator},
    color::Color,
    drawers::{Uv, VxColor, VxPos, HULL_HEIGHT_LABEL, HULL_WIDTH_LABEL},
    file_animations,
    texture::{DefaultAnimation, TextureInterface, TextureInterfaceExtra},
    texture_loader::TextureLoader,
    BevyColor
};
use crate::{
    embedded_assets::embedded_asset_path,
    map::{
        brush::convex_polygon::NEW_VX,
        drawer::{drawers::IntoArray3, texture::Texture},
        editor::{
            state::{
                clipboard::{PropCameras, PropCamerasMut},
                core::cursor_delta::CursorDelta,
                grid::Grid
            },
            Placeholder
        },
        indexed_map::IndexedMap,
        thing::{catalog::ThingsCatalog, ThingInterface}
    },
    utils::{
        collections::{hv_hash_map, hv_hash_set, hv_vec},
        hull::Hull,
        math::{points::rotate_point_around_origin, HashVec2},
        misc::{vertex_highlight_square, AssertedInsertRemove, Camera}
    },
    HvHashMap,
    HvHashSet,
    HvVec
};

//=======================================================================//
// STATICS
//
//=======================================================================//

macro_rules! handles {
    ($($material:ident),+) => { paste::paste!{ $(
        #[inline]
        #[must_use]
        fn $material(&self, color: Color) -> Handle<ColorMaterial>
        {
            let materials = &self.[< $material s >];

            match color
            {
                Color::NonSelectedEntity | Color::SelectedEntity |
                Color::HighlightedNonSelectedEntity | Color::HighlightedSelectedEntity |
                Color::NonSelectedVertex |
                Color::ClippedPolygonsToSpawn | Color::SubtractorBrush |
                Color::SubtracteeBrush | Color::OpaqueEntity => &materials.semitransparent,
                _ => panic!("Color with no associated material: {color:?}.")
            }
            .clone_weak()
        }
    )+ }};
}

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

meshes_indexes!(INDEXES, 128);

//=======================================================================//
// TYPES
//
//=======================================================================//

#[must_use]
struct Materials
{
    semitransparent: Handle<ColorMaterial>,
    pure:            Handle<ColorMaterial>
}

impl Placeholder for Materials
{
    #[inline]
    unsafe fn placeholder() -> Self
    {
        Self {
            semitransparent: Handle::default(),
            pure:            Handle::default()
        }
    }
}

impl Materials
{
    #[inline]
    fn new(handle: Handle<Image>, materials: &mut Assets<ColorMaterial>) -> Self
    {
        Self {
            semitransparent: materials.add(ColorMaterial {
                color:   BevyColor::srgba(1.0, 1.0, 1.0, 1f32 / 4f32),
                texture: handle.clone_weak().into()
            }),
            pure:            materials.add(handle)
        }
    }
}

//=======================================================================//

/// A [`Texture`] and the [`Handle`]s of the [`ColorMaterial`]s necessary to draw the entities.
#[allow(clippy::missing_docs_in_private_items)]
#[must_use]
pub(in crate::map) struct TextureMaterials
{
    texture:          Texture,
    egui_id:          egui::TextureId,
    repeat_materials: Materials,
    clamp_materials:  Materials
}

impl Placeholder for TextureMaterials
{
    #[inline]
    unsafe fn placeholder() -> Self
    {
        Self {
            texture:          Texture::placeholder(),
            egui_id:          egui::TextureId::default(),
            repeat_materials: Materials::placeholder(),
            clamp_materials:  Materials::placeholder()
        }
    }
}

impl TextureMaterials
{
    handles!(repeat_material, clamp_material);

    /// Returns a new [`TextureMaterials`].
    #[inline]
    fn new(
        texture: Texture,
        egui_id: egui::TextureId,
        materials: &mut Assets<ColorMaterial>
    ) -> Self
    {
        Self {
            egui_id,
            repeat_materials: Materials::new(texture.repeat_handle(), materials),
            clamp_materials: Materials::new(texture.clamp_handle(), materials),
            texture
        }
    }

    /// Returns a reference to the [`Texture`].
    #[inline]
    pub const fn texture(&self) -> &Texture { &self.texture }

    /// Returns the [`egui::TextureId`] of the texture.
    #[inline]
    pub const fn egui_id(&self) -> egui::TextureId { self.egui_id }

    /// Returns a [`TextureMaterials`] that does not have colored materials.
    #[inline]
    fn error(texture: (Texture, egui::TextureId), materials: &mut Assets<ColorMaterial>) -> Self
    {
        Self {
            repeat_materials: Materials::new(texture.0.repeat_handle(), materials),
            clamp_materials:  Materials::new(texture.0.clamp_handle(), materials),
            texture:          texture.0,
            egui_id:          texture.1
        }
    }
}

//=======================================================================//

/// The resources needed to draw things onto the map.
pub(in crate::map) struct DrawingResources
{
    /// The container of the generated brushes and handles.
    brush_meshes: Meshes,
    /// The [`Mesh2dHandle`] of the vertex highlight square.
    vertex_highlight_mesh: Mesh2dHandle,
    /// The [`Mesh2dHandle`] of the [`Prop`] pivot displayed in front of the the paint tool camera.
    paint_tool_vertex_highlight_mesh: Mesh2dHandle,
    /// The [`Mesh2dHandle`]s of the [`Prop`] pivots displayed in front of the prop cameras.
    props_pivots_mesh: HvHashMap<Entity, Mesh2dHandle>,
    /// The [`Mesh2dHandle`] of the circular highlight of the brushes other brushes are
    /// tied to.
    attachment_highlight_mesh: Mesh2dHandle,
    /// The [`Mesh2dHandle`] of the circular highlight of the brushes that own a sprite.
    sprite_highlight_mesh: Mesh2dHandle,
    /// The tooltip labels generator.
    tt_label_gen: TooltipLabelGenerator,
    /// The default [`ColorMaterial`].
    default_material: Handle<ColorMaterial>,
    /// The textures loaded from the assets folder.
    textures: IndexedMap<String, TextureMaterials>,
    /// The error texture.
    error_texture: TextureMaterials,
    /// The clip overlay texture.
    clip_texture: Handle<ColorMaterial>,
    /// The names of the textures with [`Animations`].
    animated_textures: HvHashSet<String>,
    /// Whether any default texture animation was changed.
    default_animation_changed: bool
}

impl Placeholder for DrawingResources
{
    #[inline]
    unsafe fn placeholder() -> Self
    {
        Self {
            brush_meshes: Meshes::default(),
            vertex_highlight_mesh: Mesh2dHandle::default(),
            paint_tool_vertex_highlight_mesh: Mesh2dHandle::default(),
            props_pivots_mesh: hv_hash_map![],
            attachment_highlight_mesh: Mesh2dHandle::default(),
            sprite_highlight_mesh: Mesh2dHandle::default(),
            tt_label_gen: TooltipLabelGenerator::default(),
            default_material: Handle::default(),
            textures: IndexedMap::new(hv_vec![], |tex| tex.texture.name().to_owned()),
            error_texture: TextureMaterials::placeholder(),
            clip_texture: Handle::default(),
            animated_textures: hv_hash_set![],
            default_animation_changed: false
        }
    }
}

impl DrawingResources
{
    /// The amount of sides the circle highlight has.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    const ANCHOR_HIGHLIGHT_RESOLUTION: u8 = Self::CIRCLE_HIGHLIGHT_MULTI as u8;
    /// The multiplier applied to the current frame's scale to generate the vertexes of the circle
    /// highlight.
    const CIRCLE_HIGHLIGHT_MULTI: f32 = 15f32;
    /// The multiplier applied to the current frame's scale to generate the vertexes of the sprite
    /// highlight.
    const SPRITE_HIGHLIGHT_RESOLUTION: u8 = 4;

    //==============================================================
    // New

    /// Returns a new [`Mesh`].
    #[inline]
    #[must_use]
    fn mesh(primitive_topology: PrimitiveTopology) -> Mesh
    {
        Mesh::new(primitive_topology, RenderAssetUsages::all())
    }

    /// Returns a new [`DrawingResources`].
    #[inline]
    #[must_use]
    pub fn new(
        prop_cameras: &PropCamerasMut,
        asset_server: &AssetServer,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<ColorMaterial>,
        user_textures: &mut EguiUserTextures,
        texture_loader: &mut TextureLoader
    ) -> Self
    {
        /// The name of the error texture.
        const ERROR_TEXTURE_NAME: &str = "error.png";
        /// The name of the clip overlay texture.
        const CLIP_OVERLAY_TEXTURE_NAME: &str = "clip_overlay.png";

        /// Returns an highlight mesh.
        macro_rules! highlight_mesh {
            ($func:ident) => {{
                let mut mesh = Self::mesh(PrimitiveTopology::LineStrip);

                mesh.insert_attribute(
                    Mesh::ATTRIBUTE_POSITION,
                    Self::$func(1f32).map(IntoArray3::as_f32x3).collect::<Vec<_>>()
                );

                mesh
            }};
        }

        // Square highlight.
        let mut square_mesh = Self::mesh(PrimitiveTopology::TriangleList);

        square_mesh.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            Self::vertex_highlight_vxs(1f32)
                .map(IntoArray3::as_f32x3)
                .collect::<Vec<_>>()
        );

        let mut idxs = Vec::with_capacity(6);
        idxs.extend_from_slice(unsafe { &(&*INDEXES)[..3] });
        idxs.extend_from_slice(unsafe { &(&*INDEXES)[3..6] });

        square_mesh.insert_indices(Indices::U16(idxs));

        let props_vertex_highlight_mesh = hv_hash_map![collect;
            prop_cameras
                .iter()
                .map(|(id, ..)| (id, meshes.add(square_mesh.clone()).into()))
        ];
        let err_tex = {
            let handle = asset_server.load(embedded_asset_path(ERROR_TEXTURE_NAME));
            let clamp = handle.clone_weak();
            Texture::from_parts(ERROR_TEXTURE_NAME, UVec2::splat(64), handle, clamp)
        };
        let err_id = user_textures.add_image(err_tex.repeat_handle());

        Self {
            brush_meshes: Meshes::default(),
            vertex_highlight_mesh: meshes.add(square_mesh.clone()).into(),
            paint_tool_vertex_highlight_mesh: meshes.add(square_mesh).into(),
            props_pivots_mesh: props_vertex_highlight_mesh,
            attachment_highlight_mesh: meshes.add(highlight_mesh!(attachment_highlight_vxs)).into(),
            sprite_highlight_mesh: meshes.add(highlight_mesh!(sprite_highlight_vxs)).into(),
            tt_label_gen: TooltipLabelGenerator::default(),
            default_material: materials.add(ColorMaterial::default()),
            textures: Self::sort_textures(materials, texture_loader.loaded_textures()),
            error_texture: TextureMaterials::error((err_tex, err_id), materials),
            clip_texture: materials
                .add(asset_server.load(embedded_asset_path(CLIP_OVERLAY_TEXTURE_NAME))),
            animated_textures: hv_hash_set![],
            default_animation_changed: false
        }
    }

    /// Initialized the labels used by the tooltips,
    #[inline]
    pub fn init(ctx: &egui::Context)
    {
        for label in [
            CursorDelta::X_DELTA,
            CursorDelta::Y_DELTA,
            NEW_VX,
            HULL_WIDTH_LABEL,
            HULL_HEIGHT_LABEL
        ]
        .into_iter()
        .chain(TooltipLabelGenerator::iter())
        {
            egui::Area::new(label.into())
                .order(egui::Order::Background)
                .show(ctx, |ui| {
                    egui::Frame::none().fill(egui::Color32::TRANSPARENT).show(ui, |ui| {
                        ui.label(egui::RichText::default().color(egui::Color32::TRANSPARENT));
                    });
                });
        }
    }

    /// The amount of default texture animations.
    #[inline]
    #[must_use]
    pub fn animations_amount(&self) -> usize { self.animated_textures.len() }

    /// Imports the animations contained in `file`.
    #[inline]
    pub fn import_animations(
        &mut self,
        amount: usize,
        file: &mut BufReader<File>
    ) -> Result<(), &'static str>
    {
        self.execute_import(amount, file, |_| {})
    }

    /// Replaces the animations with the ones contained in `file`.
    #[inline]
    pub fn reset_animations(
        &mut self,
        amount: usize,
        file: &mut BufReader<File>
    ) -> Result<(), &'static str>
    {
        self.execute_import(amount, file, |resources| {
            for tex in &resources.animated_textures
            {
                *resources.textures.get_mut(tex).unwrap().texture.animation_mut() = Animation::None;
            }

            resources.animated_textures.clear();
        })
    }

    #[inline]
    pub fn execute_import<F>(
        &mut self,
        amount: usize,
        file: &mut BufReader<File>,
        f: F
    ) -> Result<(), &'static str>
    where
        F: FnOnce(&mut Self)
    {
        match file_animations(amount, file)
        {
            Ok(animations) =>
            {
                f(self);

                for default in animations
                {
                    *continue_if_none!(self.texture_mut(&default.texture))
                        .animation_mut_set_dirty() = default.animation;
                }

                Ok(())
            },
            Err(err) => Err(err)
        }
    }

    /// Exports the default texture animations to `writer`.
    #[inline]
    pub fn export_animations(
        &mut self,
        mut writer: &mut BufWriter<&mut Vec<u8>>
    ) -> Result<(), &'static str>
    {
        match self
            .animated_textures
            .iter()
            .map(|tex| {
                let texture = self.texture(tex).unwrap();

                DefaultAnimation {
                    texture:   texture.name().to_string(),
                    animation: texture.animation().clone()
                }
            })
            .find(|animation| ciborium::ser::into_writer(&animation, &mut writer).is_err())
        {
            Some(_) => Err("Error saving animations"),
            None => Ok(())
        }
    }

    /// Whether a default animation was changed.
    #[inline]
    #[must_use]
    pub const fn default_animations_changed(&self) -> bool { self.default_animation_changed }

    /// Sets the default animation changed flag to false.
    #[inline]
    pub fn reset_default_animation_changed(&mut self) { self.default_animation_changed = false; }

    //==============================================================
    // Info

    /// Returns the [`Handle<ColorMaterial`>] of the default material.
    #[inline]
    #[must_use]
    pub(in crate::map::drawer) fn default_material(&self) -> Handle<ColorMaterial>
    {
        self.default_material.clone()
    }

    /// Returns the vertexes of the square highlight based on `camera_scale`.
    #[inline]
    fn vertex_highlight_vxs(camera_scale: f32) -> impl ExactSizeIterator<Item = Vec2>
    {
        vertex_highlight_square(camera_scale).rectangle().into_iter()
    }

    /// Returns the vertexes of a circle with `resolution` sides.
    #[inline]
    pub(in crate::map::drawer) fn circle_vxs(
        resolution: u8,
        radius: f32
    ) -> impl Iterator<Item = Vec2>
    {
        let circle = Hull::new(radius, -radius, -radius, radius).circle(resolution);
        circle.chain(Some(Vec2::new(0f32, radius)))
    }

    /// Returns an iterator to the vertexes of the circle highlight.
    #[inline]
    fn attachment_highlight_vxs(camera_scale: f32) -> impl Iterator<Item = Vec2>
    {
        Self::circle_vxs(
            Self::ANCHOR_HIGHLIGHT_RESOLUTION,
            camera_scale * Self::CIRCLE_HIGHLIGHT_MULTI
        )
    }

    /// Returns an iterator to the vertexes of the sprite highlight.
    #[inline]
    fn sprite_highlight_vxs(camera_scale: f32) -> impl Iterator<Item = Vec2>
    {
        Self::circle_vxs(
            Self::SPRITE_HIGHLIGHT_RESOLUTION,
            camera_scale * Self::CIRCLE_HIGHLIGHT_MULTI
        )
    }

    /// Returns a static [`str`] to be used as tooltip label for `pos`.
    #[inline]
    #[must_use]
    pub fn vx_tooltip_label(&mut self, pos: Vec2) -> Option<&'static str>
    {
        self.tt_label_gen.vx_label(pos)
    }

    #[inline]
    #[must_use]
    pub fn tooltip_label(&mut self) -> Option<&'static str> { self.tt_label_gen.label() }

    /// Returns the [`egui::TextureId`], size, and size [`String`] of the texture named `name`.
    #[inline]
    #[must_use]
    pub fn egui_texture(&self, name: &str) -> (egui::TextureId, UVec2, &str)
    {
        self.textures.get(name).map_or(
            (
                self.error_texture.egui_id,
                self.error_texture.texture.size(),
                self.error_texture.texture.size_str()
            ),
            |tex| (tex.egui_id, tex.texture.size(), tex.texture.size_str())
        )
    }

    /// Returns a reference to the [`Texture`] named `name`, if it exists.
    #[inline]
    pub fn texture(&self, name: &str) -> Option<&Texture>
    {
        self.textures.get(name).map(|tex| &tex.texture)
    }

    /// Returns the [`TextureMut`] wrapping the [`Texture`] named `name`, if it exists.
    #[inline]
    pub fn texture_mut(&mut self, name: &str) -> Option<TextureMut> { TextureMut::new(self, name) }

    /// Returns a reference to the [`Texture`] named `name` if it exists. Otherwise returns the
    /// error texture.
    #[inline]
    pub fn texture_or_error(&self, name: &str) -> &Texture
    {
        self.texture(name).unwrap_or(self.error_texture())
    }

    /// Returns a reference to the [`TextureMaterials`] of the texture named `name`.
    #[inline]
    pub(in crate::map::drawer) fn texture_materials(&self, name: &str) -> &TextureMaterials
    {
        self.textures.get(name).unwrap_or(&self.error_texture)
    }

    /// Returns the [`Handle`] to the error texture.
    #[inline]
    pub const fn error_texture(&self) -> &Texture { self.error_texture.texture() }

    /// Returns the [`Handle`] to the clip texture.
    #[inline]
    pub fn clip_texture(&self) -> Handle<ColorMaterial> { self.clip_texture.clone() }

    /// Returns a [`Chunks`] iterator with `chunk_size` to the [`TextureMaterials`].
    #[inline]
    pub fn chunked_textures<'a, F>(
        &'a self,
        chunk_size: usize,
        chunks_container: &'a mut HvVec<&'static TextureMaterials>,
        f: Option<F>
    ) -> ChunkedTextures<'a, F>
    where
        F: Fn(&&'a TextureMaterials) -> bool
    {
        let iter = match f
        {
            Some(f) => TexturesIter::Filtered(self.textures.values().filter(f)),
            None => TexturesIter::Unfiltered(self.textures.values())
        };

        ChunkedTextures {
            iter,
            chunk_size,
            container: chunks_container
        }
    }

    #[inline]
    #[must_use]
    pub fn is_animated(&self, texture: &str) -> bool { self.animated_textures.contains(texture) }

    //==============================================================
    // Texture loading

    /// Sort the textures.
    #[inline]
    fn sort_textures(
        materials: &mut Assets<ColorMaterial>,
        mut textures: Vec<(Texture, egui::TextureId)>
    ) -> IndexedMap<String, TextureMaterials>
    {
        textures.sort_by(|a, b| a.0.name().cmp(b.0.name()));
        let textures = hv_vec![collect; textures.into_iter().map(|(tex, id)| {
            TextureMaterials::new(tex, id, materials)
        })];

        IndexedMap::new(textures, |tex| tex.texture.name().to_owned())
    }

    /// Reloads the textures.
    #[inline]
    pub fn reload_textures(
        &mut self,
        materials: &mut Assets<ColorMaterial>,
        textures: Vec<(Texture, egui::TextureId)>
    )
    {
        let mut textures = Self::sort_textures(materials, textures);
        let mut to_remove = hv_hash_set![];

        for t in &self.animated_textures
        {
            let tex_materials = match textures.get_mut(t)
            {
                Some(texture) => texture,
                None =>
                {
                    to_remove.asserted_insert(t.clone());
                    continue;
                }
            };

            *tex_materials.texture.animation_mut_set_dirty() =
                std::mem::take(self.textures.get_mut(t).unwrap().texture.animation_mut());
        }

        for t in to_remove
        {
            self.animated_textures.asserted_remove(&t);
        }

        self.textures = textures;
    }

    //==============================================================
    // Update

    /// Sets up `self` for the current frame.
    #[inline]
    pub(in crate::map::drawer) fn setup_frame(
        &mut self,
        commands: &mut Commands,
        prop_cameras: &PropCameras,
        meshes: &mut Assets<Mesh>,
        meshes_query: &Query<Entity, With<Mesh2dHandle>>,
        camera_scale: f32,
        paint_tool_camera_scale: f32
    )
    {
        /// Refreshes a highlight mesh.
        #[inline]
        fn refresh_highlight<I: Iterator<Item = Vec2>>(
            meshes: &mut Assets<Mesh>,
            handle: &Mesh2dHandle,
            camera_scale: f32,
            generator: fn(f32) -> I
        )
        {
            for (f32x3, vx) in match_or_panic!(
                meshes
                    .get_mut(&handle.0)
                    .unwrap()
                    .attribute_mut(Mesh::ATTRIBUTE_POSITION)
                    .unwrap(),
                VertexAttributeValues::Float32x3(vxs),
                vxs
            )
            .iter_mut()
            .zip(generator(camera_scale))
            {
                *f32x3 = vx.as_f32x3();
            }
        }

        self.brush_meshes
            .collect_previous_frame_meshes(commands, meshes, meshes_query);
        self.tt_label_gen.reset();

        refresh_highlight(
            meshes,
            &self.vertex_highlight_mesh,
            camera_scale,
            Self::vertex_highlight_vxs
        );
        refresh_highlight(
            meshes,
            &self.attachment_highlight_mesh,
            camera_scale,
            Self::attachment_highlight_vxs
        );
        refresh_highlight(
            meshes,
            &self.sprite_highlight_mesh,
            camera_scale,
            Self::sprite_highlight_vxs
        );
        refresh_highlight(
            meshes,
            &self.paint_tool_vertex_highlight_mesh,
            paint_tool_camera_scale / 2f32,
            Self::vertex_highlight_vxs
        );

        for (camera_scale, handle) in self.props_pivots_mesh.iter().filter_map(|(id, handle)| {
            let camera = prop_cameras.get(*id).unwrap();
            camera.0.is_active.then_some((camera.1.scale() / 2f32, handle))
        })
        {
            refresh_highlight(meshes, handle, camera_scale, Self::vertex_highlight_vxs);
        }
    }

    /// Returns a new [`MeshGenerator`].
    #[inline]
    pub(in crate::map::drawer) fn mesh_generator(&mut self) -> MeshGenerator
    {
        MeshGenerator::new(self)
    }

    /// Returns a [`Transform`] for the [`Mesh`] entities.
    #[inline]
    #[must_use]
    const fn mesh_transform(center: Vec2, height: f32) -> Transform
    {
        Transform::from_translation(center.extend(height))
    }

    /// Queues a new [`Mesh`] to be drawn at the end of the frame.
    #[inline]
    pub(in crate::map::drawer) fn push_mesh(
        &mut self,
        mesh: Mesh2dHandle,
        material: Handle<ColorMaterial>,
        height: f32
    )
    {
        let transform = Self::mesh_transform(Vec2::ZERO, height);

        self.brush_meshes.push(MaterialMesh2dBundle {
            mesh,
            material,
            global_transform: transform.into(),
            transform,
            ..Default::default()
        });
    }

    /// Pushes a textured mesh.
    #[inline]
    pub(in crate::map::drawer) fn push_textured_mesh<T: TextureInterface>(
        &mut self,
        mesh: Mesh2dHandle,
        settings: &T,
        color: Color
    )
    {
        self.push_mesh(
            mesh,
            self.texture_materials(settings.name()).repeat_material(color),
            color.entity_height() + settings.height_f32()
        );
    }

    /// Pushes a map preview textured mesh.
    #[inline]
    pub(in crate::map::drawer) fn push_map_preview_textured_mesh<T: TextureInterface>(
        &mut self,
        mesh: Mesh2dHandle,
        texture: &TextureMaterials,
        settings: &T
    )
    {
        self.push_mesh(mesh, texture.repeat_materials.pure.clone_weak(), settings.height_f32());
    }

    /// Pushes a sprite mesh.
    #[inline]
    pub(in crate::map::drawer) fn push_sprite<T: TextureInterface>(
        &mut self,
        mesh: Mesh2dHandle,
        settings: &T,
        color: Color
    )
    {
        self.push_mesh(
            mesh,
            self.texture_materials(settings.name()).clamp_material(color),
            color.entity_height() + settings.height_f32()
        );
    }

    /// Pushes a map preview sprite mesh.
    #[inline]
    pub(in crate::map::drawer) fn push_map_preview_sprite<T: TextureInterface>(
        &mut self,
        mesh: Mesh2dHandle,
        texture: &TextureMaterials,
        settings: &T
    )
    {
        self.push_mesh(mesh, texture.clamp_materials.pure.clone_weak(), settings.height_f32());
    }

    /// Pushes a thing mesh.
    #[inline]
    pub(in crate::map::drawer) fn push_thing<T: ThingInterface>(
        &mut self,
        mesh: Mesh2dHandle,
        catalog: &ThingsCatalog,
        thing: &T,
        color: Color
    )
    {
        self.push_mesh(
            mesh,
            self.texture_materials(self.texture_or_error(catalog.texture(thing.thing())).name())
                .clamp_material(color),
            color.entity_height() + thing.draw_height_f32()
        );
    }

    /// Pushes a map preview thing mesh.
    #[inline]
    pub(in crate::map::drawer) fn push_map_preview_thing<T: ThingInterface>(
        &mut self,
        mesh: Mesh2dHandle,
        texture: &TextureMaterials,
        thing: &T
    )
    {
        self.push_mesh(mesh, texture.clamp_materials.pure.clone_weak(), thing.draw_height_f32());
    }

    /// Queues a new square hightligh [`Mesh`] to be drawn at the end of the frame.
    #[inline]
    pub(in crate::map::drawer) fn push_square_highlight_mesh(
        &mut self,
        material: Handle<ColorMaterial>,
        center: Vec2,
        height: f32
    )
    {
        let transform = Self::mesh_transform(center, height);

        self.brush_meshes.push_highlight(MaterialMesh2dBundle {
            mesh: self.vertex_highlight_mesh.clone(),
            material,
            global_transform: transform.into(),
            transform,
            ..Default::default()
        });
    }

    /// Pushes a [`Prop`] pivot mesh.
    #[inline]
    pub(in crate::map::drawer) fn push_prop_pivot_mesh(
        &mut self,
        material: Handle<ColorMaterial>,
        center: Vec2,
        height: f32,
        camera_id: Option<Entity>
    )
    {
        let transform = Self::mesh_transform(center, height);

        let handle = match camera_id
        {
            Some(id) => self.props_pivots_mesh.get(&id).unwrap(),
            None => &self.paint_tool_vertex_highlight_mesh
        };

        self.brush_meshes.push_highlight(MaterialMesh2dBundle {
            mesh: handle.clone(),
            material,
            global_transform: transform.into(),
            transform,
            ..Default::default()
        });
    }

    /// Queues a new attachment highlight [`Mesh`] to be drawn at the end of the frame.
    #[inline]
    pub(in crate::map::drawer) fn push_attachment_highlight_mesh(
        &mut self,
        material: Handle<ColorMaterial>,
        center: Vec2,
        height: f32
    )
    {
        let transform = Self::mesh_transform(center, height);

        self.brush_meshes.push_highlight(MaterialMesh2dBundle {
            mesh: self.attachment_highlight_mesh.clone(),
            material,
            global_transform: transform.into(),
            transform,
            ..Default::default()
        });
    }

    /// Pushes the mesh of a sprite highlight.
    #[inline]
    pub(in crate::map::drawer) fn push_sprite_highlight_mesh(
        &mut self,
        material: Handle<ColorMaterial>,
        center: Vec2,
        height: f32
    )
    {
        let transform = Self::mesh_transform(center, height);

        self.brush_meshes.push_highlight(MaterialMesh2dBundle {
            mesh: self.sprite_highlight_mesh.clone(),
            material,
            global_transform: transform.into(),
            transform,
            ..Default::default()
        });
    }

    /// Queues the [`Mesh`] of the map grid to be drawn at the end of the frame.
    #[inline]
    pub(in crate::map::drawer) fn push_grid_mesh(&mut self, mesh: Mesh2dHandle)
    {
        let transform = Self::mesh_transform(Vec2::ZERO, Color::GridLines.line_height());

        self.brush_meshes.push_grid(MaterialMesh2dBundle {
            mesh,
            material: self.default_material.clone_weak(),
            global_transform: transform.into(),
            transform,
            ..Default::default()
        });
    }

    /// Spawns all the queued [`Mesh`]es.
    #[inline]
    pub(in crate::map::drawer) fn spawn_meshes(&mut self, commands: &mut Commands)
    {
        self.brush_meshes.spawn_batch(commands);
    }

    /// Renders one tooltip for each label that has not been utilized this frame, to fix an egui
    /// issue where the first queued tooltip is not rendered during the frame where the amount of
    /// tooltips to render increases from the previous frame.
    #[inline]
    pub fn leftover_labels(&mut self) -> impl Iterator<Item = &'static str>
    {
        self.tt_label_gen.leftover_labels()
    }

    //==============================================================
    // Cleanup

    /// Shutdown cleanup to avoid double frees.
    #[inline]
    pub fn cleanup(&self, meshes: &mut Assets<Mesh>) { self.brush_meshes.cleanup(meshes); }
}

//=======================================================================//

/// The generator of the UI tooltips labels.
struct TooltipLabelGenerator
{
    /// The vertexes which already have an assigned tooltip.
    assigned_vertexes: HvHashSet<HashVec2>,
    /// The amount of labels used by tooltips in this frame.
    vx_labels_index:   usize
}

impl Default for TooltipLabelGenerator
{
    #[inline]
    #[must_use]
    fn default() -> Self
    {
        Self {
            assigned_vertexes: hv_hash_set![],
            vx_labels_index:   0
        }
    }
}

impl TooltipLabelGenerator
{
    str_array!(VX_LABELS, 128, vx_);

    /// Returns an iterator to all assignable labels.
    #[inline]
    fn iter() -> impl Iterator<Item = &'static str> { Self::VX_LABELS.into_iter() }

    /// Resets the assigned labels.
    #[inline]
    pub fn reset(&mut self)
    {
        self.assigned_vertexes.clear();
        self.vx_labels_index = 0;
    }

    /// Returns a static [`str`] to be used as label for a new tooltip located at position `pos`, if
    /// `pos` has not already gotten a tooltip during this frame and there are still available
    /// labels.
    #[inline]
    #[must_use]
    pub fn vx_label(&mut self, pos: Vec2) -> Option<&'static str>
    {
        if self.vx_labels_index == Self::VX_LABELS.len() ||
            !self.assigned_vertexes.insert(HashVec2(pos))
        {
            return None;
        }

        let value = Self::VX_LABELS[self.vx_labels_index];
        self.vx_labels_index += 1;
        Some(value)
    }

    #[inline]
    #[must_use]
    pub fn label(&mut self) -> Option<&'static str>
    {
        if self.vx_labels_index == Self::VX_LABELS.len()
        {
            return None;
        }

        let value = Self::VX_LABELS[self.vx_labels_index];
        self.vx_labels_index += 1;
        Some(value)
    }

    /// Renders one tooltip for each label that has not been utilized this frame, to fix an egui
    /// issue where the first queued tooltip is not rendered during the frame where the amount of
    /// tooltips to render has increased from the previous frame.
    #[inline]
    pub fn leftover_labels(&mut self) -> impl Iterator<Item = &'static str>
    {
        let iter = Self::VX_LABELS[self.vx_labels_index..Self::VX_LABELS.len()]
            .iter()
            .copied();
        self.vx_labels_index = Self::VX_LABELS.len();
        iter
    }
}

//=======================================================================//

/// The container of the generated brushes and handles.
struct Meshes
{
    /// The meshes to batch spawn at the end of the frame.
    spawn:       Vec<MaterialMesh2dBundle<ColorMaterial>>,
    /// The meshes to remove from the assets at the start of the frame.
    remove:      HvVec<Handle<Mesh>>,
    /// The meshes that can be reused to generate new ones.
    parts:       MeshParts,
    /// The grid [`Mesh`] to spawn.
    grid:        Option<MaterialMesh2dBundle<ColorMaterial>>,
    /// The [`Handle`] of the grid [`Mesh`].
    grid_handle: Option<Handle<Mesh>>
}

impl Default for Meshes
{
    #[inline]
    #[must_use]
    fn default() -> Self
    {
        Self {
            spawn:       Vec::new(),
            remove:      hv_vec![],
            parts:       MeshParts::default(),
            grid:        None,
            grid_handle: None
        }
    }
}

impl Meshes
{
    /// Collects the meshes created in the previous frame to be reused, and despawns the entities
    /// that employed them.
    #[inline]
    pub fn collect_previous_frame_meshes(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        meshes_query: &Query<Entity, With<Mesh2dHandle>>
    )
    {
        for handle in &self.remove
        {
            self.parts.push(meshes.remove(handle).unwrap());
        }

        self.remove.clear();

        if let Some(handle) = std::mem::take(&mut self.grid_handle)
        {
            self.parts.push_grid(meshes.remove(&handle).unwrap());
        }

        for id in meshes_query
        {
            commands.entity(id).despawn();
        }
    }

    /// Pushes a new [`MaterialMesh2dBundle`] generated from a square or circle highlight mesh.
    #[inline]
    pub fn push(&mut self, mesh: MaterialMesh2dBundle<ColorMaterial>)
    {
        self.remove.push(mesh.mesh.0.clone());
        self.spawn.push(mesh);
    }

    /// Pushes a new [`MaterialMesh2dBundle`] belonging to a square or attachment highlight.
    #[inline]
    pub fn push_highlight(&mut self, mesh: MaterialMesh2dBundle<ColorMaterial>)
    {
        self.spawn.push(mesh);
    }

    /// Pushes a new [`MaterialMesh2dBundle`] of the map grid.
    #[inline]
    pub fn push_grid(&mut self, mesh: MaterialMesh2dBundle<ColorMaterial>)
    {
        assert!(self.grid_handle.is_none() && self.grid.is_none(), "Grid mesh already exists.");
        self.grid_handle = mesh.mesh.0.clone().into();
        self.grid = mesh.into();
    }

    /// Spawns all the [`MaterialMesh2dBundle`] as entities into the map.
    #[inline]
    pub fn spawn_batch(&mut self, commands: &mut Commands)
    {
        commands.spawn_batch(
            std::mem::take(&mut self.spawn)
                .into_iter()
                .map(|mesh| (mesh, NoFrustumCulling))
                .chain(std::mem::take(&mut self.grid).map(|mesh| (mesh, NoFrustumCulling)))
        );
    }

    /// Shutdown cleanup to avoid double frees.
    #[inline]
    pub fn cleanup(&self, meshes: &mut Assets<Mesh>)
    {
        for handle in &self.remove
        {
            MeshParts::cleanup_indexes(meshes.remove(handle).unwrap());
        }
    }
}

//=======================================================================//

/// A container of the leftover components of the [`Mesh`]es drawn the previous frame that can be
/// reused in the current one.
struct MeshParts
{
    /// The positions vectors
    pos:        HvVec<Vec<VxPos>>,
    /// The color vectors.
    color:      HvVec<Vec<VxColor>>,
    /// The uv vectors.
    uv:         HvVec<Vec<Uv>>,
    /// A positions vector just for the grid.
    grid_pos:   Vec<VxPos>,
    /// A colors vector just for the grid.
    grid_color: Vec<VxColor>
}

impl Default for MeshParts
{
    #[inline]
    #[must_use]
    fn default() -> Self
    {
        Self {
            pos:        hv_vec![Vec::with_capacity(4); 500],
            color:      hv_vec![Vec::with_capacity(4); 500],
            uv:         hv_vec![Vec::with_capacity(4); 500],
            grid_pos:   Vec::with_capacity(256),
            grid_color: Vec::with_capacity(256)
        }
    }
}

impl MeshParts
{
    /// Breaks `mesh` down into its components for later usage.
    #[inline]
    pub fn push(&mut self, mut mesh: Mesh)
    {
        let mut vxs = match_or_panic!(
            mesh.remove_attribute(Mesh::ATTRIBUTE_POSITION).unwrap(),
            VertexAttributeValues::Float32x3(vxs),
            vxs
        );
        vxs.clear();
        self.pos.push(vxs);

        if let Some(colors) = mesh.remove_attribute(Mesh::ATTRIBUTE_COLOR)
        {
            let mut colors =
                match_or_panic!(colors, VertexAttributeValues::Float32x4(colors), colors);
            colors.clear();
            self.color.push(colors);
        }

        if let Some(uvs) = mesh.remove_attribute(Mesh::ATTRIBUTE_UV_0)
        {
            let mut uvs = match_or_panic!(uvs, VertexAttributeValues::Float32x2(colors), colors);
            uvs.clear();
            self.uv.push(uvs);
        }

        Self::cleanup_indexes(mesh);
    }

    /// Pushes a grid mesh.
    #[inline]
    pub fn push_grid(&mut self, mut mesh: Mesh)
    {
        let mut vxs = match_or_panic!(
            mesh.remove_attribute(Mesh::ATTRIBUTE_POSITION).unwrap(),
            VertexAttributeValues::Float32x3(vxs),
            vxs
        );
        vxs.clear();
        self.grid_pos = vxs;

        let colors = mesh.remove_attribute(Mesh::ATTRIBUTE_COLOR).unwrap();
        let mut colors = match_or_panic!(colors, VertexAttributeValues::Float32x4(colors), colors);
        colors.clear();
        self.grid_color = colors;
    }

    /// Returns the last stored [`VxPos`] vector.
    #[inline]
    #[must_use]
    pub fn pop_pos(&mut self) -> Vec<VxPos> { self.pos.pop().unwrap_or(Vec::with_capacity(4)) }

    /// Returns the last stored [`VxColor`] vector.
    #[inline]
    #[must_use]
    pub fn pop_color(&mut self) -> Vec<VxColor>
    {
        self.color.pop().unwrap_or(Vec::with_capacity(4))
    }

    /// Returns the last stored [`VxColor`] vector.
    #[inline]
    #[must_use]
    pub fn pop_uv(&mut self) -> Vec<Uv> { self.uv.pop().unwrap_or(Vec::with_capacity(4)) }

    /// Stores a [`VxColor`] vector.
    #[inline]
    pub fn store_unused_color_vec(&mut self, vec: Vec<VxColor>) { self.color.push(vec); }

    /// Stores a [`Uv`] vector.
    #[inline]
    pub fn store_unused_uv_vec(&mut self, vec: Vec<Uv>) { self.uv.push(vec); }

    /// Removed the indexes values from `mesh`.
    #[inline]
    pub fn cleanup_indexes(mut mesh: Mesh)
    {
        match_or_panic!(
            return_if_none!(mesh.indices_mut()),
            Indices::U16(idxs),
            std::mem::take(idxs)
        )
        .leak();
    }
}

//=======================================================================//

/// The struct used to generate a [`Mesh`].
#[must_use]
pub(in crate::map::drawer) struct MeshGenerator<'a>(
    Vec<VxPos>,
    Vec<VxColor>,
    usize,
    Vec<Uv>,
    &'a mut DrawingResources
);

impl<'a> MeshGenerator<'a>
{
    /// Creates a new [`MeshGenerator`] for the vectors available in `parts`.
    #[inline]
    fn new(drawing_resources: &'a mut DrawingResources) -> Self
    {
        Self(
            drawing_resources.brush_meshes.parts.pop_pos(),
            drawing_resources.brush_meshes.parts.pop_color(),
            0,
            drawing_resources.brush_meshes.parts.pop_uv(),
            drawing_resources
        )
    }

    /// Adds the vertexes in `iter`.
    #[inline]
    pub fn push_positions_skewed(&mut self, grid: Grid, iter: impl IntoIterator<Item = Vec2>)
    {
        self.0
            .extend(iter.into_iter().map(|vx| grid.transform_point(vx).as_f32x3()));
    }

    #[inline]
    pub fn push_positions(&mut self, iter: impl IntoIterator<Item = Vec2>)
    {
        self.0.extend(iter.into_iter().map(IntoArray3::as_f32x3));
    }

    /// Adds the `VxColor` in `iter`.
    #[inline]
    pub fn push_colors(&mut self, iter: impl IntoIterator<Item = VxColor>) { self.1.extend(iter); }

    /// Returns the UV of the sprite.
    #[allow(clippy::cast_precision_loss)]
    #[inline]
    #[must_use]
    fn sprite_uv<T: TextureInterface + TextureInterfaceExtra>(
        &self,
        texture: &str,
        settings: &T
    ) -> [Uv; 4]
    {
        let size = self.4.texture_or_error(texture).size().as_vec2();

        let mut left = 1f32 / (size.x * settings.scale_x());
        let mut top = 1f32 / (size.y * settings.scale_y());
        let mut right = settings.scale_x().signum() + left;
        let mut bottom = settings.scale_y().signum() + top;

        if let Animation::Atlas(anim) = settings.overall_animation(self.4)
        {
            let anim_x = anim.x_partition() as f32;
            let anim_y = anim.y_partition() as f32;

            for x in [&mut left, &mut right]
            {
                *x /= anim_x;
            }

            for y in [&mut top, &mut bottom]
            {
                *y /= anim_y;
            }
        }

        [[right, top], [left, top], [left, bottom], [right, bottom]]
    }

    /// Sets the UV to the one of a sprite.
    #[inline]
    pub fn set_sprite_uv<T: TextureInterface + TextureInterfaceExtra>(
        &mut self,
        texture: &str,
        settings: &T
    )
    {
        let uvs = self.sprite_uv(texture, settings);
        self.3.extend(uvs);
    }

    /// Sets the UV to the one of an animated sprite.
    #[inline]
    pub fn set_animated_sprite_uv<T: TextureInterface + TextureInterfaceExtra>(
        &mut self,
        settings: &T,
        animator: &AtlasAnimator
    )
    {
        let pivot = animator.pivot();
        let mut uvs = self.sprite_uv(settings.name(), settings);

        for uv in &mut uvs
        {
            uv[0] += pivot[0];
            uv[1] += pivot[1];
        }

        self.3.extend(uvs);
    }

    #[allow(clippy::cast_precision_loss)]
    #[inline]
    fn thing_uv(&self, texture: &str) -> [Uv; 4]
    {
        let mut x = 1f32;
        let mut y = 1f32;

        if let Animation::Atlas(anim) = self.4.texture_or_error(texture).animation()
        {
            x /= anim.x_partition() as f32;
            y /= anim.y_partition() as f32;
        }

        [[x, 0f32], [0f32, 0f32], [0f32, y], [x, y]]
    }

    /// Sets the UV coordinates to the one of a thing.
    #[allow(clippy::cast_precision_loss)]
    #[inline]
    pub fn set_thing_uv(&mut self, texture: &str) { self.3.extend(self.thing_uv(texture)); }

    #[allow(clippy::cast_precision_loss)]
    #[inline]
    pub fn set_animated_thing_uv(&mut self, texture: &str, animator: &AtlasAnimator)
    {
        let pivot = animator.pivot();
        let mut uvs = self.thing_uv(texture);

        for uv in &mut uvs
        {
            uv[0] += pivot[0];
            uv[1] += pivot[1];
        }

        self.3.extend(uvs);
    }

    /// Sets the UV coordinates based on `f`.
    #[inline]
    fn texture_uv<T: TextureInterface, F>(
        &mut self,
        camera: &Transform,
        settings: &T,
        center: Vec2,
        elapsed_time: f32,
        parallax_enabled: bool,
        f: F
    ) where
        F: Fn([f32; 2], &Texture, &T, f32, Vec2) -> Uv
    {
        let parallax = if parallax_enabled
        {
            -(center - camera.pos()) * Vec2::new(settings.parallax_x(), settings.parallax_y())
        }
        else
        {
            Vec2::ZERO
        };
        let angle = settings.angle();
        let texture = self.4.texture_or_error(settings.name());

        if angle != 0f32
        {
            let angle = angle.to_radians();

            self.3.extend(self.0.iter().map(|vx| {
                f(
                    rotate_point_around_origin([vx[0], vx[1]].into(), angle).to_array(),
                    texture,
                    settings,
                    elapsed_time,
                    parallax
                )
            }));

            return;
        }

        self.3.extend(
            self.0
                .iter()
                .map(|vx| f([vx[0], vx[1]], texture, settings, elapsed_time, parallax))
        );
    }

    #[inline]
    #[must_use]
    fn common_uv_coordinate<T: TextureInterface>(
        vx: [f32; 2],
        texture: &Texture,
        settings: &T,
        elapsed_time: f32,
        parallax: Vec2
    ) -> Uv
    {
        let size = texture.size().as_vec2();

        [
            (vx[0] + settings.draw_offset_x() + settings.draw_scroll_x(elapsed_time) + parallax.x) /
                (size.x * settings.scale_x()),
            (-vx[1] + settings.draw_offset_y() + settings.draw_scroll_y(elapsed_time) + parallax.y) /
                (size.y * settings.scale_y())
        ]
    }

    /// Sets the texture UV.
    #[inline]
    pub fn set_texture_uv<T: TextureInterface>(
        &mut self,
        camera: &Transform,
        settings: &T,
        center: Vec2,
        elapsed_time: f32,
        parallax_enabled: bool
    )
    {
        self.texture_uv(camera, settings, center, elapsed_time, parallax_enabled, Self::common_uv_coordinate);
    }

    /// Sets the UV to the one of an animated texture.
    #[inline]
    pub fn set_animated_texture_uv<T: TextureInterface>(
        &mut self,
        camera: &Transform,
        settings: &T,
        animator: &AtlasAnimator,
        center: Vec2,
        elapsed_time: f32,
        parallax_enabled: bool
    )
    {
        /// Returns the UV coordinates of a vertex.
        #[inline]
        #[must_use]
        fn uv_coordinate<T: TextureInterface>(
            vx: [f32; 2],
            texture: &Texture,
            settings: &T,
            elapsed_time: f32,
            parallax: Vec2,
            pivot: Uv
        ) -> Uv
        {
            let [x, y] = MeshGenerator::common_uv_coordinate(vx, texture, settings, elapsed_time, parallax);
            [x + pivot[0], y + pivot[1]]
        }

        let pivot = animator.pivot();

        self.texture_uv(
            camera,
            settings,
            center,
            elapsed_time,
            parallax_enabled,
            |vx, texture, settings, elapsed_time, parallax| {
                uv_coordinate(vx, texture, settings, elapsed_time, parallax, pivot)
            }
        );
    }

    /// Sets the UV to the one of the clip texture.
    #[inline]
    pub fn clip_uv(&mut self)
    {
        self.3.extend(self.0.iter().map(|vx| [vx[0] / 64f32, -vx[1] / 64f32]));
    }

    /// Adds the indexes in `iter`.
    #[inline]
    pub fn set_indexes(&mut self, sides: usize)
    {
        assert!(sides > 2, "Sides is lower than 3.");
        assert!(sides < MAX_MESH_TRIANGLES * 3, "Too many sides.");
        self.2 = (sides - 2) * 3;
    }

    /// Generates a [`Mesh`].
    /// Gives the unused memory allocated back to `reources`.
    /// # Panics
    /// Panics if `self` has no vertexes..
    #[inline]
    #[must_use]
    pub fn mesh(self, primitive_topology: PrimitiveTopology) -> Mesh
    {
        assert!(!self.0.is_empty(), "No vertexes.");

        let mut mesh = DrawingResources::mesh(primitive_topology);

        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.0);

        if self.1.is_empty()
        {
            self.4.brush_meshes.parts.store_unused_color_vec(self.1);
        }
        else
        {
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.1);
        }

        if self.2 != 0
        {
            mesh.insert_indices(Indices::U16(unsafe {
                Vec::from_raw_parts(INDEXES.cast::<u16>(), self.2, self.2)
            }));
        }

        if self.3.is_empty()
        {
            self.4.brush_meshes.parts.store_unused_uv_vec(self.3);
        }
        else
        {
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.3);
        }

        mesh
    }

    /// Returns a [`Mesh`] representing the map grid.
    /// # Panics
    /// Panics if the values are not suitable for the creation of a grid mesh.
    /// Position and color attributes must be not empty and there must be no indexes.
    #[inline]
    #[must_use]
    pub fn grid_mesh(self) -> Mesh
    {
        assert!(!self.0.is_empty(), "Grid mesh has no vertexes.");
        assert!(!self.1.is_empty(), "Grid mesh has no colors.");
        assert!(self.2 == 0, "Grid mesh has indexes.");

        DrawingResources::mesh(PrimitiveTopology::LineList)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.0)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.1)
    }
}

//=======================================================================//

/// A wrapper to a mutable reference to a [`Texture`].
#[must_use]
pub(in crate::map) struct TextureMut<'a>
{
    /// The drawing resources.
    resources:     &'a mut DrawingResources,
    /// The name of the texture.
    name:          String,
    /// Whether the [`Texture`] had no animation when the struct was created.
    was_anim_none: bool
}

impl<'a> Deref for TextureMut<'a>
{
    type Target = Texture;

    #[inline]
    #[must_use]
    fn deref(&self) -> &Self::Target { &self.resources.textures.get(&self.name).unwrap().texture }
}

impl<'a> DerefMut for TextureMut<'a>
{
    #[inline]
    #[must_use]
    fn deref_mut(&mut self) -> &mut Self::Target
    {
        &mut self.resources.textures.get_mut(&self.name).unwrap().texture
    }
}

impl<'a> Drop for TextureMut<'a>
{
    #[inline]
    fn drop(&mut self)
    {
        if !self.dirty()
        {
            return;
        }

        self.clear_dirty_flag();
        self.resources.default_animation_changed = true;
        let is_none = self.animation().is_none();

        if self.was_anim_none
        {
            if !is_none
            {
                self.resources
                    .animated_textures
                    .asserted_insert(std::mem::take(&mut self.name));
            }
        }
        else if is_none
        {
            self.resources.animated_textures.asserted_remove(&self.name);
        }
    }
}

impl<'a> TextureMut<'a>
{
    /// Returns a new [`TextureMut`] if three is a texture named `name`.
    #[inline]
    fn new(resources: &'a mut DrawingResources, name: &str) -> Option<Self>
    {
        let was_anim_none = resources.texture(name)?.animation().is_none();

        Self {
            resources,
            name: name.to_string(),
            was_anim_none
        }
        .into()
    }
}

//=======================================================================//

#[must_use]
pub(in crate::map) struct ChunkedTextures<'a, F>
where
    F: Fn(&&'a TextureMaterials) -> bool
{
    iter:       TexturesIter<'a, F>,
    chunk_size: usize,
    container:  &'a mut HvVec<&'static TextureMaterials>
}

impl<'a, F> ChunkedTextures<'a, F>
where
    F: Fn(&&'a TextureMaterials) -> bool
{
    #[inline]
    pub fn next(&mut self) -> Option<&[&'static TextureMaterials]>
    {
        self.container.clear();

        for _ in 0..self.chunk_size
        {
            match self.iter.next()
            {
                Some(e) => self.container.push(unsafe { std::mem::transmute(e) }),
                None =>
                {
                    if self.container.is_empty()
                    {
                        return None;
                    }

                    break;
                }
            };
        }

        Some(self.container)
    }
}

//=======================================================================//

#[must_use]
enum TexturesIter<'a, F>
where
    F: Fn(&&'a TextureMaterials) -> bool
{
    Unfiltered(std::slice::Iter<'a, TextureMaterials>),
    Filtered(std::iter::Filter<std::slice::Iter<'a, TextureMaterials>, F>)
}

impl<'a, F> Iterator for TexturesIter<'a, F>
where
    F: Fn(&&'a TextureMaterials) -> bool
{
    type Item = &'a TextureMaterials;

    #[inline]
    fn next(&mut self) -> Option<Self::Item>
    {
        match self
        {
            Self::Unfiltered(i) => i.next(),
            Self::Filtered(i) => i.next()
        }
    }
}
