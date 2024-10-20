# Changelog

## 0.9.0

### Changes
- Simplified texture parallax calculations, changed signature of `draw_offset_with_parallax_and_scroll`;
- `Tab` and `Alt+Tab` can now be used to scroll through the ui widgets;
- decreased file size of `Prop`s;
- `Node`'s fields are now public;
- reduced crates required to compile;
- improved drawing of things in `Prop` previews.

### Fixes
- Added missing collision overlay in clipped brushes;
- fixed layout of default `.ini` settings;
- fixed empty edit sometimes created by the selection of attached brushes;
- fixed a paint tool crash occurring when no prop is selected;
- fixed crash that could occur when creating a shape with the draw tool;
- fixed sometimes incorrect camera zoom;
- fixed sometimes incorrect path nodes selection;
- fixed a few user definer properties related issues.

## 0.8.2

### Fixes
- Fixed Windows compilation;
- fixed wrong texture offset on moving brushes;
- fixed missing and incorrect vertex tooltips in the side tool.

## 0.8.1

### Fixes
Fixed brush texture settings not being appropriately converted to the new format to be drawn the same way as in previous versions.

## 0.8.0

### Changes
- Revamped texture scale and rotation for better texture editing;
- improved texture editor;
- added "Reset" button in the texture editor;
- scroll and parallax direction are now always perpendicular to the x and y axis regardless of the texture angle. Scroll values from older map files are automatically converted to the new system, unfortunately parallax is a breaking change;
- added window message in case of a fatal error;
- added a column in the properties editor displaying their types.
- added `draw_offset` and `draw_offset_with_parallax_and_scroll` to `TextureInterface`;
- grid skew and angle changes now mark the map as having unsaved changes;
- improved the way the camera position is capped to the map size;
- improved the reliability of the selection of map items;
- merged the brush and thing hardcoded properties in their respective `properties` hashmaps.

### Fixes
- Fixed overlapping actions in rotate tool;
- fixed an issue where undoing changes past the last saved edit would not result in the map marked as in need of being saved when quitting the application;
- fixed an issue where properties could all be converted to a `String`;
- fixes brush sprites being incorrectly rendered;
- fixed entity tool crash when attempting to move textures but none were selected;
- fixed one of the brushes created from a subtraction having no texture;
- fixed incorrect sprite hull drawing when using isometric grid;
- fixed broken sprite selection when using isometric grid.

### !!!
Due to the unwieldy size that the code that ensure compatibility with previous map versions was reaching, this version on HillVacuum only successfully imports map files created with versions 0.7.0->0.7.2.

## 0.7.2

### Changes
Removed texture pivoted rotation, requires refinements.

## 0.7.1

### Changes
- Rotation pivot is no longer updated after undo/redo;
- improved rotation pivot, it can be quickly placed elsewhere by pressing `Alt` and left clicking anywhere on the map without first pressing it;
- now textures can be rotated around the pivot as well;
- rotation angle can be changed by typing the value in the UI element, 0 is free rotation;
- made the bottom half of the left panel vertically scrollable for small screen/windows sizes.

### Fixes
- Fixed broken rotate undo/redo;
- fixed crash occurring when moving textures if no sprites were selected;
- fixed rotation angle inconsistencies;
- fixed a crash that could occur with undo/redo of a texture scale edit;
- fixed values sometimes not updating in the texture editor after undo/redo.

## 0.7.0

### Changes
- Added grid settings to the `Exporter` struct;
- Allowed zoom in/out with keyboard even when an UI element is hovered;
- renamed `Mover` to `Group` to clarify the purpose of the enum;
- made `Node::world_pos` private;
- replaced `Path` with `HvVec<Node>`;
- made `Path` and `Hull` private as they are no longer required to be public.

### Fixes
- fixed things tooltip text not being customized according to the setting;
- fixed settings window color options disappearing when editing a bind.

## 0.6.6

### Changes
- Customized colors now also change the color of the related tooltips;
- added option to customize tooltips font color;
- added a tooltip on top of all Things that displays their names;
- removed Anchor subtool. The hardcoded bind associated to anchoring brushes is `Right mouse`, having to enable a subtool an then clicking with the `Left mouse` increased the complexity instead of lowering it;
- partial documentation revamp.

## 0.6.5

### Changes
Thing textures are now drawn accordingly to the animation assigned to them.

### Fixes
Fixed a crash that could occur on shutdown on some computers.

## 0.6.4

### Changes
- Brushes that are split into brushes that amount to all or a fraction of their original surface (ex: cutting them using the clip tool) preserve their Path and/or their attached brushes;
- added missing flip tool documentation.

## Fixes
- Fixed a bug with vertex/side drag selection that could occur when multiple brushes were selected;
- fixed texture editor crash when textures were filtered;
- fixed crash occurring when a thing with the error texture was spawned;
- fixed `hardcoded_things!` macro.

## 0.6.3

### Changes
- Added tool icons to `MANUAL.md`;
- improved documentation;
- added `MANUAL.pdf` to the documentation;
- created a separate application to generate the manuals instead of relying on the main application compilation.

### Fixes
- Documented missing features;
- when opening a new map, the default texture animations of the previously opened map are now cleared before importing the new ones;
- fixed anchor tool crash;
- fixed duplicate crashes;
- fixed missing brush attachments drawing when using the Path tool;
- fixed an issue with the Hollow tool where some wall brushes would not be spawned if multiple brushes were hollowed.

## 0.6.2

### Changes
- Partial renderer rework to favor understanding of which entities are selected/highlighted;
- improved things angle indicator by drawing it in code instead of using an image;
- added `MANUAL.md`, for those who prefer to read the manual in a separate file instead of using the built-in one;
- improved app-manual consistency.

### Fixes
- Fixed height tooltip of the highlighted entity drawn slightly off-center;
- fixed sprites not being rendered when picking the brushes to spawn after a clip;
- fixed extrusion/intrusion not being initiated right after `Alt` clicking on a previously non-selected side;
- fixed vertexes/nodes selection when the area described by the cursor drag includes all of the surface of the owning entity.

## 0.6.1

### Changes
- Further decreased `.hv` file size;
- changed the name of some edits.

### Fixes
- Fixed menu bar edits history label;
- fixed compilation errors.

## 0.6.0

### New
Added edits history window.

### Changes
- the prop screenshots are retaken each time the grid's skew/rotation is changed;
- the camera can be moved while the the UI is focused if the keys required to be pressed are just `shift`, `alt`, or `ctrl`;
- the compatibility code for older `.hv` file versions has been integrated in the crate without having to add older `hill_vacuum` versions to the dependencies.

### Fixes
- Fixed buggy selection of extruded brushes;
- fixed rotation tool related crashes and issues;
- fixed texture angle not rotating the drawn sprite.

## 0.5.0

### Changes
- Changed `.hv` file structure (old files are automatically converted);
- moving the camera with `Ctrl+directional keys` now moves it taking into account the grid's skew and rotation;
- slightly changed how textures are drawn.

### Fixes
- fixed inconsistent brushes outline in the flip tool between the one created when the tool is enabled and the one created after brushes have been flipped;
- fixed "Camera" UI section not displaying the correct position when the grid is rotated and/or skewed;
- fixed prop screenshots when the grid is rotated and/or skewed;
- fixed a formatting error in the default `.ini` settings file. Deleting the file so that the application can create a new one is recommended.

## 0.4.2

### Changes
- Properties window can now be resized;
- texture gallery filters have been moved down in the texture editor window.

### Fixes
Fixed typos in the code and documentation.

## 0.4.1

### Fixes
- Fixed sprite being rendered in the wrong position when the grid is skewed or rotated;
- hidden internal struct `Sprite`, exposed by mistake;
- fixed crate description not to include previews markdown;
- fixed `arena_alloc` compilation error.

## 0.4.0

### New
- Added option to filter textures in the texture editor by name and size;
- `dynamic_linking` feature for faster compile times;
- added a warning message on the first application boot.

### Changes
- Slight changes to the `TextureSettings` interface;
- lowered memory size of `TextureSettings`;
- texture scrolling only applies to textures now, not sprites;
- improved files formats, old files will be automatically converted;
- disabled VSync;
- changed manual window key to `` Ctrl+` ``;
- now the window opens maximized;
- slightly reworked the order the entities are drawn to make the grid lines more visible.

### Fixes
- Fixed properties window bounds;
- fixed camera off center on load;
- fixed blinking tooltip labels;
- fixed free draw edits counting for unsaved map changes;
- fixed glitchy rendering of things and sprites (glitchy textures mapped to polygons still unsolved);
- fixed animation editor crashes;
- fixed `arena_alloc` feature compilation errors;
- centered texture loading progress bar on boot;
- fixed an issue with entities selection where if selected and non selected entities overlapped it would not be possible to select all of them by pressing tab;
- fixed incorrect window name if map file loading generated an error.

## 0.3.6

### Changes
Exposed `BrushCompat` struct for HillVacuum 0.4 file reading compatibility.

### Fixes
- Fixed file loading failing when animations are stored in it;
- fixed `Exporter` struct panicking when exporting a saved file.

## 0.3.5

### Fixes
- fixed `ui` feature compilation errors;
- fixed broken texture rendering;
- exposed `Atlas` struct `Timing`.

## 0.3.4

### Changes
- Removed `debug` feature;
- added `ui` default feature.

## 0.3.2 - 0.3.3

### Fixes
Fixed build script crashing the docs.rs documentation generation.

## 0.3.1

### Changes
- upgraded `bevy` to 0.14;
- updated documentation;
- removed now useless code.

### Fixes
Fixed controls binds not being set to defaults on load if there is no `hill_vacuum.ini` file in the folder.

## 0.3.0

### New
Isometric grid.

### Changes
- improved documentation;
- improved editor windows closing.

### Fixes
- fixed typos in the documentation ([cloudcalvin](https://github.com/cloudcalvin));
- fixed crash when using shatter tool (reported and diagnosed by [cloudcalvin](https://github.com/cloudcalvin));
- fixed crash when despawning Things;
- fixed crash when undoing/redoing a free draw point.

## 0.2.8
Improved public documentation.

## 0.2.6 - 0.2.7
crates.io publishing.

## 0.2.5

### Fixes
Fixed entity tool name in sidebar.
Added missing manual documentation.

## 0.2.4

### New
Added the possibility to edit the animation of the textures independently of the selected brushes (left click with the mouse on the texture in the preview gallery).
Added things snap.

### Changes
Updated the manual.

### Misc
Added some private code documentation.

## 0.2.3

### Fixes
Fixed an arena_alloc compilation issue.

## 0.2.2

### New
Added color customization.

### Changes
- Grouped controls, colors, and exporter settings into a single Settings window;
- changed key to close in-editor windows from Esc to F4 for better integration with egui;
- added button to reset keyboard binds to default.

### Fixes
- Fixed an error in the file loading routine possibly leading to failed file loading;
- fixed prop screenshots not being properly created after texture reload;
- fixed crash after collapsing the properties window.

### Misc
Updated README.

## 0.2.0

### New
Added customizable properties for both `Brush` and `Thing` entities. See `brush_properties` and `thing_properties` macros.

### Changes
- Updated crates;
- slight `.hv` file format changes;
- many optimizations.

### Fixes
Fixed numerous crashes and bugs.

## 0.1.0
Initial release
