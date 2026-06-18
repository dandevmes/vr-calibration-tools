// ipd_align — dichoptic nonius IPD / centering alignment for OpenVR (Pimax Dream Air).
// NO eye tracking. Pure rendering, your eyes do the measuring.
//
// Five targets across the lens — centre + top/bottom/left/right:
//   * CENTRE is a binocular box (both eyes see the same box). Correct IPD -> one
//     crisp box. Wrong IPD -> you see two, shifted apart. (Coarse; eyes can fuse it,
//     so trust the crosses for the fine read.)
//   * TOP/BOTTOM/LEFT/RIGHT are dichoptic '+' crosses: the left eye gets the top and
//     left strokes, the right eye the bottom and right strokes. Aligned -> each fuses
//     into a clean unbroken plus. Sideways kink in a vertical stroke = IPD (horizontal)
//     error; step in a horizontal stroke = headset height (vertical) error; uneven
//     across targets = tilt.
//
// Adjust IPD in Pimax Play (and headset height/tilt) until all five are whole.
//
// Zero dependencies. Loads openvr_api.dll at runtime and calls the IVROverlay_028
// function table directly. If your SteamVR is older/newer and the overlay fails to
// come up, bump IFACE / the I_* indices to match that header version (see notes).
//
//   ipd_align.exe                       # show overlay (dist 1.5 m, width 1.6 m)
//   ipd_align.exe --dist 2.0 --width 1.3
//   ipd_align.exe --preview out.ppm     # write the SBS texture as a PPM, no headset

use std::ffi::c_void;
use std::io::Write;
use std::os::raw::c_char;

// ----- OpenVR constants (verified against IVROverlay_028 / current runtime) -----
const IFACE: &[u8] = b"FnTable:IVROverlay_028\0";
const APP_OVERLAY: i32 = 2; // EVRApplicationType::VRApplication_Overlay
const SBS_PARALLEL: i32 = 1024; // VROverlayFlags_SideBySide_Parallel (1<<10)
                                // if the halves are swapped on your rig, use Crossed = 2048
const HMD_INDEX: u32 = 0; // k_unTrackedDeviceIndex_Hmd

// IVROverlay_028 fn-table slot indices (0-based), pulled from the generated binding.
const I_CREATE: usize = 1;
const I_DESTROY: usize = 3;
const I_FLAG: usize = 11;
const I_WIDTH: usize = 22;
const I_XFORM: usize = 35;
const I_SHOW: usize = 43;
const I_HIDE: usize = 44;
const I_RAW: usize = 62;

#[repr(C)]
struct HmdMatrix34 {
    m: [[f32; 4]; 3],
}

// overlay fn-table method signatures (extern "C" == correct ABI on x86_64-windows)
type FnCreate = unsafe extern "C" fn(*const c_char, *const c_char, *mut u64) -> i32;
type FnHandle = unsafe extern "C" fn(u64) -> i32; // destroy / show / hide
type FnFlag = unsafe extern "C" fn(u64, i32, bool) -> i32;
type FnWidth = unsafe extern "C" fn(u64, f32) -> i32;
type FnXform = unsafe extern "C" fn(u64, u32, *const HmdMatrix34) -> i32;
type FnRaw = unsafe extern "C" fn(u64, *const c_void, u32, u32, u32) -> i32;

// flat openvr_api.dll exports
type FnInit = unsafe extern "C" fn(*mut i32, i32, *const c_char) -> u32;
type FnGet = unsafe extern "C" fn(*const c_char, *mut i32) -> *const c_void;
type FnShut = unsafe extern "C" fn();

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryA(name: *const c_char) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *const c_void;
}

// ---------------- texture: nonius side-by-side ----------------
const HALF: i32 = 512; // per-eye square
const W: i32 = HALF * 2;
const H: i32 = HALF;
const SPREAD: i32 = 200; // outer targets' distance from centre
const ARM: i32 = 32; // half-length of each stroke
const GAP: i32 = 7; // central nonius break
const THICK: i32 = 2; // line half-thickness
const LOCK: i32 = 16; // centre box half-size
const CYAN: [u8; 3] = [70, 220, 255];
const ORANGE: [u8; 3] = [255, 140, 40];

fn put(buf: &mut [u8], x: i32, y: i32, c: [u8; 3]) {
    if x >= 0 && x < W && y >= 0 && y < H {
        let i = ((y * W + x) * 4) as usize;
        buf[i] = c[0];
        buf[i + 1] = c[1];
        buf[i + 2] = c[2];
        buf[i + 3] = 255;
    }
}

fn vline(buf: &mut [u8], half: i32, cx: i32, y0: i32, y1: i32, c: [u8; 3]) {
    let gx = half * HALF + cx;
    for y in y0..y1 {
        for t in -THICK..=THICK {
            put(buf, gx + t, y, c);
        }
    }
}

fn hline(buf: &mut [u8], half: i32, cy: i32, x0: i32, x1: i32, c: [u8; 3]) {
    for x in x0..x1 {
        let gx = half * HALF + x;
        for t in -THICK..=THICK {
            put(buf, gx, cy + t, c);
        }
    }
}

fn boxoutline(buf: &mut [u8], half: i32, cx: i32, cy: i32, r: i32, c: [u8; 3]) {
    for x in (cx - r)..=(cx + r) {
        put(buf, half * HALF + x, cy - r, c);
        put(buf, half * HALF + x, cy + r, c);
    }
    for y in (cy - r)..=(cy + r) {
        put(buf, half * HALF + cx - r, y, c);
        put(buf, half * HALF + cx + r, y, c);
    }
}

fn build_texture() -> Vec<u8> {
    let mut buf = vec![0u8; (W * H * 4) as usize];
    let c = HALF / 2;
    // outer targets: (cx, cy)
    let pts = [
        (c, c - SPREAD), // top
        (c, c + SPREAD), // bottom
        (c - SPREAD, c), // left
        (c + SPREAD, c), // right
    ];
    for (cx, cy) in pts {
        // vertical stroke: top half -> LEFT eye, bottom half -> RIGHT eye  (kink = IPD)
        vline(&mut buf, 0, cx, cy - ARM, cy - GAP, CYAN);
        vline(&mut buf, 1, cx, cy + GAP, cy + ARM, CYAN);
        // horizontal stroke: left half -> LEFT eye, right half -> RIGHT eye (step = height)
        hline(&mut buf, 0, cy, cx - ARM, cx - GAP, CYAN);
        hline(&mut buf, 1, cy, cx + GAP, cx + ARM, CYAN);
    }
    // centre binocular fusion box (identical to both eyes)
    boxoutline(&mut buf, 0, c, c, LOCK, ORANGE);
    boxoutline(&mut buf, 1, c, c, LOCK, ORANGE);
    buf
}

// ---------------- dll loading ----------------
fn cstr(s: &str) -> Vec<c_char> {
    let mut v: Vec<c_char> = s.bytes().map(|b| b as c_char).collect();
    v.push(0);
    v
}

// fall back to the runtime path recorded in openvrpaths.vrpath if the dll isn't on PATH
fn runtime_dll_path() -> Option<String> {
    let local = std::env::var("LOCALAPPDATA").ok()?;
    let vp = format!("{}\\openvr\\openvrpaths.vrpath", local);
    let txt = std::fs::read_to_string(vp).ok()?;
    let i = txt.find("\"runtime\"")?;
    let rest = &txt[i..];
    let lb = rest.find('[')?;
    let after = &rest[lb..];
    let q1 = after.find('"')?;
    let q2 = after[q1 + 1..].find('"')?;
    let raw = &after[q1 + 1..q1 + 1 + q2];
    let unescaped = raw.replace("\\\\", "\\");
    Some(format!("{}\\bin\\win64\\openvr_api.dll", unescaped))
}

unsafe fn load_openvr() -> *mut c_void {
    let n = cstr("openvr_api.dll");
    let mut h = LoadLibraryA(n.as_ptr());
    if h.is_null() {
        if let Some(p) = runtime_dll_path() {
            let n2 = cstr(&p);
            h = LoadLibraryA(n2.as_ptr());
        }
    }
    h
}

unsafe fn sym(h: *mut c_void, name: &str) -> *const c_void {
    let n = cstr(name);
    GetProcAddress(h, n.as_ptr())
}

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1);
}

fn main() {
    // ---- args ----
    let mut dist: f32 = 1.5;
    let mut width: f32 = 1.6;
    let mut preview: Option<String> = None;
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--dist" => {
                i += 1;
                dist = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(dist);
            }
            "--width" => {
                i += 1;
                width = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(width);
            }
            "--preview" => {
                i += 1;
                preview = args.get(i).cloned();
            }
            "-h" | "--help" => {
                println!("ipd_align [--dist M] [--width M] [--preview out.ppm]");
                return;
            }
            _ => {}
        }
        i += 1;
    }

    let tex = build_texture();

    // ---- preview path: write SBS texture as PPM, no VR ----
    if let Some(path) = preview {
        let mut rgb = Vec::with_capacity((W * H * 3) as usize);
        for px in tex.chunks_exact(4) {
            rgb.extend_from_slice(&px[0..3]);
        }
        let mut f = std::fs::File::create(&path).unwrap_or_else(|e| die(&format!("create {path}: {e}")));
        let _ = write!(f, "P6\n{W} {H}\n255\n");
        let _ = f.write_all(&rgb);
        println!("wrote {path} ({W}x{H} PPM)");
        return;
    }

    // ---- run overlay ----
    unsafe {
        let h = load_openvr();
        if h.is_null() {
            die("could not load openvr_api.dll — start SteamVR, or drop openvr_api.dll next to this exe");
        }
        let p_init = sym(h, "VR_InitInternal2");
        let p_get = sym(h, "VR_GetGenericInterface");
        let p_shut = sym(h, "VR_ShutdownInternal");
        if p_init.is_null() || p_get.is_null() || p_shut.is_null() {
            die("openvr_api.dll is missing expected exports");
        }
        let vr_init: FnInit = std::mem::transmute(p_init);
        let vr_get: FnGet = std::mem::transmute(p_get);
        let vr_shut: FnShut = std::mem::transmute(p_shut);

        let mut err: i32 = 0;
        let empty = cstr("");
        vr_init(&mut err, APP_OVERLAY, empty.as_ptr());
        if err != 0 {
            die(&format!("VR_InitInternal2 failed (EVRInitError {err}) — is SteamVR running?"));
        }

        let iface = vr_get(IFACE.as_ptr() as *const c_char, &mut err);
        if iface.is_null() || err != 0 {
            vr_shut();
            die(&format!(
                "no IVROverlay interface (err {err}); your runtime may use a different version than {}",
                std::str::from_utf8(&IFACE[8..IFACE.len() - 1]).unwrap_or("?")
            ));
        }
        let tbl = iface as *const *const c_void;

        let create: FnCreate = std::mem::transmute(*tbl.add(I_CREATE));
        let destroy: FnHandle = std::mem::transmute(*tbl.add(I_DESTROY));
        let set_flag: FnFlag = std::mem::transmute(*tbl.add(I_FLAG));
        let set_width: FnWidth = std::mem::transmute(*tbl.add(I_WIDTH));
        let set_xform: FnXform = std::mem::transmute(*tbl.add(I_XFORM));
        let show: FnHandle = std::mem::transmute(*tbl.add(I_SHOW));
        let hide: FnHandle = std::mem::transmute(*tbl.add(I_HIDE));
        let set_raw: FnRaw = std::mem::transmute(*tbl.add(I_RAW));

        let key = cstr("ipd.align.nonius");
        let name = cstr("IPD Align");
        let mut handle: u64 = 0;
        let e = create(key.as_ptr(), name.as_ptr(), &mut handle);
        if e != 0 {
            vr_shut();
            die(&format!("CreateOverlay failed (EVROverlayError {e})"));
        }

        set_raw(
            handle,
            tex.as_ptr() as *const c_void,
            W as u32,
            H as u32,
            4,
        );
        set_flag(handle, SBS_PARALLEL, true);
        set_width(handle, width);

        // head-locked, `dist` metres straight ahead (OpenVR is right-handed, -z forward)
        let m = HmdMatrix34 {
            m: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, -dist],
            ],
        };
        set_xform(handle, HMD_INDEX, &m);
        show(handle);

        println!("Overlay up at {dist:.2} m. Look at each of the five targets and adjust the IPD");
        println!("in Pimax Play until every '+' is a clean unbroken plus and the centre box is");
        println!("single. Vertical kink = IPD; horizontal step = headset height; uneven = tilt.");
        println!("(If the targets diverge no matter what, the texture is crossed on your rig —");
        println!(" rebuild with SBS_PARALLEL changed to 2048.)");
        println!("\nPress Enter to exit.");
        let mut s = String::new();
        let _ = std::io::stdin().read_line(&mut s);

        hide(handle);
        destroy(handle);
        vr_shut();
    }
}