//=======================================================================//
// IMPORTS
//
//=======================================================================//

use glam::Vec2;
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
            texture::{
                TextureInterface,
                TextureReset,
                TextureRotation,
                TextureScale,
                TextureSettings,
                TextureSpriteSet
            }
        },
        editor::state::{core::UndoRedoInterface, grid::Grid, ui::Ui},
        path::{MovementValueEdit, NodesMove, Path, StandbyValueEdit},
        properties::Value,
        thing::{catalog::ThingsCatalog, ThingId, ThingInstanceData}
    },
    utils::{hull::Flip, identifiers::Id},
    HvVec
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

/// An enum used to categorize brushes in three categories: not selected, selected, and drawn.
#[derive(Clone, Copy)]
pub(in crate::map::editor::state) enum BrushType
{
    /// Non selected brush.
    NotSelected,
    /// Selected brush.
    Selected,
    /// Drawn brush.
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

    /// Whether `self` is `BrushType::Selected`.
    #[inline]
    #[must_use]
    pub const fn selected(self) -> bool { !matches!(self, Self::NotSelected) }

    /// Whether `self` is `BrushType::Drawn`.
    #[inline]
    #[must_use]
    pub const fn drawn(self) -> bool { matches!(self, Self::Drawn) }
}

//=======================================================================//

/// The type of the [`Edit`] stored in the [`EditsHistory`].
pub(in crate::map::editor::state::edits_history) enum EditType
{
    /// Drawn brush.
    DrawnBrush(Option<BrushData>),
    /// Drawn brush despawned.
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
    BrushFlip(Flip, bool),
    /// Path created.
    PathCreation(Option<Path>),
    /// Path deleted.
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
    PathNodeAcceleration(MovementValueEdit),
    /// Changed path node deceleration percentage.
    PathNodeDeceleration(MovementValueEdit),
    /// Changed path node max speed.
    PathNodeMaxSpeed(MovementValueEdit),
    /// Changed path node minimum speed.
    PathNodeMinSpeed(MovementValueEdit),
    /// Brush attached.
    BrushAttachment(Id),
    /// Brush disachored.
    BrushDetachment(Id),
    /// Thing drawn on map.
    DrawnThing(Option<ThingInstanceData>),
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
    /// Brush texture change.
    TextureChange(Option<String>),
    /// Brush texture removed.
    TextureRemoval(Option<TextureSettings>),
    /// Toggled sprite setting of texture.
    SpriteToggle(TextureSpriteSet),
    /// Texture flip, true -> vertical, false -> horizontal.
    TextureFlip(bool),
    /// Texture scaled with specified delta.
    TextureScale(TextureScale),
    /// Texture x scale changed.
    TextureScaleX(f32),
    /// Texture y scale changed.
    TextureScaleY(f32),
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
    /// Texture rotated.
    TextureRotation(TextureRotation),
    /// Texture draw height change.
    TextureHeight(i8),
    /// Texture animation change.
    AnimationChange(Animation),
    /// Texture reset.
    TextureReset(TextureReset),
    /// Texture animation frame info moved up. true -> atlas, false -> list.
    ListAnimationFrameMoveUp(usize, bool),
    /// Texture animation frame info moved down. true -> atlas, false -> list.
    ListAnimationFrameMoveDown(usize, bool),
    /// List animation frame addition.
    ListAnimationNewFrame(String),
    /// List animation frame texture change.
    ListAnimationTexture(usize, String),
    /// List animation frame time change.
    ListAnimationTime(usize, f32),
    /// List animation frame removal.
    ListAnimationFrameRemoval(usize, String, f32),
    /// Atlas animation x partitioning change.
    AtlasAnimationColumns(u32),
    /// Atlas animation y partitioning change.
    AtlasAnimationRows(u32),
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
    /// Entity property change.
    PropertyChange(Value)
}

impl std::fmt::Debug for EditType
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        let str = match self
        {
            Self::DrawnBrush(_) => "DrawnBrush",
            Self::DrawnBrushDespawn(_) => "DrawnBrushDespawn",
            Self::BrushSpawn(..) => "BrushSpawn",
            Self::BrushDespawn(..) => "BrushDespawn",
            Self::EntitySelection => "EntitySelection",
            Self::EntityDeselection => "EntityDeselection",
            Self::SubtracteeSelection => "SubtracteeSelection",
            Self::SubtracteeDeselection => "SubtracteeDeselection",
            Self::PolygonEdit(_) => "PolygonEdit",
            Self::BrushMove(..) => "BrushMove",
            Self::FreeDrawPointInsertion(..) => "FreeDrawPointInsertion",
            Self::FreeDrawPointDeletion(..) => "FreeDrawPointDeletion",
            Self::VertexInsertion(_) => "VertexInsertion",
            Self::VertexesDeletion(_) => "VertexesDeletion",
            Self::VertexesMove(_) => "VertexesMove",
            Self::SidesDeletion(_) => "SidesDeletion",
            Self::VertexesSelection(_) => "VertexesSelection",
            Self::VertexesSnap(_) => "VertexesSnap",
            Self::BrushFlip(..) => "BrushFlip",
            Self::PathCreation(_) => "PathCreation",
            Self::PathDeletion(_) => "PathDeletion",
            Self::PathNodesSelection(_) => "PathNodesSelection",
            Self::PathNodeInsertion(_) => "PathNodeInsertion",
            Self::PathNodesMove(_) => "PathNodesMove",
            Self::PathNodesDeletion(_) => "PathNodesDeletion",
            Self::PathNodesSnap(_) => "PathNodesSnap",
            Self::PathNodeStandby(_) => "PathNodeStandby",
            Self::PathNodeAcceleration(_) => "PathNodeAcceleration",
            Self::PathNodeDeceleration(_) => "PathNodeDeceleration",
            Self::PathNodeMaxSpeed(_) => "PathNodeMaxSpeed",
            Self::PathNodeMinSpeed(_) => "PathNodeMinSpeed",
            Self::BrushAttachment(_) => "BrushAttachment",
            Self::BrushDetachment(_) => "BrushDetachment",
            Self::DrawnThing(_) => "DrawnThing",
            Self::DrawnThingDespawn(_) => "DrawnThingDespawn",
            Self::ThingSpawn(_) => "ThingSpawn",
            Self::ThingDespawn(_) => "ThingDespawn",
            Self::ThingMove(_) => "ThingMove",
            Self::ThingChange(_) => "ThingChange",
            Self::TextureChange(_) => "TextureChange",
            Self::TextureRemoval(_) => "TextureRemoval",
            Self::SpriteToggle(_) => "SpriteToggle",
            Self::TextureFlip(_) => "TextureFlip",
            Self::TextureScale(_) => "TextureScale",
            Self::TextureScaleX(_) => "TextureScaleX",
            Self::TextureScaleY(_) => "TextureScaleY",
            Self::TextureOffsetX(_) => "TextureOffsetX",
            Self::TextureOffsetY(_) => "TextureOffsetY",
            Self::TextureScrollX(_) => "TextureScrollX",
            Self::TextureScrollY(_) => "TextureScrollY",
            Self::TextureParallaxX(_) => "TextureParallaxX",
            Self::TextureParallaxY(_) => "TextureParallaxY",
            Self::TextureMove(_) => "TextureMove",
            Self::TextureRotation(_) => "TextureRotation",
            Self::TextureHeight(_) => "TextureHeight",
            Self::AnimationChange(_) => "AnimationChange",
            Self::TextureReset(_) => "TextureReset",
            Self::ListAnimationFrameMoveUp(..) => "ListAnimationFrameMoveUp",
            Self::ListAnimationFrameMoveDown(..) => "ListAnimationFrameMoveDown",
            Self::ListAnimationNewFrame(_) => "ListAnimationNewFrame",
            Self::ListAnimationTexture(..) => "ListAnimationTexture",
            Self::ListAnimationTime(..) => "ListAnimationTime",
            Self::ListAnimationFrameRemoval(..) => "ListAnimationFrameRemoval",
            Self::AtlasAnimationColumns(_) => "AtlasAnimationColumns",
            Self::AtlasAnimationRows(_) => "AtlasAnimationRows",
            Self::AtlasAnimationLen(_) => "AtlasAnimationLen",
            Self::AtlasAnimationTiming(_) => "AtlasAnimationTiming",
            Self::AtlasAnimationUniformTime(_) => "AtlasAnimationUniformTime",
            Self::AtlasAnimationFrameTime(..) => "AtlasAnimationFrameTime",
            Self::TAnimation(..) => "TAnimation",
            Self::TAnimationMoveUp(..) => "TAnimationMoveUp",
            Self::TAnimationMoveDown(..) => "TAnimationMoveDown",
            Self::TListAnimationNewFrame(..) => "TListAnimationNewFrame",
            Self::TListAnimationTexture(..) => "TListAnimationTexture",
            Self::TListAnimationTime(..) => "TListAnimationTime",
            Self::TListAnimationFrameRemoval(..) => "TListAnimationFrameRemoval",
            Self::TAtlasAnimationX(..) => "TAtlasAnimationX",
            Self::TAtlasAnimationY(..) => "TAtlasAnimationY",
            Self::TAtlasAnimationLen(..) => "TAtlasAnimationLen",
            Self::TAtlasAnimationTiming(..) => "TAtlasAnimationTiming",
            Self::TAtlasAnimationUniformTime(..) => "TAtlasAnimationUniformTime",
            Self::TAtlasAnimationFrameTime(..) => "TAtlasAnimationFrameTime",
            Self::PropertyChange(_) => "PropertyChange"
        };

        write!(f, "{str}")
    }
}

impl EditType
{
    #[inline]
    #[must_use]
    pub const fn tag(&self) -> &'static str
    {
        match self
        {
            Self::DrawnBrush(..) | Self::BrushSpawn(..) => "Brushes spawn",
            Self::DrawnBrushDespawn(..) | Self::BrushDespawn(..) => "Brushes despawn",
            Self::EntitySelection | Self::EntityDeselection => "Entities selection",
            Self::SubtracteeSelection | Self::SubtracteeDeselection => "Subtractees selection",
            Self::PolygonEdit(..) => "Polygon edit",
            Self::BrushMove(..) => "Brushes move",
            Self::FreeDrawPointInsertion(..) => "Free draw point insertion",
            Self::FreeDrawPointDeletion(..) => "Free draw point deletion",
            Self::VertexInsertion(..) => "Vertex insertion",
            Self::VertexesDeletion(..) => "Vertexes deletion",
            Self::VertexesMove(..) => "Vertexes move",
            Self::SidesDeletion(..) => "Sides deletion",
            Self::VertexesSelection(..) => "Vertexes selection",
            Self::VertexesSnap(..) => "Vertexes snap",
            Self::BrushFlip(..) => "Brushes flip",
            Self::PathCreation(..) => "Path creation",
            Self::PathDeletion(..) => "Paths deletion",
            Self::PathNodesSelection(..) => "Path nodes selection",
            Self::PathNodeInsertion(..) => "Path nodes insertion",
            Self::PathNodesMove(..) => "Path nodes move",
            Self::PathNodesDeletion(..) => "Path nodes deletion",
            Self::PathNodesSnap(..) => "Path nodes snap",
            Self::PathNodeStandby(..) => "Path node standby",
            Self::PathNodeAcceleration(..) => "Path node acceleration",
            Self::PathNodeDeceleration(..) => "Path node deceleration",
            Self::PathNodeMaxSpeed(..) => "Path node max speed",
            Self::PathNodeMinSpeed(..) => "Path node min speed",
            Self::BrushAttachment(..) => "Brush attachment",
            Self::BrushDetachment(..) => "Brush detachment",
            Self::DrawnThing(..) | Self::ThingSpawn(..) => "Thing spawn",
            Self::ThingDespawn(..) | Self::DrawnThingDespawn(..) => "Things despawn",
            Self::ThingMove(..) => "Thing move",
            Self::ThingChange(..) => "Things change",
            Self::TextureChange(..) => "Textures change",
            Self::TextureRemoval(..) => "Textures removal",
            Self::SpriteToggle(..) => "Sprites toggle",
            Self::TextureFlip(..) | Self::TextureScale(..) => "Textures scale",
            Self::TextureScaleX(..) => "Textures scale x",
            Self::TextureScaleY(..) => "Textures scale y",
            Self::TextureOffsetX(..) => "Textures offset x",
            Self::TextureOffsetY(..) => "Textures offset y",
            Self::TextureScrollX(..) => "Textures scroll x",
            Self::TextureScrollY(..) => "Textures scroll y",
            Self::TextureParallaxX(..) => "Textures parallax x",
            Self::TextureParallaxY(..) => "Textures parallax y",
            Self::TextureMove(..) => "Textures move",
            Self::TextureRotation(..) => "Textures rotation",
            Self::TextureHeight(..) => "Textures height",
            Self::AnimationChange(..) => "Animations change",
            Self::TextureReset(..) => "Textures reset",
            Self::ListAnimationFrameMoveUp(..) => "List animation frame move up",
            Self::ListAnimationFrameMoveDown(..) => "List animation frame move down",
            Self::ListAnimationNewFrame(..) => "List animation new frame",
            Self::ListAnimationTexture(..) => "List animation texture",
            Self::ListAnimationTime(..) => "List animation time",
            Self::ListAnimationFrameRemoval(..) => "List animation frame removal",
            Self::AtlasAnimationColumns(..) => "Atlas animations columns",
            Self::AtlasAnimationRows(..) => "Atlas animations rows",
            Self::AtlasAnimationLen(..) => "Atlas animations length",
            Self::AtlasAnimationTiming(..) => "Atlas animations timing",
            Self::AtlasAnimationUniformTime(..) => "Atlas animations uniform time",
            Self::AtlasAnimationFrameTime(..) => "Atlas animations frame time",
            Self::TAnimation(..) => "Texture default animation",
            Self::TAnimationMoveUp(..) => "Texture default animation move up",
            Self::TAnimationMoveDown(..) => "Texture default animation move down",
            Self::TListAnimationNewFrame(..) => "Texture default list animation new frame",
            Self::TListAnimationTexture(..) => "Texture default list animation texture",
            Self::TListAnimationTime(..) => "Texture default list animation time",
            Self::TListAnimationFrameRemoval(..) => "Texture default list animation frame removal",
            Self::TAtlasAnimationX(..) => "Texture default atlas animation x",
            Self::TAtlasAnimationY(..) => "Texture default atlas animation y",
            Self::TAtlasAnimationLen(..) => "Texture default atlas animation len",
            Self::TAtlasAnimationTiming(..) => "Texture default atlas animation timing",
            Self::TAtlasAnimationUniformTime(..) => "Texture default atlas animation uniform time",
            Self::TAtlasAnimationFrameTime(..) => "Texture default atlas animation frame time",
            Self::PropertyChange(..) => "Properties change"
        }
    }

    /// Whether `self` is an edit that is only useful as long as the current tool remains unchanged.
    #[inline]
    #[must_use]
    pub const fn tool_edit(&self) -> bool
    {
        matches!(
            self,
            Self::DrawnBrush(_) |
                Self::DrawnBrushDespawn(_) |
                Self::DrawnThing(..) |
                Self::DrawnThingDespawn(..) |
                Self::VertexesSelection(_) |
                Self::PathNodesSelection(_) |
                Self::SubtracteeSelection |
                Self::SubtracteeDeselection |
                Self::FreeDrawPointInsertion(..) |
                Self::FreeDrawPointDeletion(..)
        )
    }

    /// Whether `self` is a texture edit.
    #[inline]
    #[must_use]
    pub const fn texture_edit(&self) -> bool
    {
        matches!(
            self,
            Self::TextureChange(_) |
                Self::TextureRemoval(_) |
                Self::SpriteToggle(..) |
                Self::TextureFlip(_) |
                Self::TextureScale(_) |
                Self::TextureScaleX(_) |
                Self::TextureScaleY(_) |
                Self::TextureOffsetX(_) |
                Self::TextureOffsetY(_) |
                Self::TextureScrollX(_) |
                Self::TextureScrollY(_) |
                Self::TextureParallaxX(_) |
                Self::TextureParallaxY(_) |
                Self::TextureMove(_) |
                Self::TextureRotation(_) |
                Self::TextureHeight(_) |
                Self::AnimationChange(_) |
                Self::ListAnimationFrameMoveUp(..) |
                Self::ListAnimationFrameMoveDown(..) |
                Self::ListAnimationNewFrame(_) |
                Self::ListAnimationTexture(..) |
                Self::ListAnimationTime(..) |
                Self::ListAnimationFrameRemoval(..) |
                Self::AtlasAnimationColumns(_) |
                Self::AtlasAnimationRows(_) |
                Self::AtlasAnimationLen(_) |
                Self::AtlasAnimationTiming(_) |
                Self::AtlasAnimationUniformTime(_) |
                Self::AtlasAnimationFrameTime(..)
        )
    }

    /// Whether `self` represents a [`ThingInstance`] edit.
    #[inline]
    #[must_use]
    pub const fn thing_edit(&self) -> bool
    {
        matches!(
            self,
            Self::ThingChange(_) |
                Self::DrawnThing(..) |
                Self::ThingSpawn(..) |
                Self::DrawnThingDespawn(..) |
                Self::ThingDespawn(..) |
                Self::ThingMove(_)
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

    /// Actions common to both the undo and redo procedures that apply to multiple brushes.
    /// Returns whether the edit was undone/redone.
    #[inline]
    #[must_use]
    fn brushes_common(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        interface: &mut UndoRedoInterface,
        identifiers: &HvVec<Id>
    ) -> bool
    {
        match self
        {
            Self::BrushMove(d, move_texture) =>
            {
                *d = -*d;

                for id in identifiers
                {
                    interface
                        .brush_mut(drawing_resources, grid, *id)
                        .move_polygon(*d, *move_texture);
                }
            },
            Self::TextureMove(delta) =>
            {
                *delta = -*delta;

                for id in identifiers
                {
                    interface.brush_mut(drawing_resources, grid, *id).move_texture(*delta);
                }
            },
            Self::TextureFlip(y) =>
            {
                if *y
                {
                    for id in identifiers
                    {
                        interface.brush_mut(drawing_resources, grid, *id).flip_scale_y();
                    }
                }
                else
                {
                    for id in identifiers
                    {
                        interface
                            .brush_mut(drawing_resources, grid, *id)
                            .flip_texture_scale_x();
                    }
                }
            },
            Self::ListAnimationTexture(index, name) =>
            {
                let mut iter = identifiers.iter();

                let prev = std::mem::replace(
                    name,
                    interface
                        .brush_mut(drawing_resources, grid, *iter.next_value())
                        .set_list_animation_texture(*index, name)
                        .unwrap()
                        .clone()
                );

                for id in identifiers
                {
                    _ = interface
                        .brush_mut(drawing_resources, grid, *id)
                        .set_list_animation_texture(*index, &prev);
                }
            },
            Self::ListAnimationTime(index, time) =>
            {
                let mut iter = identifiers.iter();
                let value = interface
                    .brush_mut(drawing_resources, grid, *iter.next_value())
                    .set_texture_list_animation_time(*index, *time)
                    .unwrap();

                for id in iter
                {
                    _ = interface
                        .brush_mut(drawing_resources, grid, *id)
                        .set_texture_list_animation_time(*index, *time);
                }

                *time = value;
            },
            Self::ListAnimationFrameMoveUp(index, atlas) =>
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
                    func(&mut interface.brush_mut(drawing_resources, grid, *id), *index);
                }
            },
            Self::ListAnimationFrameMoveDown(index, atlas) =>
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
                    func(&mut interface.brush_mut(drawing_resources, grid, *id), *index);
                }
            },
            Self::TextureReset(value) =>
            {
                for id in identifiers
                {
                    interface
                        .brush_mut(drawing_resources, grid, *id)
                        .undo_redo_texture_reset(value);
                }
            },
            _ => return false
        };

        true
    }

    /// Actions common to both the undo and redo procedures that apply to a single brush.
    /// Returns whether the edit was undone/redone.
    #[inline]
    #[must_use]
    fn brush_common(
        &mut self,
        drawing_resources: &DrawingResources,
        grid: &Grid,
        interface: &mut UndoRedoInterface,
        identifier: Id
    ) -> bool
    {
        if let Self::SpriteToggle(value) = self
        {
            interface.undo_redo_texture_sprite(drawing_resources, grid, identifier, value);
            return true;
        }

        let mut brush = interface.brush_mut(drawing_resources, grid, identifier);

        /// Generates the match arms.
        macro_rules! arms {
            ($((
                $arm:ident,
                $value:ident
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
                    Self::VertexesSnap(snap) =>
                    {
                        for (_, delta) in &mut *snap
                        {
                            *delta = -*delta;
                        }

                        brush.move_vertexes_at_indexes(
                            snap.iter().map(|(idxs, delta)| (idxs.iter(), *delta))
                        );
                    },
                    Self::PolygonEdit(cp) => brush.swap_polygon(cp),
                    $(Self::$arm(value) =>
                    {
                        paste::paste! { *value = brush.[< set_texture_ $value >](*value).unwrap(); }
                    }),+
                    Self::TextureScale(scale) => brush.scale_texture(scale),
                    Self::TextureRotation(rotation) => brush.rotate_texture(rotation),
                    Self::AnimationChange(value) =>
                    {
                        *value = brush.set_texture_animation(std::mem::take(value));
                    },
                    Self::AtlasAnimationTiming(timing) =>
                    {
                        *timing = brush.set_texture_atlas_animation_timing(std::mem::take(timing).unwrap()).into();
                    },
                    Self::AtlasAnimationFrameTime(index, time) =>
                    {
                        *time = brush.set_texture_atlas_animation_frame_time(*index, *time).unwrap();
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
            (TextureOffsetX, offset_x),
            (TextureOffsetY, offset_y),
            (TextureScaleX, scale_x),
            (TextureScaleY, scale_y),
            (TextureHeight, height),
            (AtlasAnimationColumns, atlas_animation_x_partition),
            (AtlasAnimationRows, atlas_animation_y_partition),
            (AtlasAnimationLen, atlas_animation_len),
            (AtlasAnimationUniformTime, atlas_animation_uniform_time)
        );

        true
    }

    #[inline]
    #[must_use]
    fn things_common(
        &mut self,
        things_catalog: &ThingsCatalog,
        interface: &mut UndoRedoInterface,
        identifiers: &HvVec<Id>
    ) -> bool
    {
        match self
        {
            Self::ThingMove(d) =>
            {
                *d = -*d;

                for id in identifiers
                {
                    interface.thing_mut(things_catalog, *id).move_by_delta(*d);
                }
            },
            _ => return false
        };

        true
    }

    /// Actions common to both the undo and redo procedures that apply to a single [`Thing`].
    /// Returns whether the edit was undone/redone.
    #[inline]
    #[must_use]
    fn thing_common(
        &mut self,
        things_catalog: &ThingsCatalog,
        interface: &mut UndoRedoInterface,
        identifier: Id
    ) -> bool
    {
        match self
        {
            Self::ThingChange(id) => *id = interface.set_thing(things_catalog, identifier, *id),
            _ => return false
        };

        true
    }

    #[inline]
    #[must_use]
    fn moving_common(
        &mut self,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid,
        interface: &mut UndoRedoInterface,
        identifier: Id
    ) -> bool
    {
        match self
        {
            Self::PathNodesSnap(snap) =>
            {
                for (_, delta) in &mut *snap
                {
                    *delta = -*delta;
                }

                interface
                    .moving_mut(drawing_resources, things_catalog, grid, identifier)
                    .move_path_nodes_at_indexes(snap);
            },
            _ => return false
        };

        true
    }

    /// Actions common to both the undo and redo procedures that apply to a default animation.
    /// Returns whether the edit was undone/redone.
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
                        use crate::utils::misc::{ReplaceValue, TakeValue};
                        *value = animation.replace_value(value.take_value());
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
    /// Returns whether the edit was undone/redone.
    #[inline]
    #[must_use]
    fn property(
        &mut self,
        interface: &mut UndoRedoInterface,
        drawing_resources: &DrawingResources,
        things_catalog: &ThingsCatalog,
        grid: &Grid,
        identifiers: &HvVec<Id>,
        key: Option<&String>
    ) -> bool
    {
        if let Self::PropertyChange(value) = self
        {
            *value = interface.set_property(
                drawing_resources,
                things_catalog,
                grid,
                identifiers[0],
                key.unwrap(),
                value
            );
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
        things_catalog: &ThingsCatalog,
        grid: &Grid,
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
                interface.moving_mut(drawing_resources, things_catalog, grid, single!())
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

        if self.property(interface, drawing_resources, things_catalog, grid, identifiers, property)
        {
            return;
        }

        if self.brushes_common(drawing_resources, grid, interface, identifiers) ||
            self.things_common(things_catalog, interface, identifiers)
        {
            return;
        }

        match_and_return!(
            self,
            Self::DrawnBrush(data) =>
            {
                *data = interface.despawn_brush(drawing_resources, grid,  single!(), BrushType::Drawn).into();
            },
            Self::BrushSpawn(data, selected) =>
            {
                *data = interface.despawn_brush(drawing_resources, grid, single!(), BrushType::from_selection(*selected)).into();
            },
            Self::DrawnBrushDespawn(data) =>
            {
                interface.spawn_brush(
                    drawing_resources,
                    grid,
                    single!(),
                    std::mem::take(data).unwrap(),
                    BrushType::Drawn
                );
            },
            Self::BrushDespawn(data, selected) =>
            {
                interface.spawn_brush(
                    drawing_resources,
                    grid,
                    single!(),
                    std::mem::take(data).unwrap(),
                    BrushType::from_selection(*selected)
                );
            },
            Self::PathDeletion(path) =>
            {
                interface.schedule_overall_node_update();
                interface.set_path(drawing_resources, things_catalog, grid, single!(), std::mem::take(path).unwrap());
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
            Self::FreeDrawPointInsertion(p, idx) => interface.delete_free_draw_point(*p, *idx as usize),
            Self::FreeDrawPointDeletion(p, idx) => interface.insert_free_draw_point(*p, *idx as usize),
            Self::BrushFlip(flip, flip_texture) =>
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
                        &mut interface.brush_mut(drawing_resources, grid, *id),
                        flip.mirror(),
                        *flip_texture
                    );
                }
            },
            Self::PathCreation(path) => *path = interface.remove_path(drawing_resources, things_catalog, grid, single!()).into(),
            Self::BrushAttachment(attachment) => interface.detach(single!(), *attachment),
            Self::BrushDetachment(attachment) => interface.attach(single!(), *attachment),
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
            Self::PathNodeAcceleration(edit) =>
            {
                interface.schedule_overall_node_update();
                moving_mut!().undo_path_nodes_accel_travel_percentage_edit(edit);
            },
            Self::PathNodeDeceleration(edit) =>
            {
                interface.schedule_overall_node_update();
                moving_mut!().undo_path_nodes_decel_travel_percentage_edit(edit);
            },
            Self::DrawnThing(thing) => *thing = interface.despawn_thing(single!(), true).into(),
            Self::DrawnThingDespawn(thing) => interface.spawn_thing(things_catalog, single!(), std::mem::take(thing).unwrap(), true),
            Self::ThingSpawn(thing) => *thing = interface.despawn_thing(single!(), false).into(),
            Self::ThingDespawn(thing) => interface.spawn_thing(things_catalog, single!(), std::mem::take(thing).unwrap(), false),
            Self::TextureChange(texture) =>
            {
                match texture
                {
                    Some(tex) =>
                    {
                        match interface.set_texture(drawing_resources, grid, single!(), tex)
                        {
                            TextureSetResult::Unchanged => panic!("Texture change undo failed."),
                            TextureSetResult::Changed(prev) => *tex = prev,
                            TextureSetResult::Set => *texture = None
                        };
                    },
                    None =>
                    {
                        *texture =
                            interface.remove_texture(drawing_resources, grid, single!()).name().to_owned().into();
                    }
                };
            },
            Self::TextureRemoval(texture) =>
            {
                interface.set_texture_settings(drawing_resources, grid, single!(), std::mem::take(texture).unwrap());
            },
            Self::ListAnimationFrameRemoval(index, name, time) =>
            {
                for id in identifiers
                {
                    interface.brush_mut(drawing_resources, grid, *id).insert_list_animation_frame(
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
                    interface.brush_mut(drawing_resources, grid, *id).pop_list_animation_frame();
                }
            }
        );

        let id = single!();

        if self.thing_common(things_catalog, interface, id) ||
            self.brush_common(drawing_resources, grid, interface, id) ||
            self.moving_common(drawing_resources, things_catalog, grid, interface, id)
        {
            return;
        }

        let mut brush = interface.brush_mut(drawing_resources, grid, id);

        match self
        {
            Self::VertexesMove(vxs_moves) =>
            {
                for vxs_move in vxs_moves.iter().rev()
                {
                    brush.undo_vertexes_move(vxs_move);
                }
            },
            Self::VertexInsertion((_, idx)) =>
            {
                brush.delete_vertex_at_index((*idx).into());
            },
            Self::VertexesDeletion(vxs) =>
            {
                for (vx, idx) in vxs
                {
                    brush.insert_vertex_at_index(*vx, (*idx).into(), true);
                }
            },
            Self::SidesDeletion(vxs) =>
            {
                for (vx, idx, selected) in vxs
                {
                    brush.insert_vertex_at_index(*vx, (*idx).into(), *selected);
                }
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
        things_catalog: &ThingsCatalog,
        grid: &Grid,
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
                interface.moving_mut(drawing_resources, things_catalog, grid, single!())
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

        if self.property(interface, drawing_resources, things_catalog, grid, identifiers, property)
        {
            return;
        }

        if self.brushes_common(drawing_resources, grid, interface, identifiers) ||
            self.things_common(things_catalog, interface, identifiers)
        {
            return;
        }

        match_and_return!(
            self,
            Self::DrawnBrush(data) =>
            {
                interface.spawn_brush(
                    drawing_resources,
                    grid,
                    single!(),
                    std::mem::take(data).unwrap(),
                    BrushType::Drawn
                );
            },
            Self::BrushSpawn(data, selected) =>
            {
                interface.spawn_brush(
                    drawing_resources,
                    grid,
                    single!(),
                    std::mem::take(data).unwrap(),
                    BrushType::from_selection(*selected)
                );
            },
            Self::DrawnBrushDespawn(data) =>
            {
                *data = interface.despawn_brush(drawing_resources, grid, single!(), BrushType::Drawn).into();
            },
            Self::BrushDespawn(data, selected) =>
            {
                *data = interface.despawn_brush(drawing_resources, grid, single!(), BrushType::from_selection(*selected)).into();
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
            Self::FreeDrawPointInsertion(p, idx) => interface.insert_free_draw_point(*p, *idx as usize),
            Self::FreeDrawPointDeletion(p, idx) => interface.delete_free_draw_point(*p, *idx as usize),
            Self::BrushFlip(flip, flip_texture) =>
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
                        &mut interface.brush_mut(drawing_resources, grid, *id),
                        flip.mirror(),
                        *flip_texture
                    );
                }
            },
            Self::PathCreation(path) => interface.set_path(drawing_resources, things_catalog, grid, single!(), std::mem::take(path).unwrap()),
            Self::PathDeletion(path) =>
            {
                interface.schedule_overall_node_update();
                *path = interface.remove_path(drawing_resources, things_catalog, grid, single!()).into();
            },
            Self::BrushAttachment(attachment) => interface.attach(single!(), *attachment),
            Self::BrushDetachment(attachment) => interface.detach(single!(), *attachment),
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
            Self::PathNodeAcceleration(edit) =>
            {
                interface.schedule_overall_node_update();
                moving_mut!().redo_path_nodes_accel_travel_percentage_edit(edit);
            },
            Self::PathNodeDeceleration(edit) =>
            {
                interface.schedule_overall_node_update();
                moving_mut!().redo_path_nodes_decel_travel_percentage_edit(edit);
            },
            Self::DrawnThing(thing) => interface.spawn_thing(things_catalog, single!(), std::mem::take(thing).unwrap(), true),
            Self::DrawnThingDespawn(thing) => *thing = interface.despawn_thing(single!(), true).into(),
            Self::ThingSpawn(thing) => interface.spawn_thing(things_catalog, single!(), std::mem::take(thing).unwrap(), false),
            Self::ThingDespawn(thing) => *thing = interface.despawn_thing(single!(), false).into(),
            Self::TextureChange(texture) =>
            {
                *texture = match interface.set_texture(
                    drawing_resources,
                    grid,
                    single!(),
                    texture.as_ref().unwrap()
                )
                {
                    TextureSetResult::Unchanged => panic!("Texture change redo failed."),
                    TextureSetResult::Changed(prev) => prev.into(),
                    TextureSetResult::Set => None
                };
            },
            Self::TextureRemoval(texture) => *texture = interface.remove_texture(drawing_resources, grid, single!()).into(),
            Self::ListAnimationFrameRemoval(index, ..) =>
            {
                for id in identifiers
                {
                    interface.brush_mut(drawing_resources, grid, *id).remove_list_animation_frame(*index);
                }
            },
            Self::ListAnimationNewFrame(name) =>
            {
                for id in identifiers
                {
                    interface.brush_mut(drawing_resources, grid, *id).push_list_animation_frame(name);
                }
            }
        );

        let id = single!();

        if self.thing_common(things_catalog, interface, id) ||
            self.brush_common(drawing_resources, grid, interface, id) ||
            self.moving_common(drawing_resources, things_catalog, grid, interface, id)
        {
            return;
        }

        let mut brush = interface.brush_mut(drawing_resources, grid, id);

        match self
        {
            Self::VertexesMove(vxs_moves) =>
            {
                for vxs_move in vxs_moves
                {
                    brush.redo_vertexes_move(vxs_move);
                }
            },
            Self::VertexInsertion((vx, idx)) =>
            {
                brush.insert_vertex_at_index(*vx, (*idx).into(), false);
            },
            Self::VertexesDeletion(vxs) =>
            {
                for idx in vxs.iter().rev().map(|(_, idx)| idx)
                {
                    brush.delete_vertex_at_index((*idx).into());
                }
            },
            Self::SidesDeletion(vxs) =>
            {
                for idx in vxs.iter().rev().map(|(_, idx, _)| idx)
                {
                    brush.delete_vertex_at_index((*idx).into());
                }
            },
            _ => unreachable!()
        };
    }
}
