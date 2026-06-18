# sharp_fov

Interactive sharp-FOV / blur-boundary measurement overlay for OpenVR — Pimax Dream Air.
Companion to `ipd_align`: that one centres the lens, this one measures how big the sharp
zone is and how it trades against IPD.

## Build

```
cargo build --release
```

`target\release\sharp_fov.exe`. Zero crates — loads `openvr_api.dll` (search path, then
`%LOCALAPPDATA%\openvr\openvrpaths.vrpath`) and `msvcrt.dll` (`_getch`) at runtime. If the
OpenVR dll isn't found, drop `...\SteamVR\bin\win64\openvr_api.dll` next to the exe.

## Use

Start SteamVR, then:

```
sharp_fov.exe                       # BOTH EYES, 1.5 m distance, 3.8 m width (~+-52 deg)
sharp_fov.exe --eye right --ipd 58  # measure the RIGHT lens only
sharp_fov.exe --dist 2.0 --width 4.5
```

**Both eyes open is the default and the right method for IPD tuning.** The binocular sharp
zone is centred on your forward axis, so it matches the field, no marker has to cross
centre, and it's your real viewing condition. The reason foveation matters: most edge blur
with both eyes staring forward is your retina, not the lens — looking straight *at* the
marker's rings is what isolates the optical blur.

**To measure one lens**, pass `--eye left|right` (and `--ipd MM`). The field shifts ~IPD/2
to sit in front of that eye; close the other eye and the four bars measure that lens
nasal/temporal/up/down with nothing crossing centre. A head-centred field *can't* measure a
single eye, because that eye's field is centred on its own lens, ~29 mm off your head
centre — so the single-lens mode slides the field over to compensate.

1. Select a marker — `Tab` cycles, or `1`/`2`/`3`/`4` = Left/Right/Top/Bottom. The
   selected one turns thick yellow with a yellow halo; the rest are thin cyan.
2. Move it — arrows (or WASD) push the selected bar the way they point, no mental
   translation. Each bar moves on its own axis:
   - **Left/Right bars**: `Left`/`Right` or `a`/`d`
   - **Top/Bottom bars**: `Up`/`Down` or `w`/`s`
   - `A`/`D`/`W`/`S` (caps) = big step; `r` resets the selected bar to the edge.
   Look at its ring emblem and walk it inward until the rings stop resolving.
3. Do all four, then `Enter` to print the half-angles and sharp FOV. `q`/`Esc` quits.

The console shows live angles as you move. Absolute half-angles are the numbers to compare
across runs — re-run as you nudge IPD in Pimax Play, chasing the largest sharp box. Because
the Dream Air's IPD motor shifts the lens, the IPD that gives the biggest sharp FOV may sit
slightly off the one that centres best (58) — this is how you see that tradeoff as numbers.

Distance note: in a VR headset the panel's image sits at one fixed optical focal plane, so
overlay distance changes size/convergence, not focus — the measured *angles* are exact at
any `--dist`. 1.5 m is the default because that's the convergence geometry the Dream Air is
comfortable at (matches what worked in ipd_align).

## Caveats

* Interface pinned to `IVROverlay_028` (current SteamVR), same as `ipd_align`. If a
  different runtime rejects it, the exe says so; bump `IFACE` + the `I_*` indices.
* If top/bottom feel swapped, that's the texture's vertical origin — the angle magnitudes
  are still correct as up/down extents.
* `--width`/`--dist` set how wide the test field floats. Push them up to probe further into
  the periphery; the angle math accounts for the geometry exactly.