//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::fmt::Write;

use bevy::{
    asset::{Assets, Handle},
    color::{ColorToComponents, LinearRgba},
    ecs::{
        entity::Entity,
        query::With,
        system::{Commands, Query}
    },
    prelude::Mesh2d,
    render::{mesh::Mesh, render_resource::PrimitiveTopology},
    sprite::ColorMaterial,
    transform::components::Transform,
    window::Window
};
use bevy_egui::egui;
use glam::Vec2;
use hill_vacuum_shared::{return_if_none, NextValue};

use super::{
    animation::Animator,
    color::{Color, ColorResources},
    drawing_resources::DrawingResources,
    texture::TextureInterfaceExtra
};
use crate::{
    map::{
        editor::state::{
            clipboard::PropCameras,
            editor_state::ToolsSettings,
            grid::{Grid, GridLines},
            manager::Animators
        },
        thing::{catalog::ThingsCatalog, ThingInterface},
        MAP_SIZE
    },
    utils::{
        hull::{CircleIterator, Corner, Hull, Side},
        iterators::{PairIterator, SkipIndexIterator},
        math::points::rotate_point,
        misc::{Camera, VX_HGL_SIDE}
    },
    Animation,
    TextureInterface
};

//=======================================================================//
// CONSTANTS
//
//=======================================================================//

pub(in crate::map::drawer) const HULL_HEIGHT_LABEL: &str = "hull_height";
pub(in crate::map::drawer) const HULL_WIDTH_LABEL: &str = "hull_width";
/// The size of the tooltips' font.
const TOOLTIP_FONT_SIZE: f32 = 13f32;
/// The coefficient the tooltip's text needs to be offset to be spawned centered with respect to a
/// certain coordinate.
const TEXT_WIDTH_X_CENTER_COEFFICIENT: f32 = TOOLTIP_FONT_SIZE / 3.25;
const TOOLTIP_ROUNDING: f32 = 3f32;

//=======================================================================//
// MACROS
//
//=======================================================================//

macro_rules! unskewed_funcs {
    ($(($(^ $unskewed:ident,)? $positions:ident $(, $grid:ident)?)),+) => { paste::paste! {$(
        /// Draws the sides of a polygon.
        #[inline]
        pub fn [< $($unskewed _)? sides >](&mut self, vertexes: impl IntoIterator<Item = Vec2>, color: Color)
        {
            let mut vertexes = vertexes.into_iter();
            let mut mesh = self.resources.mesh_generator();

            let vx_0 = vertexes.next_value();
            mesh.$positions(
                $(self.$grid,)?
                Some(vx_0).into_iter().chain(vertexes).chain(Some(vx_0))
            );

            let mesh = mesh.mesh(PrimitiveTopology::LineStrip);
            self.push_mesh(mesh, self.color_resources.line_material(color), color.line_height());
        }

        /// Returns the [`Mesh`] of a polygon.
        #[inline]
        #[must_use]
        fn [< $($unskewed _)? polygon_mesh >](&mut self, vertexes: impl ExactSizeIterator<Item = Vec2>) -> Mesh
        {
            let len = vertexes.len();

            let mut mesh = self.resources.mesh_generator();
            mesh.$positions($(self.$grid,)? vertexes);
            mesh.set_indexes(len);
            mesh.mesh(PrimitiveTopology::TriangleList)
        }
    )+}};
}

//=======================================================================//
// TRAITS
//
//=======================================================================//

/// A trait for creating an array of XYZ coordinates from a value.
pub(in crate::map::drawer) trait IntoArray3
{
    /// Returns an array of three `f32` representation of `self`.
    #[allow(clippy::wrong_self_convention)]
    #[must_use]
    fn as_f32x3(self) -> [f32; 3];
}

impl IntoArray3 for (f32, f32)
{
    #[inline]
    fn as_f32x3(self) -> [f32; 3] { [self.0, self.1, 0f32] }
}

impl IntoArray3 for Vec2
{
    #[inline]
    fn as_f32x3(self) -> [f32; 3] { [self.x, self.y, 0f32] }
}

//=======================================================================//

trait AsRgba32
{
    #[must_use]
    fn as_rgba_f32(&self) -> [f32; 4];
}

impl AsRgba32 for bevy::color::Color
{
    #[inline]
    fn as_rgba_f32(&self) -> [f32; 4] { LinearRgba::from(*self).to_f32_array() }
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The position of a vertex.
pub(in crate::map::drawer) type VxPos = [f32; 3];

//=======================================================================//

/// The color of a vertex.
pub(in crate::map::drawer) type VxColor = [f32; 4];

//=======================================================================//

/// A UV coordinate.
pub(in crate::map::drawer) type Uv = [f32; 2];

//=======================================================================//

/// The struct handling all the draw calls while editing the map.
pub(in crate::map) struct EditDrawer<'w, 's, 'a>
{
    /// The [`Commands`] necessary to spawn the new [`Mesh`]es.
    commands:               &'a mut Commands<'w, 's>,
    /// The created [`Mesh`]es.
    meshes:                 &'a mut Assets<Mesh>,
    egui_context:           &'a egui::Context,
    /// The resources required to draw things.
    resources:              &'a mut DrawingResources,
    /// The color resources.
    color_resources:        &'a ColorResources,
    grid:                   &'a Grid,
    /// The scale of the current frame's camera.
    camera_scale:           f32,
    /// The time that has passed since startup.
    elapsed_time:           f32,
    /// Whether the collision overlay of the brushes should be shown.
    show_collision_overlay: bool,
    parallax_camera_pos:    Vec2,
    show_tooltips:          bool
}

impl<'w: 'a, 's: 'a, 'a> Drop for EditDrawer<'w, 's, 'a>
{
    #[inline]
    fn drop(&mut self)
    {
        const COORDINATE: f32 = crate::map::MAP_SIZE * 20f32;

        for label in self.resources.leftover_labels()
        {
            self.draw_tooltip(
                label,
                "",
                egui::Pos2::new(COORDINATE, COORDINATE),
                egui::Color32::WHITE,
                egui::Color32::WHITE
            );
        }

        self.resources.spawn_meshes(self.commands);
    }
}

impl<'w: 'a, 's: 'a, 'a> EditDrawer<'w, 's, 'a>
{
    unskewed_funcs!((push_positions_skewed, grid), (^unskewed, push_positions));

    //==============================================================
    // New

    /// Returns a new [`EditDrawer`].
    #[allow(clippy::too_many_arguments)]
    #[inline]
    #[must_use]
    pub fn new(
        commands: &'a mut Commands<'w, 's>,
        camera: &Transform,
        prop_cameras: &PropCameras,
        meshes: &'a mut Assets<Mesh>,
        meshes_query: &Query<Entity, With<Mesh2d>>,
        egui_context: &'a egui::Context,
        resources: &'a mut DrawingResources,
        color_resources: &'a ColorResources,
        settings: &ToolsSettings,
        grid: &'a Grid,
        mut elapsed_time: f32,
        paint_tool_camera_scale: f32,
        show_collision_overlay: bool,
        show_tooltips: bool
    ) -> Self
    {
        let camera_scale = camera.scale();

        resources.setup_frame(
            commands,
            prop_cameras,
            meshes,
            meshes_query,
            camera_scale,
            paint_tool_camera_scale
        );

        if !settings.scroll_enabled
        {
            elapsed_time = 0f32;
        }

        #[allow(clippy::match_bool)]
        let parallax_camera_pos = match settings.parallax_enabled
        {
            true => camera.pos(),
            false => Vec2::ZERO
        };

        Self {
            commands,
            meshes,
            egui_context,
            resources,
            color_resources,
            grid,
            camera_scale,
            elapsed_time,
            show_collision_overlay,
            parallax_camera_pos,
            show_tooltips
        }
    }

    //==============================================================
    // Resources

    #[inline]
    pub const fn resources(&self) -> &DrawingResources { self.resources }

    #[inline]
    #[must_use]
    pub const fn egui_context(&self) -> &egui::Context { self.egui_context }

    //==============================================================
    // Mesh creation

    /// Returns the [`egui::Color32`] associated with [`Color`].
    #[inline]
    #[must_use]
    pub fn egui_color(&self, color: Color) -> egui::Color32
    {
        self.color_resources.egui_color(color)
    }

    #[inline]
    #[must_use]
    pub fn tooltip_text_color(&self) -> egui::Color32 { self.color_resources.tooltip_text_color() }

    /// Queues a new [`Mesh`] to spawn.
    #[inline]
    fn push_mesh(&mut self, mesh: Mesh, material: Handle<ColorMaterial>, height: f32)
    {
        self.resources
            .push_mesh(self.meshes.add(mesh).into(), material, height);
    }

    /// Queues a new square [`Mesh`] to spawn.
    #[inline]
    fn push_square_highlight_mesh(
        &mut self,
        material: Handle<ColorMaterial>,
        center: Vec2,
        color: Color
    )
    {
        self.resources.push_square_highlight_mesh(
            material,
            self.grid.transform_point(center),
            color.square_hgl_height()
        );
    }

    /// Queues a new grid [`Mesh`] to spawn.
    #[inline]
    fn push_grid_mesh(&mut self, mesh: Mesh)
    {
        self.resources.push_grid_mesh(self.meshes.add(mesh).into());
    }

    /// Returns the [`Mesh`] of a line that goes from points `start` to `end`.
    #[inline]
    fn line_mesh(&mut self, start: Vec2, end: Vec2) -> Mesh
    {
        let mut mesh = self.resources.mesh_generator();
        mesh.push_positions_skewed(self.grid, [start, end]);
        mesh.mesh(PrimitiveTopology::LineStrip)
    }

    /// Returns a [`Mesh`] of a line with an arrow in the middle that points toward `end`.
    #[inline]
    fn arrowed_line_mesh(&mut self, start: Vec2, end: Vec2) -> Mesh
    {
        // Basic line.
        let mut mesh = self.resources.mesh_generator();
        mesh.push_positions_skewed(self.grid, [start, end]);

        // Arrow.
        let half_height = VX_HGL_SIDE * self.camera_scale;
        let mid = (start + end) / 2f32;
        let mut tip = Vec2::new(mid.x + half_height, mid.y);
        let bottom_x = mid.x - half_height;
        let mut top_left = Vec2::new(bottom_x, mid.y + half_height);
        let mut bottom_left = Vec2::new(bottom_x, mid.y - half_height);
        let angle = -(end - start).angle_to(Vec2::X);

        for vx in [&mut tip, &mut top_left, &mut bottom_left]
        {
            *vx = rotate_point(*vx, mid, angle);
        }

        mesh.push_positions_skewed(self.grid, [top_left, tip, tip, bottom_left]);
        mesh.mesh(PrimitiveTopology::LineList)
    }

    /// Returns the [`Mesh`] of a circle with `resolution` sides.
    #[inline]
    #[must_use]
    fn circle_mesh(&mut self, center: Vec2, resolution: u8, radius: f32) -> Mesh
    {
        assert!(resolution != 0, "Cannot create a circle with 0 sides.");

        let mut mesh = self.resources.mesh_generator();
        let center = self.grid.transform_point(center);
        mesh.push_positions_skewed(
            self.grid,
            DrawingResources::circle_vxs(resolution, radius).map(|vx| vx + center)
        );
        mesh.mesh(PrimitiveTopology::LineStrip)
    }

    //==============================================================
    // Info

    /// Returns the camera scale.
    #[inline]
    #[must_use]
    pub const fn camera_scale(&self) -> f32 { self.camera_scale }

    #[inline]
    pub const fn grid(&self) -> &Grid { self.grid }

    #[inline]
    #[must_use]
    pub const fn show_tooltips(&self) -> bool { self.show_tooltips }

    //==============================================================
    // Draw

    /// Draws a line.
    #[inline]
    pub fn line(&mut self, start: Vec2, end: Vec2, color: Color)
    {
        let line = self.line_mesh(start, end);
        self.push_mesh(line, self.color_resources.line_material(color), color.line_height());
    }

    #[inline]
    pub fn unskewed_line(&mut self, start: Vec2, end: Vec2, color: Color)
    {
        let mut mesh = self.resources.mesh_generator();
        mesh.push_positions([start, end]);
        let mesh = mesh.mesh(PrimitiveTopology::LineStrip);
        self.push_mesh(mesh, self.color_resources.line_material(color), color.line_height());
    }

    /// Draws a semitransparent line.
    #[inline]
    pub fn semitransparent_line(&mut self, start: Vec2, end: Vec2, color: Color)
    {
        let line = self.line_mesh(start, end);

        self.push_mesh(
            line,
            self.color_resources.semitransparent_line_material(color),
            color.line_height()
        );
    }

    /// Draws an arrowed line.
    #[inline]
    pub fn arrowed_line(&mut self, start: Vec2, end: Vec2, color: Color)
    {
        let line = self.arrowed_line_mesh(start, end);
        self.push_mesh(line, self.color_resources.line_material(color), color.line_height());
    }

    /// Draws a semitransparent arrowed line.
    #[inline]
    pub fn semitransparent_arrowed_line(&mut self, start: Vec2, end: Vec2, color: Color)
    {
        let line = self.arrowed_line_mesh(start, end);

        self.push_mesh(
            line,
            self.color_resources.semitransparent_line_material(color),
            color.line_height()
        );
    }

    /// Draws `grid`.
    #[inline]
    pub fn grid_lines(&mut self, window: &Window, camera: &Transform)
    {
        #[inline]
        fn axis_polygon(drawer: &mut EditDrawer, vertexes: impl ExactSizeIterator<Item = Vec2>)
        {
            let mesh = drawer.polygon_mesh(vertexes);
            drawer.push_mesh(
                mesh,
                drawer.color_resources.line_material(Color::OriginGridLines),
                Color::OriginGridLines.line_height()
            );
        }

        if !self.grid.visible
        {
            return;
        }

        let GridLines {
            axis,
            parallel_lines
        } = self.grid.lines(window, camera);

        // The grid lines.
        let mut mesh = self.resources.mesh_generator();

        for (start, end, color) in parallel_lines
        {
            mesh.push_positions_skewed(self.grid, [start, end]);
            mesh.push_colors([self.color_resources.bevy_color(color).as_rgba_f32(); 2]);
        }

        let mesh = mesh.grid_mesh();
        self.push_grid_mesh(mesh);

        // The x and y axis.
        let side = camera.scale() / 2f32;

        if let Some((left, right)) = axis.x
        {
            axis_polygon(
                self,
                [
                    Vec2::new(right.x, right.y + side),
                    Vec2::new(left.x, left.y + side),
                    Vec2::new(left.x, left.y - side),
                    Vec2::new(right.x, right.y - side)
                ]
                .into_iter()
            );
        }

        let (top, bottom) = return_if_none!(axis.y);

        axis_polygon(
            self,
            [
                Vec2::new(top.x + side, top.y),
                Vec2::new(top.x - side, top.y),
                Vec2::new(bottom.x - side, bottom.y),
                Vec2::new(bottom.x + side, bottom.y)
            ]
            .into_iter()
        );
    }

    /// Draws the lines returned by `lines`.
    #[inline]
    fn lines(&mut self, lines: impl Iterator<Item = (Vec2, Vec2, Color)>)
    {
        let mut mesh = self.resources.mesh_generator();
        let mut max_height = f32::MIN;

        for (start, end, color) in lines
        {
            let (color, height) = self.color_resources.line_color_height(color);
            mesh.push_positions_skewed(self.grid, [start, end]);
            mesh.push_colors([color.as_rgba_f32(); 2]);
            max_height = f32::max(max_height, height);
        }

        let mesh = mesh.mesh(PrimitiveTopology::LineList);

        self.push_mesh(mesh, self.resources.default_material(), max_height);
    }

    #[inline]
    fn infinite_line_vec(a: Vec2, b: Vec2) -> Vec2 { (b - a) * MAP_SIZE * 2f32 }

    /// Draws a line within the bounds of `window`.
    #[inline]
    pub fn infinite_line(&mut self, a: Vec2, b: Vec2, color: Color)
    {
        let vec = Self::infinite_line_vec(a, b);
        self.line(a - vec, a + vec, color);
    }

    #[inline]
    pub fn unskewed_infinite_line(&mut self, a: Vec2, b: Vec2, color: Color)
    {
        let vec = Self::infinite_line_vec(a, b);
        self.unskewed_line(a - vec, a + vec, color);
    }

    #[inline]
    pub fn hull_extensions<F, G>(
        &mut self,
        window: &Window,
        camera: &Transform,
        hull: &Hull,
        f: F,
        g: G
    ) where
        F: Fn(&Grid, Vec2) -> Vec2,
        G: Fn(&Grid, Vec2) -> Vec2
    {
        /// The color of the text of the tooltip showing the size of the hull.
        const HULL_TOOLTIP_TEXT_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 165, 0);

        let rect = hull.rectangle();

        for [a, b] in rect.pair_iter().unwrap()
        {
            self.unskewed_infinite_line(f(self.grid, *a), f(self.grid, *b), Color::HullExtensions);
        }

        let [Vec2 { x: right, y: top }, _, Vec2 { x: left, y: bottom }, _] = rect;
        let (x, y) = (
            g(self.grid, Vec2::new((left + right) / 2f32, top)),
            g(self.grid, Vec2::new(right, (bottom + top) / 2f32))
        );

        let mut value = format!("{}", hull.height());

        self.draw_tooltip_y_centered(
            window,
            camera,
            HULL_HEIGHT_LABEL,
            value.as_str(),
            y,
            Vec2::new(4f32, -4f32),
            HULL_TOOLTIP_TEXT_COLOR,
            egui::Color32::from_black_alpha(0)
        );

        value.clear();
        write!(&mut value, "{}", hull.width()).ok();

        self.draw_tooltip_x_centered_above_pos(
            window,
            camera,
            HULL_WIDTH_LABEL,
            value.as_str(),
            x,
            Vec2::new(0f32, -4f32),
            HULL_TOOLTIP_TEXT_COLOR,
            egui::Color32::from_black_alpha(0)
        );
    }

    /// Draws a circle.
    #[inline]
    pub fn circle(&mut self, center: Vec2, resolution: u8, radius: f32, color: Color)
    {
        let mesh = self.circle_mesh(center, resolution, radius);
        self.push_mesh(mesh, self.color_resources.line_material(color), color.line_height());
    }

    /// Draws `hull`.
    #[inline]
    pub fn hull(&mut self, hull: &Hull, color: Color) { self.sides(hull.vertexes(), color); }

    /// Draws `hull` with corners highlights. The selected [`Corner`] is drawn with `hgl_color`.
    #[inline]
    pub fn hull_with_corner_highlights(
        &mut self,
        hull: &Hull,

        corner: Corner,
        color: Color,
        hgl_color: Color
    )
    {
        for vx in hull.corners().skip_index(corner as usize).unwrap().map(|(_, vx)| vx)
        {
            self.square_highlight(vx, color);
        }

        self.square_highlight(hull.corner_vertex(corner), hgl_color);
        self.sides(hull.vertexes(), color);
    }

    /// Draws `hull` with an highlighted side.
    #[inline]
    pub fn hull_with_highlighted_side(
        &mut self,
        hull: &Hull,
        side: Side,
        color: Color,
        hgl_color: Color
    )
    {
        let hgl_side = hull.side_segment(side);
        self.line(hgl_side[0], hgl_side[1], hgl_color);
        self.sides(hull.vertexes(), color);
    }

    /// Draws a square.
    #[inline]
    pub fn square_highlight(&mut self, center: Vec2, color: Color)
    {
        self.push_square_highlight_mesh(self.color_resources.line_material(color), center, color);
    }

    /// Draws the pivot of a [`Prop`].
    #[inline]
    pub fn prop_pivot(&mut self, center: Vec2, color: Color, camera_id: Option<Entity>)
    {
        self.resources.push_prop_pivot_mesh(
            self.color_resources.line_material(color),
            self.grid.transform_point(center),
            color.square_hgl_height(),
            camera_id
        );
    }

    /// Draws a semitransparent square.
    #[inline]
    pub fn semitransparent_square_highlight(&mut self, center: Vec2, color: Color)
    {
        self.push_square_highlight_mesh(
            self.color_resources.semitransparent_line_material(color),
            center,
            color
        );
    }

    /// Draws a brush attachment highlight.
    #[inline]
    pub fn attachment_highlight(&mut self, center: Vec2, color: Color)
    {
        self.resources.push_attachment_highlight_mesh(
            self.color_resources.line_material(color),
            self.grid.transform_point(center),
            color.square_hgl_height()
        );
    }

    /// Draws a sprite highlight.
    #[inline]
    pub fn sprite_highlight(&mut self, center: Vec2, color: Color)
    {
        self.resources.push_sprite_highlight_mesh(
            self.color_resources.line_material(color),
            self.grid.transform_point(center),
            color.square_hgl_height()
        );
    }

    /// Draws the collision overlay.
    #[inline]
    fn collision_overlay(&mut self, vertexes: impl ExactSizeIterator<Item = Vec2>)
    {
        let mut mesh_generator = self.resources.mesh_generator();
        mesh_generator.set_indexes(vertexes.len());
        mesh_generator.push_positions_skewed(self.grid, vertexes);
        mesh_generator.clip_uv();
        let mesh = mesh_generator.mesh(PrimitiveTopology::TriangleList);

        self.push_mesh(mesh, self.resources.clip_texture(), Color::clip_height());
    }

    /// Draws `settings` mapped to `vertexes`.
    #[inline]
    fn polygon_texture<T: TextureInterface>(
        &mut self,
        camera_pos: Vec2,
        vertexes: impl ExactSizeIterator<Item = Vec2> + Clone,
        color: Color,
        settings: &T
    )
    {
        let mut mesh_generator = self.resources.mesh_generator();
        mesh_generator.set_indexes(vertexes.len());
        mesh_generator.push_positions_skewed(self.grid, vertexes);
        mesh_generator.set_texture_uv(camera_pos, settings, self.elapsed_time);
        let mesh = mesh_generator.mesh(PrimitiveTopology::TriangleList);

        self.resources
            .push_textured_mesh(self.meshes.add(mesh).into(), settings, color);
    }

    /// Draws `settings` as a brush.
    #[inline]
    pub fn sideless_brush<T: TextureInterface>(
        &mut self,
        vertexes: impl ExactSizeIterator<Item = Vec2> + Clone,
        color: Color,
        texture: Option<&T>,
        collision: bool
    )
    {
        if self.show_collision_overlay && collision
        {
            self.collision_overlay(vertexes.clone());
        }

        if let Some(texture) = texture
        {
            if !texture.sprite()
            {
                self.polygon_texture(self.parallax_camera_pos, vertexes.clone(), color, texture);
            }
        }

        let mesh = self.polygon_mesh(vertexes);
        self.push_mesh(mesh, self.color_resources.polygon_material(color), color.polygon_height());
    }

    /// Draws `settings` as a brush also drawing the sides.
    #[inline]
    pub fn brush<T: TextureInterface>(
        &mut self,
        vertexes: impl ExactSizeIterator<Item = Vec2> + Clone,
        color: Color,
        texture: Option<&T>,
        collision: bool
    )
    {
        self.sides(vertexes.clone(), color);
        self.sideless_brush(vertexes, color, texture, collision);
    }

    /// Draws a polygon filled with a solid color.
    #[inline]
    pub fn polygon_with_solid_color(
        &mut self,
        vertexes: impl ExactSizeIterator<Item = Vec2>,
        color: Color
    )
    {
        let mesh = self.polygon_mesh(vertexes);
        self.push_mesh(mesh, self.color_resources.line_material(color), color.polygon_height());
    }

    /// Draws `settings` mapping the texture to `sides` and also drawing colored lines at the sides.
    #[inline]
    pub fn brush_with_sides_colors<T: TextureInterface>(
        &mut self,
        sides: impl ExactSizeIterator<Item = (Vec2, Vec2, Color)> + Clone,
        body_color: Color,
        texture: Option<&T>,
        collision: bool
    )
    {
        self.lines(sides.clone());
        self.sideless_brush(sides.map(|(vx, ..)| vx), body_color, texture, collision);
    }

    /// Draws `settings` as a sprite.
    #[inline]
    pub fn sprite<T: TextureInterface + TextureInterfaceExtra>(
        &mut self,
        brush_center: Vec2,
        settings: &T,
        color: Color,
        show_outline: bool
    )
    {
        let vxs = settings.sprite_vxs(self.resources, self.grid, brush_center).unwrap();

        let mut mesh_generator = self.resources.mesh_generator();
        mesh_generator.set_indexes(4);
        mesh_generator.push_positions(vxs.iter().copied());
        mesh_generator.set_sprite_uv(settings);
        let mesh = mesh_generator.mesh(PrimitiveTopology::TriangleList);

        self.resources
            .push_sprite(self.meshes.add(mesh).into(), settings, color);

        if !show_outline
        {
            return;
        }

        self.unskewed_sides(vxs.iter().copied(), color);

        let mesh = self.unskewed_polygon_mesh(vxs.into_iter());
        self.push_mesh(mesh, self.color_resources.polygon_material(color), color.polygon_height());
    }

    /// Draws `thing`.
    #[allow(clippy::cast_precision_loss)]
    #[inline]
    pub fn thing<T: ThingInterface>(&mut self, catalog: &ThingsCatalog, thing: &T, color: Color)
    {
        /// The resolution of the corners of the [`ThingOutline`].
        const CORNER_RESOLUTION: u8 = 6;

        /// The steps to draw a rectangle with smoothed corners.
        #[derive(Clone, Copy)]
        enum SmoothRectangleSteps
        {
            /// Drawing the top left corner.
            TopLeftCorner(u8),
            /// Drawing the bottom left corner.
            BottomLeftCorner(u8),
            /// Drawing the bottom right corner.
            BottomRightCorner(u8),
            /// Drawing the top right corner.
            TopRightCorner(u8),
            /// Drawing the line going from the top right corner to the first point.
            Last,
            /// No more drawing.
            Finished
        }

        impl SmoothRectangleSteps
        {
            /// Returns the next point of the outline.
            #[inline]
            fn next(&mut self, iter: &mut CircleIterator)
            {
                /// Progresses the iteration.
                macro_rules! countdown {
                    ($res:ident, $next:expr) => {{
                        *$res -= 1;

                        if *$res != 0
                        {
                            return;
                        }

                        iter.regress();
                        $next
                    }};
                }

                *self = match self
                {
                    Self::TopLeftCorner(res) =>
                    {
                        countdown!(res, Self::BottomLeftCorner(CORNER_RESOLUTION))
                    },
                    Self::BottomLeftCorner(res) =>
                    {
                        countdown!(res, Self::BottomRightCorner(CORNER_RESOLUTION))
                    },
                    Self::BottomRightCorner(res) =>
                    {
                        countdown!(res, Self::TopRightCorner(CORNER_RESOLUTION - 1))
                    },
                    Self::TopRightCorner(res) => countdown!(res, Self::Last),
                    Self::Last => Self::Finished,
                    Self::Finished => panic!("Smoothed rectangle steps already finished.")
                };
            }
        }

        /// The outline of a [`ThingInstance`] showing its bounding box.
        #[must_use]
        #[derive(Clone, Copy)]
        struct ThingOutline
        {
            /// The horizontal distance between two corners.
            x_delta:     f32,
            /// The vertical distance between two corners.
            y_delta:     f32,
            /// The points of the corners.
            circle_iter: CircleIterator,
            /// The draw progress.
            step:        SmoothRectangleSteps
        }

        impl ThingOutline
        {
            /// Returns a new [`ThingOutline`].
            #[inline]
            fn new<T: ThingInterface>(catalog: &ThingsCatalog, thing: &T) -> Self
            {
                let hull = thing.thing_hull(catalog);
                let (width, height) = hull.dimensions();
                let ray = (width.min(height) / 8f32).min(24f32);
                let x_delta = width / 2f32 - ray;
                let y_delta = height / 2f32 - ray;
                let center = hull.center();
                let circle_iter =
                    Hull::new(center.y + ray, center.y - ray, center.x - ray, center.x + ray)
                        .unwrap()
                        .circle(CORNER_RESOLUTION * 4 - 4);

                Self {
                    x_delta,
                    y_delta,
                    circle_iter,
                    step: SmoothRectangleSteps::TopLeftCorner(CORNER_RESOLUTION)
                }
            }
        }

        impl ExactSizeIterator for ThingOutline
        {
            #[inline]
            fn len(&self) -> usize
            {
                let len = self.circle_iter.len();
                len + len.div_ceil(CORNER_RESOLUTION as usize - 1)
            }
        }

        #[allow(clippy::copy_iterator)]
        impl Iterator for ThingOutline
        {
            type Item = Vec2;

            #[inline]
            fn next(&mut self) -> Option<Self::Item>
            {
                let pos = match self.step
                {
                    SmoothRectangleSteps::TopLeftCorner(_) |
                    SmoothRectangleSteps::BottomLeftCorner(_) |
                    SmoothRectangleSteps::BottomRightCorner(_) |
                    SmoothRectangleSteps::TopRightCorner(_) => self.circle_iter.next_value(),
                    SmoothRectangleSteps::Last => self.circle_iter.starting_point(),
                    SmoothRectangleSteps::Finished => return None
                };

                let pos = pos +
                    match self.step
                    {
                        SmoothRectangleSteps::TopLeftCorner(_) =>
                        {
                            Vec2::new(-self.x_delta, self.y_delta)
                        },
                        SmoothRectangleSteps::BottomLeftCorner(_) =>
                        {
                            Vec2::new(-self.x_delta, -self.y_delta)
                        },
                        SmoothRectangleSteps::BottomRightCorner(_) =>
                        {
                            Vec2::new(self.x_delta, -self.y_delta)
                        },
                        SmoothRectangleSteps::TopRightCorner(_) | SmoothRectangleSteps::Last =>
                        {
                            Vec2::new(self.x_delta, self.y_delta)
                        },
                        SmoothRectangleSteps::Finished => unreachable!()
                    };

                self.step.next(&mut self.circle_iter);

                pos.into()
            }
        }

        // Sides and overlay.
        let iter = ThingOutline::new(catalog, thing);
        self.sides(iter, color);
        let mesh = self.polygon_mesh(iter);
        self.push_mesh(mesh, self.color_resources.polygon_material(color), color.entity_height());

        // Angle indicator.
        let preview = catalog.thing_or_error(thing.thing_id()).preview();
        let angle = thing.angle_f32().to_radians();
        let hull = thing.thing_hull(catalog);
        let half_side = (hull.width().min(hull.height()) / 2f32).min(64f32);
        let mut center = self.grid.transform_point(hull.center());

        if self.grid.isometric()
        {
            center.y += (self.resources.texture_or_error(preview).size().y / 2) as f32;
        }

        let hull = Hull::new(
            center.y + half_side,
            center.y - half_side,
            center.x - half_side,
            center.x + half_side
        )
        .unwrap();

        let half_width = hull.half_width();
        let distance = half_width * 0.75;
        let arrow_width = half_width * 0.0625;
        let v0 = Vec2::new(center.x + distance, center.y);
        let v1 = Vec2::new(center.x, center.y + distance);
        let v2 = Vec2::new(center.x, center.y - distance);
        let [black_height, white_height] = Color::thing_angle_indicator_height();

        for (s, height, material) in [
            (arrow_width * 2f32, black_height, self.color_resources.solid_white()),
            (arrow_width, white_height, self.color_resources.solid_black())
        ]
        {
            let v0_left = Vec2::new(v0.x - s, v0.y);
            let v0_right = Vec2::new(v0.x + s, v0.y);

            let mut top_wing = [
                Vec2::new(v1.x, v1.y + s),
                Vec2::new(v1.x - s, v1.y),
                v0_left,
                v0_right
            ];
            let mut bottom_wing = [
                Vec2::new(v2.x - s, v2.y),
                Vec2::new(v2.x, v2.y - s),
                v0_right,
                v0_left
            ];

            for vxs in [&mut top_wing, &mut bottom_wing]
            {
                for vx in &mut *vxs
                {
                    *vx = rotate_point(*vx, center, angle);
                }

                let mut mesh_generator = self.resources.mesh_generator();
                mesh_generator.set_indexes(4);
                mesh_generator.push_positions(vxs.iter().copied());
                let mesh = mesh_generator.mesh(PrimitiveTopology::TriangleList);
                self.push_mesh(mesh, material.clone_weak(), height);
            }
        }

        // Texture
        self.thing_texture(catalog, thing, color);
    }

    #[inline]
    pub fn thing_texture<T: ThingInterface>(
        &mut self,
        catalog: &ThingsCatalog,
        thing: &T,
        color: Color
    )
    {
        let preview = catalog.thing_or_error(thing.thing_id()).preview();
        let vxs = thing_texture_hull(self.resources, self.grid, thing, preview);

        let mut mesh_generator = self.resources.mesh_generator();
        mesh_generator.set_indexes(4);
        mesh_generator.push_positions(vxs.rectangle());
        mesh_generator.set_thing_uv(preview);
        let mesh = mesh_generator.mesh(PrimitiveTopology::TriangleList);

        self.resources
            .push_thing(self.meshes.add(mesh).into(), catalog, thing, color);
    }

    //==============================================================
    // Tooltips

    /// Returns a static `str` to be used as tooltip label for `pos`.
    #[inline]
    #[must_use]
    pub fn vx_tooltip_label(&mut self, pos: Vec2) -> Option<&'static str>
    {
        if !self.show_tooltips
        {
            return None;
        }

        self.resources.vx_tooltip_label(pos)
    }

    #[inline]
    #[must_use]
    pub fn tooltip_label(&mut self) -> Option<&'static str>
    {
        if !self.show_tooltips
        {
            return None;
        }

        self.resources.tooltip_label()
    }

    /// Draws a tooltip an position 'pos'.
    #[inline]
    pub fn draw_tooltip(
        &self,
        label: &'static str,
        text: &str,
        pos: egui::Pos2,
        text_color: egui::Color32,
        fill_color: egui::Color32
    )
    {
        egui::Area::new(label.into())
            .fixed_pos(pos)
            .order(egui::Order::Background)
            .constrain(false)
            .movable(false)
            .show(self.egui_context, |ui| {
                egui::Frame::NONE
                    .fill(fill_color)
                    .inner_margin(TOOLTIP_ROUNDING)
                    .outer_margin(0f32)
                    .corner_radius(TOOLTIP_ROUNDING)
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(text)
                                    .color(text_color)
                                    .text_style(egui::TextStyle::Monospace)
                                    .size(TOOLTIP_FONT_SIZE)
                            )
                            .extend()
                        );
                    });
            });
    }

    /// Returns the amount a tooltip needs to be horizontally offset to be centered with respect to
    /// a certain coordinate.
    #[allow(clippy::cast_precision_loss)]
    #[inline]
    #[must_use]
    fn x_center_text_offset(text: &str) -> f32
    {
        text.len() as f32 * TEXT_WIDTH_X_CENTER_COEFFICIENT
    }

    /// Draws a tooltip with center latitude equal to `pos.y`.
    #[inline]
    pub fn draw_tooltip_y_centered(
        &self,
        window: &Window,
        camera: &Transform,
        label: &'static str,
        text: &str,
        pos: Vec2,
        mut offset: Vec2,
        text_color: egui::Color32,
        fill_color: egui::Color32
    )
    {
        offset.y -= TOOLTIP_FONT_SIZE / 1.75;

        self.draw_tooltip(
            label,
            text,
            camera.to_egui_coordinates(window, self.grid, pos) + egui::vec2(offset.x, offset.y),
            text_color,
            fill_color
        );
    }

    /// Draws a tooltip with center at longitude `pos.x` with the bottom
    /// of the frame lying right above `pos.y`.
    #[inline]
    pub fn draw_tooltip_x_centered_above_pos(
        &self,
        window: &Window,
        camera: &Transform,
        label: &'static str,
        text: &str,
        pos: Vec2,
        offset: Vec2,
        text_color: egui::Color32,
        fill_color: egui::Color32
    )
    {
        self.draw_tooltip(
            label,
            text,
            camera.to_egui_coordinates(window, self.grid, pos) +
                egui::vec2(
                    offset.x - Self::x_center_text_offset(text) - TOOLTIP_ROUNDING,
                    offset.y - TOOLTIP_FONT_SIZE - TOOLTIP_ROUNDING * 2f32
                ),
            text_color,
            fill_color
        );
    }
}

//=======================================================================//

/// The struct handling all the draw calls during the map preview.
pub(in crate::map) struct MapPreviewDrawer<'w, 's, 'a>
{
    /// The [`Commands`] necessary to spawn the new [`Mesh`]es.
    commands:     &'a mut Commands<'w, 's>,
    /// The created [`Mesh`]es.
    meshes:       &'a mut Assets<Mesh>,
    /// The resources required to draw things.
    resources:    &'a mut DrawingResources,
    grid:         &'a Grid,
    /// The time that has passed.
    elapsed_time: f32
}

impl<'w: 'a, 's: 'a, 'a> Drop for MapPreviewDrawer<'w, 's, 'a>
{
    #[inline]
    fn drop(&mut self) { self.resources.spawn_meshes(self.commands); }
}

impl<'w: 'a, 's: 'a, 'a> MapPreviewDrawer<'w, 's, 'a>
{
    /// Returns a new [`MapPreviewDrawer`].
    #[inline]
    #[must_use]
    pub fn new(
        commands: &'a mut Commands<'w, 's>,
        prop_cameras: &PropCameras,
        meshes: &'a mut Assets<Mesh>,
        meshes_query: &Query<Entity, With<Mesh2d>>,
        resources: &'a mut DrawingResources,
        grid: &'a Grid,
        elapsed_time: f32
    ) -> Self
    {
        resources.setup_frame(commands, prop_cameras, meshes, meshes_query, 1f32, 1f32);

        Self {
            commands,
            meshes,
            resources,
            grid,
            elapsed_time
        }
    }

    #[inline]
    pub const fn grid(&self) -> &Grid { self.grid }

    /// Draws `settings` mapping the texture to `vertexes`.
    #[inline]
    pub fn brush<T: TextureInterface + TextureInterfaceExtra>(
        &mut self,
        camera: &Transform,
        vertexes: impl ExactSizeIterator<Item = Vec2> + Clone,
        animator: Option<&Animator>,
        settings: &T
    )
    {
        let resources = unsafe { std::ptr::from_mut(self.resources).as_mut().unwrap() };

        let mut mesh_generator = resources.mesh_generator();
        mesh_generator.set_indexes(vertexes.len());
        mesh_generator.push_positions_skewed(self.grid, vertexes);

        let texture = match animator
        {
            Some(animator) =>
            {
                match animator
                {
                    Animator::List(animator) =>
                    {
                        let materials = animator.texture(
                            self.resources,
                            settings.overall_animation(self.resources).get_list_animation()
                        );
                        mesh_generator.set_texture_uv(camera.pos(), settings, self.elapsed_time);

                        materials
                    },
                    Animator::Atlas(animator) =>
                    {
                        mesh_generator.set_animated_texture_uv(
                            camera.pos(),
                            settings,
                            animator,
                            self.elapsed_time
                        );

                        self.resources.texture_materials(settings.name())
                    }
                }
            },
            None =>
            {
                let texture = self.resources.texture_or_error(settings.name());
                mesh_generator.set_texture_uv(camera.pos(), settings, self.elapsed_time);
                self.resources.texture_materials(texture.name())
            }
        };

        let mesh = mesh_generator.mesh(PrimitiveTopology::TriangleList);
        resources.push_map_preview_textured_mesh(self.meshes.add(mesh).into(), texture, settings);
    }

    /// Draws `settings` as a sprite.
    #[inline]
    pub fn sprite<T: TextureInterface + TextureInterfaceExtra>(
        &mut self,
        brush_center: Vec2,
        animator: Option<&Animator>,
        settings: &T
    )
    {
        let vxs = settings
            .animated_sprite_vxs(self.resources, self.grid, animator, brush_center)
            .unwrap();
        let resources = unsafe { std::ptr::from_mut(self.resources).as_mut().unwrap() };

        let mut mesh_generator = resources.mesh_generator();
        mesh_generator.set_indexes(4);

        let texture = match animator
        {
            Some(animator) =>
            {
                match animator
                {
                    Animator::List(animator) =>
                    {
                        let materials = animator.texture(
                            self.resources,
                            settings.overall_animation(self.resources).get_list_animation()
                        );
                        mesh_generator.set_sprite_uv(settings);
                        materials
                    },
                    Animator::Atlas(animator) =>
                    {
                        mesh_generator.set_animated_sprite_uv(settings, animator);
                        self.resources.texture_materials(settings.name())
                    }
                }
            },
            None =>
            {
                let texture = self.resources.texture_or_error(settings.name()).name();
                mesh_generator.set_sprite_uv(settings);
                self.resources.texture_materials(texture)
            }
        };

        mesh_generator.push_positions(vxs);

        let mesh = mesh_generator.mesh(PrimitiveTopology::TriangleList);
        resources.push_map_preview_sprite(self.meshes.add(mesh).into(), texture, settings);
    }

    /// Draws `thing`.
    #[inline]
    pub fn thing<T: ThingInterface>(
        &mut self,
        catalog: &ThingsCatalog,
        thing: &T,
        animators: &Animators
    )
    {
        let texture = catalog.texture(thing.thing_id());
        let resources = unsafe { std::ptr::from_mut(self.resources).as_mut().unwrap() };
        let mut mesh_generator = resources.mesh_generator();

        let texture = match animators.get_thing_animator(texture)
        {
            Some(animator) =>
            {
                match animator
                {
                    Animator::List(animator) =>
                    {
                        let materials = animator.texture(
                            self.resources,
                            self.resources
                                .texture(texture)
                                .unwrap()
                                .animation()
                                .get_list_animation()
                        );

                        mesh_generator.push_positions(
                            thing_texture_hull(
                                self.resources,
                                self.grid,
                                thing,
                                materials.texture().name()
                            )
                            .vertexes()
                        );

                        mesh_generator.set_thing_uv(texture);
                        materials
                    },
                    Animator::Atlas(animator) =>
                    {
                        mesh_generator.push_positions(
                            thing_texture_hull(self.resources, self.grid, thing, texture)
                                .vertexes()
                        );

                        mesh_generator.set_animated_thing_uv(texture, animator);
                        self.resources.texture_materials(texture)
                    }
                }
            },
            None =>
            {
                mesh_generator.push_positions(
                    thing_texture_hull(
                        self.resources,
                        self.grid,
                        thing,
                        catalog.texture(thing.thing_id())
                    )
                    .vertexes()
                );
                mesh_generator.set_thing_uv(texture);
                self.resources.texture_materials(texture)
            }
        };

        mesh_generator.set_indexes(4);
        let mesh = mesh_generator.mesh(PrimitiveTopology::TriangleList);

        resources.push_map_preview_thing(self.meshes.add(mesh).into(), texture, thing);
    }
}

//=======================================================================//
// FUNCTIONS
//
//=======================================================================//

#[inline]
#[allow(clippy::cast_precision_loss)]
pub(in crate::map::drawer) fn thing_texture_hull<T: ThingInterface>(
    resources: &DrawingResources,
    grid: &Grid,
    thing: &T,
    texture: &str
) -> Hull
{
    let texture = resources.texture_or_error(texture);
    let mut vxs = texture.hull();

    if let Animation::Atlas(anim) = texture.animation()
    {
        let half_width = (vxs.width() / anim.x_partition() as f32) / 2f32;
        let half_height = (vxs.height() / anim.y_partition() as f32) / 2f32;

        vxs = Hull::from_opposite_vertexes(
            Vec2::new(half_width, half_height),
            Vec2::new(-half_width, -half_height)
        );
    }

    vxs += grid.transform_point(thing.pos());
    let y_offset = vxs.half_height();

    if grid.isometric()
    {
        vxs += Vec2::new(0f32, y_offset);
    }

    vxs
}
