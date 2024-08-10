pub(in crate::map) mod _03;
pub(in crate::map) mod _04;

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
            pub mover:      Mover,
            pub properties: Properties
        }

        //=======================================================================//

        #[must_use]
        #[derive(Serialize, Deserialize)]
        pub(in crate::map) struct Brush
        {
            pub(in crate::map::brush) id:   Id,
            pub(in crate::map::brush) data: BrushData
        }
    };
}

use impl_brush;
