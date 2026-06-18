// wheel_ring — true-metric steering-wheel reference ring as an OpenVR overlay.
//
// Renders a crisp ring of a known real diameter (default 381 mm = 15" Max Papis
// sprint wheel) in seated tracking space, plus faint +/-5% and +/-10% reference
// rings so you can read a world-scale error directly. Tilt/position it onto the
// in-sim wheel, then judge alignment WITH BOTH EYES (world scale is a stereo-depth
// effect; a one-eyed flat fit is invariant to it). Null head-sway parallax for the
// precise match.
//
// Zero crates. Talks to openvr_api.dll directly via the IVROverlay_028 fn-table.
// Build:  cargo build --release   ->   target\release\wheel_ring.exe
// Run while SteamVR is up (iRacing in OpenXR keeps SteamVR's compositor live).

#![allow(non_snake_case, non_camel_case_types)]

use std::ffi::{c_char, c_void, CString};
use std::mem::transmute;
use std::ptr;

#[repr(C)]
#[derive(Clone, Copy)]
struct HmdMatrix34 {
    m: [[f32; 4]; 3],
}

// ---------------- Win32 / CRT FFI ----------------
#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryA(name: *const c_char) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
}
#[link(name = "msvcrt")]
extern "C" {
    fn _kbhit() -> i32;
    fn _getch() -> i32;
}

// ---------------- OpenVR flat exports ----------------
type FnInitInternal2 = unsafe extern "C" fn(*mut i32, i32, *const c_char) -> u32;
type FnGetGenericInterface = unsafe extern "C" fn(*const c_char, *mut i32) -> *const c_void;
type FnShutdownInternal = unsafe extern "C" fn();

// ---------------- IVROverlay_028 fn-table slots ----------------
type FnCreateOverlay = unsafe extern "C" fn(*const c_char, *const c_char, *mut u64) -> u32;
type FnDestroyOverlay = unsafe extern "C" fn(u64) -> u32;
type FnSetWidth = unsafe extern "C" fn(u64, f32) -> u32;
type FnSetRaw = unsafe extern "C" fn(u64, *const c_void, u32, u32, u32) -> u32;
type FnSetXformAbs = unsafe extern "C" fn(u64, i32, *const HmdMatrix34) -> u32;
type FnShow = unsafe extern "C" fn(u64) -> u32;
type FnSetColorSpace = unsafe extern "C" fn(u64, i32) -> u32;
type FnSetAlpha = unsafe extern "C" fn(u64, f32) -> u32;
type FnSetColor = unsafe extern "C" fn(u64, f32, f32, f32) -> u32;

const SLOT_CREATE: usize = 1;
const SLOT_DESTROY: usize = 3;
const SLOT_SETCOLOR: usize = 14;
const SLOT_SETALPHA: usize = 16;
const SLOT_SETWIDTH: usize = 22;
const SLOT_COLORSPACE: usize = 28;
const SLOT_XFORM_ABS: usize = 33;
const SLOT_SHOW: usize = 43;
const SLOT_SETRAW: usize = 62;

const APP_OVERLAY: i32 = 2; // EVRApplicationType_VRApplication_Overlay
const UNIVERSE_SEATED: i32 = 0; // ETrackingUniverseOrigin_TrackingUniverseSeated
const N: usize = 1024; // texture side

unsafe fn slot(table: *const c_void, i: usize) -> *const c_void {
    *((table as *const *const c_void).add(i))
}

unsafe fn proc_addr(dll: *mut c_void, name: &str) -> *mut c_void {
    let c = CString::new(name).unwrap();
    GetProcAddress(dll, c.as_ptr())
}

// Resolve openvr_api.dll: try the loader path, then the SteamVR runtime recorded
// in %LOCALAPPDATA%\openvr\openvrpaths.vrpath.
unsafe fn load_openvr() -> *mut c_void {
    let direct = CString::new("openvr_api.dll").unwrap();
    let h = LoadLibraryA(direct.as_ptr());
    if !h.is_null() {
        return h;
    }
    if let Some(p) = runtime_dll_path() {
        if let Ok(c) = CString::new(p) {
            let h2 = LoadLibraryA(c.as_ptr());
            if !h2.is_null() {
                return h2;
            }
        }
    }
    ptr::null_mut()
}

fn runtime_dll_path() -> Option<String> {
    let local = std::env::var("LOCALAPPDATA").ok()?;
    let s = std::fs::read_to_string(format!("{}\\openvr\\openvrpaths.vrpath", local)).ok()?;
    let key = s.find("\"runtime\"")?;
    let after_colon = key + s[key..].find(':')?;
    let q_start = after_colon + s[after_colon..].find('"')? + 1;
    let q_end = q_start + s[q_start..].find('"')?;
    let raw = &s[q_start..q_end];
    let path = raw.replace("\\\\", "\\");
    Some(format!("{}\\bin\\win64\\openvr_api.dll", path))
}

// Rotation = Ry(yaw) * Rx(pitch), translation (x,y,z). Overlay normal is +Z.
fn make_matrix(x: f32, y: f32, z: f32, pitch: f32, yaw: f32) -> HmdMatrix34 {
    let (sp, cp) = (pitch.sin(), pitch.cos());
    let (sy, cy) = (yaw.sin(), yaw.cos());
    HmdMatrix34 {
        m: [
            [cy, sy * sp, sy * cp, x],
            [0.0, cp, -sp, y],
            [-sy, cy * sp, cy * cp, z],
        ],
    }
}

// Straight-alpha RGBA. Bright green primary ring at `ring_mm` diameter; blue rings
// at -5/-10% (in-sim wheel smaller than reference), red at +5/+10% (larger); faint
// white center cross. Everything else transparent.
fn gen_texture(field_mm: f32, ring_mm: f32) -> Vec<u8> {
    let mut buf = vec![0u8; N * N * 4];
    let c = (N as f32 - 1.0) / 2.0;
    let mm_per_px = field_mm / N as f32;
    let r_main = ring_mm / 2.0;
    let refs: [f32; 4] = [-10.0, -5.0, 5.0, 10.0];
    for py in 0..N {
        for px in 0..N {
            let dx = px as f32 - c;
            let dy = py as f32 - c;
            let r_mm = (dx * dx + dy * dy).sqrt() * mm_per_px;
            let i = (py * N + px) * 4;

            if (r_mm - r_main).abs() < 1.5 {
                buf[i] = 0;
                buf[i + 1] = 255;
                buf[i + 2] = 120;
                buf[i + 3] = 255;
                continue;
            }
            let mut drew = false;
            for &p in refs.iter() {
                let rr = r_main * (1.0 + p / 100.0);
                if (r_mm - rr).abs() < 1.0 {
                    if p < 0.0 {
                        buf[i] = 80;
                        buf[i + 1] = 160;
                        buf[i + 2] = 255;
                        buf[i + 3] = 165;
                    } else {
                        buf[i] = 255;
                        buf[i + 1] = 90;
                        buf[i + 2] = 90;
                        buf[i + 3] = 165;
                    }
                    drew = true;
                    break;
                }
            }
            if drew {
                continue;
            }
            if (dx.abs() < 1.0 || dy.abs() < 1.0) && r_mm < r_main * 1.12 {
                buf[i] = 255;
                buf[i + 1] = 255;
                buf[i + 2] = 255;
                buf[i + 3] = 80;
            }
        }
    }
    buf
}

fn main() {
    let mut ring_mm = 381.0f32; // 15" Max Papis sprint wheel
    let mut field_mm = 520.0f32; // texture spans this; must exceed ring*1.10
    let mut a = std::env::args().skip(1);
    while let Some(arg) = a.next() {
        match arg.as_str() {
            "--diameter" | "-d" => {
                ring_mm = a.next().and_then(|v| v.parse().ok()).unwrap_or(ring_mm)
            }
            "--field" | "-f" => field_mm = a.next().and_then(|v| v.parse().ok()).unwrap_or(field_mm),
            "--help" | "-h" => {
                print_help();
                return;
            }
            _ => {}
        }
    }
    if field_mm < ring_mm * 1.15 {
        field_mm = ring_mm * 1.15;
    }
    unsafe { run(ring_mm, field_mm) }
}

fn print_help() {
    println!("wheel_ring [--diameter MM] [--field MM]");
    println!("  default diameter 381mm (15\" sprint wheel), field auto >= 1.15x diameter");
}

unsafe fn run(ring_mm0: f32, field_mm: f32) {
    let dll = load_openvr();
    if dll.is_null() {
        eprintln!("could not load openvr_api.dll (is SteamVR installed/running?)");
        return;
    }
    let p_init = proc_addr(dll, "VR_InitInternal2");
    let p_geti = proc_addr(dll, "VR_GetGenericInterface");
    let p_shut = proc_addr(dll, "VR_ShutdownInternal");
    if p_init.is_null() || p_geti.is_null() || p_shut.is_null() {
        eprintln!("openvr exports missing");
        return;
    }
    let vr_init: FnInitInternal2 = transmute(p_init);
    let vr_geti: FnGetGenericInterface = transmute(p_geti);
    let vr_shut: FnShutdownInternal = transmute(p_shut);

    let mut err: i32 = 0;
    let _tok = vr_init(&mut err, APP_OVERLAY, ptr::null());
    if err != 0 {
        eprintln!("VR_InitInternal2 failed ({}). Start SteamVR first.", err);
        return;
    }

    let iface = CString::new("FnTable:IVROverlay_028").unwrap();
    let mut e2: i32 = 0;
    let table = vr_geti(iface.as_ptr(), &mut e2);
    if table.is_null() {
        eprintln!("IVROverlay_028 unavailable ({})", e2);
        vr_shut();
        return;
    }

    let createOverlay: FnCreateOverlay = transmute(slot(table, SLOT_CREATE));
    let destroyOverlay: FnDestroyOverlay = transmute(slot(table, SLOT_DESTROY));
    let setColor: FnSetColor = transmute(slot(table, SLOT_SETCOLOR));
    let setAlpha: FnSetAlpha = transmute(slot(table, SLOT_SETALPHA));
    let setWidth: FnSetWidth = transmute(slot(table, SLOT_SETWIDTH));
    let setColorSpace: FnSetColorSpace = transmute(slot(table, SLOT_COLORSPACE));
    let setXform: FnSetXformAbs = transmute(slot(table, SLOT_XFORM_ABS));
    let showOverlay: FnShow = transmute(slot(table, SLOT_SHOW));
    let setRaw: FnSetRaw = transmute(slot(table, SLOT_SETRAW));

    let key = CString::new("dandevmes.wheelring").unwrap();
    let name = CString::new("Wheel Ring").unwrap();
    let mut handle: u64 = 0;
    let r = createOverlay(key.as_ptr(), name.as_ptr(), &mut handle);
    if r != 0 {
        eprintln!("createOverlay error {}", r);
        vr_shut();
        return;
    }

    setWidth(handle, field_mm / 1000.0);
    setColorSpace(handle, 0); // Auto
    setColor(handle, 1.0, 1.0, 1.0);
    setAlpha(handle, 1.0);

    let mut ring = ring_mm0;
    let tex = gen_texture(field_mm, ring);
    setRaw(handle, tex.as_ptr() as *const c_void, N as u32, N as u32, 4);

    // calibrated start pose (lands on the wheel); nudge from here with the keys
    let mut x = 0.465f32;
    let mut y = 0.045f32;
    let mut z = 0.220f32;
    let mut pitch = (-116.5f32).to_radians();
    let mut yaw = 88.5f32.to_radians();
    let m = make_matrix(x, y, z, pitch, yaw);
    setXform(handle, UNIVERSE_SEATED, &m);
    showOverlay(handle);

    controls();
    print_state(ring, x, y, z, pitch, yaw);

    let t_small = 0.005f32;
    let t_big = 0.02f32;
    let r_small = 0.5f32.to_radians();
    let r_big = 2.0f32.to_radians();

    loop {
        let mut moved = false;
        let mut retex = false;
        let mut quit = false;

        // drain buffered keys this tick (bounded), so held keys don't lag
        let mut guard = 0;
        while _kbhit() != 0 && guard < 256 {
            guard += 1;
            let c = _getch();
            if c == 0 || c == 224 {
                // arrow/function prefix; second byte should be ready
                if _kbhit() == 0 {
                    break;
                }
                match _getch() {
                    72 => {
                        pitch += r_small;
                        moved = true;
                    } // up
                    80 => {
                        pitch -= r_small;
                        moved = true;
                    } // down
                    75 => {
                        yaw -= r_small;
                        moved = true;
                    } // left
                    77 => {
                        yaw += r_small;
                        moved = true;
                    } // right
                    73 => {
                        pitch += r_big;
                        moved = true;
                    } // PageUp  - coarse pitch
                    81 => {
                        pitch -= r_big;
                        moved = true;
                    } // PageDn  - coarse pitch
                    71 => {
                        yaw -= r_big;
                        moved = true;
                    } // Home    - coarse yaw
                    79 => {
                        yaw += r_big;
                        moved = true;
                    } // End     - coarse yaw
                    _ => {}
                }
                continue;
            }
            match c as u8 as char {
                'w' => {
                    y += t_small;
                    moved = true;
                }
                'W' => {
                    y += t_big;
                    moved = true;
                }
                's' => {
                    y -= t_small;
                    moved = true;
                }
                'S' => {
                    y -= t_big;
                    moved = true;
                }
                'a' => {
                    x -= t_small;
                    moved = true;
                }
                'A' => {
                    x -= t_big;
                    moved = true;
                }
                'd' => {
                    x += t_small;
                    moved = true;
                }
                'D' => {
                    x += t_big;
                    moved = true;
                }
                'r' => {
                    z -= t_small;
                    moved = true;
                } // away from face
                'R' => {
                    z -= t_big;
                    moved = true;
                }
                'f' => {
                    z += t_small;
                    moved = true;
                } // toward face
                'F' => {
                    z += t_big;
                    moved = true;
                }
                '[' => {
                    ring -= 1.0;
                    retex = true;
                }
                '{' => {
                    ring -= 10.0;
                    retex = true;
                }
                ']' => {
                    ring += 1.0;
                    retex = true;
                }
                '}' => {
                    ring += 10.0;
                    retex = true;
                }
                'p' | 'P' => moved = true, // just reprint
                'q' | 'Q' | '\u{1b}' => {
                    quit = true;
                    break;
                }
                _ => {}
            }
        }

        if quit {
            break;
        }
        if retex {
            ring = ring.clamp(50.0, field_mm / 1.15);
            let t = gen_texture(field_mm, ring);
            setRaw(handle, t.as_ptr() as *const c_void, N as u32, N as u32, 4);
            moved = true;
        }
        if moved {
            let m = make_matrix(x, y, z, pitch, yaw);
            setXform(handle, UNIVERSE_SEATED, &m);
            print_state(ring, x, y, z, pitch, yaw);
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }

    destroyOverlay(handle);
    vr_shut();
    println!("closed.");
}

fn controls() {
    println!("=== wheel_ring ===");
    println!("  arrows  tilt fine (Up/Dn pitch, L/R yaw)   [hold-repeat ok]");
    println!("  PgUp/Dn pitch coarse     Home/End  yaw coarse");
    println!("  w/s     up / down        a/d  left / right");
    println!("  r/f     away / toward     (SHIFT = big step)");
    println!("  [ ]     ring -/+ 1mm      {{ }}  ring -/+ 10mm");
    println!("  p       print state       q/Esc  quit");
    println!("  -> judge with BOTH eyes; null head-sway parallax for the exact match.");
    println!("  -> rim on +N% red ring => world over-scaled; new% = cur% * 381 / apparent_mm");
}

fn print_state(ring: f32, x: f32, y: f32, z: f32, pitch: f32, yaw: f32) {
    println!(
        "ring {:.0}mm  pos[{:+.3} {:+.3} {:+.3}]  pitch {:+.1}  yaw {:+.1}",
        ring,
        x,
        y,
        z,
        pitch.to_degrees(),
        yaw.to_degrees()
    );
}