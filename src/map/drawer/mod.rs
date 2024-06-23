pub mod animation;
pub(crate) mod color;
pub(in crate::map) mod drawing_resources;
pub mod texture;
pub(in crate::map) mod texture_loader;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::fmt::Write;

use bevy::{prelude::*, render::render_resource::PrimitiveTopology, sprite::Mesh2dHandle};
use bevy_egui::egui;
use hill_vacuum_shared::{return_if_none, NextValue};

use self::{
    animation::Animator,
    color::{Color, ColorResources},
    drawing_resources::DrawingResources,
    texture::{TextureInterface, TextureInterfaceExtra}
};
use super::{
    editor::state::{clipboard::PropCameras, editor_state::ToolsSettings, grid::Grid},
    thing::{catalog::ThingsCatalog, ThingInterface}
};
use crate::utils::{
    hull::{CircleIterator, Corner, EntityHull, Hull, Side},
    iterators::SkipIndexIterator,
    math::{
        lines_and_segments::{line_equation, LineEquation},
        points::rotate_point
    },
    misc::{Camera, VX_HGL_SIDE},
    tooltips::{draw_tooltip_x_centered_above_pos, draw_tooltip_y_centered, to_egui_coordinates}
};

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
// TYPES
//
//=======================================================================//

/// The position of a vertex.
type VxPos = [f32; 3];

//=======================================================================//

/// The color of a vertex.
type VxColor = [f32; 4];

//=======================================================================//

/// A UV coordinate.
type Uv = [f32; 2];

//=======================================================================//

/// The struct handling all the draw calls while editing the map.
pub(in crate::map) struct EditDrawer<'w, 's, 'a>
{
    /// The [`Commands`] necessary to spawn the new [`Mesh`]es.
    commands:               &'a mut Commands<'w, 's>,
    /// The created [`Mesh`]es.
    meshes:                 &'a mut Assets<Mesh>,
    /// The resources required to draw things.
    resources:              &'a mut DrawingResources,
    /// The color resources.
    color_resources:        &'a ColorResources,
    /// The scale of the current frame's camera.
    camera_scale:           f32,
    /// The time that has passed since startup.
    elapsed_time:           f32,
    /// Whever the collision overlay of the [`Brush`]es should be shown.
    show_collision_overlay: bool,
    /// Whever parallax is enabled.
    parallax_enabled:       bool
}

impl<'w: 'a, 's: 'a, 'a> Drop for EditDrawer<'w, 's, 'a>
{
    #[inline]
    fn drop(&mut self) { self.spawn_meshes() }
}

impl<'w: 'a, 's: 'a, 'a> EditDrawer<'w, 's, 'a>
{
    //==============================================================
    // Info

    /// Returns the camera scale.
    #[inline]
    #[must_use]
    pub const fn camera_scale(&self) -> f32 { self.camera_scale }

    //==============================================================
    // Draw

    /// Spawns the meshes.
    #[inline]
    fn spawn_meshes(&mut self) { self.resources.spawn_meshes(self.commands); }

    /// Draws a line.
    #[inline]
    pub fn line(&mut self, start: Vec2, end: Vec2, color: Color)
    {
        let line = self.line_mesh(start, end);
        self.push_mesh(line, self.color_resources.line_material(color), color.line_height());
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

    /// Draws the sides of a polygon.
    #[inline]
    pub fn sides(&mut self, mut vertexes: impl Iterator<Item = Vec2>, color: Color)
    {
        let mut mesh = self.resources.mesh_generator();

        let vx_0 = vertexes.next_value();
        mesh.push_positions(Some(vx_0).into_iter().chain(vertexes).chain(Some(vx_0)));

        let mesh = mesh.mesh(PrimitiveTopology::LineStrip);

        self.push_mesh(mesh, self.color_resources.line_material(color), color.line_height());
    }

    /// Draws `grid`.
    #[inline]
    pub fn grid(&mut self, grid: Grid, window: &Window, camera: &Transform)
    {
        let (grid, axis) = grid.lines(window, camera);

        // The grid lines.
        let mut mesh = self.resources.mesh_generator();

        for (start, end, color) in grid
        {
            mesh.push_positions([start, end]);
            mesh.push_colors([self.color_resources.bevy_color(color).as_rgba_f32(); 2]);
        }

        let mesh = mesh.grid_mesh();
        self.push_grid_mesh(mesh);

        // The x and y axis.
        let side = camera.scale() / 3f32;

        if let Some((a, b)) = axis.x
        {
            self.polygon(
                Hull::new(a.y + side, a.y - side, a.x, b.x).vertexes(),
                Color::OriginGridLines
            );
        }

        let (a, b) = return_if_none!(axis.y);

        self.polygon(
            Hull::new(a.y, b.y, a.x - side, a.x + side).vertexes(),
            Color::OriginGridLines
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
            mesh.push_positions([start, end]);
            mesh.push_colors([color.as_rgba_f32(); 2]);
            max_height = f32::max(max_height, height);
        }

        let mesh = mesh.mesh(PrimitiveTopology::LineList);

        self.push_mesh(mesh, self.resources.default_material(), max_height);
    }

    /// Draws a line within the bounds of `window`.
    #[inline]
    pub fn line_within_window_bounds(
        &mut self,
        window: &Window,
        camera: &Transform,
        points: (Vec2, Vec2),
        color: Color
    )
    {
        let (half_width, half_height) = camera.scaled_window_half_sizes(window);
        let camera_pos = camera.pos();

        // Draw line passing through the two points.
        let [start, end] = match line_equation(&points.into())
        {
            LineEquation::Horizontal(y) =>
            {
                [
                    Vec2::new(camera_pos.x - half_width, y),
                    Vec2::new(camera_pos.x + half_width, y)
                ]
            },
            LineEquation::Vertical(x) =>
            {
                [
                    Vec2::new(x, camera_pos.y + half_height),
                    Vec2::new(x, camera_pos.y - half_height)
                ]
            },
            LineEquation::Generic(m, q) =>
            {
                let left_border = camera_pos.x - half_width;
                let right_border = camera_pos.x + half_width;
                let bottom_border = camera_pos.y - half_height;
                let top_border = camera_pos.y + half_height;

                let mut j = 0;
                let mut screen_intersections = [None, None];

                for x in [left_border, right_border]
                {
                    let y = m * x + q;

                    if y <= top_border && y >= bottom_border
                    {
                        screen_intersections[j] = Vec2::new(x, y).into();
                        j += 1;
                    }
                }

                for y in [top_border, bottom_border]
                {
                    if j >= 2
                    {
                        break;
                    }

                    let x = (y - q) / m;

                    if x <= right_border && x >= left_border
                    {
                        screen_intersections[j] = Vec2::new(x, y).into();
                        j += 1;
                    }
                }

                [
                    screen_intersections[0].unwrap(),
                    screen_intersections[1].unwrap()
                ]
            }
        };

        self.line(start, end, color);
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

    /// Draws the line extensions of `hull`.
    #[inline]
    pub fn hull_extensions(
        &mut self,
        hull: &Hull,
        window: &Window,
        camera: &Transform,
        egui_context: &egui::Context
    )
    {
        /// The color of the text of the tooltip showing the size of the hull.
        const TOOLTIP_TEXT_COLOR: egui::Color32 = egui::Color32::from_rgb(255, 165, 0);

        let window_hull = camera.viewport_ui_constricted(window);

        for x in [hull.left(), hull.right()]
        {
            self.line(
                Vec2::new(x, window_hull.bottom()),
                Vec2::new(x, window_hull.top()),
                Color::HullExtensions
            );
        }

        for y in [hull.top(), hull.bottom()]
        {
            self.line(
                Vec2::new(window_hull.left(), y),
                Vec2::new(window_hull.right(), y),
                Color::HullExtensions
            );
        }

        let mut value = format!("{}", hull.height());

        draw_tooltip_y_centered(
            egui_context,
            "hull_height",
            egui::Order::Background,
            value.as_str(),
            egui::TextStyle::Monospace,
            to_egui_coordinates(
                Vec2::new(hull.right(), (hull.bottom() + hull.top()) / 2f32),
                window,
                camera
            ),
            egui::Vec2::new(4f32, 0f32),
            TOOLTIP_TEXT_COLOR,
            egui::Color32::from_black_alpha(0),
            0f32
        );

        value.clear();
        write!(&mut value, "{}", hull.width()).ok();

        draw_tooltip_x_centered_above_pos(
            egui_context,
            "hull_width",
            egui::Order::Background,
            value.as_str(),
            egui::TextStyle::Monospace,
            to_egui_coordinates(
                Vec2::new((hull.left() + hull.right()) / 2f32, hull.top()),
                window,
                camera
            ),
            egui::Vec2::new(0f32, -4f32),
            TOOLTIP_TEXT_COLOR,
            egui::Color32::from_black_alpha(0),
            0f32
        );
    }

    /// Draws a square.
    #[inline]
    pub fn square_highlight(&mut self, center: Vec2, color: Color)
    {
        self.push_square_highlight_mesh(self.color_resources.line_material(color), center, color);
    }

    /// Draws the pivot of a [`Prop`].
    #[inline]
    pub fn prop_pivot(
        &mut self,
        center: Vec2,
        color: Color,
        camera_id: Option<bevy::prelude::Entity>
    )
    {
        self.resources.push_prop_pivot_mesh(
            self.color_resources.line_material(color),
            center,
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

    /// Draws a [`Brush`] anchor highlight.
    #[inline]
    pub fn anchor_highlight(&mut self, center: Vec2, color: Color)
    {
        self.resources.push_anchor_highlight_mesh(
            self.color_resources.line_material(color),
            center,
            color.square_hgl_height()
        );
    }

    /// Draws a sprite highlight.
    #[inline]
    pub fn sprite_highlight(&mut self, center: Vec2, color: Color)
    {
        self.resources.push_sprite_highlight_mesh(
            self.color_resources.line_material(color),
            center,
            color.square_hgl_height()
        );
    }

    /// Draws the collision overlay.
    #[inline]
    fn collision_overlay(&mut self, vertexes: impl ExactSizeIterator<Item = Vec2>, color: Color)
    {
        let mut mesh_generator = self.resources.mesh_generator();
        mesh_generator.set_indexes(vertexes.len());
        mesh_generator.push_positions(vertexes);
        mesh_generator.clip_uv();
        let mesh = mesh_generator.mesh(PrimitiveTopology::TriangleList);

        self.push_mesh(mesh, self.resources.clip_texture(), color.clip_height());
    }

    /// Draws `settings` mapped to `vertexes`.
    #[inline]
    fn polygon_texture<T: TextureInterface>(
        &mut self,
        camera: &Transform,
        vertexes: impl ExactSizeIterator<Item = Vec2> + Clone,
        center: Vec2,
        color: Color,
        settings: &T,
        collision: bool
    )
    {
        if self.show_collision_overlay && collision
        {
            self.collision_overlay(vertexes.clone(), color);
        }

        let mut mesh_generator = self.resources.mesh_generator();
        mesh_generator.set_indexes(vertexes.len());
        mesh_generator.push_positions(vertexes);
        mesh_generator.set_texture_uv(
            camera,
            settings,
            center,
            self.elapsed_time,
            self.parallax_enabled
        );
        let mesh = mesh_generator.mesh(PrimitiveTopology::TriangleList);

        self.resources
            .push_textured_mesh(self.meshes.add(mesh).into(), settings, color);
    }

    /// Draws `settings` as a [`Brush`].
    #[inline]
    pub fn sideless_brush<T: TextureInterface>(
        &mut self,
        camera: &Transform,
        vertexes: impl ExactSizeIterator<Item = Vec2> + Clone,
        center: Vec2,
        color: Color,
        texture: Option<&T>,
        collision: bool
    )
    {
        if let Some(texture) = texture
        {
            if !texture.sprite()
            {
                self.polygon_texture(camera, vertexes, center, color, texture, collision);
                return;
            }
        }

        if self.show_collision_overlay && collision
        {
            self.collision_overlay(vertexes.clone(), color);
        }

        let mesh = self.polygon_mesh(vertexes.clone());
        self.push_mesh(mesh, self.color_resources.brush_material(color), color.height());
    }

    /// Draws `settings` as a [`Brush`] also drawing the sides.
    #[inline]
    pub fn brush<T: TextureInterface>(
        &mut self,
        camera: &Transform,
        vertexes: impl ExactSizeIterator<Item = Vec2> + Clone,
        center: Vec2,
        color: Color,
        texture: Option<&T>,
        collision: bool
    )
    {
        self.sides(vertexes.clone(), color);
        self.sideless_brush(camera, vertexes, center, color, texture, collision);
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
        self.push_mesh(mesh, self.color_resources.line_material(color), color.height());
    }

    /// Queues the [`Mesh`] of a polygon to draw.
    #[inline]
    fn polygon(&mut self, vertexes: impl ExactSizeIterator<Item = Vec2>, color: Color)
    {
        let mesh = self.polygon_mesh(vertexes);
        self.push_mesh(mesh, self.color_resources.line_material(color), color.height());
    }

    /// Draws `settings` mapping the texture to `sides` and also drawing colored lines at the sides.
    #[inline]
    pub fn brush_with_sides_colors<T: TextureInterface>(
        &mut self,
        camera: &Transform,
        sides: impl ExactSizeIterator<Item = (Vec2, Vec2, Color)> + Clone,
        center: Vec2,
        body_color: Color,
        texture: Option<&T>,
        collision: bool
    )
    {
        self.lines(sides.clone());
        self.sideless_brush(
            camera,
            sides.map(|(vx, ..)| vx),
            center,
            body_color,
            texture,
            collision
        );
    }

    /// Draws `settings` as a sprite.
    #[inline]
    pub fn sprite<T: TextureInterface + TextureInterfaceExtra>(
        &mut self,
        brush_center: Vec2,
        settings: &T,
        color: Color
    )
    {
        let mut mesh_generator = self.resources.mesh_generator();
        mesh_generator.set_indexes(4);

        mesh_generator.push_positions(settings.sprite_vertexes(brush_center));
        mesh_generator.set_sprite_uv(settings.name(), settings, self.elapsed_time);
        let mesh = mesh_generator.mesh(PrimitiveTopology::TriangleList);

        self.resources
            .push_sprite(self.meshes.add(mesh).into(), settings, color);
    }

    /// Draws `thing`.
    #[inline]
    pub fn thing<T: ThingInterface + EntityHull>(
        &mut self,
        catalog: &ThingsCatalog,
        thing: &T,
        color: Color
    )
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
            fn new<T: ThingInterface + EntityHull>(thing: &T) -> Self
            {
                let hull = thing.hull();
                let (width, height) = hull.dimensions();
                let ray = (width.min(height) / 8f32).min(24f32);
                let x_delta = width / 2f32 - ray;
                let y_delta = height / 2f32 - ray;
                let center = hull.center();
                let circle_iter =
                    Hull::new(center.y + ray, center.y - ray, center.x - ray, center.x + ray)
                        .circle(CORNER_RESOLUTION * 4 - 4);

                Self {
                    x_delta,
                    y_delta,
                    circle_iter,
                    step: SmoothRectangleSteps::TopLeftCorner(CORNER_RESOLUTION)
                }
            }
        }

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

        // Sides
        self.sides(ThingOutline::new(thing), color);

        // Angle indicator
        let mut mesh_generator = self.resources.mesh_generator();
        mesh_generator.set_indexes(4);

        let hull = thing.hull();
        let half_side = (hull.width().min(hull.height()) / 2f32).min(64f32);
        let center = hull.center();
        let hull = Hull::new(
            center.y + half_side,
            center.y - half_side,
            center.x - half_side,
            center.x + half_side
        );
        mesh_generator.push_positions(hull.rectangle());

        mesh_generator.set_thing_angle_indicator_uv(thing.angle());
        let mesh = mesh_generator.mesh(PrimitiveTopology::TriangleList);
        self.push_mesh(
            mesh,
            self.resources.thing_angle_texture(),
            color.thing_angle_indicator_height()
        );

        // Texture
        let vxs = self
            .resources
            .texture_materials(
                self.resources.texture_or_error(catalog.texture(thing.thing())).name()
            )
            .texture()
            .hull() +
            thing.pos();

        let mut mesh_generator = self.resources.mesh_generator();
        mesh_generator.set_indexes(4);
        mesh_generator.push_positions(vxs.rectangle());
        mesh_generator.set_thing_uv(catalog, thing);
        let mesh = mesh_generator.mesh(PrimitiveTopology::TriangleList);

        self.resources
            .push_thing(self.meshes.add(mesh).into(), catalog, thing, color);
    }

    //==============================================================
    // Misc

    /// Returns a static `str` to be used as tooltip label for `pos`.
    #[inline]
    #[must_use]
    pub fn vx_tooltip_label(&mut self, pos: Vec2) -> Option<&'static str>
    {
        self.resources.vx_tooltip_label(pos)
    }
}

impl<'w: 'a, 's: 'a, 'a> EditDrawer<'w, 's, 'a>
{
    //==============================================================
    // New

    /// Returns a new [`EditDrawer`].
    #[allow(clippy::too_many_arguments)]
    #[inline]
    #[must_use]
    pub fn new(
        commands: &'a mut Commands<'w, 's>,
        prop_cameras: &PropCameras,
        meshes: &'a mut Assets<Mesh>,
        meshes_query: &Query<Entity, With<Mesh2dHandle>>,
        resources: &'a mut DrawingResources,
        color_resources: &'a ColorResources,
        settings: &ToolsSettings,
        mut elapsed_time: f32,
        camera_scale: f32,
        paint_tool_camera_scale: f32,
        show_collision_overlay: bool
    ) -> Self
    {
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

        Self {
            commands,
            meshes,
            resources,
            color_resources,
            camera_scale,
            elapsed_time,
            parallax_enabled: settings.parallax_enabled,
            show_collision_overlay
        }
    }

    //==============================================================
    // Mesh creation

    /// Returns a reference to the [`ColorResources`].
    #[inline]
    pub const fn color_resources(&self) -> &ColorResources { self.color_resources }

    /// Returns the [`egui::Color32`] associated with [`Color`].
    #[inline]
    #[must_use]
    pub fn egui_color(&self, color: Color) -> egui::Color32
    {
        self.color_resources.egui_color(color)
    }

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
        self.resources
            .push_square_highlight_mesh(material, center, color.square_hgl_height());
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
        mesh.push_positions([start, end]);
        mesh.mesh(PrimitiveTopology::LineStrip)
    }

    /// Returns a [`Mesh`] of a line with an arrow in the middle that points toward `end`.
    #[inline]
    fn arrowed_line_mesh(&mut self, start: Vec2, end: Vec2) -> Mesh
    {
        // Basic line.
        let mut mesh = self.resources.mesh_generator();
        mesh.push_positions([start, end]);

        // Arrow.
        let half_height = VX_HGL_SIDE * self.camera_scale;
        let mid = (start + end) / 2f32;
        let mut tip = Vec2::new(mid.x + half_height, mid.y);
        let bottom_x = mid.x - half_height;
        let mut top_left = Vec2::new(bottom_x, mid.y + half_height);
        let mut bottom_left = Vec2::new(bottom_x, mid.y - half_height);
        let angle = -(end - start).angle_between(Vec2::X);

        for vx in [&mut tip, &mut top_left, &mut bottom_left]
        {
            *vx = rotate_point(*vx, mid, angle);
        }

        mesh.push_positions([top_left, tip, tip, bottom_left]);
        mesh.mesh(PrimitiveTopology::LineList)
    }

    /// Returns the [`Mesh`] of a circle with `resolution` sides.
    #[inline]
    #[must_use]
    fn circle_mesh(&mut self, center: Vec2, resolution: u8, radius: f32) -> Mesh
    {
        assert!(resolution != 0, "Cannot create a circle with 0 sides.");

        let mut mesh = self.resources.mesh_generator();
        mesh.push_positions(DrawingResources::circle_vxs(resolution, radius).map(|vx| vx + center));
        mesh.mesh(PrimitiveTopology::LineStrip)
    }

    /// Returns the [`Mesh`] of a polygon.
    #[inline]
    #[must_use]
    fn polygon_mesh(&mut self, vertexes: impl ExactSizeIterator<Item = Vec2>) -> Mesh
    {
        let len = vertexes.len();

        let mut mesh = self.resources.mesh_generator();
        mesh.push_positions(vertexes);
        mesh.set_indexes(len);
        mesh.mesh(PrimitiveTopology::TriangleList)
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
    /// The time that has passed.
    elapsed_time: f32
}

impl<'w: 'a, 's: 'a, 'a> Drop for MapPreviewDrawer<'w, 's, 'a>
{
    #[inline]
    fn drop(&mut self) { self.spawn_meshes() }
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
        meshes_query: &Query<Entity, With<Mesh2dHandle>>,
        resources: &'a mut DrawingResources,
        elapsed_time: f32
    ) -> Self
    {
        resources.setup_frame(commands, prop_cameras, meshes, meshes_query, 1f32, 1f32);

        Self {
            commands,
            meshes,
            resources,
            elapsed_time
        }
    }

    /// Spawns the meshes.
    #[inline]
    fn spawn_meshes(&mut self) { self.resources.spawn_meshes(self.commands); }

    /// Draws `settings` mapping the texture to `vertexes`.
    #[inline]
    pub fn brush<T: TextureInterface + TextureInterfaceExtra>(
        &mut self,
        camera: &Transform,
        vertexes: impl ExactSizeIterator<Item = Vec2> + Clone,
        center: Vec2,
        animator: Option<&Animator>,
        settings: &T
    )
    {
        let resources = unsafe { std::ptr::from_mut(self.resources).as_mut().unwrap() };

        let mut mesh_generator = resources.mesh_generator();
        mesh_generator.set_indexes(vertexes.len());
        mesh_generator.push_positions(vertexes);

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
                        mesh_generator.set_texture_uv(
                            camera,
                            settings,
                            center,
                            self.elapsed_time,
                            true
                        );

                        materials
                    },
                    Animator::Atlas(animator) =>
                    {
                        mesh_generator.set_animated_texture_uv(
                            camera,
                            settings,
                            animator,
                            center,
                            self.elapsed_time,
                            true
                        );

                        self.resources.texture_materials(settings.name())
                    }
                }
            },
            None =>
            {
                let texture = self.resources.texture_or_error(settings.name());
                mesh_generator.set_texture_uv(camera, settings, center, self.elapsed_time, true);
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
        let mut mesh_generator = self.resources.mesh_generator();
        mesh_generator.set_indexes(4);

        mesh_generator.push_positions(settings.sprite_vertexes(brush_center));

        match animator
        {
            Some(animator) =>
            {
                mesh_generator.set_animated_sprite_uv(settings, animator, self.elapsed_time);
            },
            None =>
            {
                mesh_generator.set_sprite_uv(settings.name(), settings, self.elapsed_time);
            }
        }

        let mesh = mesh_generator.mesh(PrimitiveTopology::TriangleList);

        self.resources
            .push_map_preview_sprite(self.meshes.add(mesh).into(), settings);
    }

    /// Draws `thing`.
    #[inline]
    pub fn thing<T: ThingInterface + EntityHull>(&mut self, catalog: &ThingsCatalog, thing: &T)
    {
        let mut mesh_generator = self.resources.mesh_generator();
        mesh_generator.set_indexes(4);
        mesh_generator.push_positions(thing.hull().rectangle());
        mesh_generator.set_thing_uv(catalog, thing);

        let mesh = mesh_generator.mesh(PrimitiveTopology::TriangleList);

        self.resources
            .push_map_preview_thing(self.meshes.add(mesh).into(), catalog, thing);
    }
}
