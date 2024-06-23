//=======================================================================//
// IMPORTS
//
//=======================================================================//

use bevy_egui::egui;

use super::{window::Window, ToolsButtons, WindowCloser, WindowCloserInfo};
use crate::{
    map::editor::{
        state::core::tool::{SubTool, Tool, ToolInterface},
        StateUpdateBundle
    },
    utils::misc::Toggle,
    HardcodedActions
};

//=======================================================================//
// TYPES
//
//=======================================================================//

/// The manual window.
#[derive(Default)]
pub(in crate::map::editor::state::ui) struct Manual(Window);

impl Toggle for Manual
{
    #[inline]
    fn toggle(&mut self) { self.0.toggle() }
}

impl WindowCloserInfo for Manual
{
    #[inline]
    fn window_closer(&self) -> Option<WindowCloser>
    {
        /// Calls the close function.
        #[inline]
        fn close(manual: &mut Manual) { manual.0.close() }

        self.0
            .layer_id()
            .map(|id| WindowCloser::Manual(id, close as fn(&mut Self)))
    }
}

impl Manual
{
    /// Shows the manual window.
    #[inline]
    pub fn show(&mut self, bundle: &mut StateUpdateBundle, tools_buttons: &ToolsButtons)
    {
        /// Draws the UI elements.
        #[inline]
        fn draw(ui: &mut egui::Ui, tools_buttons: &ToolsButtons)
        {
            /// Generates a section of the manual.
            macro_rules! manual_section {
                ($name:literal, $(($command:literal, $explanation:literal)),+) => {
                    manual_section!(no_separator, $name, $(($command, $explanation)),+);
                    ui.separator();
                };

                (no_separator, $name:literal, $(($command:literal, $explanation:literal)),+) => {
                    ui.collapsing($name, |ui| {
                        ui.vertical(|ui| { $(
                            manual_section!(ui, |ui: &mut egui::Ui| ui.label($command), $explanation);
                        )+})
                    });
                };

                (
                    $tool:ident,
                    $(($command:literal, $explanation:literal)),+
                    $(, $(($subtool:ident, $sub_explanation:literal)),+)?
                    $(, TEXTURE, $tex_explanation:literal)?
                ) => {
                    manual_section!(
                        no_separator,
                        $tool,
                        $(($command, $explanation)),+
                        $($(($subtool, $sub_explanation)),+)?
                    );

                    ui.separator();
                };

                (
                    no_separator,
                    $tool:ident,
                    $(($command:literal, $explanation:literal)),+
                    $($(($subtool:ident, $sub_explanation:literal)),+)?
                    $(, TEXTURE, $tex_explanation:literal)?
                ) => {
                    ui.collapsing(Tool::$tool.header(), |ui| {
                        ui.vertical(|ui| {
                            tools_buttons.image(ui, Tool::$tool);
                            $( manual_section!(ui, $command, $explanation); )+
                            $($( manual_section!(ui, $subtool, $sub_explanation); )+)?
                            $($( manual_section!(ui, $tex_explanation); )+)?
                        })
                    });
                };

                ($ui:ident, $command:literal, $explanation:literal) => {
                    manual_section!($ui, |ui: &mut egui::Ui| ui.label($command), $explanation);
                };

                ($ui:ident, $subtool:ident, $explanation:literal) => {
                    manual_section!($ui, |ui| tools_buttons.image(ui, SubTool::$subtool), $explanation);
                };

                ($ui:ident, $explanation:literal) => {
                    manual_section!($ui, |ui| ui.label("TEXTURE"), $explanation);
                };

                ($ui:ident, $left:expr, $explanation:literal) => {
                    $ui.horizontal_wrapped(|ui| {
                        egui_extras::StripBuilder::new(ui)
                            .size(egui_extras::Size::exact(250f32))
                            .size(egui_extras::Size::remainder())
                            .horizontal(|mut strip| {

                                #[allow(clippy::redundant_closure_call)]
                                strip.cell(|ui| { $left(ui); });

                                strip.cell(|ui| {
                                    ui.label($explanation);
                                });
                            });
                    });
                };
            }

            manual_section!(
                "GENERAL",
                (
                    "Tab",
                    "Whenever a tool allows you to select/deselect entities, tab can be pressed \
                     to select the next element beneath it, if any."
                ),
                (
                    "Cursor",
                    "A semitransparent square is shown on screen to represent the world position \
                     of the cursor. This is usefull to show where the camera will zoom in/out \
                     when pressing Ctrl + Mouse wheel."
                ),
                (
                    "Tools",
                    "Tools can be selected by clicking the icons on the left side of the screen \
                     or by pressing the bound key.\nBound keys can be viewed and changed through \
                     the bind menu."
                ),
                (
                    "Subtools",
                    "Subtools are UI elements that show up on the right when certain tools are \
                     selected to provide easy access to functions bound to hardcoded keyboard \
                     shortcuts.\nLeft clicking the first time enables them, clicking a second \
                     time disables them.\nEvery subtool shows the associated keyboard shortcut in \
                     the tooltip.\nSubtools can be disabled by pressing Escape."
                ),
                (
                    "Brushes",
                    "Brushes are convex polygonal surfaces.\nThey can have an associated texture \
                     which can either be drawn filling their area or as a sprite. The sprite can \
                     be displaced independently of the brush surface.\nBrushes can also be \
                     assigned a path that describes how it moves in the bidimensional space and \
                     that can be edited with the Path tool.\nFinally, brushes have a built-in \
                     property, collision, which determines whever they should represent a \
                     clipping surface or not. It can be edited in the properties window."
                ),
                (
                    "Things",
                    "Things are objects which can be placed around the map. They area \
                     characterized by an ID, a width and height, a name, and a texture which \
                     represents them.\nThings can also be assigned a path that describes how it \
                     moves in the bidimensional space and that can be edited with the Path \
                     tool.\nThings can either be defined in one or many .ini files to be placed \
                     in the assets/things/ folder or, if HillVacuum is used as a library, \
                     implementing the MapThing interface for the structs representing an object \
                     to be placed in the map and using the \"hardcoded_things\" macro to insert \
                     them in the bevy App.\n\nIf defined in the .ini files, the things must \
                     follow a similar format:\n[Name]\nwidth = N\nheight = M\nid = ID\npreview = \
                     TEX\nWhere N and M represent the sizes of the thing's bounding box, ID is a \
                     unique identifier between 0 and 65534, and TEX is the name of the texture to \
                     be drawn along with the bounding box.\nIf a thing defined through the \
                     MapThing interface has the same ID as one loaded from file, the latter will \
                     overwrite the former.\n\nFinally, things have two built-in properties, angle \
                     and draw height. The orientation of the arrow drawn on top of the things \
                     will change based on the value of angle, and draw height determines its draw \
                     order. They can be edited in the properties window.\n\nThings can be \
                     reloaded while the application is running through the UI button in the \
                     Options menu."
                ),
                (
                    "Properties",
                    "Properties are custom user defined values which can be associated to brushes \
                     and things.\nSuch values can be defined through the \"brush_properties\" and \
                     \"thing_properties\" macros defining the pairs (name, default_value) of the \
                     properties.\nProperties can be edited per-entity using the properties \
                     window.\nCurrently supported value types are bool, u8, u16, u32, u64, u128, \
                     i8, i16, i32, i64, i128, f32, f64, and String.\n\n!!! If a saved map \
                     contains properties that differ in type and/or name from the ones defined in \
                     the aforementioned resources, a warning window will appear on screen when \
                     trying to load the .hv file, asking whever you'd like to use the app or map \
                     ones."
                ),
                (
                    "Textures",
                    "Textures must be placed in the assets/textures/ folder to be loaded.\nThe \
                     texture editor can be opened at any time to edit the properties of the \
                     textures of the selected brushes.\nEntity, scale, and rotate tool also \
                     feature texture editing capabilities. These capabilities can be either \
                     enabled through the dedicated \"Target\" UI element in the bottom left area, \
                     or by pressing Alt + texture editor bind.\n\nTextures can have an associated \
                     animation which can either consist of a list of textures to display, each \
                     one for a specific time, or an atlas of textures generated by subdividing \
                     the textures in subareas. The animations can be applied to the texture as a \
                     default or to the texture of the selected brushes only.\nWhen editing a list \
                     type animation, it is possible to add a texture by clicking it with the left \
                     mouse button.\nTo edit the default animation of a texture that is not the \
                     one of the selected brushes, it needs to be clicked with the right mouse \
                     button.\nTextures can be reloaded while the application is running through \
                     the UI button in the Options menu.\n\nDefault textures animation can be \
                     exported and imported between map files. The file extension of the \
                     animations files is .anms."
                )
            );

            manual_section!(
                "EDIT",
                ("INFO", "These commands can only be used while there is no ongoing edit."),
                (
                    "Ctrl + A",
                    "Select all, selects all the elements of the category the currently selected \
                     tool is capable of editing (entities, vertexes, sides, etc. etc.)."
                ),
                (
                    "Ctrl + C",
                    "Copy, copies the selected entities, or the path of the entity beneath the \
                     cursor, if any, when using the Path tool."
                ),
                (
                    "Ctrl + V",
                    "Paste, creates copies of the selected entities, or sets the path of the \
                     entity beneath the cursor to the copied one, if any, when using the Path \
                     tool."
                ),
                (
                    "Ctrl + X",
                    "Cut, cuts the selected entities, or cuts the path of the entity beneath the \
                     cursor, if any, when using the Path tool."
                ),
                (
                    "Ctrl + D",
                    "Duplicates the entities. Equivalent to Ctrl + Alt + Right when using the \
                     Entity tool."
                ),
                ("Ctrl + Z", "Undo."),
                ("Ctrl + Y", "Redo.")
            );

            manual_section!(
                "VIEW",
                ("Space", "Drags the camera around."),
                (
                    "Ctrl + Up/Down/Left/Right",
                    "Moves the camera one grid square in the pressed direction."
                ),
                ("Ctrl + Plus", "Zooms the camera in."),
                ("Ctrl + Minus", "Zooms the camera out."),
                ("Mouse wheel", "Moves the camera up/down."),
                ("Shift + Mouse wheel", "Moves the camera left/right."),
                ("Ctrl + Mouse wheel", "Zooms the camera towards/outwards the cursor position."),
                ("Alt + Zoom tool bind", "Zooms the camera on the selected entities.")
            );

            manual_section!(
                Square,
                ("Left mouse", "Spawns a grid square shaped brush."),
                (
                    "Left mouse + cursor drag",
                    "Creates a rectangular shaped brush spawned when the mouse button is released."
                ),
                ("Backspace", "Deletes all drawn brushes.")
            );

            manual_section!(
                Triangle,
                (
                    "Left mouse",
                    "Spawns a right triangle with right angle placed at the closest grid lines \
                     intersection and legs grid-square-side sized."
                ),
                (
                    "Left mouse + cursor drag",
                    "Creates a right triangle shaped brush spawned when the mouse button is \
                     released."
                ),
                ("Tab", "Changes the orientation of the triangle being drag spawned."),
                ("Backspace", "Deletes all drawn brushes.")
            );

            manual_section!(
                Circle,
                (
                    "Left mouse",
                    "Spawns a ellipse shaped brush inscribed in the hovered grid square."
                ),
                (
                    "Left mouse + cursor drag",
                    "Creates an ellipse shaped brush spawned when the mouse button is released."
                ),
                ("Plus", "Increases the ellipse resolution."),
                ("Minus", "Drecreases the ellipse resolution."),
                ("Backspace", "Deletes all drawn brushes.")
            );

            manual_section!(
                FreeDraw,
                (
                    "Left mouse",
                    "Attempts to add a vertex to the shape being drawn. Nothing will happen if \
                     the shape generated adding such vertex is concave, or the shape already \
                     contains that vertex."
                ),
                ("Right mouse", "Deletes the vertex beneath the cursor."),
                (
                    "Enter",
                    "Attempts to spawn the shape currently being drawn. Nothing will happen if \
                     the shape is just a point or a line."
                ),
                ("Escape", "Erases the brush being drawn."),
                ("Backspace", "Deletes all drawn brushes.")
            );

            manual_section!(
                Thing,
                (
                    "PIVOT",
                    "Determines how the selected thing is spawned on the map with respect to the \
                     mouse position. For example, if the pivot is set to TopLeft the thing will \
                     be spawned with its top left corner placed at the mouse position."
                ),
                ("Left mouse", "Spawn the selected thing based on the selected pivot."),
                (
                    "Tab",
                    "Set the pivot to the next value. If Shift is pressed as well it is set to \
                     the previous value."
                ),
                ("Backspace", "Deletes all drawn things."),
                (
                    ThingChange,
                    "Thing change subtool. Allows to change the selected things placed on the map \
                     to the thing clicked in the UI."
                )
            );

            manual_section!(
                Entity,
                (
                    "INFO",
                    "Brushes can be tied together into a group through the Anchor subtool. This \
                     establishes a owner-anchored relation between the brushes. An \"owner\" \
                     brush can have an unlimited amount of brushes tied to it. A brush that is \
                     anchored can have none.\nWhen the \"owner\" brush is moved all anchored \
                     brushes will be moved as well even if they are not selected."
                ),
                (
                    "Left mouse",
                    "If there is a non-selected entity beneath the cursor, it will be exclusively \
                     selected. If there is no entity, all entities will be deselected upon mouse \
                     button release.\nPressing Ctrl on a brush causes all anchored brushes to be \
                     selected as well."
                ),
                (
                    "Left mouse + Shift",
                    "If there is an entity beneath the cursor, its selection status will be \
                     toggled.\nPressing Ctrl on a brush causes all anchored brushes to be toggled \
                     as well."
                ),
                (
                    "Left mouse + cursor drag",
                    "If there is a selected entity beneath the cursor, all selected entities will \
                     be dragged around the map. If there is no entitt, a drag selection will be \
                     initiated.\nWhen the mouse button is released, the entities within the drag \
                     selection area will be exclusively selected.\nPressing Ctrl and the \
                     selection contains brushes, all anchored brushes are selected as well."
                ),
                (
                    "Left mouse + Shift + cursor drag",
                    "Same as Left mouse + drag, except the entities within the boundary of the \
                     drag selection are added to the selected brushes, if they are not already \
                     selected.\nPressing Ctrl and the selection contains brushes, all anchored \
                     brushes are selected as well."
                ),
                (
                    "Left mouse + Alt + cursor drag",
                    "If there is a selected entity beneath the cursor, copies of the selected \
                     entities will be spawned in the direction the cursor is moved."
                ),
                (
                    "Up/Down/Left/Right",
                    "Moves the selected entities one grid square away in the pressed direction."
                ),
                (
                    "Alt + Up/Down/Left/Right",
                    "Creates copies of the selected entities one grid square away in the pressed \
                     direction."
                ),
                (
                    "Right mouse",
                    "Clicking a brush with no path and not anchored allows to anchor it to \
                     another brush. Clicking on an anchored brush disanchors it."
                ),
                (
                    EntityDragSpawn,
                    "Drag spawn subtool. Selecting it and then pressing a directional key, or \
                     left clicking and dragging with the cursor a selected brush, will spawn \
                     copies of the selected entities in the direction the cursor is moved."
                ),
                (
                    EntityAnchor,
                    "Anchor subtool. Selecting it and then left clicking a brush with no path and \
                     not anchored allows to anchor it to another brush. Clicking on an anchored \
                     brush disanchors it."
                ),
                TEXTURE,
                "Target:\n-Polygon, only interact with brushes;\n-Both, interact with both \
                 textures and brushes;\n-Texture, interact with textures."
            );

            manual_section!(
                Vertex,
                (
                    "Left mouse",
                    "If there is a non-selected vertex beneath the cursor, it will be exclusively \
                     selected. If there is no vertex underneath, when the mouse button is \
                     released all selected vertexes will be deselected."
                ),
                (
                    "Shift + Left mouse",
                    "If there is a vertex beneath the cursor, its selection status will be \
                     toggled."
                ),
                (
                    "Left mouse + cursor drag",
                    "If there is a selected vertex beneath the cursor, all selected vertexes will \
                     be dragged around the map. Unless the move generates at least one illegally \
                     shaped brush (concave). If there is no vertex, a drag selection will be \
                     initiated.\nWhen the mouse button is released, the vertexes within the drag \
                     selection area will be exclusively selected.\nIf a moved vertex overlaps a \
                     non selected one, this vertex will be selected as well."
                ),
                (
                    "Shift + Left mouse + cursor drag",
                    "Same as Left mouse + drag, except the vertexes within the boundary of the \
                     drag selection are added to the selected brushes, if they are not already \
                     selected."
                ),
                (
                    "Alt + Left mouse + cursor drag",
                    "Adds a new vertex on the line that passes through the cursor position.\nSuch \
                     vertex can then be dragged around as long as it does not cause the resulting \
                     shape to be concave.\nIf the generated vertex is not moved from its original \
                     position it will be purged."
                ),
                ("Alt + Merge tool bind", "Generates a new brush from the selected vertexes."),
                (
                    "Up/Down/Left/Right",
                    "Moves the selected vertexes one grid square away in the pressed direction, \
                     unless the move generates at least one illegally shaped brush (concave).\nIf \
                     a moved vertex overlaps a non selected one, this vertex will be selected as \
                     well."
                ),
                (
                    "Enter",
                    "If there are only two selected vertexes on each selected brush that has \
                     selected vertexes, it will split them in two using the line passing through \
                     the vertexes as clip line.\nIt will fail if at least one brush is a triangle."
                ),
                (
                    "Backspace",
                    "Deletes all selected vertexes, unless there is at least one brush that would \
                     become a point or line, or be erased, if such vertexes were deleted."
                ),
                (
                    VertexInsert,
                    "Vertex insertion subtool. Selecting it and then left clicking on the side of \
                     a selected brush will enabled vertex insertion.  *"
                ),
                (
                    VertexMerge,
                    "Vertexes merge subtool. Generates a new brush from the selected vertexes, if \
                     there are more than 3."
                ),
                (
                    VertexSplit,
                    "Vertexes split subtool. Splits brushes that have two selected vertexes in \
                     two. It will fail if at least one brush is a triangle "
                )
            );

            manual_section!(
                Side,
                (
                    "Left mouse",
                    "If there is a non-selected side beneath the cursor, it will be exclusively \
                     selected. If there is no side underneath, when the mouse button is released, \
                     all selected sides they will be deselected."
                ),
                (
                    "Shift + Left mouse",
                    "If there is a selected side beneath the cursor, its selection status will be \
                     toggled."
                ),
                (
                    "Left mouse + cursor drag",
                    "If there is a selected side beneath the cursor, all selected sides will be \
                     dragged around the map. Unless the move generates at least one illegally \
                     shaped brush (concave). If there is no side, a drag selection will be \
                     initiated.\nWhen the mouse button is released, the sides within the drag \
                     selection area will be exclusively selected.\nIf a moved side overlaps a non \
                     selected one, this side will be selected as well."
                ),
                (
                    "Shift + Left mouse + cursor drag",
                    "Same as Left mouse + drag, except the sides within the boundary of the drag \
                     selection are added to the selected brushes, if they are not already \
                     selected."
                ),
                (
                    "Alt + Left mouse + cursor drag",
                    "Initiates the xtrusion process on the selected side.\nIf the cursor is moved \
                     away from the brush, the side will be extruded, generating a new brush. \
                     Otherwise the brush will be split in two parts by a line with the same slope \
                     as the selected side.\nBoth extrusion and intrusion can be executed on \
                     multiple selected sides, as long as they all have the same normal."
                ),
                (
                    "Up/Down/Left/Right",
                    "Moves the selected sides one grid square away in the pressed direction, \
                     unless the move generates at least one illegally shaped brush (concave). \
                     \nIf a moved side overlaps a non selected one, this side will be selected as \
                     well."
                ),
                (
                    "Backspace",
                    "Deletes all selected sides, unless there is at least one brush that would \
                     become a point or line if such sides were deleted."
                ),
                (
                    SideXtrusion,
                    "Side xtrusion subtool. Selecting it and then left clicking on the selected \
                     side of a selected brush will start the xtrusion process. Pressing Esc \
                     disables it."
                ),
                (
                    SideMerge,
                    "Sides merge subtool. Generates a new brush from the selected sides, if there \
                     are more than two."
                )
            );

            manual_section!(
                Snap,
                (
                    "Snap Tool key",
                    "Based on the active tool the following will be snapped to the grid:\n- \
                     Vertex Tool: selected vertexes;\n- Side Tool: selected sides;\n- Thing tool: \
                     selected things;\n- Entity tool: selected entities;\n- any other tool: \
                     selected brushes."
                ),
                (
                    "Alt + Snap Tool key",
                    "Quick snap: snaps the entities to a two-units size grid."
                )
            );

            manual_section!(
                Clip,
                ("Left mouse", "Places the points through which the clipping line passes."),
                (
                    "Alt + Left mouse",
                    "If there is a side of a selected brush beneath the cursor, and there are two \
                     or more selected brushes, all brushes are clipped by the line passing \
                     through the vertexes of such side."
                ),
                (
                    "Tab",
                    "Changes the brushes that are spawned after the clip has been executed.\nBy \
                     default, both brushes on the right and left of the clip line are spawned, \
                     but this can be changed to just the left or right ones. Pressing Alt along \
                     with Tab cycles which brushes are spawned in the opposite order."
                ),
                ("Enter", "Confirms the clip."),
                (
                    ClipSide,
                    "Side clip subtool. Selecting it allows to choose the side of the brush to be \
                     used as clipping line. Can only be enabled when there are two or more \
                     selected brushes."
                )
            );

            manual_section!(
                Shatter,
                (
                    "Left mouse",
                    "Shatters the highlighted selected brush beneath the cursor into triangles \
                     which have a common vertex in the cursor position.\nThe common vertex can be \
                     a vertex of the original brush, a point on a side, or a point inside the \
                     brush's area."
                )
            );

            manual_section!(
                Hollow,
                (
                    "Hollow Tool key",
                    "Creates rooms out of the selected brushes with walls that are as thick as \
                     the grid size. Does nothing if there is at least one selected brush which \
                     cannot be properly hollowed."
                )
            );

            manual_section!(
                Scale,
                (
                    "Left mouse + cursor drag",
                    "Clicking a corner of the outline encompassing all selected brushes and \
                     dragging it shears the selected brushes.\nThe scale will not occur if the \
                     moved corner would overlap or 'go over' a nearby side."
                ),
                (
                    "Tab",
                    "Changes the outline's selected vertex. The selection order is clockwise. \
                     Pressing Alt along with Tab reverses the order."
                ),
                (
                    "Up/Down/Left/Right",
                    "Moves the outline's selected corner one grid square away in the pressed \
                     direction."
                ),
                TEXTURE,
                "Target:\n-Polygon, only the polygons are scaled;\n-Texture, only the textures \
                 are scaled."
            );

            manual_section!(
                Shear,
                (
                    "Left mouse + cursor drag",
                    "Clicking a side of the outline encompassing all selected brushes and \
                     dragging it shears the selected brushes."
                ),
                (
                    "Tab",
                    "Changes the outline's selected side. The selection order is clockwise. \
                     Pressing Alt along with Tab reverses the order."
                ),
                (
                    "Up/Down/Left/Right",
                    "Moves the outline's selected side one grid square away in the pressed \
                     direction."
                )
            );

            manual_section!(
                Rotate,
                (
                    "Left mouse + cursor drag",
                    "If the rotation pivot is clicked it will be dragged at a new location. \
                     Otherwise rotates the selected brushes around the pivot by the selected \
                     angle snap."
                ),
                (
                    "Left/Right",
                    "Rotates the selected brush in clockwise (Right) or counterclokwise (Left) \
                     direction by the set angle snap."
                ),
                (
                    "Alt + Up/Down/Left/Right",
                    "Moves the pivot a grid square away in the pressed direction."
                ),
                (
                    RotatePivot,
                    "Pivot subtool. Changes the position of the rotation pivot either by pressing \
                     the directional keys or left clicking with the mouse."
                ),
                TEXTURE,
                "Target:\n-Polygon, only the polygons are rotated;\n-Both, both polygons and \
                 associated textures are rotated;\n-Texture, only the textures are rotated."
            );

            manual_section!(
                Flip,
                (
                    "Up/Down/Left/Right",
                    "Creates mirrored copies of the selected brushes in the pressed direction."
                )
            );

            manual_section!(
                Intersection,
                (
                    "Intersection Tool key",
                    "Generates the intersection brush of the selected brushes. If not all \
                     selected brushes overlap over a common area they will be erased from the map."
                )
            );

            manual_section!(
                Merge,
                (
                    "Merge Tool key",
                    "Merges all the vertexes of the selected brushes into one convex encompassing \
                     brush. The selected brushes are erased."
                )
            );

            manual_section!(
                Subtract,
                (
                    "Left mouse",
                    "Selects/deselects the brush beneath the cursor, from which the selected \
                     brush will be subtracted."
                ),
                ("Enter", "Executes the subtraction.")
            );

            manual_section!(
                Paint,
                (
                    "PROPS",
                    "A prop is a collection of entities which can be painted around the map like \
                     the brushes of a image editing tool.\nEach prop has a pivot, the point \
                     relative to which the it is painted onto the map.\n\nProps can be imported \
                     and exported between map files. The file extension of the props files is \
                     .prps."
                ),
                (
                    "Enter",
                    "Initiates the prop creation process. A prop is generated from the selected \
                     entities, and after a pivot is specified it can be stored in the specified \
                     slot and later be painted around the map after being selected.\nIf no slot \
                     number is specified the prop is stored in a temporary slot."
                ),
                (
                    "Left mouse",
                    "After the prop creation process is initiated, pressing left mouse within the \
                     borders of the prop outline will set its pivot."
                ),
                (
                    "Left mouse + cursor drag",
                    "Paints the prop in the selected slot around the map."
                ),
                ("Backspace", "Removes the prop in the selected slot."),
                (PaintCreation, "Initiates the prop creation process."),
                (PaintQuick, "Generates a prop and places it in the temporary slot.")
            );

            manual_section!(
                Path,
                (
                    "INFO",
                    "When enabled, the entities (brushes and things) will be split in three \
                     groups, red, grey and opaque.\nThe red entities include those that are \
                     moving and were previously selected, and the brushes anchored to them.\nThe \
                     grey entities include those that were previously selected, but not moving \
                     and are not anchored to another brush. Therefore they are entities which \
                     make for moving candidate.\nThe opaque entities include all other cases, \
                     that is, entities that were not selected and\\or cannot be transformed into \
                     moving.\n\nA path can have overlapping nodes. However, two consecutive nodes \
                     cannot overlap. Overlapping nodes are clearly shown in the tooltips. \
                     Therefore, it is highly encouraged to leave them on."
                ),
                ("Alt + Left mouse", "If a grey brush is clicked the path creation is enabled."),
                (
                    "Left mouse",
                    "If path creation is enabled a new node will be placed.\nOtherwise, if a \
                     non-selected node is clicked, it will be exclusively selected."
                ),
                (
                    "Shift + Left mouse",
                    "Clicking a node will toggle its selection status, adding or removing it to \
                     the selected nodes."
                ),
                (
                    "Left mouse + cursor drag",
                    "If a selected node is clicked, all selected nodes will be dragged. \
                     Otherwise, a drag selection will be initiated. When the mouse button is \
                     released, all nodes within the boundaries of the outline will be exclusively \
                     selected.\nIf a new node is being inserted in a path that single node will \
                     be dragged around."
                ),
                (
                    "Shift + Left mouse + cursor drag",
                    "Releasing the Left mouse button after having initiated a drag selection, \
                     while pressing Shift, will select all non-selected nodes within the \
                     boundaries."
                ),
                ("Right mouse", "While creating a new path, clicking on a node will remove it."),
                (
                    "Backspace",
                    "Deletes all selected nodes, unless doing so would generate a path with a \
                     single node or a path with consecutive overlapping nodes.."
                ),
                ("Alt + backspace", "Deletes the paths of the red entities."),
                (
                    "Up/Down/Left/Right",
                    "Moves all selected nodes a grid square away in the pressed direction."
                ),
                (
                    "Enter",
                    "Ends the new path free draw.\nToggles and pauses the moving platforms \
                     movement simulation."
                ),
                ("Esc", "Exits path creation and movement simulation."),
                (
                    PathFreeDraw,
                    "Path free draw subtool. Selecting it and then left clicking a brush with no \
                     path and not anchored start the path drawing process. Pressing Esc disables \
                     it."
                ),
                (
                    PathAddNode,
                    "Add node subtool. Selecting it and then left clicking a node will start the \
                     node insertion process."
                ),
                (
                    PathSimulation,
                    "Movement simulation subtool. Selecting it starts the movement simulation."
                )
            );

            manual_section!(
                no_separator,
                Zoom,
                (
                    "Left mouse + cursor drag",
                    "Creates a drag selection that determines the area onto which the viewport \
                     will be zoomed. Zoom is actuated once the Left mouse button is released."
                )
            );
        }

        let StateUpdateBundle {
            egui_context,
            key_inputs,
            ..
        } = bundle;

        if !self
            .0
            .check_open(key_inputs.just_pressed(HardcodedActions::ToggleManual.key()))
        {
            return;
        }

        self.0.show(
            egui_context,
            egui::Window::new("Manual")
                .vscroll(true)
                .min_width(400f32)
                .default_width(800f32)
                .min_height(300f32)
                .default_height(600f32),
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0f32, 12f32);
                ui.add_space(8f32);
                draw(ui, tools_buttons);
                // You would think this does nothing, but it actually does something
                ui.add_space(0f32);
            }
        );
    }
}
