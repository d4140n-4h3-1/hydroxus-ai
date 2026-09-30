# hydroxus-ai

Stealth AI for games on [Hydroxus](https://github.com/d4140n-4h3-1/Hydroxus), the Vulkan fork
of [Fyrox](https://github.com/FyroxEngine/Fyrox): NPCs that walk a level, see what is in front
of them, hear what carries to them, and hunt the player through Metal Gear's alert phases. It is
what the droids and drones of [Maze](https://github.com/d4140n-4h3-1/MazeGame) think with.

It knows nothing of the engine but its vectors (nalgebra's, which the engine re-exports). The game
casts the rays that say whether anything is in the way, moves the nodes and plays the animations;
this decides what an NPC makes of what is round it.

| Module    | What it does |
|-----------|--------------|
| `grid`    | A level's walkable ground as a grid of cells, with the floor's height in each; the cheapest ways across it, keeping to the middle of the corridors - or, weighted, out of harm's way; whether one cell can be seen from another along the ground; and a small seeded random number generator. |
| `route`   | Routes over the grid, as points on the floor: to somewhere, within a reach, or a trip away; and routes that keep out of ground made dear, such as ground an enemy can see. |
| `sight`   | What an NPC can see: only in a cone in front of it, whatever it is doing - so it can be crept up on from behind, or slipped round - less far the lower the player is unless it is hunting them, and less far in the dark. |
| `hearing` | How far a noise carries: along the corridors, not through the walls. |
| `alert`   | Metal Gear's phases - Alert, Evasion, Caution - and how one leads to the next. |
| `steer`   | Making way in a corridor: veering right round someone ahead, stopping only for someone right in front, stepping aside for someone coming. |
| `search`  | Searching for someone lost from sight, together: where they could be by now, spreading along the ground as fast as they could go, cleared wherever a searcher looks; and where each searcher should look next, clear of where the others are going. In a simulated maze, three searchers find a player who has run off and hidden 98% of the time with it, against 83% wandering. |

```toml
[dependencies]
hydroxus-ai = { git = "https://github.com/d4140n-4h3-1/hydroxus-ai.git", branch = "main" }
```

```rust
use hydroxus_ai::prelude::*;

// A guard facing +z at the origin, searching; the player crouched 10 m behind it.
let sight = Sight::default();
let behind = nalgebra::Vector3::new(0.0, 0.0, -10.0);
assert!(!sight.could_see(Some(Alert::Evasion), nalgebra::Vector3::zeros(), 0.0, behind, Stance::Crouching, false));
```

## License

MIT - see `LICENSE`.
