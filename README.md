# hydroxus-ai

AI for games on [Hydroxus](https://github.com/d4140n-4h3-1/Hydroxus), the Vulkan fork
of [Fyrox](https://github.com/FyroxEngine/Fyrox): NPCs that get about a level, see what is in
front of them, hear what carries to them, and hunt the player through Metal Gear's alert phases. It is
what the droids, drones and riders of [Maze](https://github.com/d4140n-4h3-1/MazeGame) think with.

It knows nothing of the engine but its vectors (nalgebra's, which the engine re-exports). The game
casts the rays that say whether anything is in the way, moves the nodes and plays the animations;
this decides what an NPC makes of what is round it.

It is in four folders, and its alert phases besides.

### `perception` - what an NPC takes in

| Module | What it does |
|--------|--------------|
| `sight` | What an NPC can see: clearly in a cone in front of it, and near out of the corner of its eye, but nothing behind it, whatever it is doing - so it can be crept up on from behind, or slipped round - less far the lower the player is unless it is hunting them, and less far in the dark. |
| `hearing` | How far a noise carries: along the corridors, not through the walls. |
| `bolts` | A bolt flying by: noticed going past in front of it, or felt close by wherever it goes. |
| `witness` | Whether one that saw a shooting knows who did it: only if it saw them near enough. |

### `tactics` - what it does about it

| Module | What it does |
|--------|--------------|
| `search` | Searching for someone lost from sight, together: where they could be by now, spreading along the ground as fast as they could go, cleared wherever a searcher looks; and where each searcher should look next, clear of where the others are going. In a simulated maze, three searchers find a player who has run off and hidden 98% of the time with it, against 83% wandering. |
| `join` | A sentry that sees another after the player joins in, calm or not, as it would see the player standing - so a chase draws in every one that catches sight of it. |
| `pace` | Sprinting now and then in a chase, never as it starts; and, for one with breath to keep, easing off to a jog before it runs out, and keeping some back for a last burst. |
| `chase` | Cutting off whoever is being chased, a little ahead of them, rather than running after them; and running from a pursuer. |
| `threat` | A weapon pointed at an NPC: in the middle of the view or not; how long it stands for it, warns, warns again and then does something about it. |
| `fire` | When and how wide an armed NPC shoots: from how far, how often, facing how nearly, and a rifle's bursts. |
| `blind` | Shot at by someone it cannot see, firing blind back where the shots came from, as Fallout's NPCs do. |
| `cover` | A bystander running from a shooting to somewhere it cannot be seen from. |
| `team` | Fighting as a team: sightings shared, targets picked and kept, the ground the enemy sees kept out of, cover ranked, flanking, sidestepping, moving only under covering fire, and noticing when it is stuck. |

### `navigation` - getting about

| Module | What it does |
|--------|--------------|
| `grid` | A level's walkable ground as a grid of cells, with the floor's height in each, stepping only between floors a stair apart, not up a ledge; the cheapest ways across it, keeping to the middle of the corridors - or, weighted, out of harm's way; whether one cell can be seen from another along the ground; and a small seeded random number generator. |
| `reach` | Which cells can be got to at all, a bit each, so that no one sets off for where there is no getting to. |
| `route` | Routes over the grid, as points on the floor: to somewhere, within a reach, or a trip away; routes that keep out of ground made dear, such as ground an enemy can see; trips to somewhere off ground kept off, such as a city's roads; and a spot on the floor near a point. And, on a city-sized grid, a spot well away from somewhere picked at random rather than from the walk to everywhere. |
| `wander` | Going about one's business in a city: off the roads but to cross them, and off the turf a gang keeps to. |
| `trip` | Getting about over more than one floor: a point put on the floor nearest it at its own height, and ferries - platforms that carry whoever stands on them between two places - taken where that is quicker than walking: to the dock, aboard while it waits, across, and off. |
| `ride` | Taking a train: up to the platform, waiting by a doorway no one else is, on only with time to, and off at the station it is going to. |
| `steer` | Making way in a corridor: veering right round someone ahead, stopping only for someone right in front, stepping aside for someone coming. |
| `drive` | Riding a vehicle round a course of checkpoints - flat out on the straights, easing into the turns, over the speed pads - pulling out round one ahead; and in traffic, stopping short of whoever is in the way, and for an amber light if it can. |

### `social` - NPCs among each other

| Module | What it does |
|--------|--------------|
| `group` | Walking about together: a pair one beside the other, a group fanned out behind the one leading it; who is in which, who speaks for it, who leads it on when the one leading it is gone, and waiting for one kept talking or fallen behind. |
| `talk` | Who the player can talk to - close by and in front of them, nearest first; two going about by themselves that meet and stop to talk; a group talking among itself near the player; and how far off what they call out is heard. |
| `report` | A witness going to tell the nearest sentry of what it saw, or hiding with none about till it gives up; and who hears an alarm. |
| `opinion` | What an NPC thinks of the player, from -1 to 1: moved by deeds it sees - shooting, robbing, threatening, bribing, talking it round - easing back with time; friendly, neutral, wary or hostile; and how far that sways a talk check. |
| `rumour` | What each NPC knows of the player (`Minds`): deeds seen at full strength, told on weaker each time and minded less, fading and forgotten; one turning up afresh has heard what the town talks of. |
| `chat` | What NPCs talk about among themselves: news one has for another, told on as they talk; or, feeling strongly, what one thinks of the player, swaying the rest; or nothing much. |
| `crowd` | A city's crowd: only so many NPCs, all about the player, one gone far off turning up again nearer by, out of sight. |

### Besides

| Module | What it does |
|--------|--------------|
| `alert` | Metal Gear's phases - Alert, Evasion, Caution - and how one leads to the next. What an NPC perceives and what it does both turn on it. |

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
