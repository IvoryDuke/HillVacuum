pub(in crate::map) mod _03;
pub(in crate::map) mod _04;
pub(in crate::map) mod _061;

//=======================================================================//
// IMPORTS
//
//=======================================================================//

use serde::{Deserialize, Serialize};

use crate::{utils::containers::Ids, Id, Path};

//=======================================================================//
// MACROS
//
//=======================================================================//

macro_rules! impl_brush {
    () => {
        #[derive(Serialize, Deserialize)]
        pub(in crate::map::brush) struct BrushData
        {
            pub polygon:    ConvexPolygon,
            pub mover:      crate::map::brush::compatibility::Mover,
            pub properties: crate::map::properties::Properties
        }

        //=======================================================================//

        #[must_use]
        #[derive(Serialize, Deserialize)]
        pub(in crate::map) struct Brush
        {
            pub(in crate::map::brush) id:   crate::Id,
            pub(in crate::map::brush) data: BrushData
        }
    };
}

use impl_brush;

//=======================================================================//
// ENUMS
//
//=======================================================================//

#[must_use]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(in crate::map) enum Mover
{
    #[default]
    None,
    Anchors(Ids),
    Motor(Motor),
    Anchored(Id)
}

//=======================================================================//
// TYPES
//
//=======================================================================//

#[must_use]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::map) struct Motor
{
    /// The [`Path`].
    pub path:             Path,
    /// The [`Id`]s of the attached [`Brush`]es.
    pub anchored_brushes: Ids
}
