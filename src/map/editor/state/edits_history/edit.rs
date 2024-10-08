//=======================================================================//
// IMPORTS
//
//=======================================================================//

use super::edit_type::EditType;
use crate::{
    map::{
        drawer::drawing_resources::DrawingResources,
        editor::state::{core::UndoRedoInterface, grid::Grid, ui::Ui},
        hv_vec,
        properties::Value
    },
    utils::{identifiers::Id, misc::ReplaceValue},
    HvVec
};

//=======================================================================//
// STRUCTS
//
//=======================================================================//

/// A map edit which can be undone and redone, and be made of multiple sub-edits.
pub(in crate::map::editor::state::edits_history) struct Edit
{
    edits:    HvVec<(HvVec<Id>, EditType)>,
    property: Option<String>,
    tag:      String
}

impl Default for Edit
{
    #[inline]
    #[must_use]
    fn default() -> Self
    {
        Self {
            edits:    hv_vec![],
            property: None,
            tag:      String::new()
        }
    }
}

impl Edit
{
    //==============================================================
    // Info

    /// The amount of sub-edits the edit is made of.
    #[inline]
    #[must_use]
    pub fn tag(&self) -> &str { &self.tag }

    /// The amount of sub-edits the edit is made of.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize { self.edits.len() }

    /// Whether `self` contains no sub-edits.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool { self.len() == 0 }

    /// Whether `self` contains a sub-edits that is only useful as long as the current tool remains
    /// unchanged.
    #[inline]
    #[must_use]
    pub fn contains_tool_edit(&self) -> bool { self.edits.iter().any(|(_, et)| et.tool_edit()) }

    /// Whether `self` contains a texture sub-edit.
    #[inline]
    #[must_use]
    pub fn contains_texture_edit(&self) -> bool
    {
        self.edits.iter().any(|(_, et)| et.texture_edit())
    }

    /// Whether `self` contains a thing edit.
    #[inline]
    #[must_use]
    pub fn contains_thing_edit(&self) -> bool { self.edits.iter().any(|(_, et)| et.thing_edit()) }

    /// Whether `self` contains a free draw sub-edit.
    #[inline]
    #[must_use]
    pub fn contains_free_draw_edit(&self) -> bool
    {
        self.edits.iter().any(|(_, et)| {
            matches!(et, EditType::FreeDrawPointInsertion(..) | EditType::FreeDrawPointDeletion(..))
        })
    }

    /// Whether `self` only contains entity selection sub-edits.
    #[inline]
    #[must_use]
    pub fn only_contains_entity_selection_edits(&self) -> bool
    {
        if self.is_empty()
        {
            return false;
        }

        self.edits
            .iter()
            .all(|(_, et)| matches!(et, EditType::EntitySelection | EditType::EntityDeselection))
    }

    /// Whether `self` only contains selection sub-edits.
    #[inline]
    #[must_use]
    pub fn only_contains_selection_edits(&self) -> bool
    {
        self.only_contains_entity_selection_edits() ||
            self.edits.iter().all(|(_, et)| {
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
                EditType::BrushFlip(..) |
                EditType::FreeDrawPointInsertion(..) |
                EditType::FreeDrawPointDeletion(..) |
                EditType::ThingMove(_) |
                EditType::TextureMove(_) |
                EditType::TextureScale(_) |
                EditType::TextureRotation(_) |
                EditType::TextureFlip(..) |
                EditType::ListAnimationNewFrame(..) |
                EditType::ListAnimationTexture(..) |
                EditType::ListAnimationTime(..) |
                EditType::ListAnimationFrameRemoval(..) |
                EditType::ListAnimationFrameMoveDown(..) |
                EditType::ListAnimationFrameMoveUp(..) |
                EditType::PropertyChange(..)
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

        self.push_tag(edit.tag());

        if !despawn
        {
            self.edits.push((identifiers, edit));
            return;
        }

        self.edits.insert(0, (identifiers, edit));
    }

    /// Pushes a property sub-edit.
    #[inline]
    pub fn push_property(&mut self, key: &str, iter: impl IntoIterator<Item = (Id, Value)>)
    {
        assert!(
            self.property.replace_value(key.to_owned().into()).is_none(),
            "Property edit already stored."
        );

        self.push_tag("Property Change");

        for (id, value) in iter
        {
            self.edits.push((hv_vec![id], EditType::PropertyChange(value)));
        }
    }

    #[inline]
    fn push_tag(&mut self, tag: &str)
    {
        if self.tag.is_empty()
        {
            self.tag.push_str(tag);
        }
    }

    #[inline]
    pub fn override_tag(&mut self, tag: &str)
    {
        self.tag.clear();
        self.tag.push_str(tag);
    }

    /// Remove all contained sub-edits.
    #[inline]
    pub fn clear(&mut self) { self.edits.clear(); }

    /// Removes all free draw sub-edits.
    #[inline]
    #[must_use]
    pub fn purge_free_draw_edits(&mut self) -> bool
    {
        self.edits.retain_mut(|x| {
            !matches!(
                x.1,
                EditType::FreeDrawPointInsertion(..) | EditType::FreeDrawPointDeletion(..)
            )
        });

        self.edits.is_empty()
    }

    /// Removes all the sub-edits that were only useful to the previously active tool.
    #[inline]
    #[must_use]
    pub fn purge_tools_edits(&mut self) -> bool
    {
        self.edits.retain_mut(|x| {
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
                EditType::DrawnBrush(data) =>
                {
                    x.1 = EditType::BrushSpawn(std::mem::take(data), true);
                },
                EditType::DrawnBrushDespawn(data) =>
                {
                    x.1 = EditType::BrushDespawn(std::mem::take(data), true);
                },
                EditType::PolygonEdit(cp) => cp.deselect_vertexes_no_indexes(),
                EditType::DrawnThing(thing) =>
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

        self.edits.is_empty()
    }

    /// Removes all the texture sub-edits.
    #[inline]
    #[must_use]
    pub fn purge_texture_edits(&mut self) -> bool
    {
        self.edits.retain_mut(|x| !x.1.texture_edit());
        self.edits.is_empty()
    }

    /// Purges all things edits.
    #[inline]
    #[must_use]
    pub fn purge_thing_edits(&mut self) -> bool
    {
        self.edits.retain_mut(|x| !x.1.thing_edit());
        self.edits.is_empty()
    }

    /// Triggers the undo procedures of the sub-edits in the reverse order they were stored.
    #[inline]
    pub fn undo(
        &mut self,
        interface: &mut UndoRedoInterface,
        drawing_resources: &mut DrawingResources,
        grid: &Grid,
        ui: &mut Ui
    )
    {
        for (ids, ed_type) in self.edits.iter_mut().rev()
        {
            ed_type.undo(interface, drawing_resources, grid, ui, ids, self.property.as_ref());
        }
    }

    /// Triggers the redo procedures of the sub-edits in the order they were stored.
    #[inline]
    pub fn redo(
        &mut self,
        interface: &mut UndoRedoInterface,
        drawing_resources: &mut DrawingResources,
        grid: &Grid,
        ui: &mut Ui
    )
    {
        for (ids, ed_type) in &mut self.edits
        {
            ed_type.redo(interface, drawing_resources, grid, ui, ids, self.property.as_ref());
        }
    }
}
