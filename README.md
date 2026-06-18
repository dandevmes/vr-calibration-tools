# VR Optical Calibration Tools

A small suite of **zero-dependency** OpenVR overlay tools for measuring and
calibrating the optics of a SteamVR headset by hand — interpupillary centering,
real (sharp) field of view, and world scale.

Each tool talks to `openvr_api.dll` **directly** through the `IVROverlay_028`
function table. No SteamVR SDK, no build-time binding generators, no crates.
A binary is one small `.exe` that loads the runtime DLL at startup, draws an
overlay, reads the keyboard, and exits clean.

Developed against a Pimax Dream Air running through the community
`CustomHeadsetOpenVR` SteamVR driver, but they work against any headset that
SteamVR exposes.

---

## The tools

| tool | answers | method |
|------|---------|--------|
| **ipd_align** | Is my IPD / lens centering correct? | dichoptic nonius targets, both eyes |
| **sharp_fov** | How wide is my *actually sharp* field of view? | per-eye blur-boundary measurement |
| **wheel_ring** | Is the world rendering at true 1:1 scale? | true-metric reference ring |

Run them in that order for a full pass: get the optics centered, measure what
you actually have, then pin per-application world scale against a known real
object.

Each interactive tool **prints its key bindings on launch** — the briefs below
cover what each does and the flags; the on-screen printout is authoritative for
exact keys.

### ipd_align — IPD / centering check

Renders a set of **dichoptic nonius targets**: paired half-marks where the left
eye sees one half and the right eye the other, using the overlay's
side-by-side-parallel flag. If your IPD and lens centering are correct, each
pair fuses into a single continuous, aligned mark. A horizontal offset between
the halves is a direct readout of a centering error. Four targets sit around the
field plus a central binocular box, so you can check centering across the lens,
not just dead ahead.

```
ipd_align.exe                 # defaults: --dist 1.5  --width 1.6
ipd_align.exe --dist 1.5 --width 1.6
```

`--dist` pulls the targets off the distortion-heavy upper field; `--width` sets
their spread. The defaults were tuned so a true-centered headset shows clean
fusion on every target.

### sharp_fov — sharp field-of-view measurement

Draws a wide test field (checkerboard + center cross) with four movable boundary
markers — left, right, top, bottom — each carrying a concentric zone-plate
emblem that makes the onset of blur easy to judge foveally. Walk each marker out
to where the image stops being sharp; the tool reports the per-side half-angles
and the resulting **sharp HFOV / VFOV** and sharp box in deg². Movement is
screen-directional and each bar clamps to its own half so they can't cross.

```
sharp_fov.exe                            # defaults: --dist 1.5 --width 3.8 --ipd 63
sharp_fov.exe --eye left  --ipd 60       # shift the field onto the left lens
sharp_fov.exe --eye right --ipd 60       # ...and the right
```

Because each eye's field is centered on its own lens (off-axis from the head),
measure one eye at a time with `--eye` + your real `--ipd` for true per-lens
numbers; run with no `--eye` for the binocular field.

### wheel_ring — world-scale calibration

Renders a **true-metric reference ring** (default 381 mm = 15" Max Papis sprint
wheel) as a tiltable, positionable overlay, with faint ±5% / ±10% reference
rings. Lay it onto the in-sim steering wheel and adjust SteamVR's per-application
World Scale until the in-sim rim sits on the ring — then the world is rendering
at true 1:1.

```
wheel_ring.exe                # 381 mm ring
wheel_ring.exe -d 330         # different wheel diameter
wheel_ring.exe -d 330 -f 480  # also set the texture field span
```

**Important — world scale is a stereo-depth effect, not a flat zoom.** It changes
binocular disparity (perceived depth → perceived size), *not* the monocular
angular size of the wheel. So judge the match **with both eyes open**, and for
the precise null use **head-sway parallax** (sway left-right; matched scale =
ring and rim move together). A one-eyed "fits inside the circle" check is
invariant to world scale and will look matched at every value.

The ±% rings give a one-pass readout: see which the rim lands on, then
`new% = current% × 381 ÷ apparent_mm`. World Scale is a single per-app number and
every car is modeled to real size, so calibrate once (on a car whose real wheel
matches your reference) and it carries across all of them.

---

## Building

Each tool is a standalone Cargo project in its own folder. **Windows only** (they
use `kernel32` for dynamic DLL loading and `msvcrt` for console keyboard input).

Requirements:
- Rust toolchain, stable, MSVC target (`rustup default stable-x86_64-pc-windows-msvc`).
- SteamVR installed — it provides `openvr_api.dll`. The tools locate it via the
  loader, falling back to the runtime path in
  `%LOCALAPPDATA%\openvr\openvrpaths.vrpath`.

Build a tool:
```
cd wheel_ring
cargo build --release
```
The binary lands at `target\release\wheel_ring.exe` (same pattern for each).

Running:
- **SteamVR must be live** — overlays need its compositor. When iRacing runs in
  OpenXR, SteamVR's compositor is already up, so overlays show on top of the sim.
- Running an OpenVR overlay alongside an OpenXR app costs FPS. Fine for a
  one-time calibration; close the tool before you race.

The release profile is size-tuned (`opt-level = "z"`, LTO, single codegen unit,
symbols stripped, `panic = "abort"`) — binaries come out small with no runtime.

---

## How they work (shared design)

All three share the same thin FFI core:

- **Load** `openvr_api.dll` at runtime (`LoadLibraryA`), with the
  `openvrpaths.vrpath` runtime fallback. No link-time SteamVR dependency.
- **Init** as an overlay app: `VR_InitInternal2(…, VRApplication_Overlay, …)`,
  then `VR_GetGenericInterface("FnTable:IVROverlay_028", …)` to get the overlay
  function table.
- **Call** overlay functions by their 0-based slot in that table —
  `createOverlay` (1), `setOverlayWidthInMeters` (22),
  `setOverlayTransformAbsolute` (33), `showOverlay` (43), `setOverlayRaw` (62),
  etc. Textures are uploaded as raw straight-alpha RGBA.
- **Transforms** are `HmdMatrix34` (`[[f32;4];3]`), placed in seated or
  head-relative tracking space.
- Interactive tools read the keyboard via `msvcrt` `_kbhit`/`_getch` in a
  **non-blocking poll loop**: drain buffered keys each tick (bounded), re-upload
  the texture only when its contents actually change, and sleep ~15 ms — which
  keeps input responsive without flooding the compositor.

No frameworks, no telemetry, no background services. Each tool is the code it
needs and nothing else.