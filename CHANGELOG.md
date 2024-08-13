# Changelog

## 0.6.1

### Changes
- Further decreased `.hv` file size;
- removed some unused code;
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
- slight .hv file format changes;
- many optimizations.

### Fixes
Fixed numerous crashes and bugs.

## 0.1.0
Initial release