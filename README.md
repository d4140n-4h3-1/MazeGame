# Maze

A small maze game in Rust, built on the [Fyrox](https://github.com/FyroxEngine/Fyrox)
engine and rendered with Vulkan.

Every round is a new maze, put together at random from four tile models: a straight pipe, a
corner, a T and a crossroads. You start at one end of the longest route through it, and a
glowing exit waits at the other end. The clock runs until you reach it.

- Random mazes of any size, with loops, built from tiles whose shapes are measured from the models
  themselves.
- A droid to play as, seen from behind over its shoulder, that walks, runs, sprints and crouches
  with you, as fast as its feet carry it. V switches to seeing through its eyes; holding the
  middle mouse button swings the camera round it.
- Movement with walking, running and a breath-limited sprint, crouching, crawling,
  jumping, leaning round corners and looking behind.
- Ray-traced shadows from every lamp, refractive glass, floor reflections and ambient occlusion.
- Only what can be seen from where you stand is drawn and lit, so big mazes stay fast.
- A pause menu, with a switch that turns every light in the maze off and leaves you with your
  flashlight.

## Requirements

- **A graphics card with Vulkan.** The game checks at startup that it is rendering with Vulkan,
  and exits if it is not. Ray tracing is used for shadows when the card supports it; without it
  the game falls back to shadow maps.
- **Rust 1.94 or newer**, the version the engine requires.
- **The engine and its effects, checked out next to this project.** Both are dependencies by
  local path, so the three have to sit side by side:

  ```sh
  mkdir game_dev && cd game_dev
  git clone -b vulkan https://github.com/d4140n-4h3-1/Fyrox.git
  git clone https://github.com/d4140n-4h3-1/fyrox-gfx.git
  git clone https://github.com/d4140n-4h3-1/MazeGame.git
  ```

  ```
  game_dev/
  ├── Fyrox/       the engine, on its `vulkan` branch
  ├── fyrox-gfx/   the glass, shadow, reflection and other graphics effects
  └── MazeGame/    this project
  ```

  The engine is modified, so upstream Fyrox will not do: the
  [`vulkan` branch of this fork](https://github.com/d4140n-4h3-1/Fyrox/tree/vulkan) makes the
  wgpu backend render like the OpenGL one and adds the hardware ray tracing that the traced
  shadows use. `Fyrox/VULKAN.md` describes every change.
  [`fyrox-gfx`](https://github.com/d4140n-4h3-1/fyrox-gfx) holds the graphics effects the game
  adds on top of the engine.

This project is a Cargo workspace of its own. That keeps other crates from switching on the
engine's OpenGL backend, which it would otherwise pick over Vulkan.

## What's changed

### The engine, compared with upstream Fyrox

The fork's `vulkan` branch is two commits on top of upstream Fyrox as of 13 September 2026:

1. **Make the wgpu (Vulkan) backend render like the OpenGL one.** Fixes found by rendering the
   same scenes on both backends and comparing the frames:
   - clip depth is remapped to wgpu's range;
   - textures sampled from projected positions (shadows, SSAO, decals, rendered cube maps) and
     UI rendered into textures are no longer upside down;
   - light volumes and bloom line up with the G-buffer;
   - a GPU hang from stale uniforms is fixed, and integer vertex attributes, pipeline caching,
     scissors, readback padding and rendering into mip levels are corrected;
   - uniform pages are capped at 1 MB, where a 2 GB limit reported by the driver froze loading.

   The `fyrox` crate also stops pulling in the OpenGL backend by default, so choosing
   `backend_wgpu` takes effect.
2. **Add hardware ray-traced shadows and further wgpu fixes.**
   - Hardware ray tracing, where the graphics card has it, traces every light's shadows. It is off
     unless a game asks for it.
   - Point and spot lights are drawn only over the part of the screen they can reach.
   - The soft-shadow filter no longer leaves a hard edge, on both backends.
   - With FXAA off, the frame is no longer upside down.
   - glTF meshes without a material come out plain white instead of dark white metal.
   - Animations saved by older engine versions load their property paths.

`VULKAN.md` in the fork explains each change in detail.

### fyrox-gfx

Graphics effects the game adds on top of the engine, kept out of it: refractive glass, softer
shadow edges, temporal anti-aliasing, further-reaching ambient occlusion, screen-space
reflections, a budget that keeps shadow maps for the nearest lamps only, and the ray-traced
shadows (its `raytracing` feature).

### The game

22 September 2026:

- **A droid to play as.** The player is seen from behind as a droid (`data/droid_full_deform.glb`),
  with the camera over its shoulder and pulled in when a wall is in the way. V switches to first
  person, and holding the middle mouse button swings the camera round the droid.
- **Its feet set the speed.** Walking, running, sprinting and crouching each play the droid's own
  cycle, and each gait goes as fast as that cycle's stride, so the feet stay on the floor. That
  makes every gait slower than before.
- **No more head bob.** The camera is held steady; the droid's cycles show the stride.
- The droid is not yet in the traced shadows, which are gathered once and would leave its shadow
  where it started.

19 September 2026:

- **Pause menu.** Escape now opens a menu with Resume, Lights, New maze and Quit, and stops the
  clock, the player and the physics. It used to only release the mouse. A round in play also
  pauses when the window loses focus.
- **Lights switch.** From the pause menu, the lamps, the glow of their fixtures, the sun and nearly
  all the ambient light can be turned off, leaving the flashlight to see by. The setting carries
  over to each new maze.
- **The end-of-round banner** is now centred on the window.
- **The code is split into modules** - `game`, `level`, `culling`, `fixtures`, `survey`, `hud`,
  `menu`, `diagnostics` and a `player/` folder - instead of one large `main.rs` and
  `player.rs`. Nothing about how the game plays changed with it.

## Running

```sh
cargo run
```

It can be started from anywhere; it finds its models in `data/` next to `Cargo.toml`. The engine
is compiled with optimizations even in a debug build, and the game itself is not, so `cargo run`
is quick enough to play while still easy to debug. `cargo run --release` optimizes the game as
well.

## Controls

| Key              | Action                                                        |
| ---------------- | ------------------------------------------------------------- |
| W A S D, arrows  | Move                                                          |
| Mouse            | Look around                                                   |
| Caps Lock        | Walk or run; it stays as you left it                          |
| Shift (hold)     | Sprint. Costs breath, and running out leaves you walking      |
| Space            | Jump                                                          |
| C                | Crouch, or stand back up                                      |
| Z                | Crawl, or stand back up                                       |
| Ctrl (hold)      | Lean round the corner ahead                                   |
| Q (hold)         | Look behind you while still moving forward                    |
| F                | Flashlight on or off                                          |
| V                | Third person (behind the droid) or first person               |
| Middle mouse (hold) | Swing the camera round the droid to see it from any side    |
| R                | New maze                                                      |
| `[` `]`          | Turn slower or faster                                         |
| `-` `=`          | Narrower or wider view                                        |
| Escape           | Pause                                                         |

### Pause menu

Escape pauses the game: the clock, the player and the physics all stop. Switching to another
window in the middle of a round pauses it too. The menu has:

- **Resume**: carry on where you left off.
- **Lights**: switch the maze's lights off, or back on. Off, the lamps, the glow of their
  fixtures, the sun and nearly all the ambient light go out, leaving your flashlight and the
  exit's own glow. The setting carries over to each new maze.
- **New maze**: the same as R. **New round** when playing a fixed maze model.
- **Quit**.

## Options

Options are set with environment variables, for example `MAZE_SIZE=10x10 cargo run`.

| Variable                 | Effect                                                                  |
| ------------------------ | ----------------------------------------------------------------------- |
| `MAZE_SIZE=<w>x<d>`      | How many junctions wide and deep the maze is. The default is `20x20`.   |
| `MAZE_SEED=<n>`          | Makes every maze and round the same, for comparing two runs.            |
| `MAZE_MODEL=<path>`      | Plays a fixed maze model (`.glb`, `.gltf` or `.fbx`) instead of random mazes. |
| `MAZE_DEBUG=1`           | Logs the walkable map of each level, and rendering statistics once a second. |
| `MAZE_VSYNC=0`           | Uncaps the frame rate, for measuring what a frame costs.                |
| `MAZE_RT=0`              | Shadow maps instead of ray-traced shadows.                              |
| `MAZE_HARD_SHADOWS=1`    | Ray-traced shadows with sharp edges instead of soft ones.               |
| `MAZE_SHADOW_BUDGET=0`   | With shadow maps, gives every lamp in range one, not just the nearest.  |
| `MAZE_SSAO=0`            | Turns ambient occlusion off.                                            |
| `MAZE_REFLECTIONS=0`     | Turns floor reflections off.                                            |

### Fixed maze models

A model given with `MAZE_MODEL` needs no special structure, but the game reads a few things from
it:

- Surfaces colored **pure magenta** (255, 0, 255) are the glass of light fixtures. They become
  refractive glass, and each fixture gets a lamp.
- Small meshes standing on their own, narrower than 3.5 m, are taken as **something to find**:
  the round ends there instead of at a random exit.
- FBX models are taken to be in centimeters and scaled down.

Anything in the model that glows by itself goes dark with the lights.

## Tests

```sh
cargo test
```

The tests cover maze generation and tile fitting, what can be seen from where, the walkable grid
and round planning, and the player's movement, breath, head motion, leaning and keys.

## How it fits together

| Module          | What it does                                                               |
| --------------- | -------------------------------------------------------------------------- |
| `main.rs`       | Starts the engine and sets up the graphics effects.                        |
| `game.rs`       | The game: loading levels, rounds, input, the pause menu, the lights.       |
| `level.rs`      | A level in the scene: its pieces, collider, lamps and walkable ground.     |
| `generate.rs`   | Plans random mazes on a grid of junctions and turns them into tiles.       |
| `tiles.rs`      | Measures the tile models and assembles a maze from them.                   |
| `layout.rs`     | The walkable grid, and where a round starts and ends.                      |
| `survey.rs`     | Finds the walkable ground of a level by casting rays into it.              |
| `culling.rs`    | Hides the pieces and lamps that cannot be seen from where the player is.   |
| `fixtures.rs`   | Light fixtures: the glass, the lamps, and everything that glows.           |
| `inward.rs`     | Makes the tiles' surfaces visible from inside and out.                     |
| `hud.rs`        | The status line and the banner.                                            |
| `menu.rs`       | The pause menu.                                                            |
| `diagnostics.rs`| The Vulkan check and the rendering statistics.                             |
| `player/`       | The player, one file per part: posture, movement, breath, head, lean, input, view, the droid (`avatar`) and the camera behind it (`third_person`). |

The tile models and the droid (`droid_full_deform.glb`) are in `data/`.

## License

MIT.
