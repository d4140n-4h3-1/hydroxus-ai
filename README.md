# hydroxus-ai

AI for games on [Hydroxus](https://github.com/d4140n-4h3-1/Hydroxus), the Vulkan fork
of [Fyrox](https://github.com/FyroxEngine/Fyrox): NPCs that get about a level, see what is in
front of them, hear what carries to them, and hunt the player through Metal Gear's alert phases. It is
what the droids, drones and riders of [Maze](https://github.com/d4140n-4h3-1/MazeGame) think with.

It knows nothing of the engine but its vectors (nalgebra's, which the engine re-exports). The game
casts the rays that say whether anything is in the way, moves the nodes and plays the animations;
this decides what an NPC makes of what is round it.

It is in five folders, and its alert phases besides.

### `perception` - what an NPC takes in

| Module | What it does |
|--------|--------------|
| `sight` | What an NPC can see: clearly in a cone in front of it, and near out of the corner of its eye, but nothing behind it, whatever it is doing - so it can be crept up on from behind, or slipped round - less far the lower the player is unless it is hunting them, and less far in the dark; and how visible someone is to it, from 0 to 1. |
| `detection` | Becoming aware of someone gradually, as Fallout's sneaking has it: awareness filling while they are in view, the quicker the more visible they are and the keener it is looking, draining a while after, and growing with every noise it hears, the more the nearer - `[HIDDEN]`, `[CAUTION]`, `[DANGER]` - with events as it notices, spots and loses them, and the most aware of many for a meter. |
| `hearing` | How far a noise carries: along the floor of the navmesh, round the corners, not through the walls; and how near it was to whoever hears it. |
| `bolts` | A bolt flying by: noticed going past in front of it, or felt close by wherever it goes. |
| `witness` | Whether one that saw a shooting knows who did it: only if it saw them near enough. |

### `tactics` - what it does about it

| Module | What it does |
|--------|--------------|
| `search` | Searching for someone lost from sight, together, over the navmesh: where they could be by now, spreading from polygon to polygon no faster than they could go, cleared wherever a searcher looks; and where each searcher should look next, clear of where the others are going. In a simulated maze, three searchers find a player who has run off and hidden within 12 s 88% of the time with it, against 83% wandering - wandering being much better than it was on the grid, its routes running straight down the corridors. |
| `investigate` | Having noticed something, not sure enough to go after anyone: looking that way a moment, going over to where it was - to where it was glimpsed last, if again - looking about there, and going back to what it was doing; giving up on somewhere it cannot get to. |
| `join` | A sentry that sees another after the player joins in, calm or not, as it would see the player standing - so a chase draws in every one that catches sight of it. |
| `pace` | Sprinting now and then in a chase, never as it starts; and, for one with breath to keep, easing off to a jog before it runs out, and keeping some back for a last burst. |
| `chase` | Cutting off whoever is being chased, a little ahead of them, rather than running after them; and running from a pursuer. |
| `threat` | A weapon pointed at an NPC: in the middle of the view or not; how long it stands for it, warns, warns again and then does something about it; and a weapon only drawn near it - uneasy, watching whoever holds it, backing away if they come too near, at ease again once it is put away. |
| `fire` | When and how wide an armed NPC shoots: from how far, how often, facing how nearly, and a rifle's bursts. |
| `blind` | Shot at by someone it cannot see, firing blind back where the shots came from, as Fallout's NPCs do. |
| `cover` | A bystander running from a shooting to somewhere it cannot be seen from. |
| `team` | Fighting as a team: sightings shared, targets picked and kept, the ground the enemy sees kept out of, cover ranked, flanking, sidestepping, moving only under covering fire, and noticing when it is stuck. |
| `patrol` | A flying sentry - a security drone: patrolling from spot to spot or round a post, searching where the player was and the spots round it, coming on to its range on Alert and facing them ready to fire, and calm again once its search runs out; flying along its route, hovering over the floor or free through the air, never into anyone. Which it does is chosen by utility scoring, weighed its own way if a game likes. |
| `utility` | Utility scoring: each thing an NPC could do weighed against what it takes in - inputs through response curves (step, linear, logistic), multiplied and made up for their number - the best taken, with a little inertia so it does not flicker, and what each scored kept to show why. Plain data, so it can be read from a file. |

### `navigation` - getting about

| Module | What it does |
|--------|--------------|
| `grid` | What a survey of a level finds: its walkable ground as a grid of cells, with the floor's height in each, stepping only between floors a stair apart, not up a ledge; walking distances across it, for laying a level out; and a small seeded random number generator. The navmesh is made from it, and then it can go. |
| `navmesh` | The walkable floor as convex polygons made from a surveyed grid - each on one storey, level, of one area (a road, say), no more than 16 cells along a side - joined by portals where a step can be taken; the polygon under a point; routes by A* over the polygons, pulled taut through the portals and kept clear of the corners they turn round; costs to everywhere within a bound; doors, barriers and firewalls opened and shut on it, the polygons round them made afresh and nothing else, a cell shut remembering its floor to open again; and the grid's cells as it has them, for laying a level out. All that is kept of the ground once it is made. On Nexus, 120,659 cells: 7,619 polygons, 1 MB against the grid's 23, a route four times quicker (0.32 ms against 1.24). |
| `walk` | Getting about over the navmesh, in meters: routes to somewhere, trips a way off - or off roads and turf - spots near a point, the floor at a point's height, somewhere far off. |
| `reach` | Which polygons can be got to at all, a bit each, so that no one sets off for where there is no getting to. |
| `route` | Picking a number between bounds; how a route is kept, its next point last. |
| `wander` | Going about one's business in a city: off the roads but to cross them, and off the turf a gang keeps to. |
| `trip` | Getting about over more than one floor: a point put on the floor nearest it at its own height, and ferries - platforms that carry whoever stands on them between two places - taken where that is quicker than walking: to the dock, aboard while it waits, across, and off. |
| `ride` | Taking a train: up to the platform, waiting by a doorway no one else is, on only with time to, and off at the station it is going to. |
| `steer` | Making way in a corridor: veering right round someone ahead, stopping only for someone right in front, stepping aside for someone coming. |
| `drive` | Riding a vehicle round a course of checkpoints - flat out on the straights, easing into the turns, over the speed pads - pulling out round one ahead; and in traffic, stopping short of whoever is in the way, and for an amber light if it can. |
| `traffic` | A city's streets as junctions and lanes, which can be driven as the game's rays find them, stop lights where two main streets meet and their cycles; bikes keeping right, planning their turns, stopping on red and for anyone in the road. |
| `flight` | Flying free: a flier's way through the air - straight there, up and over what is in the way, or along the floor under a roof, its corners cut. |

### `social` - NPCs among each other

| Module | What it does |
|--------|--------------|
| `group` | Walking about together: a pair one beside the other, a group fanned out behind the one leading it; who is in which, who speaks for it, who leads it on when the one leading it is gone, and waiting for one kept talking or fallen behind. |
| `talk` | Who the player can talk to - close by and in front of them, nearest first; two going about by themselves that meet and stop to talk; a group talking among itself near the player; and how far off what they call out is heard. |
| `report` | A witness going to tell the nearest sentry of what it saw, or hiding with none about till it gives up; and who hears an alarm. |
| `opinion` | What an NPC thinks of the player, from -1 to 1: moved by deeds it sees - shooting, robbing, threatening, bribing, talking it round - easing back with time; friendly, neutral, wary or hostile; and how far that sways a talk check. |
| `rumour` | What each NPC knows of the player (`Minds`): deeds seen at full strength, told on weaker each time and minded less, fading and forgotten; one turning up afresh has heard what the town talks of. |
| `chat` | What NPCs talk about among themselves: news one has for another, told on as they talk; or, feeling strongly, what one thinks of the player, swaying the rest; or nothing much. |
| `crowd` | A city's crowd: only so many NPCs, all about the player, one gone far off turning up again nearer by, out of sight, on the navmesh's pavement. |

### `think` - learning and adapting

| Module | What it does |
|--------|--------------|
| `lesson` | What has worked: each thing an NPC could do weighed up or down by how it turned out the times it did it - a run of outcomes moving it, one fluke not - weighing a utility reasoner's choice, never opening what its considerations shut nor barring what failed; fading back over minutes, and told on to others at half strength, so a squad comes to think alike. |
| `habit` | The player's habits: how often, lately, they do each of the things a game tells apart - the way they come in, the trick they use - and how much an NPC expects each next, for how much it has seen; fading, so a player who changes their ways is met afresh, and told on. |
| `haunt` | Where the player keeps turning up: where they were seen and found hiding, by navmesh cell, fading and told on; a search map leaned on them looks there first - only where they could have got to since - and a patrol can go by the likeliest. |
| `experience` | What one NPC has been through: a game says how something it tried turned out - a search, a look, a chase, keeping safe, working with another - and it goes to what it did lately that the outcome is down to (each by how lately), to where the player hides and to their habits; its choice weighed by it, and a verdict when it comes to think something works or is no use. |

### Besides

| Module | What it does |
|--------|--------------|
| `alert` | Metal Gear's phases - Alert, Evasion, Caution - and how one leads to the next; and wariness - what an NPC has been through, from a glimpse to a body found, drawing out how long it searches and stays wary, and how keenly it looks, fading over minutes. What an NPC perceives and what it does both turn on it. |

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

## Tuning

Every number the crate's behaviour turns on - how far an NPC sees, how long it searches, how
near it comes before it shoots, how wide a lane is, how long a light stays green - is a field of
its module's `Tuning`, whose `DEFAULT` is the module's documented constants (Maze's). The
crate-wide `hydroxus_ai::Tuning` holds every module's, so a game keeps one and hands each module
its part:

```rust
let mut ai = hydroxus_ai::Tuning::DEFAULT;
ai.patrol.engage = 15.0; // drones hang back further
ai.alert.caution = 90.0; // and stay wary longer
let to = hydroxus_ai::tactics::chase::cut_off(&ai.chase, them, going, away, nearest, floor_at);
```

Those that keep their numbers with them take them as they are made: a `WalkGrid` its ground
(`with_tuning`), a `NavMesh` how it is made and walked (`build_with`, `walk`), a traffic
`Network` its layout, lights and driving (`from_roads_with`), a `Patrol`, a `SearchMap`, a
`Crowd`, a report `Witness`. With the `serde` feature the whole of it can be read from a file;
anything the file leaves out takes its default:

```toml
hydroxus-ai = { git = "https://github.com/d4140n-4h3-1/hydroxus-ai.git", branch = "main", features = ["serde"] }
```

```json
{ "patrol": { "engage": 15.0 }, "alert": { "caution": 90.0 } }
```

## Events

The crate does not choose what an NPC says. What came of what it did is an event - a sentry's
`tactics::patrol::Event` (`Spotted`, `Searching`, `Alarmed`, `Calm`), a witness's
`social::report::Telling`, a ride's `navigation::ride::Rides`, a threat's `tactics::threat::Threat`
- and the game maps it to its own lines, sounds or UI.

## Rust version and checks

Rust 1.89 or newer (nalgebra 0.35's own minimum; `rust-version` in `Cargo.toml`). CI
(`.github/workflows/ci.yml`, once pushed) builds and tests on stable and 1.89, with and without the `serde`
feature, and fails on any clippy or rustdoc warning. Locally:

```sh
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
```

Where the crate is headed, and how far each part has got, is in [ROADMAP.md](ROADMAP.md).

## License

MIT - see `LICENSE`.
