//=======================================================================//
// IMPORTS
//
//=======================================================================//

use std::{cmp::Ordering, fmt::Debug, ops::Index};

use bevy::prelude::Vec2;

use crate::{
    map::{
        editor::{
            cursor_pos::Cursor,
            hv_vec,
            state::{editor_state::InputsPresses, manager::EntitiesManager}
        },
        HvVec
    },
    utils::{identifiers::EntityId, misc::next}
};

//=======================================================================//
// TYPES
//
//=======================================================================//

type SelectorFunc<T> = fn(&EntitiesManager, Vec2, f32, &mut ItemsBeneathCursor<T>);

//=======================================================================//

#[derive(Debug)]
pub(in crate::map::editor::state::core) struct ItemsBeneathCursor<T>(HvVec<T>, usize)
where
    T: EntityId + Copy + PartialEq;

impl<T> Default for ItemsBeneathCursor<T>
where
    T: EntityId + Copy + PartialEq
{
    #[inline]
    #[must_use]
    fn default() -> Self { Self(hv_vec![], Default::default()) }
}

impl<T> Index<usize> for ItemsBeneathCursor<T>
where
    T: EntityId + Clone + Copy + PartialEq
{
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output { &self.0[index] }
}

impl<T> ItemsBeneathCursor<T>
where
    T: EntityId + Clone + Copy + PartialEq
{
    #[inline]
    fn len(&self) -> usize { self.0.len() }

    #[inline]
    fn is_empty(&self) -> bool { self.0.is_empty() }

    #[inline]
    fn position(&self, value: T) -> Option<usize> { self.0.iter().position(|v| *v == value) }

    #[inline]
    pub fn push(&mut self, identifier: T, selected: bool)
    {
        if selected
        {
            self.0.insert(self.1, identifier);
            self.1 += 1;
        }
        else
        {
            self.0.push(identifier);
        }
    }

    #[inline]
    fn clear(&mut self)
    {
        self.0.clear();
        self.1 = 0;
    }

    #[inline]
    fn sort(&mut self, manager: &EntitiesManager)
    {
        let (selected, non_selected) = self.0.split_at_mut(self.1);

        for slice in [selected, non_selected]
        {
            slice.sort_by(|a, b| {
                let height_a = manager.entity(a.id()).draw_height();
                let height_b = manager.entity(b.id()).draw_height();

                match (height_a, height_b)
                {
                    (None, None) => Ordering::Equal,
                    (None, Some(_)) => Ordering::Less,
                    (Some(_), None) => Ordering::Greater,
                    (Some(a), Some(b)) => a.total_cmp(&b)
                }
            });
        }
    }
}

//=======================================================================//

#[derive(Debug)]
pub(in crate::map::editor::state::core) struct ItemSelector<T>
where
    T: EntityId + Copy + PartialEq
{
    brushes:  ItemsBeneathCursor<T>,
    depth:    usize,
    previous: Option<T>,
    selector: SelectorFunc<T>
}

impl<T> ItemSelector<T>
where
    T: EntityId + Copy + PartialEq
{
    #[inline]
    #[must_use]
    pub fn new(func: SelectorFunc<T>) -> Self
    {
        Self {
            brushes:  ItemsBeneathCursor::default(),
            depth:    0,
            previous: None,
            selector: func
        }
    }

    #[inline]
    #[must_use]
    pub fn item_beneath_cursor(
        &mut self,
        manager: &EntitiesManager,
        cursor: &Cursor,
        camera_scale: f32,
        inputs: &InputsPresses
    ) -> Option<T>
    {
        self.brushes.clear();
        (self.selector)(manager, cursor.world(), camera_scale, &mut self.brushes);

        if self.brushes.is_empty()
        {
            self.depth = 0;
            self.previous = None;
            return None;
        }

        self.brushes.sort(manager);

        if let Some(brush) = self.previous
        {
            if let Some(idx) = self.brushes.position(brush)
            {
                self.depth = idx;
            }
        }

        self.depth = self.depth.min(self.brushes.len() - 1);

        if inputs.tab.just_pressed()
        {
            self.depth = next(self.depth, self.brushes.len());
        }

        let value = Some(self.brushes[self.depth]);
        self.previous = value;
        value
    }
}
