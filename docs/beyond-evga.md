# What Ectomatter adds to the EVGA Z10 RGB

What Ectomatter and its builds of the Z10's firmware do that EVGA's Unleash RGB
(1.0.11.0, read from its decompiled source) and the keyboard's own firmware
(v1.17) never did. A feature goes on this list only once it is built, and only
when EVGA's software and firmware are checked and found without it. Add to it
whenever such a feature lands.

## The keyboard's firmware

Ectomatter's builds of the Z10's firmware (notes in `z10-rgb/firmware/FINDINGS.md`).
Each is EVGA's v1.17 with a few bytes changed and a little code added in its
unused flash; flashing EVGA's own file puts everything back.

| Feature | Since | EVGA's firmware |
|---|---|---|
| The volume slider reaches the whole range: it sends steps for where it sits, not for how far it moved, so a fast slide loses none | the slider build | Steps from movement; most were dropped on Linux and the volume never reached the ends |
| Both sliders' positions can be read by the computer | build 2 | The positions stayed inside the keyboard |
| The computer can set the dimmer's level and hold it there | build 2 | Only the slider set the backlight |
| The volume slider can be lent to the computer: the keyboard stops sending volume keys from it, so it can do another job | build 3 | It always sent volume keys |
| The dimmer stays where it is set until the slider moves | build 3 | The slider's position always won |
| The keyboard tells the computer when Fn is held | build 4 | Fn stayed inside the keyboard; nothing of it reached the computer |

## Ectomatter

| Feature | Since | Unleash RGB |
|---|---|---|
| Runs on Linux | first release | Windows only |
| Each slider can be given another job: the dimmer can control the system or microphone volume, the volume slider the keyboard's brightness or the microphone, or either nothing (needs firmware build 2 or 3) | `8e177b7` | No settings for the sliders |
| A slider set aside because the other took its job gets it back when the other gives it up | `9ef9591` | (no slider jobs) |
| The sliders' positions shown live on the picture of the keyboard and in their boxes | `cd23b10` | None |
| The keyboard drawn in HDR and lit the way its LEDs light it, through the caps, with the light simulated | `ac0ae5d` | A flat picture of the keyboard |
| The Keyboard lab (the Key lab until 2026-10-21): set up the caps (shine-through, pudding or translucent), the legends (typeface, weight, size and place, for every key or one), and the LEDs, and save them as setups | `380bcda` | None |
| Changes to a profile can be undone (Ctrl+Z) and done again | `8e8be74` | No undo |
| The screen shows the computer's processor, graphics card and memory, live, from Ectomatter itself | `f7a6079` | Nothing writes to the screen; only EVGA Precision X1, a separate program for EVGA's graphics cards, could fill it |
| A game's launch and exit play on the screen, with game-helper's steps as they happen | `cfb795c` | None |
| The screen drawn live in the app, dot for dot in the keyboard's own font | `f7a6079` | A swatch of the screen light's color |
| Screen layouts edited in a window of their own: words, readings, bars and the time as blocks dragged onto the screen, moved and sized cell by cell by dragging, never covering each other, live on the keyboard while they change; words, readings and bars carried across several rows (words scrolled or cut off when they don't fit), and the date in ten formats; effects behind the other blocks (rain, snow, a starfield, code rain, fire, plasma, fireworks, a spectrum that moves with the computer's sound, a heartbeat that beats with its processor, an aquarium, Pong, Tetris, Snake, a highway, Space Invaders, a bouncing word), each at its own speed, never changing a cell faster than its step; each layout lit by its own color or the profile's, and duplicated, renamed or deleted there | `f43d757`, rows and scrolling `c11f5a2`, readings, bars and dates `188b05a`, effects `27838af`, `543c0c7`, `2f4172c` | Only the screen's light, its brightness and which of the keyboard's own screens show |
| Every sensor on the computer listed in Settings, a tab for each part (the processor, graphics, memory, the motherboard, drives, network, a laptop's battery and its own controller): each a checkbox named as people say it, its driver's own name beside it and its reading now; long runs folded into a line that opens (each core's load and clock, a card's other temperatures, voltages); a reading that can't be right in amber, with why; a board whose sensor chip Linux has no driver for said so, with the driver that reads it; any sensor renamed. The ones checked are what a screen layout can show, any of them, and the GPU readings follow whichever card is made main | `b683e9f`, `250acb3` | None: Unleash never writes to the screen, and only Precision X1 reads EVGA's own graphics cards |
| Several pages play on the screen in turn (Stats, a clock, the weather), each for its own time, coming in with a wipe, a dissolve, typed in or at once | `3d717e7` | None: Unleash never writes to the screen |
| A Library of screens, and the playlist as a strip under the screen: each turn as wide as its time, a playhead moving through it live, a turn's time set by dragging its edge, the order by dragging its block, a screen played more than once | `968ec3f` | None: Unleash never writes to the screen |
| Several playlists, each profile playing one of its own: switching profiles, in Ectomatter or with Fn+G, switches what the screen plays; playlists made, renamed in place and deleted from the panel under the screen | `bb73bf4` | None: Unleash never writes to the screen |
| A turn's time picked from a menu as well as dragged, or set to stay until you switch layouts: an infinity mark on its bar, the playhead waiting at its end | `5bd142e` | None: Unleash never writes to the screen |
| Keys that switch the screen: any key can be given Next or Previous screen layout, Next or Previous playlist, or Play one playlist, on either layer, none set until you choose; the jobs send codes the USB standard reserves, heard from the keyboard's own reports, so no key code anyone uses is given up | `eaebccf` | None: Unleash's LCD mode only turns its own screen's rows with ▲ and ▼ |
| The keyboard's own status screen drawn in the app as its firmware draws it, for each profile: its number and name, dot for dot in the keyboard's own font, lit in the profile's backlight, each part there or gone as soon as it is switched | `bb15072` | A fixed sample in an ordinary font (PROFILE : MY DEFAULT, NKEY, LCD MODE, a date from 2019); only the clock's line follows its checkbox |
| Twelve ready-made screen layouts in the Library: Stats, a clock, the weather, the day with the weather, one for games, a heartbeat between the CPU's readings, the profile, and five effects to look at (rain, a starfield, a campfire, an aquarium, music); each clock on a 12-hour clock and the weather in °F; one deleted is put back from New screen layout's menu | `92baa62`, American standard `c81c610`, `8e45275` | None: Unleash never writes to the screen |
| Layouts put in any order in the Library by dragging a card to another place: the other cards make room where it will land; carried onto the playlist instead, it is added there | `204c955` | None: Unleash never writes to the screen |
| Home is your desk: each device Ectomatter knows, drawn as its own page draws it and lit as it is now, to scale and where it sits; in Arrange the devices move on a grid of dots, zoomed and panned, and pads (a desk mat, a mousepad) are added at their makers' sizes, true to scale under the devices, or drawn, then sized to a standard size or a typed one, in a color or with a picture | `fc22902`, pads `031633d` | A page for each EVGA device it finds; never the devices together |
| One light across the desk: a wave that crosses the keyboard and then the mouse by where they sit on Home's desk, or every device going round the colors together, playing with the window closed too | `6895c45` | A button that opens EVGA LED Sync, a separate program, or offers to download it |
| Macros Ectomatter plays itself, for a key on the keyboard or a button on the mouse: as many as you want, any one on any device, while Ectomatter runs; the Macro Editor lists them first, then the keyboard's own, which play without it | `88c054e`, the keyboard's keys `848e8dc`, one list `e2371e0` | Macros kept on the computer, played only from the keyboard's own memory |
| A key on the keyboard can switch the mouse's profile or play one of Ectomatter's macros, and the mouse's buttons can switch the keyboard's profile or what its screen shows | `848e8dc` | A key does the keyboard's own jobs only |
| Profiles that follow the app in front: with a game in front, the keyboard and the mouse each switch to the profile used with it, and back to yours when it leaves (on KDE Plasma) | `080c0a0` | Profiles switched by hand, or with Fn+G on the keyboard |
