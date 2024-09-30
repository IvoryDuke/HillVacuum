//=======================================================================//
// IMPORTS
//
//=======================================================================//

use core::panic;
use std::{cmp::Ordering, fmt::Debug, ops::Index};

use crate::{
    map::{
        drawer::drawing_resources::DrawingResources,
        editor::{
            cursor::Cursor,
            state::{grid::Grid, inputs_presses::InputsPresses, manager::EntitiesManager}
        }
    },
    utils::{collections::hv_vec, identifiers::EntityId, misc::next},
    HvVec
};

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// The position of the item.
#[allow(clippy::missing_docs_in_private_items)]
#[derive(Debug, Default, Clone, Copy)]
enum Position
{
    #[default]
    None,
    Selected(usize),
    NonSelected(usize)
}

//=======================================================================//
// STRUCTS
//
//=======================================================================//

#[allow(clippy::missing_docs_in_private_items)]
type SelectorFunc<T> =
    fn(&DrawingResources, &EntitiesManager, &Cursor, Grid, f32, &mut ItemsBeneathCursor<T>);

//=======================================================================//

/// The items beneath the cursor.
#[derive(Debug)]
pub(in crate::map::editor::state::core) struct ItemsBeneathCursor<T>
where
    T: EntityId + Copy + PartialEq
{
    /// The selected items.
    selected:     HvVec<T>,
    /// The non selected items.
    non_selected: HvVec<T>
}

impl<T> Default for ItemsBeneathCursor<T>
where
    T: EntityId + Copy + PartialEq
{
    #[inline]
    #[must_use]
    fn default() -> Self
    {
        Self {
            selected:     hv_vec![],
            non_selected: hv_vec![]
        }
    }
}

impl<T> Index<Position> for ItemsBeneathCursor<T>
where
    T: EntityId + Clone + Copy + PartialEq
{
    type Output = T;

    #[inline]
    fn index(&self, index: Position) -> &Self::Output
    {
        match index
        {
            Position::None => panic!(),
            Position::Selected(idx) => &self.selected[idx],
            Position::NonSelected(idx) => &self.non_selected[idx]
        }
    }
}

impl<T> ItemsBeneathCursor<T>
where
    T: EntityId + Clone + Copy + PartialEq
{
    /// Whether there are no items.
    #[inline]
    fn is_empty(&self) -> bool { self.selected.is_empty() && self.non_selected.is_empty() }

    /// The index of `value` in the vector.
    #[inline]
    fn position(&self, value: T) -> Position
    {
        self.selected
            .iter()
            .position(|v| *v == value)
            .map(Position::Selected)
            .or_else(|| {
                self.non_selected
                    .iter()
                    .position(|v| *v == value)
                    .map(Position::NonSelected)
            })
            .unwrap_or_default()
    }

    /// Pushes `item`.
    #[inline]
    pub fn push(&mut self, item: T, selected: bool)
    {
        if selected
        {
            self.selected.push(item);
        }
        else
        {
            self.non_selected.push(item);
        }
    }

    /// Clears the stored items.
    #[inline]
    fn clear(&mut self)
    {
        self.selected.clear();
        self.non_selected.clear();
    }

    /// Sorts the items based on the draw height.
    #[inline]
    fn sort(&mut self, manager: &EntitiesManager)
    {
        for slice in [&mut self.selected, &mut self.non_selected]
        {
            slice.sort_by(|a, b| {
                let height_a = manager.entity(a.id()).draw_height();
                let height_b = manager.entity(b.id()).draw_height();

                match (height_a, height_b)
                {
                    (None, None) => Ordering::Equal,
                    (None, Some(_)) => Ordering::Less,
                    (Some(_), None) => Ordering::Greater,
                    (Some(a), Some(b)) => a.total_cmp(&b).reverse()
                }
            });
        }
    }
}

//=======================================================================//

/// The selector of map items.
#[derive(Debug)]
pub(in crate::map::editor::state::core) struct ItemSelector<T>
where
    T: EntityId + Copy + PartialEq
{
    /// The items.
    items:    ItemsBeneathCursor<T>,
    /// The position of the previously returned value in the current items set.
    depth:    Position,
    /// The previously returned item, if any.
    previous: Option<T>,
    /// The selector function.
    selector: SelectorFunc<T>
}

impl<T> ItemSelector<T>
where
    T: EntityId + Copy + PartialEq
{
    /// Returns a new [`ItemSelector`].
    #[inline]
    #[must_use]
    pub fn new(func: SelectorFunc<T>) -> Self
    {
        Self {
            items:    ItemsBeneathCursor::default(),
            depth:    Position::None,
            previous: None,
            selector: func
        }
    }

    /// The item beneath the cursor, if any. If the item returned in the previous frame is still
    /// present it is still returned.
    #[inline]
    #[must_use]
    pub fn item_beneath_cursor(
        &mut self,
        drawing_resources: &DrawingResources,
        manager: &EntitiesManager,
        cursor: &Cursor,
        grid: Grid,
        camera_scale: f32,
        inputs: &InputsPresses
    ) -> Option<T>
    {
        self.items.clear();
        (self.selector)(drawing_resources, manager, cursor, grid, camera_scale, &mut self.items);

        if self.items.is_empty()
        {
            self.depth = Position::None;
            self.previous = None;
            return None;
        }

        self.items.sort(manager);

        match self.previous
        {
            Some(prev) =>
            {
                self.depth = self.items.position(prev);

                if matches!(self.depth, Position::None)
                {
                    self.previous = None;
                    self.update_previous_value();
                }
                else if inputs.tab.just_pressed()
                {
                    macro_rules! next {
                        ($idx:ident, $current:ident, $other:ident, $new:ident) => {
                            if self.items.$other.is_empty() ||
                                *$idx != (self.items.$current.len() - 1)
                            {
                                *$idx = next(*$idx, self.items.$current.len());
                            }
                            else
                            {
                                self.depth = Position::$new(0);
                            }
                        };
                    }

                    match &mut self.depth
                    {
                        Position::None => panic!(),
                        Position::Selected(idx) => next!(idx, selected, non_selected, NonSelected),
                        Position::NonSelected(idx) => next!(idx, non_selected, selected, Selected)
                    };
                }
            },
            None => self.update_previous_value()
        }

        self.previous = Some(self.items[self.depth]);
        self.previous
    }

    #[inline]
    fn update_previous_value(&mut self)
    {
        self.depth = if self.items.selected.is_empty()
        {
            assert!(!self.items.non_selected.is_empty(), "No non selected items.");
            Position::NonSelected(0)
        }
        else
        {
            Position::Selected(0)
        };
    }
}
