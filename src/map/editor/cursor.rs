//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::{transform::components::Transform, window::Window};
use glam::Vec2;

use super::{state::editor_state::State, MAP_HALF_SIZE};
use crate::{
    map::editor::state::grid::Grid,
    utils::{hull::Hull, misc::Camera}
};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// The position of the cursor.
pub(in crate::map::editor) struct Cursor
{
    /// The position of the cursor with respect to the application window.
    ui:                     Vec2,
    /// The position of the cursor with respect to the application window snapped to the map grid.
    ui_grid_snapped:        Vec2,
    /// The position of the cursor on the map.
    world:                  Vec2,
    /// The position of the cursor on the map snapped to the grid.
    world_grid_snapped:     Vec2,
    world_no_grid:          Vec2,
    /// The amount the cursor was moved from the previous frame with respect to the application
    /// window.
    delta_ui:               Vec2,
    /// The bounding box describing the map grid square the cursor is currently in.
    grid_square:            Hull,
    /// Whether the cursor is set to be snapped to the grid.
    snap:                   bool,
    /// The position of the cursor on the map in the previous frame.
    previous_world:         Vec2,
    /// The position of the cursor on the map snapped to the grid in the previous frame.
    previous_world_snapped: Vec2
}

impl Default for Cursor
{
    /// Returns a [`Cursor`] out of the visible portion of the map on load.
    #[inline]
    #[must_use]
    fn default() -> Self
    {
        /// The position used at startup.
        const START_POS: Vec2 = Vec2::splat(MAP_HALF_SIZE / 2f32);

        Self {
            ui:                     START_POS,
            ui_grid_snapped:        START_POS,
            world:                  START_POS,
            world_grid_snapped:     START_POS,
            world_no_grid:          START_POS,
            delta_ui:               Vec2::ZERO,
            grid_square:            Grid::default().square(START_POS),
            snap:                   true,
            previous_world:         START_POS,
            previous_world_snapped: START_POS
        }
    }
}

impl Cursor
{
    /// Whether the cursor was moved.
    #[inline]
    #[must_use]
    pub fn moved(&self) -> bool { self.delta_ui != Vec2::ZERO }

    /// Returns the position of the cursor on the map.
    #[inline]
    #[must_use]
    pub const fn world(&self) -> Vec2 { self.world }

    #[inline]
    #[must_use]
    pub const fn world_no_grid(&self) -> Vec2 { self.world_no_grid }

    /// Returns the grid snapped position of the cursor on the map if snap is enabled, otherwise
    /// returns the regular map position.
    #[inline]
    #[must_use]
    pub const fn world_snapped(&self) -> Vec2
    {
        if self.snap
        {
            self.world_grid_snapped
        }
        else
        {
            self.world
        }
    }

    /// Returns the grid snapped position of the cursor on the window surface if snap is enabled,
    /// otherwise returns the regular position on the window surface.
    #[inline]
    #[must_use]
    pub const fn ui_snapped(&self) -> Vec2
    {
        if self.snap
        {
            self.ui_grid_snapped
        }
        else
        {
            self.ui
        }
    }

    /// Returns a reference to the bounding box describing the grid square the cursor is currently
    /// on.
    #[inline]
    #[must_use]
    pub const fn grid_square(&self) -> &Hull { &self.grid_square }

    /// Returns the amount the cursor was moved from the previous frame with respect to the
    /// application window.
    #[inline]
    #[must_use]
    pub const fn delta_ui(&self) -> Vec2 { self.delta_ui }

    /// Whether grid snap is enabled.
    #[inline]
    #[must_use]
    pub const fn snap(&self) -> bool { self.snap }

    /// Updates the values of `self` based on the `window` size, the `camera` position and scale,
    /// and the current editor state. Whenever space is being pressed, and therefore the camera
    /// is being dragged around, only the UI position is updated.
    #[inline]
    pub fn update(
        &mut self,
        ui: Vec2,
        window: &Window,
        camera: &Transform,
        state: &State,
        grid: Grid,
        space_pressed: bool
    )
    {
        macro_rules! clamp_world_coordinate {
            ($($xy:ident),+) => { $(
                self.world.$xy = self.world.$xy.clamp(-MAP_HALF_SIZE, MAP_HALF_SIZE);
            )+};
        }

        self.delta_ui = ui - self.ui;
        self.ui = ui;

        if space_pressed
        {
            return;
        }

        self.previous_world = self.world;
        self.previous_world_snapped = self.world_grid_snapped;
        self.world = camera.to_world_coordinates(window, grid, ui);

        let mut absolute_grid = grid;
        absolute_grid.set_angle(0);
        absolute_grid.set_skew(0);
        self.world_no_grid = camera.to_world_coordinates(window, absolute_grid, ui);

        clamp_world_coordinate!(x, y);

        self.grid_square = grid.square(self.world);
        self.world_grid_snapped = self.grid_square.nearest_corner_to_point(self.world);
        let p = camera.to_egui_coordinates(window, grid, self.world_grid_snapped);
        self.ui_grid_snapped = Vec2::new(p.x, p.y);
        self.snap = state.cursor_snap();
    }
}
