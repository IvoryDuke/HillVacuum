//=======================================================================//
// IMPORTS
//
//=======================================================================//

use serde::{Deserialize, Serialize};

#[allow(unused_imports)]
use crate::Brush;
use crate::{utils::identifiers::Id, Ids, Node};

//=======================================================================//
// ENUMS
//
//=======================================================================//

/// Information concerning a set of [`Brush`]es grouped together.
#[derive(Serialize, Deserialize)]
pub enum GroupViewer
{
    /// No group.
    None,
    /// Has some attached [`Brush`]es.
    Attachments(Ids),
    /// Has a path and maybe some attached [`Brush`]es.
    Path
    {
        /// The travel path.
        path:             Vec<Node>,
        /// The attached [`Brush`]es.
        attached_brushes: Ids
    },
    /// Is attached to a [`Brush`].
    Attached(Id)
}

//=======================================================================//
// UI
//
//=======================================================================//

#[cfg(feature = "ui")]
pub(in crate::map) mod ui_mod
{
    //=======================================================================//
    // IMPORTS
    //
    //=======================================================================//

    use bevy::utils::HashSet;
    use hill_vacuum_shared::{match_or_panic, return_if_no_match};

    use super::GroupViewer;
    use crate::{
        hash_set,
        map::{path::Path, Viewer},
        utils::misc::{AssertedInsertRemove, TakeValue},
        Id,
        Ids
    };

    //=======================================================================//
    // ENUMS
    //
    //=======================================================================//

    /// Information concerning a set of [`Brush`]es grouped together.
    #[must_use]
    #[derive(Clone, Default)]
    pub(in crate::map) enum Group
    {
        /// None.
        #[default]
        None,
        /// Has some attached [`Brush`]es.
        Attachments(Ids),
        /// Has a [`Path`] and maybe some attached [`Brush`]es.
        Path
        {
            /// The [`Path`].
            path:             Path,
            /// The [`Id`]s of the attached [`Brush`]es.
            attached_brushes: Ids
        },
        /// Attached to a [`Brush`].
        Attached(Id)
    }

    impl Viewer for Group
    {
        type Item = GroupViewer;

        #[inline]
        fn from_viewer(value: Self::Item) -> Self
        {
            match value
            {
                Self::Item::None => Self::None,
                Self::Item::Attachments(ids) => Self::Attachments(ids),
                Self::Item::Path {
                    path,
                    attached_brushes
                } =>
                {
                    Self::Path {
                        path: Path::from_viewer(path),
                        attached_brushes
                    }
                },
                Self::Item::Attached(id) => Self::Attached(id)
            }
        }

        #[inline]
        fn to_viewer(self) -> Self::Item
        {
            match self
            {
                Self::None => Self::Item::None,
                Self::Attachments(ids) => Self::Item::Attachments(ids),
                Self::Path {
                    path,
                    attached_brushes
                } =>
                {
                    Self::Item::Path {
                        path: path.to_viewer(),
                        attached_brushes
                    }
                },
                Self::Attached(id) => Self::Item::Attached(id)
            }
        }
    }

    impl Group
    {
        #[inline]
        #[must_use]
        pub(in crate::map) const fn has_path(&self) -> bool { matches!(self, Self::Path { .. }) }

        #[inline]
        #[must_use]
        pub(in crate::map) fn has_attachments(&self) -> bool
        {
            match self.attachments()
            {
                Some(ids) => !ids.is_empty(),
                None => false
            }
        }

        #[inline]
        #[must_use]
        pub(in crate::map) const fn is_attached(&self) -> Option<Id>
        {
            return_if_no_match!(self, Self::Attached(id), Some(*id), None)
        }

        #[inline]
        pub(in crate::map) const fn path(&self) -> Option<&Path>
        {
            Some(return_if_no_match!(self, Self::Path { path, .. }, path, None))
        }

        #[inline]
        pub(in crate::map::brush) fn path_mut(&mut self) -> &mut Path
        {
            match_or_panic!(self, Self::Path { path, .. }, path)
        }

        #[inline]
        pub(in crate::map::brush) fn attachments_iter(
            &self
        ) -> Option<impl ExactSizeIterator<Item = &Id> + Clone>
        {
            self.attachments().map(Ids::iter)
        }

        #[inline]
        pub(in crate::map::brush) const fn attachments(&self) -> Option<&Ids>
        {
            match self
            {
                Self::None | Self::Attached(_) => None,
                Self::Attachments(ids) => Some(ids),
                Self::Path {
                    attached_brushes, ..
                } => Some(attached_brushes)
            }
        }

        #[inline]
        pub(in crate::map::brush) fn insert_attachment(&mut self, identifier: Id)
        {
            match self
            {
                Self::None => *self = Self::Attachments(hash_set![identifier]),
                Self::Attachments(ids) => ids.asserted_insert(identifier),
                Self::Path {
                    attached_brushes, ..
                } => attached_brushes.asserted_insert(identifier),
                Self::Attached(_) => panic!("Tried to insert an attachment in an attached brush.")
            };
        }

        #[inline]
        pub(in crate::map::brush) fn remove_attachment(&mut self, identifier: Id)
        {
            match self
            {
                Self::Attachments(ids) =>
                {
                    ids.asserted_remove(&identifier);

                    if ids.is_empty()
                    {
                        *self = Self::None;
                    }
                },
                Self::Path {
                    attached_brushes, ..
                } => attached_brushes.asserted_remove(&identifier),
                _ => panic!("Brush does not have attachments.")
            }
        }

        #[inline]
        pub(in crate::map::brush) fn take_path(&mut self) -> Path
        {
            let (path, attachments) = match_or_panic!(
                self.take_value(),
                Self::Path {
                    path,
                    attached_brushes
                },
                (path, attached_brushes)
            );

            if !attachments.is_empty()
            {
                *self = Self::Attachments(attachments);
            }

            path
        }

        #[inline]
        pub(in crate::map::brush) fn set_path(&mut self, path: Path)
        {
            match self
            {
                Self::None =>
                {
                    *self = Self::Path {
                        path,
                        attached_brushes: HashSet::new()
                    }
                },
                Self::Attachments(ids) =>
                {
                    *self = Self::Path {
                        path,
                        attached_brushes: ids.take_value()
                    };
                },
                Self::Path { .. } | Self::Attached(_) =>
                {
                    panic!("Unsuitable circumstance for setting a path.")
                }
            };
        }
    }
}

#[cfg(feature = "ui")]
pub(in crate::map) use ui_mod::*;
