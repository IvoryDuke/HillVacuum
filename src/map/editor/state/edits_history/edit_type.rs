//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy::prelude::Vec2;
use hill_vacuum_shared::NextValue;

use crate::{
    map::{
        brush::{
            convex_polygon::{ConvexPolygon, TextureSetResult, VertexesMove},
            Brush,
            BrushData
        },
        drawer::{
            animation::{Animation, MoveUpDown, Timing},
            drawing_resources::{DrawingResources, TextureMut},
            texture::{Sprite, TextureInterface, TextureSettings}
        },
        editor::state::{core::UndoRedoInterface, ui::Ui},
        path::{MovementValueEdit, NodesMove, StandbyValueEdit},
        properties::Value,
        thing::{ThingId, ThingInstanceData},
        HvVec
    },
    utils::{hull::Flip, identifiers::Id},
    Path
};

//=======================================================================//
// MACROS
//
//=======================================================================//

/// A macro that executes a certain procedure if there is a match and then terminates the function
/// call.
macro_rules! match_and_return {
    ($self:ident, $($arm:pat => $f:expr),+) => {
        match $self
        {
            $($arm => {
                $f;
                return;
            }),+
            _ => ()
        };
    };
}

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// An enum used to categorize [`Brush`]es in three categories: not selected, selected, and drawn.
#[derive(Clone, Copy)]
pub(in crate::map::editor::state) enum BrushType
{
    /// Non selected [`Brush`].
    NotSelected,
    /// Selected [`Brush`].
    Selected,
    /// Drawn [`Brush`].
    Drawn
}

impl BrushType
{
    /// Generates a [`BrushType`] from `selected`. If true returns `BrushType::Selected`, otherwise
    /// `BrushType::NotSelected`
    #[inline]
    #[must_use]
    const fn from_selection(selected: bool) -> Self
    {
        if selected
        {
            Self::Selected
        }
        else
        {
            Self::NotSelected
        }
    }

    /// Whever `self` is `BrushType::Selected`.
    #[inline]
    #[must_use]
    pub const fn selected(self) -> bool { !matches!(self, Self::NotSelected) }

    /// Whever `self` is `BrushType::Drawn`.
    #[inline]
    #[must_use]
    pub const fn drawn(self) -> bool { matches!(self, Self::Drawn) }
}

//=======================================================================//

/// The type of the [`Edit`] stored in the [`EditsHistory`].
#[derive(Debug)]
pub(in crate::map::editor::state::edits_history) enum EditType
{
    /// Drawn [`Brush`].
    BrushDraw(Option<BrushData>),
    /// Drawn [`Brush`] despawned.
    DrawnBrushDespawn(Option<BrushData>),
    /// Brush spawned.
    BrushSpawn(Option<BrushData>, bool),
    /// Brush despawned
    BrushDespawn(Option<BrushData>, bool),
    /// Entity selected.
    EntitySelection,
    /// Entity deselected.
    EntityDeselection,
    /// Subtractee selected (subtract tool only).
    SubtracteeSelection,
    /// Subtractee deselected (subtract tool only).
    SubtracteeDeselection,
    /// Generic polygon edit.
    PolygonEdit(ConvexPolygon),
    /// Brush moved.
    BrushMove(Vec2, bool),
    /// Free draw point inserted (draw tools only).
    FreeDrawPointInsertion(Vec2, u8),
    /// Free draw point deleted (draw tools only).
    FreeDrawPointDeletion(Vec2, u8),
    /// Vertex inserted.
    VertexInsertion((Vec2, u8)),
    /// Vertex deleted.
    VertexesDeletion(HvVec<(Vec2, u8)>),
    /// Vertexes moved.
    VertexesMove(HvVec<VertexesMove>),
    /// Sides deleted.
    SidesDeletion(HvVec<(Vec2, u8, bool)>),
    /// Vertexes selected.
    VertexesSelection(HvVec<u8>),
    /// Vertexes snapped to grid.
    VertexesSnap(HvVec<(HvVec<u8>, Vec2)>),
    /// Polygons flipped.
    Flip(Flip, bool),
    /// Motor created.
    PathCreation(Option<Path>),
    /// Motor deleted.
    PathDeletion(Option<Path>),
    /// Path nodes selection.
    PathNodesSelection(HvVec<u8>),
    /// Path node inserion.
    PathNodeInsertion((Vec2, u8)),
    /// Path nodes move.
    PathNodesMove(HvVec<NodesMove>),
    /// Path nodes deletion.
    PathNodesDeletion(HvVec<(Vec2, u8)>),
    /// Path nodes grid snap.
    PathNodesSnap(HvVec<(HvVec<u8>, Vec2)>),
    /// Changed path node standby time.
    PathNodeStandby(StandbyValueEdit),
    /// Changed path node acceleration percentage.
    PathNodeAccel(MovementValueEdit),
    /// Changed path node deceleration percentage.
    PathNodeDecel(MovementValueEdit),
    /// Changed path node max speed.
    PathNodeMaxSpeed(MovementValueEdit),
    /// Changed path node minimum speed.
    PathNodeMinSpeed(MovementValueEdit),
    /// Brush anchored.
    Anchor(Id),
    /// Brush disachored.
    Disanchor(Id),
    /// Thing drawn on map.
    ThingDraw(Option<ThingInstanceData>),
    /// Drawn thing despawned.
    DrawnThingDespawn(Option<ThingInstanceData>),
    /// Thing spawned.
    ThingSpawn(Option<ThingInstanceData>),
    /// Thing despawned.
    ThingDespawn(Option<ThingInstanceData>),
    /// Thing moved.
    ThingMove(Vec2),
    /// Thing changed to new ID.
    ThingChange(ThingId),
    /// Thing draw height change.
    ThingHeight(i8),
    /// Thing angle change.
    ThingAngle(f32),
    /// Brush texture change.
    Texture(Option<String>),
    /// Brush texture removed.
    TextureRemoval(Option<TextureSettings>),
    /// Toggled sprite setting of texture.
    Sprite(Sprite, f32, f32),
    /// Texture flip, true -> vertical, false -> horizontal.
    TextureFlip(bool),
    /// Texture scaled with specified delta.
    TextureScaleDelta(Vec2),
    /// Texture x scale changed.
    TextureScaleX(f32),
    /// Texture y scale changed.
    TextureScaleY(f32),
    /// Texture scaled and flipped as result to new values.
    TextureScaleFlip(f32, f32),
    /// Texture x offset change.
    TextureOffsetX(f32),
    /// Texture y offset change.
    TextureOffsetY(f32),
    /// Texture x scroll change.
    TextureScrollX(f32),
    /// Texture y scroll change.
    TextureScrollY(f32),
    /// Texture x parallax change.
    TextureParallaxX(f32),
    /// Texture y parallax change.
    TextureParallaxY(f32),
    /// Texture offset moved by specified delta.
    TextureMove(Vec2),
    /// Texture angle change.
    TextureAngle(f32),
    /// Texture angle changed by specified delta.
    TextureAngleDelta(f32),
    /// Texture draw height change.
    TextureHeight(i8),
    /// Texture animation change.
    Animation(Animation),
    /// Texture animation frame info moved up. true -> atlas, false -> list.
    AnimationMoveUp(usize, bool),
    /// Texture animation frame info moved down. true -> atlas, false -> list.
    AnimationMoveDown(usize, bool),
    /// List animation frame addition.
    ListAnimationNewFrame(String),
    /// List animation frame texture change.
    ListAnimationTexture(usize, String),
    /// List animation frame time change.
    ListAnimationTime(usize, f32),
    /// List animation frame removal.
    ListAnimationFrameRemoval(usize, String, f32),
    /// Atlas animation x partitioning change.
    AtlasAnimationX(u32),
    /// Atlas animation y partitioning change.
    AtlasAnimationY(u32),
    /// Atlas animation frames length change.
    AtlasAnimationLen(usize),
    /// Atlas animation [`Timing`] change.
    AtlasAnimationTiming(Option<Timing>),
    /// Atlas animation uniform time change.
    AtlasAnimationUniformTime(f32),
    /// Atlas animation frame time change.
    AtlasAnimationFrameTime(usize, f32),
    /// Default texture animation change.
    TAnimation(String, Animation),
    /// Default texture animation frame info moved up. true -> atlas, false -> list.
    TAnimationMoveUp(String, usize, bool),
    /// Default texture animation frame info moved down. true -> atlas, false -> list.
    TAnimationMoveDown(String, usize, bool),
    /// Default list animation frame addition
    TListAnimationNewFrame(String, String),
    /// Default list animation frame texture change.
    TListAnimationTexture(String, usize, String),
    /// Default list animation frame time change
    TListAnimationTime(String, usize, f32),
    /// Default list animation frame removal.
    TListAnimationFrameRemoval(String, usize, String, f32),
    /// Default atlas animation x partitioning change
    TAtlasAnimationX(String, u32),
    /// Default atlas animation y partitioning change.
    TAtlasAnimationY(String, u32),
    /// Default atlas animation frames length change.
    TAtlasAnimationLen(String, usize),
    /// Default atlas animation [`Timing`] change.
    TAtlasAnimationTiming(String, Option<Timing>),
    /// Default atlas animation uniform time change.
    TAtlasAnimationUniformTime(String, f32),
    /// Default atlas animation frame time change.
    TAtlasAnimationFrameTime(String, usize, f32),
    /// Brush collision change.
    Collision(bool),
    /// Entity property change.
    Property(Value)
}

impl EditType
{
    /// Whever `self` is an edit that is only useful as long as the current tool remains unchanged.
    #[inline]
    #[must_use]
    pub const fn tool_edit(&self) -> bool
    {
        matches!(
            self,
            Self::BrushDraw(_) |
                Self::DrawnBrushDespawn(_) |
                Self::ThingDraw(..) |
                Self::DrawnThingDespawn(..) |
                Self::VertexesSelection(_) |
                Self::PathNodesSelection(_) |
                Self::SubtracteeSelection |
                Self::SubtracteeDeselection |
                Self::FreeDrawPointInsertion(..) |
                Self::FreeDrawPointDeletion(..)
        )
    }

    /// Whever `self` is a texture edit.
    #[inline]
    #[must_use]
    pub const fn texture_edit(&self) -> bool
    {
        matches!(
            self,
            Self::Texture(_) |
                Self::TextureRemoval(_) |
                Self::Sprite(..) |
                Self::TextureFlip(_) |
                Self::TextureScaleDelta(_) |
                Self::TextureScaleX(_) |
                Self::TextureScaleY(_) |
                Self::TextureScaleFlip(..) |
                Self::TextureOffsetX(_) |
                Self::TextureOffsetY(_) |
                Self::TextureScrollX(_) |
                Self::TextureScrollY(_) |
                Self::TextureParallaxX(_) |
                Self::TextureParallaxY(_) |
                Self::TextureMove(_) |
                Self::TextureAngle(_) |
                Self::TextureAngleDelta(_) |
                Self::TextureHeight(_) |
                Self::Animation(_) |
                Self::AnimationMoveUp(..) |
                Self::AnimationMoveDown(..) |
                Self::ListAnimationNewFrame(_) |
                Self::ListAnimationTexture(..) |
                Self::ListAnimationTime(..) |
                Self::ListAnimationFrameRemoval(..) |
                Self::AtlasAnimationX(_) |
                Self::AtlasAnimationY(_) |
                Self::AtlasAnimationLen(_) |
                Self::AtlasAnimationTiming(_) |
                Self::AtlasAnimationUniformTime(_) |
                Self::AtlasAnimationFrameTime(..)
        )
    }

    /// Whever `self` represents a [`ThingInstance`] edit.
    #[inline]
    #[must_use]
    pub const fn thing_edit(&self) -> bool
    {
        matches!(
            self,
            Self::ThingChange(_) |
                Self::ThingDraw(..) |
                Self::ThingSpawn(..) |
                Self::DrawnThingDespawn(..) |
                Self::ThingDespawn(..) |
                Self::ThingMove(_) |
                Self::ThingHeight(_) |
                Self::ThingAngle(_)
        )
    }

    /// Returns the texture associated with the edit, if any.
    #[inline]
    fn texture<'a, 'b: 'a>(
        &'a self,
        drawing_resources: &'b mut DrawingResources,
        ui: &mut Ui
    ) -> Option<TextureMut<'b>>
    {
        ui.schedule_texture_animation_update();

        match self
        {
            Self::TAnimation(t, _) |
            Self::TAnimationMoveUp(t, _, _) |
            Self::TAnimationMoveDown(t, _, _) |
            Self::TListAnimationNewFrame(t, _) |
            Self::TListAnimationTexture(t, _, _) |
            Self::TListAnimationTime(t, _, _) |
            Self::TListAnimationFrameRemoval(t, _, _, _) |
            Self::TAtlasAnimationX(t, _) |
            Self::TAtlasAnimationY(t, _) |
            Self::TAtlasAnimationLen(t, _) |
            Self::TAtlasAnimationTiming(t, _) |
            Self::TAtlasAnimationUniformTime(t, _) |
            Self::TAtlasAnimationFrameTime(t, _, _) => drawing_resources.texture_mut(t),
            _ => None
        }
    }

    /// Actions common to both the undo and redo procedures that apply to multiple [`Brush`]es.
    /// Returns whever the edit was undone/redone.
    #[inline]
    #[must_use]
    fn brushes_common(
        &mut self,
        interface: &mut UndoRedoInterface,
        drawing_resources: &DrawingResources,
        identifiers: &HvVec<Id>
    ) -> bool
    {
        match self
        {
            Self::TextureFlip(y) =>
            {
                if *y
                {
                    for id in identifiers
                    {
                        interface.brush_mut(*id).flip_scale_y(drawing_resources);
                    }
                }
                else
                {
                    for id in identifiers
                    {
                        interface.brush_mut(*id).flip_texture_scale_x(drawing_resources);
                    }
                }
            },
            Self::ListAnimationTexture(index, name) =>
            {
                let mut iter = identifiers.iter();

                let prev = std::mem::replace(
                    name,
                    interface
                        .brush_mut(*iter.next_value())
                        .set_list_animation_texture(*index, name)
                        .unwrap()
                        .clone()
                );

                for id in identifiers
                {
                    _ = interface.brush_mut(*id).set_list_animation_texture(*index, &prev);
                }
            },
            Self::ListAnimationTime(index, time) =>
            {
                let mut iter = identifiers.iter();
                let value = interface
                    .brush_mut(*iter.next_value())
                    .set_texture_list_animation_time(*index, *time)
                    .unwrap();

                for id in iter
                {
                    _ = interface
                        .brush_mut(*id)
                        .set_texture_list_animation_time(*index, *time);
                }

                *time = value;
            },
            Self::AnimationMoveUp(index, atlas) =>
            {
                let func = if *atlas
                {
                    Brush::move_up_atlas_animation_frame_time
                }
                else
                {
                    Brush::move_up_list_animation_frame
                };

                for id in identifiers
                {
                    func(&mut interface.brush_mut(*id), *index);
                }
            },
            Self::AnimationMoveDown(index, atlas) =>
            {
                let func = if *atlas
                {
                    Brush::move_down_atlas_animation_frame_time
                }
                else
                {
                    Brush::move_down_list_animation_frame
                };

                for id in identifiers
                {
                    func(&mut interface.brush_mut(*id), *index);
                }
            },
            _ => return false
        };

        true
    }

    /// Actions common to both the undo and redo procedures that apply to a single [`Brush`].
    /// Returns whever the edit was undone/redone.
    #[inline]
    #[must_use]
    fn brush_common(
        &mut self,
        drawing_resources: &DrawingResources,
        interface: &mut UndoRedoInterface,
        identifier: Id
    ) -> bool
    {
        let mut brush = interface.brush_mut(identifier);

        /// Generates the match arms.
        macro_rules! arms {
            ($((
                $arm:ident,
                $value:ident
                $(, $drawing_resources:ident)?
            )),+) => {
                match self
                {
                    Self::VertexesSelection(idxs) =>
                    {
                        for idx in idxs
                        {
                            brush.toggle_vertex_at_index((*idx).into());
                        }
                    },
                    Self::PolygonEdit(cp) => brush.swap_polygon(cp),
                    $(Self::$arm(value) =>
                    {
                        paste::paste! { *value = brush.[< set_texture_ $value >]($($drawing_resources, )? *value).unwrap(); }
                    }),+
                    Self::TextureScaleFlip(scale_x, scale_y) =>
                    {
                        let fns: [(&mut f32, fn(&TextureSettings) -> f32, fn(&mut Brush, &DrawingResources, f32) -> Option<f32>); 2] = [
                            (scale_x, TextureSettings::scale_x, Brush::set_texture_scale_x),
                            (scale_y, TextureSettings::scale_y, Brush::set_texture_scale_y)
                        ];

                        for (value, get, set) in fns
                        {
                            let value = std::mem::replace(value, get(brush.texture_settings().unwrap()));
                            set(&mut brush, drawing_resources, value);
                        }
                    },
                    Self::Animation(value) =>
                    {
                        *value = brush.set_texture_animation(drawing_resources, std::mem::take(value));
                    },
                    Self::AtlasAnimationTiming(timing) =>
                    {
                        *timing = brush.set_texture_atlas_animation_timing(std::mem::take(timing).unwrap()).into();
                    },
                    Self::AtlasAnimationFrameTime(index, time) =>
                    {
                        *time = brush.set_texture_atlas_animation_frame_time(*index, *time).unwrap();
                    },
                    Self::Collision(value) =>
                    {
                        *value = brush.set_collision(*value).unwrap();
                        drop(brush);
                        interface.schedule_overall_collision_update();
                    },
                    _ => return false
                }
            };
        }

        arms!(
            (TextureParallaxX, parallax_x),
            (TextureParallaxY, parallax_y),
            (TextureScrollX, scroll_x),
            (TextureScrollY, scroll_y),
            (TextureOffsetX, offset_x, drawing_resources),
            (TextureOffsetY, offset_y, drawing_resources),
            (TextureScaleX, scale_x, drawing_resources),
            (TextureScaleY, scale_y, drawing_resources),
            (TextureAngle, angle, drawing_resources),
            (TextureHeight, height),
            (AtlasAnimationX, atlas_animation_x_partition, drawing_resources),
            (AtlasAnimationY, atlas_animation_y_partition, drawing_resources),
            (AtlasAnimationLen, atlas_animation_len),
            (AtlasAnimationUniformTime, atlas_animation_uniform_time)
        );

        true
    }

    /// Actions common to both the undo and redo procedures that apply to a single [`Thing`].
    /// Returns whever the edit was undone/redone.
    #[inline]
    #[must_use]
    fn thing_common(&mut self, interface: &mut UndoRedoInterface, identifier: Id) -> bool
    {
        match self
        {
            Self::ThingChange(id) => *id = interface.set_thing(identifier, *id),
            Self::ThingHeight(height) =>
            {
                *height = interface.thing_mut(identifier).set_draw_height(*height).unwrap();
                interface.schedule_overall_things_info_update();
            },
            Self::ThingAngle(angle) =>
            {
                *angle = interface.thing_mut(identifier).set_angle(*angle).unwrap();
                interface.schedule_overall_things_info_update();
            },
            _ => return false
        };

        true
    }

    /// Actions common to both the undo and redo procedures that apply to a default animation.
    /// Returns whever the edit was undone/redone.
    #[inline]
    #[must_use]
    fn default_animation_common(&mut self, animation: &mut Animation) -> bool
    {
        /// Generates the match arms.
        macro_rules! arms {
            ($(($arm:ident, $value:ident)),+) => {
                match self
                {
                    $(Self::$arm(_, value) =>
                    {
                        let animation = animation.get_atlas_animation_mut();
                        paste::paste! { *value = animation.[< set_ $value >](*value).unwrap(); }
                    }),+
                    Self::TAnimation(_, value) =>
                    {
                        *value = std::mem::replace(animation, std::mem::take(value));
                    },
                    Self::TAnimationMoveUp(_, index, atlas) =>
                    {
                        if *atlas
                        {
                            animation.get_atlas_animation_mut().move_up(*index);
                        }
                        else
                        {
                            animation.get_list_animation_mut().move_up(*index);
                        }
                    },
                    Self::TAnimationMoveDown(_, index, atlas) =>
                    {
                        if *atlas
                        {
                            animation.get_atlas_animation_mut().move_down(*index);
                        }
                        else
                        {
                            animation.get_list_animation_mut().move_down(*index);
                        }
                    },
                    Self::TListAnimationTexture(_, index, name) =>
                    {
                        *name = animation
                            .get_list_animation_mut()
                            .set_texture(*index, name)
                            .unwrap();
                    },
                    Self::TListAnimationTime(_, index, time) =>
                    {
                        *time = animation
                            .get_list_animation_mut()
                            .set_time(*index, *time)
                            .unwrap();
                    },
                    Self::TAtlasAnimationTiming(_, timing) =>
                    {
                        *timing = animation
                            .get_atlas_animation_mut()
                            .set_timing(std::mem::take(timing).unwrap())
                            .into();
                    },
                    Self::TAtlasAnimationFrameTime(_, index, time) =>
                    {
                        *time = animation
                            .get_atlas_animation_mut()
                            .set_frame_time(*index, *time)
                            .unwrap();
                    },
                    _ => return false
                }
            };
        }

        arms!(
            (TAtlasAnimationX, x_partition),
            (TAtlasAnimationY, y_partition),
            (TAtlasAnimationLen, len),
            (TAtlasAnimationUniformTime, uniform_time)
        );

        true
    }

    /// Action common to both the undo and redo procedures concerning a property edit.
    /// Returns whever the edit was undone/redone.
    #[inline]
    #[must_use]
    fn property(
        &mut self,
        interface: &mut UndoRedoInterface,
        identifier: Id,
        key: Option<&String>
    ) -> bool
    {
        if let Self::Property(value) = self
        {
            *value = interface.set_property(identifier, key.unwrap(), value);
            return true;
        }

        false
    }

    //==============================================================
    // Undo

    /// Executes the undo procedure associated with the value of `self`.
    #[inline]
    pub fn undo(
        &mut self,
        interface: &mut UndoRedoInterface,
        drawing_resources: &mut DrawingResources,
        ui: &mut Ui,
        identifiers: &HvVec<Id>,
        property: Option<&String>
    )
    {
        /// Returns the first [`Id`] of `identifiers`
        macro_rules! single {
            () => {
                identifiers[0]
            };
        }

        /// Returns the [`MovingMut`] of the entity with the [`Id`] returned by `single`.
        macro_rules! moving_mut {
            () => {
                interface.moving_mut(single!())
            };
        }

        if let Some(texture) = &mut self
            .texture(unsafe { std::ptr::from_mut(drawing_resources).as_mut() }.unwrap(), ui)
        {
            if self.default_animation_common(texture.animation_mut_set_dirty())
            {
                return;
            }

            match self
            {
                Self::TListAnimationFrameRemoval(_, index, name, time) =>
                {
                    texture
                        .animation_mut_set_dirty()
                        .get_list_animation_mut()
                        .insert(*index, name, *time);
                },
                Self::TListAnimationNewFrame(..) =>
                {
                    texture.animation_mut_set_dirty().get_list_animation_mut().pop();
                },
                _ => unreachable!()
            };

            return;
        }

        if self.property(interface, single!(), property)
        {
            return;
        }

        if self.brushes_common(interface, drawing_resources, identifiers)
        {
            return;
        }

        match_and_return!(
            self,
            Self::BrushDraw(data) =>
            {
                *data = interface.despawn_brush(single!(), BrushType::Drawn).into();
            },
            Self::BrushSpawn(data, selected) =>
            {
                *data = interface.despawn_brush(single!(), BrushType::from_selection(*selected)).into();
            },
            Self::DrawnBrushDespawn(data) =>
            {
                interface.spawn_brush(
                    single!(),
                    std::mem::take(data).unwrap(),
                    BrushType::Drawn
                );
            },
            Self::BrushDespawn(data, selected) =>
            {
                interface.spawn_brush(
                    single!(),
                    std::mem::take(data).unwrap(),
                    BrushType::from_selection(*selected)
                );
            },
            Self::PathDeletion(path) =>
            {
                interface.schedule_overall_node_update();
                interface.set_path(single!(), std::mem::take(path).unwrap());
            },
            Self::EntitySelection =>
            {
                for id in identifiers
                {
                    interface.deselect_entity(*id);
                }
            },
            Self::EntityDeselection =>
            {
                for id in identifiers
                {
                    interface.select_entity(*id);
                }
            },
            Self::SubtracteeSelection =>
            {
                for id in identifiers
                {
                    interface.remove_subtractee(*id);
                }
            },
            Self::SubtracteeDeselection =>
            {
                for id in identifiers
                {
                    interface.insert_subtractee(*id);
                }
            },
            Self::BrushMove(d, move_texture) =>
            {
                for id in identifiers
                {
                    interface
                        .brush_mut(*id)
                        .move_polygon(drawing_resources, -*d, *move_texture);
                }
            },
            Self::FreeDrawPointInsertion(p, idx) => interface.delete_free_draw_point(*p, *idx as usize),
            Self::FreeDrawPointDeletion(p, idx) => interface.insert_free_draw_point(*p, *idx as usize),
            Self::Flip(flip, flip_texture) =>
            {
                let func = match flip
                {
                    Flip::Above(_) => Brush::flip_below,
                    Flip::Below(_) => Brush::flip_above,
                    Flip::Left(_) => Brush::flip_right,
                    Flip::Right(_) => Brush::flip_left
                };

                for id in identifiers
                {
                    func(
                        &mut interface.brush_mut(*id),
                        drawing_resources,
                        flip.mirror(),
                        *flip_texture
                    );
                }
            },
            Self::PathCreation(path) => *path = interface.remove_path(single!()).into(),
            Self::Anchor(anchor) => interface.remove_anchor(single!(), *anchor),
            Self::Disanchor(anchor) => interface.insert_anchor(single!(), *anchor),
            Self::PathNodesSelection(idxs) =>
            {
                interface.schedule_overall_node_update();

                let mut moving = moving_mut!();

                for idx in idxs
                {
                    moving.toggle_path_node_at_index((*idx).into());
                }
            },
            Self::PathNodeInsertion((_, idx)) =>
            {
                moving_mut!().remove_path_node_at_index(*idx as usize);
            },
            Self::PathNodesMove(nodes_move) =>
            {
                let mut moving = moving_mut!();

                for nodes_move in nodes_move.iter().rev()
                {
                    moving.undo_path_nodes_move(nodes_move);
                }
            },
            Self::PathNodesDeletion(nodes) =>
            {
                interface.schedule_overall_node_update();
                moving_mut!().insert_path_nodes_at_indexes(nodes);
            },
            Self::PathNodesSnap(snap) =>
            {
                for (_, delta) in &mut *snap
                {
                    *delta = -*delta;
                }

                moving_mut!().move_path_nodes_at_indexes(snap);

                for (_, delta) in &mut *snap
                {
                    *delta = -*delta;
                }
            },
            Self::PathNodeStandby(edit) =>
            {
                interface.schedule_overall_node_update();
                moving_mut!().undo_path_nodes_standby_time_edit(edit);
            },
            Self::PathNodeMaxSpeed(edit) =>
            {
                interface.schedule_overall_node_update();
                moving_mut!().undo_path_nodes_max_speed_edit(edit);
            },
            Self::PathNodeMinSpeed(edit) =>
            {
                interface.schedule_overall_node_update();
                moving_mut!().undo_path_nodes_min_speed_edit(edit);
            },
            Self::PathNodeAccel(edit) =>
            {
                interface.schedule_overall_node_update();
                moving_mut!().undo_path_nodes_accel_travel_percentage_edit(edit);
            },
            Self::PathNodeDecel(edit) =>
            {
                interface.schedule_overall_node_update();
                moving_mut!().undo_path_nodes_decel_travel_percentage_edit(edit);
            },
            Self::ThingDraw(thing) => *thing = interface.despawn_thing(single!(), true).into(),
            Self::DrawnThingDespawn(thing) => interface.spawn_thing(single!(), std::mem::take(thing).unwrap(), true),
            Self::ThingSpawn(thing) => *thing = interface.despawn_thing(single!(), false).into(),
            Self::ThingDespawn(thing) => interface.spawn_thing(single!(), std::mem::take(thing).unwrap(), false),
            Self::ThingMove(d) =>
            {
                for id in identifiers
                {
                    interface.thing_mut(*id).move_by_delta(-*d);
                }
            },
            Self::TextureMove(delta) =>
            {
                for id in identifiers
                {
                    interface.brush_mut(*id).move_texture(drawing_resources, -*delta);
                }
            },
            Self::Texture(texture) =>
            {
                match texture
                {
                    Some(tex) =>
                    {
                        match interface.set_texture(drawing_resources, single!(), tex)
                        {
                            TextureSetResult::Unchanged => panic!("Texture change undo failed."),
                            TextureSetResult::Changed(prev) => *tex = prev,
                            TextureSetResult::Set => *texture = None
                        };
                    },
                    None =>
                    {
                        *texture =
                            interface.remove_texture(single!()).name().to_owned().into();
                    }
                };
            },
            Self::TextureRemoval(texture) =>
            {
                interface.set_texture_settings(single!(), std::mem::take(texture).unwrap());
            },
            Self::TextureScaleDelta(delta) =>
            {
                for id in identifiers
                {
                    let mut brush = interface.brush_mut(*id);

                    let texture = brush.texture_settings().unwrap();
                    let scale_x = texture.scale_x() - delta.x;
                    let scale_y = texture.scale_y() - delta.y;

                    _ = brush.set_texture_scale_x(drawing_resources, scale_x).unwrap();
                    _ = brush.set_texture_scale_y(drawing_resources, scale_y).unwrap();
                }
            },
            Self::TextureAngleDelta(delta) =>
            {
                for id in identifiers
                {
                    let mut brush = interface.brush_mut(*id);
                    let angle = brush.texture_settings().unwrap().angle() - *delta;
                    _ = brush.set_texture_angle(drawing_resources, angle).unwrap();
                }
            },
            Self::Sprite(value, offset_x, offset_y) =>
            {
                (*value, *offset_x, *offset_y) = interface.set_single_sprite(drawing_resources, single!(), value.enabled());
            },
            Self::ListAnimationFrameRemoval(index, name, time) =>
            {
                for id in identifiers
                {
                    interface.brush_mut(*id).insert_list_animation_frame(
                        *index,
                        name,
                        *time
                    );
                }
            },
            Self::ListAnimationNewFrame(_) =>
            {
                for id in identifiers
                {
                    interface.brush_mut(*id).pop_list_animation_frame();
                }
            }
        );

        let id = single!();

        if self.thing_common(interface, id) || self.brush_common(drawing_resources, interface, id)
        {
            return;
        }

        let mut brush = interface.brush_mut(id);

        match self
        {
            Self::VertexesMove(vxs_moves) =>
            {
                for vxs_move in vxs_moves.iter().rev()
                {
                    brush.undo_vertexes_move(drawing_resources, vxs_move);
                }
            },
            Self::VertexInsertion((_, idx)) =>
            {
                brush.delete_vertex_at_index(drawing_resources, (*idx).into());
            },
            Self::VertexesDeletion(vxs) =>
            {
                for (vx, idx) in vxs
                {
                    brush.insert_vertex_at_index(drawing_resources, *vx, (*idx).into(), true);
                }
            },
            Self::SidesDeletion(vxs) =>
            {
                for (vx, idx, selected) in vxs
                {
                    brush.insert_vertex_at_index(drawing_resources, *vx, (*idx).into(), *selected);
                }
            },
            Self::VertexesSnap(snap) =>
            {
                brush.move_vertexes_at_indexes(
                    snap.iter().map(|(idxs, delta)| (idxs.iter(), -*delta))
                );
            },
            _ => unreachable!()
        };
    }

    //==============================================================
    // Redo

    /// Executes the redo procedure associated with the value of `self`.
    #[inline]
    pub fn redo(
        &mut self,
        interface: &mut UndoRedoInterface,
        drawing_resources: &mut DrawingResources,
        ui: &mut Ui,
        identifiers: &HvVec<Id>,
        property: Option<&String>
    )
    {
        /// Returns the first [`Id`] of `identifiers`
        macro_rules! single {
            () => {
                identifiers[0]
            };
        }

        /// Returns the [`MovingMut`] of the entity with the [`Id`] returned by `single`.
        macro_rules! moving_mut {
            () => {
                interface.moving_mut(single!())
            };
        }

        if let Some(texture) = &mut self
            .texture(unsafe { std::ptr::from_mut(drawing_resources).as_mut() }.unwrap(), ui)
        {
            if self.default_animation_common(texture.animation_mut_set_dirty())
            {
                return;
            }

            match self
            {
                Self::TListAnimationFrameRemoval(_, index, ..) =>
                {
                    texture
                        .animation_mut_set_dirty()
                        .get_list_animation_mut()
                        .remove(*index);
                },
                Self::TListAnimationNewFrame(_, name) =>
                {
                    texture.animation_mut_set_dirty().get_list_animation_mut().push(name);
                },
                _ => unreachable!()
            };

            return;
        }

        if self.property(interface, single!(), property)
        {
            return;
        }

        if self.brushes_common(interface, drawing_resources, identifiers)
        {
            return;
        }

        match_and_return!(
            self,
            Self::BrushDraw(data) =>
            {
                interface.spawn_brush(
                    single!(),
                    std::mem::take(data).unwrap(),
                    BrushType::Drawn
                );
            },
            Self::BrushSpawn(data, selected) =>
            {
                interface.spawn_brush(
                    single!(),
                    std::mem::take(data).unwrap(),
                    BrushType::from_selection(*selected)
                );
            },
            Self::DrawnBrushDespawn(data) =>
            {
                *data = interface.despawn_brush(single!(), BrushType::Drawn).into();
            },
            Self::BrushDespawn(data, selected) =>
            {
                *data = interface.despawn_brush(single!(), BrushType::from_selection(*selected)).into();
            },
            Self::EntitySelection =>
            {
                for id in identifiers
                {
                    interface.select_entity(*id);
                }
            },
            Self::EntityDeselection =>
            {
                for id in identifiers
                {
                    interface.deselect_entity(*id);
                }
            },
            Self::SubtracteeSelection =>
            {
                for id in identifiers
                {
                    interface.insert_subtractee(*id);
                }
            },
            Self::SubtracteeDeselection =>
            {
                for id in identifiers
                {
                    interface.remove_subtractee(*id);
                }
            },
            Self::BrushMove(d, move_texture) =>
            {
                for id in identifiers
                {
                    interface
                        .brush_mut(*id)
                        .move_polygon(drawing_resources, *d, *move_texture);
                }
            },
            Self::FreeDrawPointInsertion(p, idx) => interface.insert_free_draw_point(*p, *idx as usize),
            Self::FreeDrawPointDeletion(p, idx) => interface.delete_free_draw_point(*p, *idx as usize),
            Self::Flip(flip, flip_texture) =>
            {
                let func = match flip
                {
                    Flip::Above(_) => Brush::flip_above,
                    Flip::Below(_) => Brush::flip_below,
                    Flip::Left(_) => Brush::flip_left,
                    Flip::Right(_) => Brush::flip_right
                };

                for id in identifiers
                {
                    func(
                        &mut interface.brush_mut(*id),
                        drawing_resources,
                        flip.mirror(),
                        *flip_texture
                    );
                }
            },
            Self::PathCreation(path) => interface.set_path(single!(), std::mem::take(path).unwrap()),
            Self::PathDeletion(path) =>
            {
                interface.schedule_overall_node_update();
                *path = interface.remove_path(single!()).into();
            },
            Self::Anchor(anchor) => interface.insert_anchor(single!(), *anchor),
            Self::Disanchor(anchor) => interface.remove_anchor(single!(), *anchor),
            Self::PathNodesSelection(idxs) =>
            {
                interface.schedule_overall_node_update();

                let mut moving = moving_mut!();

                for idx in idxs
                {
                    moving.toggle_path_node_at_index(*idx as usize);
                }
            },
            Self::PathNodeInsertion((pos, idx)) =>
            {
                moving_mut!().insert_path_node_at_index(*pos, *idx as usize);
            },
            Self::PathNodesMove(nodes_move) =>
            {
                let mut moving = moving_mut!();

                for node_move in nodes_move
                {
                    moving.redo_path_nodes_move(node_move);
                }
            },
            Self::PathNodesDeletion(_) =>
            {
                interface.schedule_overall_node_update();
                moving_mut!().redo_selected_path_nodes_deletion();
            },
            Self::PathNodesSnap(snap) =>
            {
                moving_mut!().move_path_nodes_at_indexes(snap);
            },
            Self::PathNodeStandby(edit) =>
            {
                interface.schedule_overall_node_update();
                moving_mut!().redo_path_nodes_standby_time_edit(edit);
            },
            Self::PathNodeMaxSpeed(edit) =>
            {
                interface.schedule_overall_node_update();
                moving_mut!().redo_path_nodes_max_speed_edit(edit);
            },
            Self::PathNodeMinSpeed(edit) =>
            {
                interface.schedule_overall_node_update();
                moving_mut!().redo_path_nodes_min_speed_edit(edit);
            },
            Self::PathNodeAccel(edit) =>
            {
                interface.schedule_overall_node_update();
                moving_mut!().redo_path_nodes_accel_travel_percentage_edit(edit);
            },
            Self::PathNodeDecel(edit) =>
            {
                interface.schedule_overall_node_update();
                moving_mut!().redo_path_nodes_decel_travel_percentage_edit(edit);
            },
            Self::ThingDraw(thing) => interface.spawn_thing(single!(), std::mem::take(thing).unwrap(), true),
            Self::DrawnThingDespawn(thing) => *thing = interface.despawn_thing(single!(), true).into(),
            Self::ThingSpawn(thing) => interface.spawn_thing(single!(), std::mem::take(thing).unwrap(), false),
            Self::ThingDespawn(thing) => *thing = interface.despawn_thing(single!(), false).into(),
            Self::ThingMove(d) =>
            {
                for id in identifiers
                {
                    interface.thing_mut(*id).move_by_delta(*d);
                }
            },
            Self::TextureMove(delta) =>
            {
                for id in identifiers
                {
                    interface.brush_mut(*id).move_texture(drawing_resources, *delta);
                }
            },
            Self::Texture(texture) =>
            {
                *texture = match interface.set_texture(
                    drawing_resources,
                    single!(),
                    texture.as_ref().unwrap()
                )
                {
                    TextureSetResult::Unchanged => panic!("Texture change redo failed."),
                    TextureSetResult::Changed(prev) => prev.into(),
                    TextureSetResult::Set => None
                };
            },
            Self::TextureRemoval(texture) => *texture = interface.remove_texture(single!()).into(),
            Self::TextureScaleDelta(delta) =>
            {
                for id in identifiers
                {
                    let mut brush = interface.brush_mut(*id);

                    let texture = brush.texture_settings().unwrap();
                    let scale_x = texture.scale_x() + delta.x;
                    let scale_y = texture.scale_y() + delta.y;

                    _ = brush.set_texture_scale_x(drawing_resources, scale_x).unwrap();
                    _ = brush.set_texture_scale_y(drawing_resources, scale_y).unwrap();
                }
            },
            Self::TextureAngleDelta(delta) =>
            {
                for id in identifiers
                {
                    let mut brush = interface.brush_mut(*id);
                    let angle = brush.texture_settings().unwrap().angle() + *delta;
                    _ = brush.set_texture_angle(drawing_resources, angle).unwrap();
                }
            },
            Self::Sprite(value, offset_x, offset_y) =>
            {
                (*value, *offset_x, *offset_y) = interface.set_single_sprite(drawing_resources, single!(), value.enabled());
            },
            Self::ListAnimationFrameRemoval(index, ..) =>
            {
                for id in identifiers
                {
                    interface.brush_mut(*id).remove_list_animation_frame(*index);
                }
            },
            Self::ListAnimationNewFrame(name) =>
            {
                for id in identifiers
                {
                    interface.brush_mut(*id).push_list_animation_frame(name);
                }
            }
        );

        let id = single!();

        if self.thing_common(interface, id) || self.brush_common(drawing_resources, interface, id)
        {
            return;
        }

        let mut brush = interface.brush_mut(id);

        match self
        {
            Self::VertexesMove(vxs_moves) =>
            {
                for vxs_move in vxs_moves
                {
                    brush.redo_vertexes_move(drawing_resources, vxs_move);
                }
            },
            Self::VertexInsertion((vx, idx)) =>
            {
                brush.insert_vertex_at_index(drawing_resources, *vx, (*idx).into(), false);
            },
            Self::VertexesDeletion(vxs) =>
            {
                for idx in vxs.iter().rev().map(|(_, idx)| idx)
                {
                    brush.delete_vertex_at_index(drawing_resources, (*idx).into());
                }
            },
            Self::SidesDeletion(vxs) =>
            {
                for idx in vxs.iter().rev().map(|(_, idx, _)| idx)
                {
                    brush.delete_vertex_at_index(drawing_resources, (*idx).into());
                }
            },
            Self::VertexesSnap(snap) =>
            {
                brush.move_vertexes_at_indexes(
                    snap.iter().map(|(idxs, delta)| (idxs.iter(), *delta))
                );
            },
            _ => unreachable!()
        };
    }
}
