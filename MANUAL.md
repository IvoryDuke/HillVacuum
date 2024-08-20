## General

### Brush
A brush is a convex polygonal surfaces. It can have an associated texture which can either be drawn filling its surface or as a sprite. The sprite can be displaced independently of the brush's surface.  
Brushes can also be assigned a path that describes how it moves in the bidimensional space and that can be edited with the Path tool.  
Finally, brushes have a built-in property, `collision`, which determines whether they should represent a clipping surface or not. It can be edited in the properties window.

### Thing
A thing is an object which can be placed around the map. It is characterized by an ID, a width and height, a name, and a texture which represents it.  
Things can also be assigned a path that describes how it moves in the bidimensional space and that can be edited with the Path tool.  
Things can either be defined in one or many .ini files to be placed in the `assets/things/` folder or, if `HillVacuum` is used as a library, implementing the `MapThing` interface for the structs representing an object to be placed in the map and using the `hardcoded_things` macro to insert them in the bevy App.  
If defined in the .ini files, the things must follow a similar format:
```ini
[Name]
width = N
height = M
id = ID
preview = TEX
```
Where `ID` is an unique identifier between 0 and 65534, and `TEX` is the name of the texture to be drawn along with the bounding box.  
If a thing defined through the `MapThing` interface has the same `ID` as one loaded from file, the latter will overwrite the former.  
Finally, things have two built-in properties, `angle` and `draw height`. The orientation of the arrow drawn on top of the things will change based on the value of `angle`, and `draw height` determines its draw order. They can be edited in the properties window.
    
Things can be reloaded while the application is running through the UI button in the Options menu.

### Properties
Properties are custom user defined values which can be associated to brushes and things.  
Such values can be inserted through the `brush_properties` and `thing_properties` macros by specifying the pairs `(name, default_value)` of the properties.  
Properties can be edited per-entity using the properties window.  
Currently supported value types are `bool`, `u8`, `u16`, `u32`, `u64`, `u128`, `i8`, `i16`, `i32`, `i64`, `i128`, `f32`, `f64`, and `String`.  
  
!!! If a saved map contains properties that differ in type and/or name from the ones defined in the aforementioned resources, a warning window will appear on screen when trying to load the .hv file, asking whether you'd like to use the app or map ones.

### Texture
Textures must be placed in the `assets/textures/` folder to be loaded.  
The texture editor can be opened at any time to edit the properties of the textures of the selected brushes.  
Entity, scale, and rotate tool also feature texture editing capabilities. These capabilities can be either enabled through the dedicated "Target" UI element in the bottom left area, or by pressing `Alt + texture editor bind`.  
Textures can have an associated animation which can either consist of a list of textures to display, each one for a specific time, or an atlas of textures generated by subdividing the textures in subareas. The animations can be applied to the texture as a default or to the texture of the selected brushes only.  
When editing a list type animation, it is possible to add a texture by clicking it with the left mouse button.  
To edit the animation of a texture that is not the one of the selected brushes, it needs to be pressed with the right mouse button.  
  
Textures can be reloaded while the application is running through the UI button in the Options menu.  
Default textures animation can be exported and imported between map files. The file extension of the animations files is `.anms`.

### Prop
A prop is a collection of entities which can be painted around the map like the brushes of an image editing tool.  
Each prop has a pivot, the point relative to which it is painted onto the map.  
Props can be imported and exported between map files. The file extension of the props files is `.prps`.

### Path
A path is a series of nodes describing how the entity that owns it moves over time around the map.  
Nodes have five customizable parameters:  
- `Standby`: the amount of time the entity stands still before starting to move to the next node;  
- `Min speed`: the minimum speed the entity moves;  
- `Max speed`: the maximum speed the entity reaches;  
- `Accel (%)`: the percentage of the distance between the current node and the next that the entity will spend accelerating from the minimum to the maximum speed;  
- `Decel (%)`: the percentage of the distance between the current node and the next that the entity will spend decelerating from the maximum to the minimum speed.  
The maximum speed can never be lower than the minimum speed and it can never be 0. The acceleration and deceleration percentages always amount to 100% at most. The acceleration phase always comes before the deceleration one.  
A path can have overlapping nodes. However, two consecutive nodes cannot overlap. Overlapping nodes are clearly shown in the tooltips. Therefore, it is highly encouraged to leave them on.

### Grid
The map grid can be skewed and/or rotated to give the map an isometric look. These two parameters can be edited in the settings window.

### Cursor
A semitransparent square is shown on screen to represent the world position of the cursor. This is useful to show where the camera will zoom in/out when pressing `Ctrl + Mouse wheel`.

### Subtools
Subtools are UI elements that show up on the right when certain tools are selected to provide easy access to functions bound to hardcoded shortcuts.  
Left clicking the first time enables them, clicking a second time disables them.  
Every subtool shows the associated keyboard shortcut in the tooltip.  
Subtools can be disabled by pressing Escape.

### Tab
Whenever a tool allows you to select/deselect entities, `Tab` can be pressed to select the next element beneath it, if any.

### Tools
Tools can be selected by clicking the icons on the left side of the screen or by pressing the bound key.  
Bound keys can be viewed and changed through the bind menu.

&nbsp;

## Edit

### INFO
These commands can only be used while there is no ongoing edit.

### Ctrl + A
Select all, selects all the elements of the category the currently selected tool is capable of editing (entities, vertexes, sides, etc. etc.).

### Ctrl + C
Copy, copies the selected entities, or the path of the entity beneath the cursor, if any, when using the Path tool.

### Ctrl + V
Paste, creates copies of the selected entities, or sets the path of the entity beneath the cursor to the copied one, if any, when using the Path tool.

### Ctrl + C
Copy, copies the selected entities, or the path of the entity beneath the cursor, if any, when using the Path tool.

### Ctrl + D
Duplicates the entities. Equivalent to `Ctrl + Alt + Right` when using the Entity tool.

### Ctrl + Z
Undo.

### Ctrl + Y
Redo.

&nbsp;

## View

### Space
Drags the camera around.

### Ctrl + Up/Down/Left/Right
Moves the camera one grid square in the pressed direction.

### Ctrl + Plus
Zooms the camera in.

### Ctrl + Minus
Zooms the camera out.

### Mouse wheel
Moves the camera up/down.

### Shift + Mouse wheel
Moves the camera left/right/

### Ctrl + Mouse wheel
Zooms the camera towards/outwards the cursor position.

### Alt + Zoom tool bind
Zooms the camera on the selected entities.

&nbsp;

## Square tool
<img src="images/square.svg" alt="square" height="48" width="48"/>  

### Left mouse
Spawns a grid square shaped brush.

### Left mouse + cursor drag
Creates a rectangular shaped brush spawned when the mouse button is released.

### Backspace
Deletes all drawn brushes.

&nbsp;

## Triangle tool
<img src="images/triangle.svg" alt="triangle" height="48" width="48"/>  

### Left mouse
Spawns a right triangle with right angle placed at the closest grid lines intersection and legs grid-square-side sized.

### Left mouse + cursor drag
Creates a right triangle shaped brush spawned when the mouse button is released.

### Tab
Changes the orientation of the triangle being drag spawned.

&nbsp;

## Circle tool
<img src="images/circle.svg" alt="circle" height="48" width="48"/>  

### Left mouse
Spawns an ellipse shaped brush inscribed in the hovered grid square.

### Left mouse + cursor drag
Spawns an ellipse shaped brush when the mouse button is released.

### Plus
Increases the ellipse resolution.

### Minus
Decreases the ellipse resolution.

&nbsp;

## Free draw tool
<img src="images/free_draw.svg" alt="free_draw" height="48" width="48"/>  

### Left mouse
Attempts to add a vertex to the shape being drawn. Nothing happens if the shape generated adding such vertex is concave, or the shape already contains that vertex.

### Right mouse
Deletes the vertex beneath the cursor.

### Enter
Attempts to spawn the shape currently being drawn. Nothing happens if the shape is just a point or a line.

### Escape
Erases the brush being drawn.

&nbsp;

## Thing tool
<img src="images/thing.svg" alt="thing" height="48" width="48"/>  

### Pivot
Determines how the selected thing is spawned on the map with respect to the mouse position. For example, if the pivot is set to TopLeft the thing is spawned with its top left corner placed at the mouse position.

### Left mouse
Spawn the selected thing based on the selected pivot.  
If a thing in the UI gallery at the bottom of the screen is pressed, all drawn things after that will represent that thing.

### Alt + Left mouse
If a thing in the UI gallery at the bottom of the screen is pressed, all selected and drawn things are changed to be that thing.

### Tab
Sets the pivot to the next possible value. If `Alt` is pressed as well it is set to the previous value.

### Backspace
Deletes all drawn things.

### Thing change subtool
<img src="images/thing_change.svg" alt="thing_change" height="48" width="48"/>  
Allows to change the selected things placed on the map to the thing clicked in the UI.

&nbsp;

## Entity tool
<img src="images/entity.svg" alt="entity" height="48" width="48"/>  

### INFO
Brushes can be tied together into a group through the Anchor subtool. This establishes a owner-anchored relation between the brushes. An "owner" brush can have an unlimited amount of brushes tied to it. A brush that is anchored can have none.  
When the "owner" brush is moved all anchored brushes are moved as well even if they are not selected.

### Left mouse
If there is a non-selected entity beneath the cursor, it is be exclusively selected. If there is no entity, all entities are deselected when the mouse button is released.  
Clicking brush while holding `Ctrl` causes all anchored brushes to be selected as well.

### Shift + Left mouse
If there is an entity beneath the cursor, its selection status is toggled.  
Pressing `Ctrl` on a brush causes all anchored brushes to be toggled as well.

### Left mouse + cursor drag
If there is a selected entity beneath the cursor, all selected entities are dragged around the map. If there is no entity, a drag selection is initiated.  
When the mouse button is released, the entities within the drag selection area are exclusively selected.  
Pressing `Ctrl` all anchored brushes are selected as well.

### Shift + Left mouse + cursor drag
Same as `Left mouse + drag`, except the entities within the boundary of the drag selection are added to the selected brushes, if they are not already selected.  
Pressing `Ctrl` all anchored brushes are selected as well.

### Alt + Left mouse + cursor drag
If there is a selected entity beneath the cursor, copies of the selected entities are spawned in the direction the cursor is moved.

### Up/Down/Left/Right
Moves the selected entities one grid square away in the pressed direction.

### Alt + Up/Down/Left/Right
Creates copies of the selected entities one grid square away in the pressed direction.

### Right mouse
Clicking a brush with no path and not anchored allows to anchor it to another brush. Clicking on an anchored brush disanchors it.

### Drag spawn subtool
<img src="images/entity_drag_spawn.svg" alt="entity_drag_spawn" height="48" width="48"/>  
Selecting it and then pressing a directional key, or left clicking and dragging with the cursor a selected brush, spawns copies of the selected entities in the direction the cursor is moved.

### Anchor subtool
<img src="images/entity_anchor.svg" alt="entity_anchor" height="48" width="48"/>  
Toggles the brush anchor routine.

### TEXTURE EDITING
Target:  
- `Entity`, only moves entities;  
- `Both`, moves both entities and textures;  
- `Texture`, only moves textures.

&nbsp;

## Vertex tool
<img src="images/vertex.svg" alt="vertex" height="48" width="48"/>  

### Left mouse
If there is a non-selected vertex beneath the cursor, it is exclusively selected. If there is no vertex underneath, when the mouse button is released all selected vertexes are deselected.

### Shift + Left mouse
If there is a vertex beneath the cursor, its selection status is toggled.

### Left mouse + cursor drag
If there is a selected vertex beneath the cursor, all selected vertexes are dragged around the map. Unless the move generates at least one illegally shaped brush. If a moved vertex overlaps a non selected one, this vertex is selected as well.  
If there is no vertex, a drag selection is initiated. When the mouse button is released, the vertexes within the drag selection area are exclusively selected.

### Shift + Left mouse + cursor drag
Same as `Left mouse + cursor drag`, except the vertexes within the boundary of the drag selection are added to the selected brushes if they are not already selected.

### Alt + Left mouse
Inserts a new vertex on the line that passes through the cursor position. Such vertex can then be dragged around as long as it does not cause the resulting shape to be concave.  

### Up/Down/Left/Right
Moves the selected vertexes one grid square away in the pressed direction, unless the move generates at least one illegally shaped brush.  
If a moved vertex overlaps a non selected one, it is selected as well.

### Enter
If there are only two selected vertexes on each selected brush that has selected vertexes, splits them in two using the line passing through the vertexes as clip line.  
It fails if at least one brush is a triangle or the selected vertexes are consecutive.  
Otherwise, if the polygon to path subtool is enabled, pressing it finalizes the path creation and the generated path can be assigned to an entity.

### Backspace
Deletes all selected vertexes, unless there is at least one brush that would become a point or line, or be erased, if such vertexes were deleted.

### Alt + Merge tool bind
Generates a new brush from the selected vertexes, if there are more than 3.

### Vertex insertion subtool
<img src="images/vertex_insert.svg" alt="vertex_insert" height="48" width="48"/>  
Selecting it and then left clicking on the side of a selected brush enables vertex insertion.

### Vertexes merge subtool
<img src="images/vertex_merge.svg" alt="vertex_merge" height="48" width="48"/>  
Executes the selected vertexes merge.

### Vertexes split subtool
<img src="images/vertex_split.svg" alt="vertex_split" height="48" width="48"/>  
Executes the brushes' split.

### Polygon to path subtool
<img src="images/vertex_polygon_to_path.svg" alt="vertex_polygon_to_path" height="48" width="48"/>  
After being enabled, vertexes of the selected brushes can be clicked in sequence to create a path which can then be assigned to an entity by pressing `Enter`.

&nbsp;

## Side tool
<img src="images/side.svg" alt="side" height="48" width="48"/>  

### Left mouse
If there is a non-selected side beneath the cursor, it is exclusively selected. If there is no side underneath, when the mouse button is released, all selected sides they are deselected.

### Shift + Left mouse
If there is a selected side beneath the cursor, its selection status is toggled.

### Left mouse + cursor drag
If there is a selected side beneath the cursor, all selected sides are dragged around the map. Unless the move generates at least one illegally shaped brush. If there is no side, a drag selection is initiated.  
When the mouse button is released, the sides within the drag selection area are exclusively selected.  
If a moved side overlaps a non selected one, this side is selected as well.

### Shift + Left mouse + cursor drag
Same as `Left mouse + cursor drag`, except the sides within the boundary of the drag selection are added to the selected brushes if they are not already selected.

### Alt + Left mouse + cursor drag
If a selected side is clicked it initiates the xtrusion process on the selected side.
If the cursor is moved away from the brush, the side is extruded, generating a new brush. Otherwise the brush is split in two by a line with the same slope as the selected side.  
Both extrusion and intrusion can be executed on multiple selected sides, as long as they all have the same slope.

### Up/Down/Left/Right
Moves the selected sides one grid square away in the pressed direction, unless the move generates at least one illegally shaped brush.  
If a moved side overlaps a non selected one, this side is selected as well.

### Backspace
Deletes all selected sides, unless there is at least one brush that would become a point or line if such sides were deleted.

### Alt + Merge tool bind
Generates a new brush from the selected sides, if there are more than 2.

### Side xtrusion subtool
<img src="images/side_xtrusion.svg" alt="side_xtrusion" height="48" width="48"/>  
Selecting it and then left clicking on a selected side of starts the xtrusion process.

### Sides merge subtool
<img src="images/side_merge.svg" alt="side_merge" height="48" width="48"/>  
Executes the selected sides merge.

&nbsp;

## Snap tool
<img src="images/snap.svg" alt="snap" height="48" width="48"/>  

### Snap Tool key
Based on the active tool the following are snapped to the grid:
- `Entity tool`: selected entities;  
- `Thing tool`: selected things;  
- `Vertex Tool`: selected vertexes;  
- `Side Tool`: selected sides;  
- any other tool: selected brushes.

### Alt + Snap Tool key
Quick snap: snaps the entities to a two-units size grid.

&nbsp;

## Clip tool
<img src="images/clip.svg" alt="clip" height="48" width="48"/>  

### Left mouse
Places the points through which the clipping line passes.

### Alt + Left mouse
If there is a side of a selected brush beneath the cursor, and there are two or more selected brushes, all brushes are clipped by the line passing through the vertexes of such side.

### Tab
Changes the brushes that are spawned after the clip has been executed.  
By default, both brushes on the right and left of the clip line are spawned, but this can be changed to just the left or right ones. If `Alt` is pressed as well the brushes are cycled in the opposite order.

### Enter
Confirms the clip.

### Side clip subtool
<img src="images/clip_side.svg" alt="clip_side" height="48" width="48"/>  
Selecting it allows to choose the side of the brush to be used as clipping line. Can only be enabled when there are two or more selected brushes.

&nbsp;

## Shatter tool
<img src="images/shatter.svg" alt="shatter" height="48" width="48"/>  

### Left mouse
Shatters the highlighted selected brush beneath the cursor into triangles which have a common vertex in the cursor position.  
The common vertex can be a vertex of the original brush, a point on a side, or a point inside the brush's area.

&nbsp;

## Hollow tool
<img src="images/hollow.svg" alt="hollow" height="48" width="48"/>  

### Hollow Tool key
Creates rooms out of the selected brushes with walls that are as thick as the grid size. Does nothing if there is at least one selected brush which cannot be properly hollowed.

&nbsp;

## Scale tool
<img src="images/scale.svg" alt="scale" height="48" width="48"/>  

### Left mouse + cursor drag
Clicking a corner of the outline encompassing all selected brushes and dragging it scales the selected brushes.  
The scale does not occur if the moved corner would overlap a nearby one.

### Up/Down/Left/Right
Scales the selected brushes in the pressed direction by one grid square.

### Tab
Changes the outline's selected vertex. The selection order is clockwise. If `Alt` is pressed as well the vertexes are cycled counter-clockwise.

### TEXTURE EDITING
Target:  
- `Entity`, only scales the polygons;  
- `Both`, scales both polygons and textures;  
- `Texture`, only scales the textures.

&nbsp;

## Shear tool
<img src="images/shear.svg" alt="shear" height="48" width="48"/>  

### Left mouse + cursor drag
Clicking a side of the outline encompassing all selected brushes and dragging it shears the selected brushes.

### Up/Down/Left/Right
Shears the selected brushes in the pressed direction by one grid square.

### Tab
Changes the outline's selected side. The selection order is clockwise. If `Alt` is pressed as well the sides are cycled in reverse order.

&nbsp;

## Rotate tool
<img src="images/rotate.svg" alt="rotate" height="48" width="48"/>  

### Left mouse + cursor drag
Drags to the mouse cursor position, if the rotation pivot is clicked. Otherwise rotates the selected brushes around the pivot by the selected angle snap.

### Left/Right
Rotates the selected brush in clockwise (`Right`) or counterclokwise (`Left`) direction by the set angle.

### Alt + Up/Down/Left/Right
Moves the rotation pivot a grid square away in the pressed direction.

### Pivot subtool
<img src="images/rotate_pivot.svg" alt="rotate_pivot" height="48" width="48"/>  
While enabled, the position of the rotation pivot can be changed by either pressing the directional keys or left clicking with the mouse.

### TEXTURE EDITING
Target:  
- `Entity`, rotates only the polygons;  
- `Both`, rotates both polygons and textures;  
- `Texture`, rotates only the textures.

&nbsp;

## Flip tool
<img src="images/flip.svg" alt="flip" height="48" width="48"/>  

### Up/Down/Left/Right
Creates mirrored copies of the selected brushes in the pressed direction.

&nbsp;

## Intersection tool
<img src="images/intersection.svg" alt="intersection" height="48" width="48"/>  

### Intersection Tool key
Generates the intersection brush of the selected brushes. If not all selected brushes overlap over a common area they are erased from the map.

&nbsp;

## Merge tool
<img src="images/merge.svg" alt="merge" height="48" width="48"/>  

### Merge Tool key
Merges all the vertexes of the selected brushes into one convex encompassing brush. The selected brushes are then erased.

&nbsp;

## Subtract tool
<img src="images/subtract.svg" alt="subtract" height="48" width="48"/>  

### Left mouse
Selects/deselects the brush beneath the cursor, from which the selected brush is subtracted.

### Enter
Executes the subtraction.

&nbsp;

## Paint tool
<img src="images/paint.svg" alt="paint" height="48" width="48"/>  

### INFO
When created, the props can be stored in slots displayed in the UI gallery at the bottom of the screen (such gallery is not shown if there are no stored props).  
Props can either be stored in a numbered slot by specifying its number in the window that pops up during the prop creation process, or in the quick slot by not typing any number. Only one prop can be placed in the quick slot.

### Enter
Initiates the prop creation process. A prop is generated from the selected entities, and after a pivot is chosen it can be stored in the specified slot and later be painted around the map after being selected.  
If no slot number is specified the prop is stored in a temporary slot.

### Alt + Enter
Selects the prop placed in the temporary slot. After drawing it on the map it is automatically deselected.

### Left mouse
Paints the prop in the selected slot, if any, so that its pivot coincide with the cursor position.  
If a prop in the UI gallery at the bottom of the screen is clicked, it is selected as the prop to be painted.  
After the prop creation process is initiated, clicking within the borders of the prop outline sets its pivot.

### Left mouse + cursor drag
Paints the prop in the selected slot around the map.

### Backspace
Removes the prop in the selected slot.

### Prop creation subtool
<img src="images/paint_creation.svg" alt="paint_creation" height="48" width="48"/>  
Initiates the prop creation process.

### Quick prop subtool
<img src="images/paint_quick.svg" alt="paint_quick" height="48" width="48"/>  
Selects the prop placed in the temporary slot.

&nbsp;

## Path tool
<img src="images/path.svg" alt="path" height="48" width="48"/>  

### INFO
When enabled, the entities are split in three groups:  
- entities that have a path and are selected, and the brushes anchored to them;  
- entities that are selected, but do not have a path and are not anchored to another brush. Therefore they are entities which can have a path;  
- all other cases, entities that are not selected and/or cannot have a path.  

### Alt + Left mouse
If an entity that can have a path is clicked the path creation is enabled.  
Otherwise, if a node is clicked, inserts a new node in the path of the clicked node, after such node. The node can then be dragged around as long as it does not cause the resulting path to have consecutive overlapping nodes.  

### Left mouse
If path creation is enabled a new node is placed. Otherwise, if a non-selected node is clicked, it is exclusively selected.

### Right mouse
While creating a new path, clicking on a node removes it.

### Shift + Left mouse
Clicking a node toggles its selection status.

### Left mouse + cursor drag
If a selected node is clicked, all selected nodes are dragged. Otherwise, a drag selection is initiated. When the mouse button is released, all nodes within the boundaries of the outline are exclusively selected.

### Shift + Left mouse + cursor drag
Same as `Left mouse + cursor_drag`, except the nodes within the boundary of the drag selection are added to the selected brushes if they are not already selected.

### Backspace
Deletes all selected nodes, unless doing so would generate a path with a single node or a path with consecutive overlapping nodes.

### Alt + backspace
Deletes the paths of the selected entities.

### Up/Down/Left/Right
Moves all selected nodes a grid square away in the pressed direction.

### Enter
If there is an ongoing path creation it ends it, otherwise toggles and pauses the moving platforms movement simulation.

### Esc
Exits path creation and movement simulation.

### Path free draw subtool
<img src="images/path_free_draw.svg" alt="path_free_draw" height="48" width="48"/>  
Selecting it and then left clicking a brush with no path and not anchored starts the path drawing process.

### Insert node subtool
<img src="images/path_insert_node.svg" alt="path_insert_node" height="48" width="48"/>  
Selecting it and then left clicking a node starts the node insertion process.

### Movement simulation subtool
<img src="images/path_simulation.svg" alt="path_simulation" height="48" width="48"/>  
Selecting it starts the movement simulation.

&nbsp;

## Zoom tool
<img src="images/zoom.svg" alt="zoom" height="48" width="48"/>  

### Left mouse + cursor drag
Creates a drag selection that determines the area onto which the viewport is zoomed. Zoom is actuated once the Left mouse button is released.

&nbsp;
