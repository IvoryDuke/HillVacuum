### Properties
Properties are custom user defined values which can be associated to brushes and things.  
Such values can be inserted through the `brush_properties` and `thing_properties` macros by specifying the pairs `(name, default_value)` of the properties.  
Properties can be edited per-entity using the properties window.  
Currently supported value types are `bool`, `u8`, `u16`, `u32`, `u64`, `u128`, `i8`, `i16`, `i32`, `i64`, `i128`, `f32`, `f64`, and `String`.  
  
!!! If a saved map contains properties that differ in type and/or name from the ones defined in the aforementioned resources, a warning window will appear on screen when trying to load the `.hv` file, asking whether you'd like to use the app or map ones.
