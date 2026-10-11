# What Ectodeck adds to the EVGA Z10 RGB

What Ectodeck and its builds of the Z10's firmware do that EVGA's Unleash RGB
(1.0.11.0, read from its decompiled source) and the keyboard's own firmware
(v1.17) never did. A feature goes on this list only once it is built, and only
when EVGA's software and firmware are checked and found without it. Add to it
whenever such a feature lands.

## The keyboard's firmware

Ectodeck's builds of the Z10's firmware (notes in `z10-rgb/firmware/FINDINGS.md`).
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

## Ectodeck

| Feature | Since | Unleash RGB |
|---|---|---|
| Runs on Linux | first release | Windows only |
| Each slider can be given another job: the dimmer can control the system or microphone volume, the volume slider the keyboard's brightness or the microphone, or either nothing (needs firmware build 2 or 3) | `b8b0e72` | No settings for the sliders |
| A slider set aside because the other took its job gets it back when the other gives it up | `286e082` | (no slider jobs) |
| The sliders' positions shown live on the picture of the keyboard and in their boxes | `ef08dae` | None |
| The keyboard drawn in HDR and lit the way its LEDs light it, through the caps, with the light simulated | `e4b9e9a` | A flat picture of the keyboard |
| The Key lab: set up the caps (shine-through, pudding or translucent), the legends (typeface, weight, size and place, for every key or one), and the LEDs, and save them as setups | `e9805b2` | None |
| Changes to a profile can be undone (Ctrl+Z) and done again | `3834c40` | No undo |
| The screen shows the computer's processor, graphics card and memory, live, from Ectodeck itself | `fc4b2b0` | Nothing writes to the screen; only EVGA Precision X1, a separate program for EVGA's graphics cards, could fill it |
| A game's launch and exit play on the screen, with game-helper's steps as they happen | `d6d93a0` | None |
| The screen drawn live in the app, dot for dot in the keyboard's own font | `fc4b2b0` | A swatch of the screen light's color |
| The screen's pages edited in the app: words, readings, bars and a clock, each placed and moved cell by cell, live on the keyboard while they change | `48c36f5` | Only the screen's light, its brightness and which of the keyboard's own screens show |
| Several pages play on the screen in turn (Stats, a clock, the weather), each for its own time, coming in with a wipe, a dissolve, typed in or at once | `6bb2332` | None: Unleash never writes to the screen |
| A Library of screens, and the playlist as a strip under the screen: each turn as wide as its time, a playhead moving through it live, a turn's time set by dragging its edge, the order by dragging its block, a screen played more than once | `55db36c` | None: Unleash never writes to the screen |
| Several playlists, each profile playing one of its own: switching profiles, in Ectodeck or with Fn+G, switches what the screen plays; playlists made, renamed in place and deleted from the panel under the screen | `9b332d5` | None: Unleash never writes to the screen |
