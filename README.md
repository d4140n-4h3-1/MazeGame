# Ruptura Systematis

A small maze game in Rust, built on the [Fyrox](https://github.com/FyroxEngine/Fyrox)
engine and rendered with Vulkan.

Every round is a new maze, put together at random from four tile models: a straight pipe, a
corner, a T and a crossroads. You start at one end of the longest route through it, and a
glowing exit waits at the other end. The clock runs until you reach it.

[![Talking to one of the droids in the maze (video)](media/talking_to_a_droid.jpg)](media/talking_to_a_droid.mp4)

*Talking to one of the droids. Click the picture to watch the video.*

**[Play it in the browser](https://d4140n-4h3-1.github.io/MazeGame-web/)** - no install, a desktop
browser with WebGL 2 is all it needs.

- Random mazes of any size, with loops, built from tiles whose shapes are measured from the models
  themselves.
- A droid to play as, seen from behind over its shoulder, that walks, runs, sprints and crouches
  with you, as fast as its feet carry it. V switches to seeing through its eyes; holding the
  middle mouse button swings the camera round it.
- Other droids living in the maze, wandering its corridors on their own, that can be talked to
  as in Fallout 3. They speak System Latin. Fail to fool a sentry and it hunts you down, Metal
  Gear style: break its line of sight and it searches for you; other sentries that see the
  chase join in; the pistol can stop it.
- Movement with walking, jogging, and a run and a sprint that cost stamina, crouching, crawling,
  jumping, taking cover and leaning round corners, and looking behind.
- Ray-traced shadows from every lamp, refractive glass, floor reflections and ambient occlusion.
- Only what can be seen from where you stand is drawn and lit, so big mazes stay fast.
- A pause menu, with a switch that turns every light in the maze off and leaves you with your
  flashlight.

## Requirements

- **A graphics card with Vulkan.** The game checks at startup that it is rendering with Vulkan,
  and exits if it is not. Ray tracing is used for shadows when the card supports it; without it
  the game falls back to shadow maps.
- **Rust 1.94 or newer**, the version the engine requires.
- **`git`, and a network connection for the first build.** Cargo downloads the engine and its
  effects by itself, straight from GitHub, so all there is to get is this project:

  ```sh
  git clone https://github.com/d4140n-4h3-1/MazeGame.git
  cd MazeGame
  cargo run
  ```

  The engine, [Hydroxus](https://github.com/d4140n-4h3-1/Hydroxus), is about 400 MB. `.cargo/config.toml` has Cargo fetch it with the system `git`
  rather than its own library, which can resume an interrupted download instead of starting it
  over. `Cargo.lock` pins the commits used; `cargo update` moves to the latest of each branch.

  The engine is modified, so upstream Fyrox will not do: Hydroxus is a fork of Fyrox whose
  [`vulkan` branch](https://github.com/d4140n-4h3-1/Hydroxus/tree/vulkan) makes the wgpu backend
  render like the OpenGL one and adds the hardware ray tracing that the traced shadows use. Its
  crates keep Fyrox's names, so the game still says `fyrox`. `VULKAN.md` in Hydroxus describes
  every change.
  [`fyrox-gfx`](https://github.com/d4140n-4h3-1/fyrox-gfx) holds the graphics effects the game
  adds on top of the engine.

This project is a Cargo workspace of its own. That keeps other crates from switching on the
engine's OpenGL backend, which it would otherwise pick over Vulkan.

## What's changed

### The engine, Hydroxus, compared with upstream Fyrox

Hydroxus's `vulkan` branch began as two commits on top of upstream Fyrox as of 13 September 2026:

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

`VULKAN.md` in Hydroxus explains each change in detail.

### fyrox-gfx

Graphics effects the game adds on top of the engine, kept out of it: refractive glass, softer
shadow edges, temporal anti-aliasing, further-reaching ambient occlusion, screen-space
reflections, a budget that keeps shadow maps for the nearest lamps only, and the ray-traced
shadows (its `raytracing` feature).

### hydroxus-ai

The droids' and drones' AI, in a crate of its own
([hydroxus-ai](https://github.com/d4140n-4h3-1/hydroxus-ai)) that knows nothing of the engine
but its vectors: the walk grid and the routes over it, what an NPC sees and hears, Metal Gear's
alert phases, and making way in a corridor. The game casts the rays, moves the droids and plays
their animations; the crate decides what they make of what is round them.

### The game

1 October 2026:

- **A choice of maps for capture the flag.** Capture the Flag in the main menu opens a page of
  maps: Lanes, the first, and Balconies, `data/arena/ctf_balconies.glb`, on two floors - each
  flag in an open well under a balcony round three sides of it, catwalks along the side walls
  joining the balconies, a raised hub in the middle, and stairs between them all.
  `data/arena/ctf_balconies.py` builds it in Blender, its cover laid out at random from a seed and
  mirrored end to end.
- **The menus work from the keyboard**: the arrow keys or W and S go up and down the buttons, the
  one picked shown in light blue, and Enter or Space presses it - in the main menu, its maps, the
  pause menu and the options.
- **Stairs.** The droids find their way up and down stairs: the walk grid's floor in each cell is
  now the highest with headroom over it, and only what can be climbed to from the ground a
  stair's step at a time (hydroxus-ai's `MAX_CLIMB`, 0.55 m) is walkable - so stairs and the
  floors they lead to are, and the tops of crates and railings no longer are. A droid on the
  stairs puts each foot down on its own step: the hips go down to the lower foot and each leg
  bends at the knee to reach its step, keeping the walk's lift and the foot's angle. The player
  walks up steps of up to 0.4 m too, the eyes coming up after the body over a moment. The team
  battle climbs as well.

30 September 2026:

- **Your allies say something when you talk to them** in capture the flag: E by one of blue's
  droids has it say one of a handful of lines, in its own voice, without a conversation. As it
  speaks it turns its head to you, as far as 90 degrees either way, without turning round. Red's
  are not talked to.
- **You play for blue in capture the flag**, and take red's flag. Your allies are blue's, in the
  cyan droid, and the guards against you red's, in the red one; the voices stay with the parts,
  your allies' high and quick, the guards' deep, slow and rasping.
- **Capture the flag is taken home.** With red's firewall down, reaching their flag takes it:
  it rides on your droid's back, shrunk and slung across it at 45 degrees, and the marker moves
  to your own flag. Bringing it home to blue's wins the round.
- **A main menu** to pick the maze or capture the flag, which the pause menu can go back to.
- **A capture-the-flag map**, `data/arena/ctf_map.glb`: a red base at one end and a cyan base at
  the other, each with its flag, joined by three lanes. The team battle can be fought in it
  with `BATTLE_MAP`.
- **Sides in capture the flag.** Each side has three droids and a drone. Red's are yours: they
  pay you no heed and go after blue's. Blue's guard their end, watching for you all the while,
  and go after you or red's, whichever they see. Each droid keeps to a post of its own in its
  side's half - inside its base, in the yard before the door, and forward in the middle lane -
  and each drone patrols round its flag. Droids
  shoot with their pistols and drones fire as ever, and each side's shots harm only the other
  side, yours included. Your own bolts pass red's by.
- **Capture the flag's droids say their own things**, from `data/dialogue/ctf.json`: red's
  Protegators and blue's Obstruators call out as they fight, and are not talked to. Each side
  has a voice of its own, red's high and quick, blue's deep, slow and rasping, to tell them
  apart by ear.
- **The firewall's shell no longer turns solid up close.** It does not bend what is seen through
  it, as glass does, and glows less, so what is inside it shows through from any distance.
- **Flags in firewalls.** Each flag stands in a firewall, a flickering shell of orange glass that
  nothing gets through, until the computer it answers to, in the same base, is hacked. Take
  blue's flag to win the round. The flags and the firewall are the models made in Blender,
  exported by `data/ctf/export_models.py`.
- **The droids see only in front of them, always.** On ALERT too, and right next to them too: a
  droid or a drone can be crept up on from behind, or slipped round while it looks the other
  way. A hunting droid that loses you round its back has to search for you.
- **The AI is a crate of its own**, `hydroxus-ai` (see above), fetched from GitHub as Hydroxus
  and fyrox-gfx are. The droids behave as before, but for what they see.

- **The engine fork is now Hydroxus.** The fork of Fyrox the game is built on has a name, and a
  README of its own crediting Fyrox. Its crates keep their Fyrox names, so nothing in the game
  changes but where Cargo fetches it from.
- **Upstream Fyrox merged into Hydroxus**, up to 27 September: Dmitry Stepanov's markdown and
  inline text elements in the UI, and fixes to the editor and inspector. Nothing in the game's
  rendering changes.
- **The team battle example shows the battle.** It started its camera above the arena's roof,
  so all it showed was the ceiling; now it starts inside, looking across the arena from one long
  side. Its scoreboard is at the top in the middle and its help along the bottom, instead of on
  top of each other.
- **The team battle is fought with the game's own droids.** Each side is one of the game's
  droids - red in `droid_hostile`, cyan in `droid_full_deform` - animated as the maze's droids
  are: jogging as they advance, running for cover, crouching behind it, pistol raised at a target,
  eyes in their team's colour and flashing white when hit. They go at the pace of their own
  animations, so their feet keep to the floor, which makes a match slower than it was. For this,
  the game is a library as well as a program (`src/lib.rs`), which the examples can use.
- **Every one of the droid's animations in the team battle.** Walking with no enemy known of,
  jogging to one, running for cover and sprinting when falling back hurt, with the skids that
  come with stopping and turning; strafing and crouch-strafing while facing a target; a short or
  high jump now and then as a droid breaks for cover; the pistol drawn, raised, aimed up, down
  and to the side at the target, fired, and holstered once the fighting is over. Shot down, a
  droid falls limp, knocked back by the shot, and may lose what it was hit in, as in the game;
  it comes back as a new droid. Fallen droids and what they lost do not stop shots.
- **The team battle's droids fire the game's green bolts**, as the player's pistol does, in
  place of red and cyan beams: each leaves the muzzle as the pistol fires, flies at the bolt's
  speed, lights up what it passes, and glows where it lands - on the droid it hits, whose harm it
  does when it gets there, or on the cover or wall it hits instead. The pistol's bolts now fly by
  themselves (`Bolts::fly`), for anyone to fire; the player's fly as before.
- **The team battle's bolts leave straight out of the pistol**, the way its barrel points, as
  the player's do in third person. The droids used to face the arena's fixed forward while they
  strafed, so their pistols pointed well off to the side of whoever they shot at; each droid's
  body now turns the way it faces, as the player's does with the camera. A droid also learns how
  far its barrel ends up above or beside where it aims, and aims that much the other way.
- **The maze's droids search together, and cut you off.** Once they lose sight of you - or
  hear you, or answer an alarm - they share one picture of where you could be by now, spreading
  along the corridors as fast as you could run, and cleared wherever one of them looks and does
  not see you. Each goes to look where you are likeliest to be, clear of where the others are
  going, instead of wandering off at random (hydroxus-ai's `search`). In a simulated maze, three
  of them find a player who has run off and hidden 98% of the time this way, against 83% before.
  While several of them are after you, only the nearest runs straight at you; the others make
  for somewhere ahead of where you are going, to cut you off.
- **An alarm you can hear.** A droid sounding the alarm, and a failed hack calling a drone in,
  set off a klaxon - three rising whoops - that carries through the maze
  (`data/sounds/alarm_formants.json`).
- **The team battle's sides fight as teams.** Each droid picks its target - whoever is shooting
  at it, hurt, or in the open, before whoever is merely nearest - and takes cover from every
  enemy its side knows of, spread out from its teammates. They move out of the enemy's sight
  where they can, two of each five go round the side, they leave cover only while a teammate is
  firing, and they fire at where an enemy was just seen to keep their head down, which does. A
  droid getting nowhere finds somewhere else to go, and one caught without cover sidesteps
  rather than stand there. `BATTLE_OLD=red` (or `cyan`, `both`) has a side fight as before:
  against the old way, fighting as a team took 64% of the kills over two runs, one from each
  end of the arena.

29 September 2026:

- **Bribes.** A Speech check can now be tried with credits instead, offered under it: `[Speech 75%]
  [40.00 CR] ...`. It is still a Speech check, only likelier to succeed, and the credits are paid
  whether it works or not; trying the check or its bribe uses up both. Without the credits, the
  bribe is dimmed and cannot be picked. Sentries take a bribe to let a "maintenance unit" through
  (40% or 75% for 40 CR) and to believe you saw something behind you (30% or 65% for 25 CR), and
  scouts to share their map to the exit (50% or 85% for 20 CR). Maintenance units cannot be
  bribed - they do not know where the exit is - and drones cannot be talked to at all. Bribes are
  set per check in `data/dialogue/droids.json`.
- **A security drone patrols the maze.** It is put down well away from where you start and
  hovers along the corridors from one spot to the next, calm, its glow green, paying you no
  heed. It turns on you only when a scout or maintenance unit sounds the alarm, or when a bolt
  from your pistol hits a droid (or the drone): it says so ("Sirenatio accipatum. Zeto
  aggressor."), flies to where you were, and searches there and nearby for 30 seconds, scanning
  at each spot, its glow orange, before it calms down and patrols again. Seeing you, it goes red,
  comes on to 6 m and fires a plasma shot every second and a half while it can see you; a shot
  that hits takes a third of your health, as a sentry's touch does. Three bolts bring it down:
  its rings break into four pieces each, which fly apart, and it drops with them to the floor.
  It sees in front of it as a sentry that is not on Alert does. What it says is in
  `data/dialogue/drone.json`, by what it says it about.
- **The drone's shot** is a glowing ball with a tail and two small rings turning about it, made
  in Blender (`data/drone_shot.glb`), in the colour of the drone's mood, lighting what it passes;
  it sounds and hums as the pistol's bolts do. It flies at 12 m/s, slow enough to step out of the
  way of from a few steps off, and stops at the first thing it hits.
- **Credits.** Every computer carries credits, whatever else is on it: a random amount, most
  of them a few credits and now and then a good deal more, up to 500. Clearing its hack
  transfers them to you - the terminal says how many under ACCESS GRANTED - and your credits
  show in gold in the top right, kept from one maze to the next. Credits go as dollars and cents
  do: whole credits and hundredths of one, written `1,234.56 CR`.
- **The fallen stay down.** A droid shot down lies where it fell, broken parts and all, for the
  rest of the round, rather than disappearing a few seconds later; the others walk over it.
- **A failed hack calls in a drone.** When the trace completes, the patrolling drone nearest the
  computer comes to search there, if it is within 25 m; if not, one more flies in from out of
  sight, 20 to 40 m off by the corridors - there are four in all - and failing that, the nearest
  one patrolling or searching goes there instead, however far. Some drone always comes, and
  finds its way however far it has to go; its 30 s of searching start once it gets there.
- **Health bars over the sentries and the drone.** A sentry's red health bar sits over its
  stamina bar, shown with it - once it is after you, has spent some breath or has been hit -
  and a bolt takes a third of it. The drone has a health bar alone, while it is after you or has
  been hit.
- **Jog, run and sprint.** Caps Lock still goes between walking and jogging. A tap of Shift now
  goes between walking and running, a pace between a jog and a sprint - the sprint's stride,
  stepped out slower - which costs stamina, if far less than a sprint: 20 seconds of it on a
  full breath, to a sprint's 6. Held down, Shift sprints for as long as it is held, from any
  pace, and lets go back to it. Running is heard further off than jogging, if not as far as a
  sprint.
- **The run has a stride of its own,** `droid_running_cycle`, in place of the sprint played
  slower: a mid-foot landing, the hands half curled, the arms pumping between the jog's and the
  sprint's. Every droid has it: a sentry runs after you once it has you, sprinting now and then,
  and jogs to where it lost you. The run has its own strafes, skids and jumps too: running flat
  out, a droid now skids round, cuts across and slides to a stop as it does sprinting, if in a
  shorter slide, and strafing - or with the pistol out - a sprint slows to a run rather than a
  jog. The other droids' colours are the player's droid recoloured by
  `data/recolour_droids.py`, to be run again whenever the droid is exported afresh, so that
  they have every animation it has.
- **The alert, as in Metal Gear and Fallout,** is at the top in the middle, in its colour: ALERT
  in red, EVASION in amber and CAUTION in yellow, each counting down but ALERT, and ALERT and
  CAUTION blinking.
- **Stamina** is a bar in the bottom left corner: green, yellow once it runs low, and red,
  blinking, while you are winded.
- **The cyber pistol's ammo** shows in the bottom right corner while it is out: ∞, since it
  never runs out.
- **Hearts float in the corridors.** A few health items - three in a small maze, up to twelve in
  a big one, the first 8 to 20 m from the start - are scattered afresh each maze on open floor
  away from the start and from each other, each floating at chest height, slowly spinning and bobbing. For now they only float
  there: there is no health for them to give back yet. The model is `data/health.glb`, exported
  from `health.blend` without texture coordinates (it has none, and a constant set of them
  leaves it unlit); its see-through outer layer is made refractive glass in the game, since the
  engine draws a glTF model's surfaces solid whatever their alpha.
- **What glows lights what is round it,** with ray-traced shadows like every other light: each
  heart in red, each computer's frame in its colour (red while locked, blue once cleared), from four lamps round its rim that together light as the frame does, and leave no spot of light on the screen, the
  cyber pistol's muzzle in green while it is out, and every droid's eyes - dimly - in the
  colour they glow. The engine lights nothing from a glowing surface itself, so each has a small
  lamp of its own, as the drone does.
- **Drones speak System Latin,** as they start each of their animations, in lines from
  `data/dialogue/drone.json` - but as beeps, hums and buzzes, which no one would hear as speech:
  each vowel a beep on a note of its own, m, n, l and r a hum, the hissing letters a buzz and the
  stops a click (`data/sounds/drone_voice.json`).
- **Sentries keep their own colours** while they are after you. The hostile droid's colours,
  `data/droid_hostile.glb`, are for another kind of droid, one given `hostile_colours` in
  `data/dialogue/droids.json`; until there is one, the model is not even loaded.
- **Shadows of what moves follow it** with ray tracing too: droids, their guns and the drone
  cast their shadows from where they are, droids in the pose they are drawn in, as they do with
  shadow maps.

26 September 2026:

- **A computer to hack, as in Welcome to the Game.** A monitor and keyboard float against a wall
  a few steps from where you start, the monitor's frame glowing red while it is locked. Close to
  it and in front of it, `Computer` and `E) Hack` show under the middle of the screen; E takes
  the view in close, square on to the screen, and a terminal in green text comes up on the
  screen itself (`MAZE_TERMINAL_OVERLAY=1` puts it over the screen instead, as a panel of the
  game's interface, as it used to be). Enter starts a breach: six commands, one at a time, each
  to be typed exactly into the box under the output before its trace runs out, 15 seconds for
  each. A wrong key does not go in, costs half a second, flashes `ERR` and jolts the
  terminal, and the trace bar flashes once it is nearly out. If the trace completes first,
  access is denied and Enter tries again at once; all six typed, access is granted and the
  frame glows blue. Tab walks away. The clock keeps running, and the droids keep going about
  their business, while you type. For now it opens nothing: it is there to try the hacking out.
  The model is `data/computer.glb`, made from `unlockables/computer.blend`; the terminal is in
  DejaVu Sans Mono (`data/fonts/`).
- **Shot down, droids fall limp**, as ragdolls: every part of the body falls as the physics has
  it, knocked back by the bolt, and a bolt that hits one lying there shoves it. A few seconds
  later the droid is gone.
- **Bolts break droids apart - no blood, just broken geometry.** The bolt that brings a droid
  down, or any bolt that hits one lying there, breaks off the part it hits: its head, an arm at
  the shoulder or the elbow, a leg at the hip or the knee (a hand breaks at the elbow, a foot at
  the knee). The part falls away by itself, both broken ends showing the torn shell and the
  glowing green voxels inside, and a handful of loose voxels spill out of them and bounce across
  the floor.
- **Running at the ready.** With the pistol held low, the droid's left arm pumps as it runs,
  its shoulders twist against its hips with each stride, and the pistol bobs, the muzzle dipping
  as each landing drops the hips.

25 September 2026:

- **Talking to the droids, as in Fallout 3.** Close to a droid and facing it, its name shows under
  the middle of the screen with `E) Talk`. E starts talking: the droid stops and turns to you,
  yours turns to it, and the camera leaves its shoulder for a close-up of the droid's face, the
  view narrowed as with a long lens. What it says runs along the bottom of the screen in a green
  panel, in System Latin with the English under it, and under that the replies. Pick one with
  the mouse, with W and S or the arrows and then E, Enter or Space, or with its number; Tab walks
  away. A reply can be a Speech check (`[Speech 40%]`), which goes one way if it succeeds and
  another if it fails, and can be tried only once. Replies already given are dimmed. The clock
  stops while you talk.
- **Three kinds of droid**, each with its own code: a sentry (Defendator), a scout (Explorator)
  and a maintenance unit (Reparator), in turn. Talk the scout or the sentry round and it tells
  you how far off the exit is and which way, as the crow flies. What they say is in
  `data/dialogue/droids.json`, which can be rewritten without a rebuild. Every line parses with
  the System Latin parser in `data/system_latin/`.
- **The droids speak.** Each line is said out loud in a voice made from formants, as the pistol's
  sounds are, heard from the droid's face. System Latin is read as it is written, each sound
  gliding into the next, each word a moment apart and each vowel on a note of its own, falling
  at the end of a sentence and rising at a question. The sentry speaks low and slow, the scout
  high and quick, and the maintenance unit breathily; every droid a little higher or lower than
  the rest of its kind. The sounds and voices are in `data/sounds/voice_formants.json`, which
  can be retuned without a rebuild. A line is cut off by the next one, or by walking away.
- **The close-up looks level and square on at the droid's face**, from low enough that the
  face is up above the conversation at the bottom of the screen.
- **Moods.** The conversation's panel takes the colour of how the droid feels about what it is
  saying: green as usual, blue for success, yellow for a warning or a question, orange for
  agitation or a failed Speech check, and red when it is hostile. Each line in
  `data/dialogue/droids.json` can say its own; one that does not is blue after a check that
  succeeded, orange after one that failed, and green otherwise.
- **The eyes go with the mood too**: the droid being talked to has its eyes glow the panel's
  colour, and their own green again as usual and once the conversation is over. The colour
  they glow as usual, and how brightly, is the model's own `eye_glow` material.
- **The voice goes with the mood**: higher when pleased, questioning or agitated, lower when
  hostile, as `moods` in `data/sounds/voice_formants.json` has it.
- **Pausing stops the sound too**: a droid's line, the pistol and its bolts carry on from where
  they were once the game is resumed.
- **Always someone to talk to.** One of the droids stands just in front of you at the start of
  each round, facing you, and stays there rather than wandering off, even once you have talked
  to it.
- **Sentries turn hostile.** Fail the sentry's Speech check to pass as a maintenance unit, and it
  orders you out of its zone, in orange; walk away and that is the end of it, but refuse and it
  says "You are an intruder. Prepare for deletion." in red. Fail to send it after something
  behind you, and it sees through you and says the same. Its eyes stay red, it cannot be
  talked to any more, and once the conversation is over, a moment later it comes after you at
  a run, and hunts you as below. If it catches you,
  you are deleted, and a new maze begins a few seconds later. Three bolts from the pistol stop
  it: it goes down in a crouch, its eyes dark, and stays there. A line in
  `data/dialogue/droids.json` with `"attacks": true` has its droid do this.
- **Stealth, as in Metal Gear.** A hostile droid goes through Metal Gear's phases, shown in the
  status line and in its eyes:
  - **ALERT** (red eyes): it can see you, and runs at you.
  - **EVASION** (orange), with the seconds of searching it has left: it has lost you. It runs to
    where you were heading when it last saw you and looks about, then walks from one spot to
    another nearby, looking about at each, for 30 seconds.
  - **CAUTION** (yellow), with the seconds left: it has given up, and wanders, still watching
    for you. After a minute it calms down, its eyes go back to green, and it can be talked to
    again.

  It sees only what is in front of it, 55 degrees either way, in every phase, even right next
  to it: it can be crept up on, or slipped round. On ALERT it sees you 30 m off however low you
  are; otherwise 30 m off standing, half as far crouched and under a third as far crawling.
  Seeing you in any phase puts it back on ALERT. A bolt that hits it without it seeing you has
  it search where you fired from. As it spots you, loses you and gives up, it says so out loud: "Intrusor detectum. Sta." ("Intruder detected.
  Halt."), "Intrusor perdatum. Zeto intrusor." ("Intruder lost. Searching for the intruder.")
  and "Phantasma. Resumo patrolium." ("A sensor ghost. Resuming patrol."), with subtitles
  under the status line. These are the sentry's `barks` in `data/dialogue/droids.json`.
- **Sentries join a chase.** A sentry that sees another sentry on ALERT, running after you, goes
  after you too, calm or not: the one it sees shows it where you are. It sees the other as it
  would see you standing there - in front of it, less far with the lights off -
  and a calm one stands a second before it sets off, as one does that has just turned hostile.
  A sentry that sees one that has joined in joins in as well. Once it sees neither you nor
  anyone after you, it searches where you were, as usual.
- **Noise.** Out of ALERT, a hostile droid listens as well. Walking, crouching and crawling are
  silent; running is heard 5 m off, sprinting 10 m, landing hard from a fall 6 m, a pistol shot
  12 m and a bolt hitting something 8 m - measured along the corridors, so a wall between you
  muffles it. A droid that hears you says "Quid? Sonus detectum. Verifico." ("What? A sound
  detected. Checking."), runs to where the noise was and searches from there, and pays no heed
  to another noise for 3 seconds. A bolt fired into a wall away from you makes a distraction.
- **Darkness.** With the lights off, a droid sees only about a third as far, whichever phase it
  is in - unless your flashlight is on, which gives you away as if the lights were on.
- **The crosshair is always there.** The pistol's screen, crosshair and all, shows the whole
  time the pistol is out, rather than only for a moment as it fires; each pull of the trigger
  now flashes it brighter, the crosshair with it. The crosshair's marks keep the model's own
  solid green glow instead of being turned into glass with the screen, and the screen's glass no longer mirrors its
  surroundings or catches highlights from lights: only what is seen through it, and its glow.
- **Don't point that at me.** Every droid minds having the pistol pointed at it - the middle
  of the view on it, within 20 m, with the pistol out. Keep it there and the droid stops, turns
  to face you and warns you, its eyes yellow. Each warning then holds 2.5 seconds longer than
  that, for it to be said and taken in, before the next: the last warning, its eyes orange, and
  then being provoked. Sentries are the least patient, warning you after 0.8 seconds, again at
  4.1 and provoked at 7.4, and provoked, they come after you ("Aggressor detectum.
  Deletabso tu." - "Attacker detected. I will delete you."). Scouts and maintenance units
  warn you after 2 seconds, again at 6.5 and provoked at 11, and then sound the alarm ("Aggressor detectum. Sireno."):
  every sentry within 40 m that is not already after you comes to search where you are
  ("Sirenatio accipatum. Zeto aggressor." - "Alarm received. Searching for the attacker.").
  Looking away only holds the next stage off; lower the pistol or keep it off a droid for 2.5
  seconds and it calms down ("Accipatum."). Shooting a droid provokes it at once. How patient each kind is, and what it
  does, is its `threatened` in `data/dialogue/droids.json`, and what it says are its `barks`.
- **The pistol in first person, as in Call of Duty.** Seen through the droid's eyes, the pistol
  is held in view, low and to the right, pointing in at the middle of the view. Holding the
  right mouse button raises it into the middle to aim, its screen - crosshair and all - square
  in the middle of the view like a sight. It comes up from below as it is drawn and goes back
  down as it is holstered, kicks back and up with each shot, and tucks down out of the way
  right up against a wall; the ball at its muzzle tumbles and its screen flashes as the
  droid's own do. In first person a shot leaves its muzzle for whatever the middle of the view
  is on.
- **A bolt shows where it hits.** It stops with its tip against whatever it hit and glows there
  a moment, lighting it up, rather than going out a step short of it.

24 September 2026:

- **A pistol.** R draws the droid's pistol and holsters it again; the left mouse button draws it
  too, and once it is drawn fires it. Drawn, the droid aims ahead and strafes as with the right
  mouse button, its upper body in the pistol's poses (`droid_pistol_draw`, `_aim`, `_fire`,
  `_holster`) while its legs walk, run or stand as ever. The pistol shows and goes partway
  through the draw and the holster, as `data/droid_motion.json` says, and is out of sight in
  its hand until then. Drawn, it is held lower, at the ready (`droid_pistol_ready`); holding
  the right mouse button raises it to aim, and a shot from the ready raises it, fires as soon as
  it is up and lowers it again a second after the last shot. Raised or at the ready, the pistol
  follows the camera: the droid's upper body leans it
  up, down and round towards wherever the camera looks, up to 60 degrees each way, blending the
  model's aims (`droid_pistol_aim_*`, laid out in `droid_motion.json`). A shot is a glowing bolt
  from the muzzle that flies the way the gun points and stops at the first thing it hits,
  harming nothing yet. The
  bolt is green and carries a green light with it, lighting up the droid as it leaves and
  everything it passes in the dark. The pistol's screen, see-through green glass, flashes with
  each pull of the trigger; the ball at its muzzle - a glowing core in a see-through green
  shell - shows all the while the pistol is out, the core and the shell tumbling every way
  round, each the opposite way to the other.
- **Square to the front.** Strafing, or with the pistol drawn, the droid's face and shoulders
  stay square to straight ahead, however its hips turn to the way it steps.
- **N for a new maze**, since R is the pistol's now.
- **Over the right shoulder.** The camera sits closer and further out to the droid's right, as in
  Fallout: the droid stands to the left of the view, the way ahead clear on the right. It had
  been over the left shoulder. Holding the right mouse button brings it in closer, to aim over
  the shoulder, and letting go takes it back out. Holding the middle mouse button to swing the
  camera round and look at the droid leaves the droid as it was: its pistol no longer follows
  the camera round.
- **Head bob in first person.** Seen through the droid's eyes, the head rises and falls in step
  with the stride again, and fades out on going over to the view from behind.
- **The pistol sounds.** A shot cracks at the muzzle, and each bolt hums as it flies. The sounds
  are made from formants when the game starts - a buzz and noise shaped by resonances, as a
  voice is - described in `data/sounds/pistol_formants.json`, which can be retuned without a
  rebuild; `src/formants` makes them. What sounds is heard from the camera.
- **The flashlight starts off.** F switches it on.

23 September 2026:

- **Strafing.** Holding the right mouse button keeps the droid facing ahead whichever way it goes:
  it steps sideways, back and along every diagonal in its new strafes (`droid_strafe_walk_*`,
  `droid_strafe_run_*` and `droid_strafe_crouch_*`, crouched or crawling, each `_L`, `_R`,
  `_FL`, `_FR`, `_B`, `_BL` and `_BR`), turning only a few degrees whichever way it goes.
  Strafing, a sprint slows to a run, picking up again once the button is let go, and the droid
  does not skid. In cover the
  wall still sets which way it faces.
- **Skids of every kind, sprinting only.** Sprinting flat out, the droid skids round
  (`droid_skid_sprint_turn_L`/`_R`) and runs out through the run, cuts across a quarter turn
  (`droid_skid_sprint_turn90_L`/`_R`), and let go of, slides to a standstill
  (`droid_skid_sprint_stop`) and idles. Running, it no longer skids at all. Whether it is
  sprinting goes by how fast it is going, not the keys held. The skids are made on the spot, and `data/droid_motion.json`, exported along with them,
  has where each takes the droid and how far round: the body follows that path, as far as it
  goes for how fast the droid went in, and the droid swings round with it.

22 September 2026:

- **A droid to play as.** The player is seen from behind as a droid (`data/droid_full_deform.glb`),
  with the camera over its shoulder and pulled in when a wall is in the way. V switches to first
  person, and holding the middle mouse button swings the camera round the droid.
- **Its feet set the speed.** Walking, running, sprinting and crouching each play the droid's own
  cycle, and each gait goes as fast as that cycle's stride, so the feet stay on the floor. That
  makes every gait slower than before.
- **It faces the way it goes, and skids round.** The droid turns to face whichever way the keys
  send it, and stands idling when still. Turning round at a run or a sprint, it skids to a stop,
  swings round and sets off the other way.
- **It jumps, low or high.** Tap Space for a low jump, hold it for a high one; each press jumps
  once. From standing still the droid springs straight up and lands on the spot; on the move it
  leaps in its stride and lands running. It lands hard from a high jump or a long drop, lightly
  otherwise, and falling off an edge it flies and lands the same way.
- **Inhabitants.** Six more droids live in the maze, made from the same model. Each wanders off
  somewhere, keeping to the middle of the corridors, idles a while when it gets there, and sets
  off again. They are solid and make way for each other: walking, they veer to their right round
  whoever is ahead, and standing about, they step aside for anyone coming straight at them.
  `MAZE_INHABITANTS` sets how many.
- **Cover.** Tab puts the droid up against the wall ahead. A and D then slide it along the wall,
  facing the way it goes, as far as the wall's edge - the corner to take cover behind. Holding A
  or D on past the edge leans out round the corner; Ctrl no longer leans. Tab again, pushing away
  from the wall, or jumping leaves cover. Until there are cover animations (`droid_cover_idle`,
  `droid_cover_walk`, picked up once the model has them) it idles and walks as usual.
- **No more head bob** from behind. The camera is held steady; the droid's cycles show the stride.
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

It opens on the main menu, to pick which game to play:

- **Maze**: find the way out of a new random maze each round.
- **Capture the Flag**: you are blue. Pick a map - Lanes (`data/arena/ctf_map.glb`) or
  Balconies (`data/arena/ctf_balconies.glb`), Escape going back - then hack the computer in red's
  base to take down the firewall round their flag, take it, and bring it home to your own, with
  blue's droids and drone on your side and red's against you.

With `MAZE_MODEL` set, it skips the menu and plays that model.

### In a browser

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version <the wasm-bindgen version in Cargo.lock>
./web/build.sh
python3 -m http.server -d site
```

`web/build.sh` builds the game to WebAssembly and puts it in `site/` with the page and `data/`,
ready to serve as it is; open http://localhost:8000 and press Play. In a browser the game draws
with WebGL 2, the one graphics API every browser has, and so shadows come from shadow maps: no
browser offers ray tracing. The published copy lives in
[MazeGame-web](https://github.com/d4140n-4h3-1/MazeGame-web), which serves `site/` on GitHub
Pages.

## Controls

| Key              | Action                                                        |
| ---------------- | ------------------------------------------------------------- |
| W A S D, arrows  | Move                                                          |
| Mouse            | Look around                                                   |
| Caps Lock        | Walk or jog; it stays as you left it                          |
| Shift (tap)      | Walk or run; it stays as you left it. Running costs stamina   |
| Shift (hold)     | Sprint while held. Costs stamina faster than running; running out leaves you walking |
| Space            | Jump: tap for a low jump, hold for a high one                 |
| C                | Crouch, or stand back up                                      |
| Z                | Crawl, or stand back up                                       |
| Q (hold)         | Look behind you while still moving forward                    |
| Tab              | Take cover against the wall ahead, or leave it                |
| A / D in cover   | Slide along the wall; hold on past its edge to lean round it  |
| F                | Flashlight on or off (it starts off)                          |
| V                | Third person (behind the droid) or first person               |
| Middle mouse (hold) | Swing the camera round the droid to see it from any side    |
| Right mouse (hold) | Strafe: keep facing ahead whichever way you go; anything faster than a walk is a jog |
| R                | Draw the pistol, or holster it                                |
| Left mouse       | Draw the pistol; once it is drawn, fire                       |
| Right mouse, drawn | Raise the pistol to aim, rather than hold it at the ready   |
| E                | Talk to the droid close by and in front of you, or use the computer in front of you |
| Talking: mouse, W / S, arrows | Pick a reply; E, Enter, Space or a click says it  |
| Talking: 1 to 9  | Say that reply                                                |
| Talking: Tab     | Walk away                                                     |
| Computer: Enter  | Start a breach, or try again once denied                      |
| Computer: typing | Type the command shown, exactly                               |
| Computer: Tab    | Walk away                                                     |
| N                | New maze                                                      |
| `[` `]`          | Turn slower or faster                                         |
| `-` `=`          | Narrower or wider view                                        |
| Escape           | Pause                                                         |

### Pause menu

Escape pauses the game: the clock, the player, the physics and every sound all stop, sounds
carrying on from where they were once it is resumed. Switching to another
window in the middle of a round pauses it too. The menu has:

- **Resume**: carry on where you left off.
- **Lights**: switch the maze's lights off, or back on. Off, the lamps, the glow of their
  fixtures, the sun and nearly all the ambient light go out, leaving your flashlight and the
  exit's own glow. The setting carries over to each new maze.
- **Options**: switch the subtitles of what the droids say on or off, the System Latin and the
  English each by itself. With only the English on, it is as big as the System Latin would be.
  Back, or Escape, returns to the menu.
- **Start again**: the same as N: a new maze, or a new round in a fixed maze model or capture the
  flag.
- **Main menu**: leave the game under way for the main menu, to pick another.
- **Quit**.

Every menu - this one, its options, the main menu and its maps - works from the keyboard as well
as the mouse: the arrow keys, or W and S, go up and down the buttons, the one picked in light
blue, and Enter or Space presses it. Start again is passed over while there is nothing to start
again.

## Options

Options are set with environment variables, for example `MAZE_SIZE=10x10 cargo run`. In the
browser, add them to the page's address instead:
`https://d4140n-4h3-1.github.io/MazeGame-web/?MAZE_SIZE=10x10&MAZE_SEED=7`. What `MAZE_DEBUG`
logs goes to the browser's console there.

| Variable                 | Effect                                                                  |
| ------------------------ | ----------------------------------------------------------------------- |
| `MAZE_SIZE=<w>x<d>`      | How many junctions wide and deep the maze is. The default is `20x20`.   |
| `MAZE_SEED=<n>`          | Makes every maze and round the same, for comparing two runs.            |
| `MAZE_MODEL=<path>`      | Plays a fixed maze model (`.glb`, `.gltf` or `.fbx`) instead of random mazes. |
| `MAZE_INHABITANTS=<n>`   | How many droids live in the maze. The default is 6.                     |
| `MAZE_DEBUG=1`           | Logs the walkable map of each level, and rendering statistics once a second. |
| `MAZE_WINDOWED=1`        | Opens the game in a window instead of filling the screen.               |
| `MAZE_VSYNC=0`           | Uncaps the frame rate, for measuring what a frame costs.                |
| `MAZE_RT=0`              | Shadow maps instead of ray-traced shadows.                              |
| `MAZE_HARD_SHADOWS=1`    | Ray-traced shadows with sharp edges instead of soft ones.               |
| `MAZE_SHADOW_BUDGET=0`   | With shadow maps, gives every lamp in range one, not just the nearest.  |
| `MAZE_SSAO=0`            | Turns ambient occlusion off.                                            |
| `MAZE_REFLECTIONS=0`     | Turns floor reflections off.                                            |
| `MAZE_KNOCKDOWN=<s>`     | That many seconds into a round, shoots down the droid nearest you, to try the ragdolls out. |
| `MAZE_DRONE_ALARM=<s>`   | That many seconds into a round, sends the drone after you as if the alarm had sounded, to try it out. |
| `MAZE_DISMEMBER=<parts>` | With `MAZE_KNOCKDOWN`, breaks those parts off it too, such as `head,forearm.L,shin.R`. |
| `MAZE_COMPUTER=1`        | Puts you at the computer, using it, as soon as it is placed; `breach` starts a breach too, and `look` only puts you in front of it. |
| `MAZE_TERMINAL_OVERLAY=1` | Shows the computer's terminal over its screen, as a panel of the game's interface, instead of on the screen itself. |

### Fixed maze models

A model given with `MAZE_MODEL` needs no special structure, but the game reads a few things from
it:

- Surfaces colored **pure magenta** (255, 0, 255) are the glass of light fixtures. They become
  refractive glass, and each fixture gets a lamp.
- Small meshes standing on their own, narrower than 3.5 m, are taken as **something to find**:
  the round ends there instead of at a random exit.
- FBX models are taken to be in centimeters and scaled down.
- Ground is only walkable with a **ceiling** somewhere above it, and the floor has to sit at or
  above the game's own safety floor, whose top is at -0.05 m.
- **Floors up above** are walkable where stairs lead up to them from the ground, each step no
  more than 0.55 m: a spot keeps one floor, the highest with 2 m of headroom, so make raised
  floors solid down to the ground rather than leaving room under them.

Anything in the model that glows by itself goes dark with the lights.

`data/arena/combat_map.glb` is one such model: an indoor arena of hexagonal pillars and
rectangular cover blocks, for trying out combat. Play it with

```sh
MAZE_MODEL=data/arena/combat_map.glb cargo run
```

`examples/team_battle.rs` plays the same arena as a battle between two teams of droids, red and
cyan, five a side, all of them AI: they fight from cover, fall back when hurt and come back at
their own end when downed, and the first team to 30 kills wins. The camera is free to fly about
the arena or follow one droid (Tab).

```sh
cargo run --release --example team_battle
```

`data/arena/ctf_map.glb` is an arena for capture the flag, 84 by 52 m. Red's base is at the -x
end and cyan's at the +x end. Each base is a walled room with its team's colour round the walls
and doorways. Its flag stands on a pad in the middle, and there are three ways in: a front door
off the yard, and a door in each side wall off a corridor round the back. Between the bases, two
long walls with doorways through them split the field into three lanes. The middle lane is open
round a central tower, and each side lane is narrower, with a low wall across its middle. Both
halves of the map are the same, mirrored, with low and tall cover and pillars throughout.
`data/arena/ctf_map.py` builds it in Blender (`blender -b --python data/arena/ctf_map.py`).
It is what **Capture the Flag** on the main menu plays, as `MAZE_MODEL` does too: you start at
blue's end, take red's flag and bring it home to blue's.

The model marks where the game puts things with empties. `flag_red` and `flag_blue` get that
side's flag, `data/ctf/flag_red.glb` or `data/ctf/flag_blue.glb`, standing in a firewall,
`data/ctf/firewall.glb`, turned the way the empty's +x points. `computer_red` and `computer_blue`
get the computer that opens that firewall, its screen facing along the empty's +x; without one,
the computer goes against the wall nearest the firewall. `post_red_1`, `post_red_2` and on, and
`post_blue_1` and on, are each side's droids' posts, the first droid at the first; a droid with
no post keeps to its flag. The firewall's orange shell stands in the
way until its computer is hacked, then goes, and the flag can be taken from beside its plinth;
taken, it is carried on the taker's back.
The shell is see-through glass that tints what is behind it orange, and its glow and its light
flicker. The team battle fights in it with

```sh
BATTLE_MAP=data/arena/ctf_map.glb cargo run --release --example team_battle
```

## Tests

```sh
cargo test
```

The tests cover maze generation and tile fitting, what can be seen from where, the walkable grid
and round planning, the player's movement, breath, head motion, leaning and keys, the ragdoll and
how a droid comes apart, and the computer's hack and where it goes.

## How it fits together

| Module          | What it does                                                               |
| --------------- | -------------------------------------------------------------------------- |
| `main.rs`       | Starts the engine and sets up the graphics effects.                        |
| `game.rs`       | The game: loading levels, rounds, input, the pause menu, the lights.       |
| `level.rs`      | A level in the scene: its pieces, collider, lamps and walkable ground.     |
| `generate.rs`   | Plans random mazes on a grid of junctions and turns them into tiles.       |
| `tiles.rs`      | Measures the tile models and assembles a maze from them.                   |
| `layout.rs`     | Where a round starts and ends, on `hydroxus-ai`'s walkable grid.           |
| `survey.rs`     | Finds the walkable ground of a level by casting rays into it.              |
| `culling.rs`    | Hides the pieces and lamps that cannot be seen from where the player is.   |
| `fixtures.rs`   | Light fixtures: the glass, the lamps, and everything that glows.           |
| `inward.rs`     | Makes the tiles' surfaces visible from inside and out.                     |
| `hud.rs`        | The status line, the banner, the alert at the top, the stamina bar and the pistol's ammo. |
| `hearts.rs`     | The hearts: where they go in each maze, and their floating and spinning.   |
| `drone.rs`      | The security drone: its patrol, the alarm, searching and firing, its glow and mood. |
| `credits.rs`    | Credits, the maze's money: amounts in hundredths, how they are written, and what a computer carries. |
| `drone_shot.rs` | The drone's shots: their flight, what they hit, and their glow.            |
| `menu.rs`       | The pause menu.                                                            |
| `computer.rs`   | The computer to hack: where it goes, the hack, and its terminal.           |
| `ragdoll.rs`    | A droid gone limp: its bodies and joints, from `data/droid_motion.json`.   |
| `dismember.rs`  | Breaking a droid apart: its pieces and broken ends, and the voxels that spill. |
| `dialogue/`     | Talking to the droids: their conversations from `data/dialogue/`, and the panel they are shown in. |
| `diagnostics.rs`| The Vulkan check and the rendering statistics.                             |
| `formants/`     | Sounds made from formants: the file format, the synthesizer that makes the pistol's sounds from `data/sounds/`, the droids' speech, and the drones' beeps (`chirps`). |
| `player/`       | The player, one file per part: posture, movement, breath, head, lean, input, view, the droid (`avatar`), the camera behind it (`third_person`) and its close-up when talking (`talk`). |

The tile models, the droid (`droid_full_deform.glb`) and the computer (`computer.glb`) are in
`data/`, along with where the droid's skids take it, its ragdoll and how it comes apart
(`droid_motion.json`), and the terminal's font (`fonts/`, DejaVu Sans Mono, under its own
license in `fonts/DejaVuSansMono.LICENSE`).

## License

Copyright (c) 2026 VulpesPhantasma ([d4140n-4h3-1](https://github.com/d4140n-4h3-1) on GitHub).

The code is under the MIT license (`LICENSE-MIT`). The models, animations and other files in
`data/` are under Creative Commons Attribution-ShareAlike 4.0 International (`LICENSE-CC-BY-SA`):
share and adapt them, crediting VulpesPhantasma (d4140n-4h3-1), with anything made from them
shared under the same license.
