# wheel_ring

A true-metric steering-wheel reference ring as an OpenVR overlay, for dialing in
**SteamVR per-app world scale** in iRacing. Zero crates; talks to `openvr_api.dll`
directly via the `IVROverlay_028` fn-table. Same scaffolding as `ipd_align` /
`sharp_fov`.

## Build
```
cargo build --release
target\release\wheel_ring.exe
```
Run with SteamVR up. iRacing in OpenXR keeps SteamVR's compositor live, so the
overlay shows on top of the sim.

```
wheel_ring.exe                  # 381 mm ring (15" Max Papis sprint wheel)
wheel_ring.exe -d 330           # different wheel diameter
wheel_ring.exe -d 330 -f 480    # set the texture field span too
```

## Controls
```
arrows   tilt: Up/Dn = pitch (rake), L/R = yaw
w / s    up / down          a / d   left / right
r / f    away / toward      SHIFT   big step
[ ]      ring -/+ 1 mm       { }     ring -/+ 10 mm
p        print state         q/Esc   quit
```
Green = primary ring (your set diameter). Blue rings = -5/-10%, red = +5/+10%.

## Why the ring, and how to read it

World scale is a **stereo-depth** effect: it changes the render IPD (binocular
disparity), which the brain reads as size via size constancy. It does **not**
change the monocular angular size of the wheel. Consequences:

- Judge **with both eyes open**. A one-eyed "does it fit the circle" check is
  invariant to world scale and tells you nothing.
- The precise null is **head-sway parallax**: place the ring at the wheel's real
  depth and sway left-right. Different depth -> ring and rim slide against each
  other; matched depth -> they move together. Null it.
- Confirm overlays bypass the per-app scale (expected): bring the ring up, change
  World Scale, and check the ring's own size/depth doesn't move. If it moves, the
  ring isn't an independent ruler and the method is invalid.

## Workflow (one pass)

1. Calibrate in a **sprint car** — its in-sim wheel is ~15"/381 mm, equal to the
   physical Max Papis, so a ring match means true 1:1. (World Scale is one number
   for all of iRacing; every car is modeled to real size, so it carries over.)
2. Position + tilt the ring onto the in-sim rim (both eyes; null parallax).
3. Read which reference ring the rim lands on:
   `new% = current% * 381 / apparent_mm`
   e.g. rim on the +8% red zone at 125% -> 125 * 381/411.5 ~= 116%.
4. Set that in SteamVR -> Settings -> Video -> Per-Application -> iRacing.

## Notes
- Overlay is in **seated** universe; recenter your seat first so it starts near
  the wheel. Then nudge with the keys.
- Running an OpenVR overlay alongside OpenXR costs FPS — fine for a one-time
  calibration; close it (`q`) before you race.
