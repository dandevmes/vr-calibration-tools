# ipd_align (Rust)

Dichoptic nonius IPD / centering alignment overlay for OpenVR — Pimax Dream Air.
No eye tracking. Pure rendering; your eyes do the measuring.

Five targets across the lens (centre box + 4 nonius crosses). Adjust IPD in Pimax
Play (and headset height/tilt) until every `+` is a clean unbroken plus and the
centre box is single. Vertical kink = IPD error; horizontal step = height; uneven
across targets = tilt.

## Build

Needs the Rust toolchain with the MSVC backend (you already have MSVC from the PVR
shim). No crates — pure std + a dynamic load of `openvr_api.dll`.

```
cargo build --release
```

Output: `target\release\ipd_align.exe` — one file, ~300 KB, no runtime, no unpack.

## Run

```
ipd_align.exe                      # default: 1.5 m distance, 1.6 m per-eye width
ipd_align.exe --dist 2.0 --width 1.3
ipd_align.exe --preview out.ppm    # write the SBS texture to a PPM, no headset needed
```

Start SteamVR first. The overlay comes up head-locked; press Enter in the console
to exit (it hides, destroys the overlay, and shuts down cleanly).

## openvr_api.dll

The exe loads `openvr_api.dll` at runtime. It tries, in order:

1. `openvr_api.dll` on the normal search path (works if SteamVR's `bin\win64` is on
   PATH, or if you drop the DLL next to the exe), then
2. the runtime path recorded in `%LOCALAPPDATA%\openvr\openvrpaths.vrpath`
   (`<runtime>\bin\win64\openvr_api.dll`).

If it can't find it, copy `openvr_api.dll` from
`...\SteamVR\bin\win64\openvr_api.dll` next to `ipd_align.exe`. That's the only
external file, and it's Valve's runtime shim — there's no way to talk to SteamVR
without it.

## Two caveats baked into the source

* **Stereo orientation.** It sets `SideBySide_Parallel` (left texture half -> left
  eye). If on your rig the targets diverge no matter the IPD, the runtime is treating
  the texture as crossed — change `SBS_PARALLEL` to `2048` (Crossed) in `main.rs` and
  rebuild.
* **Interface version.** Pinned to `IVROverlay_028` with the fn-table slot indices
  for that version (`I_CREATE`, `I_RAW`, etc.). This matches current SteamVR. If a
  future/older runtime rejects the interface, the exe says so on startup; bump `IFACE`
  and re-derive the `I_*` indices from that header's IVROverlay declaration order
  (0-based, counting every method from `findOverlay`).

Note: this wasn't compiled in the environment it was written in, so you're the first
compile — if `cargo build` flags anything it'll be a trivial fix, the OpenVR ABI
itself is correct (verified against the generated binding that already runs on your
machine).