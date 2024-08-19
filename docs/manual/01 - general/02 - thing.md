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
