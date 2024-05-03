//=======================================================================//
// IMPORTS
//
//=======================================================================//

use super::edit_type::EditType;
use crate::{
    map::{
        drawer::drawing_resources::DrawingResources,
        editor::state::{core::UndoRedoInterface, ui::Ui},
        hv_vec,
        properties::Value,
        HvVec
    },
    utils::identifiers::Id
};

//=======================================================================//
// TYPES
//
//=======================================================================//

/// A map edit which can be undone and redone, and be made of multiple sub-edits.
#[derive(Debug)]
pub(in crate::map::editor::state::edits_history) struct Edit(
    HvVec<(HvVec<Id>, EditType)>,
    Option<String>
);

impl Default for Edit
{
    #[inline]
    #[must_use]
    fn default() -> Self { Self(hv_vec![], None) }
}

impl Edit
{
    //==============================================================
    // Info

    /// The amount of sub-edits the edit is made of.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize { self.0.len() }

    /// Whever `self` contains no sub-edits.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool { self.len() == 0 }

    /// Whever `self` contains a sub-edits that is only useful as long as the current tool remains
    /// unchanged.
    #[inline]
    #[must_use]
    pub fn contains_tool_edit(&self) -> bool { self.0.iter().any(|(_, et)| et.tool_edit()) }

    /// Whever `self` contains a texture sub-edit.
    #[inline]
    #[must_use]
    pub fn contains_texture_edit(&self) -> bool { self.0.iter().any(|(_, et)| et.texture_edit()) }

    /// Whever `self` contains a thing edit.
    #[inline]
    #[must_use]
    pub fn contains_thing_edit(&self) -> bool { self.0.iter().any(|(_, et)| et.thing_edit()) }

    /// Whever `self` contains a free draw sub-edit.
    #[inline]
    #[must_use]
    pub fn contains_free_draw_edit(&self) -> bool
    {
        self.0.iter().any(|(_, et)| {
            matches!(et, EditType::FreeDrawPointInsertion(..) | EditType::FreeDrawPointDeletion(..))
        })
    }

    /// Whever `self` only contains entity selection sub-edits.
    #[inline]
    #[must_use]
    pub fn only_contains_entity_selection_edits(&self) -> bool
    {
        if self.is_empty()
        {
            return false;
        }

        self.0
            .iter()
            .all(|(_, et)| matches!(et, EditType::EntitySelection | EditType::EntityDeselection))
    }

    /// Whever `self` only contains selection sub-edits.
    #[inline]
    #[must_use]
    pub fn only_contains_selection_edits(&self) -> bool
    {
        self.only_contains_entity_selection_edits() ||
            self.0.iter().all(|(_, et)| {
                matches!(
                    et,
                    EditType::VertexesSelection(_) |
                        EditType::PathNodesSelection(_) |
                        EditType::SubtracteeSelection |
                        EditType::SubtracteeDeselection
                )
            })
    }

    //==============================================================
    // Update

    /// Pushes a new sub-edit.
    /// # Panics
    /// Panics if `identifiers` has a an amount of elements that is not appropriate for `edit`.
    #[inline]
    pub fn push(&mut self, identifiers: HvVec<Id>, edit: EditType)
    {
        let despawn = if matches!(
            edit,
            EditType::TAnimation(..) |
                EditType::TAnimationMoveUp(..) |
                EditType::TAnimationMoveDown(..) |
                EditType::TListAnimationNewFrame(..) |
                EditType::TListAnimationTexture(..) |
                EditType::TListAnimationTime(..) |
                EditType::TListAnimationFrameRemoval(..) |
                EditType::TAtlasAnimationX(..) |
                EditType::TAtlasAnimationY(..) |
                EditType::TAtlasAnimationLen(..) |
                EditType::TAtlasAnimationTiming(..) |
                EditType::TAtlasAnimationUniformTime(..) |
                EditType::TAtlasAnimationFrameTime(..)
        )
        {
            assert!(
                identifiers.is_empty(),
                "Identifiers associated to default texture animation edit."
            );

            false
        }
        else if !matches!(
            edit,
            EditType::EntitySelection |
                EditType::EntityDeselection |
                EditType::SubtracteeSelection |
                EditType::SubtracteeDeselection |
                EditType::BrushMove(..) |
                EditType::Flip(..) |
                EditType::FreeDrawPointInsertion(..) |
                EditType::FreeDrawPointDeletion(..) |
                EditType::ThingMove(_) |
                EditType::TextureMove(_) |
                EditType::TextureScaleDelta(_) |
                EditType::TextureAngleDelta(_) |
                EditType::TextureFlip(..) |
                EditType::ListAnimationNewFrame(..) |
                EditType::ListAnimationTexture(..) |
                EditType::ListAnimationTime(..) |
                EditType::ListAnimationFrameRemoval(..) |
                EditType::AnimationMoveDown(..) |
                EditType::AnimationMoveUp(..) |
                EditType::Property(..)
        )
        {
            assert!(
                identifiers.len() == 1,
                "Edit {edit:?} should have only one associated entity, not {}",
                identifiers.len()
            );

            false
        }
        else
        {
            matches!(edit, EditType::BrushDespawn(..))
        };

        if !despawn
        {
            self.0.push((identifiers, edit));
            return;
        }

        self.0.insert(0, (identifiers, edit));
    }

    /// Pushes a property sub-edit.
    #[inline]
    pub fn push_property(&mut self, key: &str, iter: impl Iterator<Item = (Id, Value)>)
    {
        assert!(
            std::mem::replace(&mut self.1, key.to_owned().into()).is_none(),
            "Property edit already stored."
        );

        for (id, value) in iter
        {
            self.0.push((hv_vec![id], EditType::Property(value)));
        }
    }

    /// Remove all contained sub-edits.
    #[inline]
    pub fn clear(&mut self) { self.0.clear(); }

    /// Removes all free draw sub-edits.
    #[inline]
    #[must_use]
    pub fn purge_free_draw_edits(&mut self) -> bool
    {
        self.0.retain_mut(|x| {
            !matches!(
                x.1,
                EditType::FreeDrawPointInsertion(..) | EditType::FreeDrawPointDeletion(..)
            )
        });

        self.0.is_empty()
    }

    /// Removes all the sub-edits that were only useful to the previously active tool.
    #[inline]
    #[must_use]
    pub fn purge_tools_edits(&mut self) -> bool
    {
        self.0.retain_mut(|x| {
            if matches!(
                x.1,
                EditType::VertexesSelection(_) |
                    EditType::PathNodesSelection(_) |
                    EditType::SubtracteeSelection |
                    EditType::SubtracteeDeselection
            )
            {
                return false;
            }

            match &mut x.1
            {
                EditType::BrushDraw(data) =>
                {
                    x.1 = EditType::BrushSpawn(std::mem::take(data), true);
                },
                EditType::DrawnBrushDespawn(data) =>
                {
                    x.1 = EditType::BrushDespawn(std::mem::take(data), true);
                },
                EditType::PolygonEdit(cp) => cp.deselect_vertexes_no_indexes(),
                EditType::ThingDraw(thing) =>
                {
                    x.1 = EditType::ThingSpawn(std::mem::take(thing));
                },
                EditType::DrawnThingDespawn(thing) =>
                {
                    x.1 = EditType::ThingDespawn(std::mem::take(thing));
                },
                _ => ()
            }

            true
        });

        self.0.is_empty()
    }

    /// Removes all the texture sub-edits.
    #[inline]
    #[must_use]
    pub fn purge_texture_edits(&mut self) -> bool
    {
        self.0.retain_mut(|x| !x.1.texture_edit());
        self.0.is_empty()
    }

    /// Purges all things edits.
    #[inline]
    #[must_use]
    pub fn purge_thing_edits(&mut self) -> bool
    {
        self.0.retain_mut(|x| !x.1.thing_edit());
        self.0.is_empty()
    }

    /// Triggers the undo procedures of the sub-edits in the reverse order they were stored.
    #[inline]
    pub fn undo(
        &mut self,
        interface: &mut UndoRedoInterface,
        drawing_resources: &mut DrawingResources,
        ui: &mut Ui
    )
    {
        for (ids, ed_type) in self.0.iter_mut().rev()
        {
            ed_type.undo(interface, drawing_resources, ui, ids, self.1.as_ref());
        }
    }

    /// Triggers the redo procedures of the sub-edits in the order they were stored.
    #[inline]
    pub fn redo(
        &mut self,
        interface: &mut UndoRedoInterface,
        drawing_resources: &mut DrawingResources,
        ui: &mut Ui
    )
    {
        for (ids, ed_type) in &mut self.0
        {
            ed_type.redo(interface, drawing_resources, ui, ids, self.1.as_ref());
        }
    }
}
