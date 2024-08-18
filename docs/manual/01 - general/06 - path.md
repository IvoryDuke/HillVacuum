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
