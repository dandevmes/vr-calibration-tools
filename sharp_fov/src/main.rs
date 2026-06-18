// sharp_fov — interactive sharp-FOV / blur-boundary measurement for OpenVR (Pimax Dream Air).
//
// Companion to ipd_align. Where that tool centres the lens on your pupil, this one
// measures how big the SHARP zone is — and how it moves as you change IPD (the Dream
// Air motor shifts the lens, trading edge sharpness and outer HFOV against centring).
//
// A wide test field floats in front of you, tiled with a faint fine checker so you can see
// where it goes soft. Four boundary markers (Left/Right/Top/Bottom) each carry a small
// concentric-ring emblem. Look straight AT the selected marker's rings and walk it inward
// until they stop resolving — that point is the optical sharp edge (foveating it is what
// separates lens blur from ordinary peripheral blur). Do all four, press Enter, read the
// half-angles. Re-run as you sweep IPD to find the setting that maximises sharp FOV.
//
// DEFAULT is BOTH EYES OPEN: the binocular sharp zone is centred on your forward axis, so
// it matches the field and no marker has to cross centre. This is the right method for IPD
// tuning (it's your real viewing condition).
//
// To measure ONE LENS, pass --eye left|right (+ --ipd MM): the field is shifted ~IPD/2 to
// sit in front of that eye, you close the other, and the four bars measure that lens
// nasal/temporal/up/down without anything crossing centre. (A head-centred field can't
// measure a single eye — that eye's field is centred on its own lens, not your head.)
//
// Controls (keypresses, no Enter needed):
//   Tab or 1/2/3/4 : select Left / Right / Top / Bottom (selected = yellow + halo)
//   move (screen-directional): arrows push the selected bar the way they point.
//     Left/Right bars: Left/Right or a/d     Top/Bottom bars: Up/Down or w/s
//     CAPITAL A/D/W/S = big step
//   r              : reset the selected marker back to the edge
//   Enter          : finish and print results
//   q / Esc        : quit
// Each bar moves only on its own axis (a vertical bar ignores Up/Down, etc.); the clamp
// keeps each bar on its own half so it stops at centre or the edge.
//
//   sharp_fov.exe                          # both eyes, 1.5 m distance, 3.8 m width (~+-52 deg)
//   sharp_fov.exe --eye right --ipd 58     # measure the right lens only

use std::ffi::c_void;
use std::io::Write;
use std::os::raw::c_char;

// ---- OpenVR constants (IVROverlay_028 / current runtime) ----
const IFACE: &[u8] = b"FnTable:IVROverlay_028\0";
const APP_OVERLAY: i32 = 2;
const HMD_INDEX: u32 = 0;

// IVROverlay_028 fn-table slot indices (0-based)
const I_CREATE: usize = 1;
const I_DESTROY: usize = 3;
const I_WIDTH: usize = 22;
const I_XFORM: usize = 35;
const I_SHOW: usize = 43;
const I_HIDE: usize = 44;
const I_RAW: usize = 62;

#[repr(C)]
struct HmdMatrix34 {
    m: [[f32; 4]; 3],
}

type FnCreate = unsafe extern "C" fn(*const c_char, *const c_char, *mut u64) -> i32;
type FnHandle = unsafe extern "C" fn(u64) -> i32;
type FnWidth = unsafe extern "C" fn(u64, f32) -> i32;
type FnXform = unsafe extern "C" fn(u64, u32, *const HmdMatrix34) -> i32;
type FnRaw = unsafe extern "C" fn(u64, *const c_void, u32, u32, u32) -> i32;

type FnInit = unsafe extern "C" fn(*mut i32, i32, *const c_char) -> u32;
type FnGet = unsafe extern "C" fn(*const c_char, *mut i32) -> *const c_void;
type FnShut = unsafe extern "C" fn();
type FnGetch = unsafe extern "C" fn() -> i32;
type FnKbhit = unsafe extern "C" fn() -> i32;

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryA(name: *const c_char) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *const c_void;
}

// ---- test-field texture ----
const W: i32 = 1024;
const H: i32 = 768;
const RING_R: i32 = 22; // emblem radius
const FINE: i32 = 3; // fine step (px)
const BIG: i32 = 18; // big step (px)

const WHITE: [u8; 3] = [245, 245, 245];
const BLACK: [u8; 3] = [10, 10, 12];
const CHK_A: [u8; 3] = [40, 40, 46];
const CHK_B: [u8; 3] = [62, 62, 72];
const ACT: [u8; 3] = [255, 225, 40]; // active marker
const INACT: [u8; 3] = [70, 200, 255]; // inactive marker
const CTR: [u8; 3] = [255, 60, 200]; // centre cross

fn put(buf: &mut [u8], x: i32, y: i32, c: [u8; 3]) {
    if x >= 0 && x < W && y >= 0 && y < H {
        let i = ((y * W + x) * 4) as usize;
        buf[i] = c[0];
        buf[i + 1] = c[1];
        buf[i + 2] = c[2];
        buf[i + 3] = 255;
    }
}

fn vline(buf: &mut [u8], x: i32, c: [u8; 3], ht: i32) {
    for y in 0..H {
        for t in -ht..=ht {
            put(buf, x + t, y, c);
        }
    }
}

fn hline(buf: &mut [u8], y: i32, c: [u8; 3], ht: i32) {
    for x in 0..W {
        for t in -ht..=ht {
            put(buf, x, y + t, c);
        }
    }
}

// thin circle outline, used to halo the selected marker's emblem
fn ring_outline(buf: &mut [u8], cx: i32, cy: i32, radius: i32, c: [u8; 3]) {
    let r = radius as f32;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let d = ((dx * dx + dy * dy) as f32).sqrt();
            if (d - r).abs() < 1.2 {
                put(buf, cx + dx, cy + dy, c);
            }
        }
    }
}

// concentric 2-px rings — a zone-plate target: crisp when in focus, greys out when blurred
fn rings(buf: &mut [u8], cx: i32, cy: i32) {
    for dy in -RING_R..=RING_R {
        for dx in -RING_R..=RING_R {
            let r2 = dx * dx + dy * dy;
            if r2 <= RING_R * RING_R {
                let r = (r2 as f32).sqrt();
                let band = (r as i32 / 2) % 2;
                put(buf, cx + dx, cy + dy, if band == 0 { WHITE } else { BLACK });
            }
        }
    }
}

fn build_bg() -> Vec<u8> {
    let mut buf = vec![0u8; (W * H * 4) as usize];
    for y in 0..H {
        for x in 0..W {
            let c = if ((x / 8) + (y / 8)) % 2 == 0 { CHK_A } else { CHK_B };
            put(&mut buf, x, y, c);
        }
    }
    // centre cross (0 deg reference)
    for d in -10..=10 {
        put(&mut buf, W / 2 + d, H / 2, CTR);
        put(&mut buf, W / 2, H / 2 + d, CTR);
    }
    buf
}

fn draw_frame(bg: &[u8], xl: i32, xr: i32, yt: i32, yb: i32, active: i32) -> Vec<u8> {
    let mut f = bg.to_vec();
    // inactive bars thin (1px), the selected bar bright + thick (5px)
    vline(&mut f, xl, if active == 0 { ACT } else { INACT }, if active == 0 { 2 } else { 0 });
    vline(&mut f, xr, if active == 1 { ACT } else { INACT }, if active == 1 { 2 } else { 0 });
    hline(&mut f, yt, if active == 2 { ACT } else { INACT }, if active == 2 { 2 } else { 0 });
    hline(&mut f, yb, if active == 3 { ACT } else { INACT }, if active == 3 { 2 } else { 0 });
    rings(&mut f, xl, H / 2);
    rings(&mut f, xr, H / 2);
    rings(&mut f, W / 2, yt);
    rings(&mut f, W / 2, yb);
    // yellow halo around the SELECTED emblem so it's unmistakable
    let (ax, ay) = match active {
        0 => (xl, H / 2),
        1 => (xr, H / 2),
        2 => (W / 2, yt),
        _ => (W / 2, yb),
    };
    ring_outline(&mut f, ax, ay, RING_R + 5, ACT);
    ring_outline(&mut f, ax, ay, RING_R + 7, ACT);
    f
}

// marker pixel positions -> half-angles in degrees (L, R, T, B)
fn angles(xl: i32, xr: i32, yt: i32, yb: i32, width: f32, dist: f32) -> (f64, f64, f64, f64) {
    let mw = width as f64;
    let mh = (width as f64) * (H as f64 / W as f64);
    let d = dist as f64;
    let l = ((0.5 - xl as f64 / W as f64) * mw / d).atan().to_degrees();
    let r = ((xr as f64 / W as f64 - 0.5) * mw / d).atan().to_degrees();
    let t = ((0.5 - yt as f64 / H as f64) * mh / d).atan().to_degrees();
    let b = ((yb as f64 / H as f64 - 0.5) * mh / d).atan().to_degrees();
    (l, r, t, b)
}

// ---- dll loading (same as ipd_align) ----
fn cstr(s: &str) -> Vec<c_char> {
    let mut v: Vec<c_char> = s.bytes().map(|b| b as c_char).collect();
    v.push(0);
    v
}

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
    Some(format!("{}\\bin\\win64\\openvr_api.dll", raw.replace("\\\\", "\\")))
}

unsafe fn load_lib(name: &str) -> *mut c_void {
    let n = cstr(name);
    LoadLibraryA(n.as_ptr())
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
    let mut width: f32 = 3.8;
    let mut ipd: f32 = 63.0;
    let mut eye = String::new();
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
            "--eye" => {
                i += 1;
                eye = args.get(i).cloned().unwrap_or(eye);
            }
            "--ipd" => {
                i += 1;
                ipd = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(ipd);
            }
            "-h" | "--help" => {
                println!("sharp_fov [--dist M] [--width M] [--eye left|right] [--ipd MM]");
                println!("  no --eye  : both eyes open, binocular sharp zone (default)");
                println!("  --eye R   : shift field onto that eye to measure ONE lens (close the other)");
                println!("Tab/1-4 select marker; arrows (or a/d, w/s) move it; Enter finish, q quit.");
                return;
            }
            _ => {}
        }
        i += 1;
    }

    // shift the field onto one eye for single-lens measurement; 0 (centred) for both eyes
    let lower = eye.to_lowercase();
    let xoff: f32 = if lower.starts_with('r') {
        ipd / 2000.0 // mm/2 -> metres, toward the right eye
    } else if lower.starts_with('l') {
        -ipd / 2000.0
    } else {
        0.0
    };
    let eye_label = if xoff != 0.0 {
        format!("{} eye (field shifted {:.0}mm, IPD {:.0})", lower, ipd / 2.0, ipd)
    } else {
        String::from("both eyes")
    };

    let bg = build_bg();
    // markers start near the edges
    let mut xl = (0.04 * W as f32) as i32;
    let mut xr = (0.96 * W as f32) as i32;
    let mut yt = (0.04 * H as f32) as i32;
    let mut yb = (0.96 * H as f32) as i32;
    let mut active: i32 = 0;

    unsafe {
        // openvr
        let mut h = load_lib("openvr_api.dll");
        if h.is_null() {
            if let Some(p) = runtime_dll_path() {
                h = load_lib(&p);
            }
        }
        if h.is_null() {
            die("could not load openvr_api.dll — start SteamVR or drop the dll next to this exe");
        }
        let p_init = sym(h, "VR_InitInternal2");
        let p_get = sym(h, "VR_GetGenericInterface");
        let p_shut = sym(h, "VR_ShutdownInternal");
        if p_init.is_null() || p_get.is_null() || p_shut.is_null() {
            die("openvr_api.dll missing expected exports");
        }
        let vr_init: FnInit = std::mem::transmute(p_init);
        let vr_get: FnGet = std::mem::transmute(p_get);
        let vr_shut: FnShut = std::mem::transmute(p_shut);

        // msvcrt _getch for single-key input
        let crt = load_lib("msvcrt.dll");
        if crt.is_null() {
            die("could not load msvcrt.dll for keyboard input");
        }
        let p_getch = sym(crt, "_getch");
        if p_getch.is_null() {
            die("msvcrt.dll missing _getch");
        }
        let getch: FnGetch = std::mem::transmute(p_getch);
        let p_kbhit = sym(crt, "_kbhit");
        if p_kbhit.is_null() {
            die("msvcrt.dll missing _kbhit");
        }
        let kbhit: FnKbhit = std::mem::transmute(p_kbhit);

        let mut err: i32 = 0;
        let empty = cstr("");
        vr_init(&mut err, APP_OVERLAY, empty.as_ptr());
        if err != 0 {
            die(&format!("VR_InitInternal2 failed (EVRInitError {err}) — is SteamVR running?"));
        }
        let iface = vr_get(IFACE.as_ptr() as *const c_char, &mut err);
        if iface.is_null() || err != 0 {
            vr_shut();
            die(&format!("no IVROverlay interface (err {err})"));
        }
        let tbl = iface as *const *const c_void;
        let create: FnCreate = std::mem::transmute(*tbl.add(I_CREATE));
        let destroy: FnHandle = std::mem::transmute(*tbl.add(I_DESTROY));
        let set_width: FnWidth = std::mem::transmute(*tbl.add(I_WIDTH));
        let set_xform: FnXform = std::mem::transmute(*tbl.add(I_XFORM));
        let show: FnHandle = std::mem::transmute(*tbl.add(I_SHOW));
        let hide: FnHandle = std::mem::transmute(*tbl.add(I_HIDE));
        let set_raw: FnRaw = std::mem::transmute(*tbl.add(I_RAW));

        let key = cstr("sharp.fov.field");
        let name = cstr("Sharp FOV");
        let mut handle: u64 = 0;
        if create(key.as_ptr(), name.as_ptr(), &mut handle) != 0 {
            vr_shut();
            die("CreateOverlay failed");
        }
        set_width(handle, width);
        let m = HmdMatrix34 {
            m: [[1.0, 0.0, 0.0, xoff], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, -dist]],
        };
        set_xform(handle, HMD_INDEX, &m);

        let upload = |xl, xr, yt, yb, active| {
            let frame = draw_frame(&bg, xl, xr, yt, yb, active);
            set_raw(handle, frame.as_ptr() as *const c_void, W as u32, H as u32, 4);
        };
        upload(xl, xr, yt, yb, active);
        show(handle);

        if xoff != 0.0 {
            println!("Sharp-FOV field up — {eye_label}. Close your OTHER eye; the field is centred");
            println!("on this lens. Look AT the SELECTED marker (yellow + halo) and walk it IN");
            println!("until its rings stop resolving.");
        } else {
            println!("Sharp-FOV field up — both eyes open (binocular sharp zone). Look AT the");
            println!("SELECTED marker (yellow + halo) and walk it IN until its rings stop resolving.");
        }
        println!("  select : Tab  or  1/2/3/4  (Left/Right/Top/Bottom)");
        println!("  move   : arrows push the selected bar that way --");
        println!("           L/R bars: Left/Right or a/d     T/B bars: Up/Down or w/s     (CAPS = big)");
        println!("  r = reset selected to edge    Enter = results    q = quit\n");

        let names = ["Left ", "Right", "Top  ", "Bottom"];
        let read_code = || -> i32 {
            let raw = getch();
            // arrow/extended keys send a 0/224 prefix THEN a scancode. Only read the
            // second byte if one is actually waiting, so a stray lone prefix (e.g. from a
            // key released mid-repeat) can never block the reader forever.
            if (raw == 0 || raw == 224) && kbhit() != 0 {
                1000 + getch()
            } else {
                raw
            }
        };
        // Poll loop. Each tick: redraw if changed, then drain ALL keys queued this tick
        // (bounded; read_code is kbhit-guarded so it can never block), then sleep. This
        // never blocks on input, never floods SteamVR, and a held key at a clamp just
        // drains to no-op instead of busy-spinning or backing up.
        let mut dirty = true;
        loop {
            if dirty {
                let (l, r, t, b) = angles(xl, xr, yt, yb, width, dist);
                print!(
                    "\rselected {}  |  L {:>5.1}  R {:>5.1}  T {:>5.1}  B {:>5.1}   HFOV {:>5.1}  VFOV {:>5.1}    ",
                    names[active as usize], l, r, t, b, l + r, t + b
                );
                let _ = std::io::stdout().flush();
                upload(xl, xr, yt, yb, active);
                dirty = false;
            }

            let before = (xl, xr, yt, yb, active);
            let mut quit = false;
            let mut done = false;
            let mut n = 0;
            while kbhit() != 0 && n < 512 {
                match apply(read_code(), &mut active, &mut xl, &mut xr, &mut yt, &mut yb) {
                    1 => {
                        done = true;
                        break;
                    }
                    2 => {
                        quit = true;
                        break;
                    }
                    _ => {}
                }
                n += 1;
            }
            xl = xl.clamp(2, W / 2 - 2);
            xr = xr.clamp(W / 2 + 2, W - 3);
            yt = yt.clamp(2, H / 2 - 2);
            yb = yb.clamp(H / 2 + 2, H - 3);

            if quit {
                hide(handle);
                destroy(handle);
                vr_shut();
                println!("\nquit.");
                return;
            }
            if done {
                break;
            }
            if (xl, xr, yt, yb, active) != before {
                dirty = true;
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }

        hide(handle);
        destroy(handle);
        vr_shut();

        // ---- results ----
        let (l, r, t, b) = angles(xl, xr, yt, yb, width, dist);
        let hfov = l + r;
        let vfov = t + b;
        let mw = width as f64;
        let mh = (width as f64) * (H as f64 / W as f64);
        let d = dist as f64;
        let full_h = 2.0 * (0.5 * mw / d).atan().to_degrees();
        let full_v = 2.0 * (0.5 * mh / d).atan().to_degrees();
        let frac = (hfov * vfov) / (full_h * full_v) * 100.0;

        println!("\n\n===== sharp FOV  [{eye_label}]  (note your current IPD) =====");
        println!("  sharp half-angles:  left {l:.1}deg   right {r:.1}deg   up {t:.1}deg   down {b:.1}deg");
        println!("  sharp HFOV {hfov:.1}deg   sharp VFOV {vfov:.1}deg");
        println!("  sharp box  {hfov:.1} x {vfov:.1} = {:.0} deg^2", hfov * vfov);
        println!("  = {frac:.0}% of the {full_h:.0} x {full_v:.0}deg test field  (blurry boundary {:.0}%)", 100.0 - frac);
        println!("\n  Absolute angles are the comparable numbers as you sweep IPD —");
        println!("  re-run per eye and per IPD setting and chase the largest sharp box.");
    }
}

// map one decoded key to an action; returns 1 = finish, 2 = quit, 0 = continue.
// movement is SCREEN-DIRECTIONAL: a key moves the selected bar the way the arrow points.
// Left/Right move the L or R bar; Up/Down move the T or B bar; cross-axis keys are no-ops.
fn apply(code: i32, active: &mut i32, xl: &mut i32, xr: &mut i32, yt: &mut i32, yb: &mut i32) -> u8 {
    match code {
        13 => return 1,            // Enter
        27 | 113 | 81 => return 2, // Esc / q / Q
        9 => *active = (*active + 1) % 4,
        49..=52 => *active = code - 49, // 1/2/3/4 -> 0..3
        // left:  Left arrow / a / A
        1075 | 97 => move_x(xl, xr, *active, -FINE),
        65 => move_x(xl, xr, *active, -BIG),
        // right: Right arrow / d / D
        1077 | 100 => move_x(xl, xr, *active, FINE),
        68 => move_x(xl, xr, *active, BIG),
        // up:    Up arrow / w / W
        1072 | 119 => move_y(yt, yb, *active, -FINE),
        87 => move_y(yt, yb, *active, -BIG),
        // down:  Down arrow / s / S
        1080 | 115 => move_y(yt, yb, *active, FINE),
        83 => move_y(yt, yb, *active, BIG),
        114 => reset_bar(xl, xr, yt, yb, *active), // r
        _ => {}
    }
    0
}

// Left/Right keys move the selected horizontal bar (Left=0 or Right=1) in screen-x
fn move_x(xl: &mut i32, xr: &mut i32, active: i32, d: i32) {
    match active {
        0 => *xl += d,
        1 => *xr += d,
        _ => {}
    }
}

// Up/Down keys move the selected vertical bar (Top=2 or Bottom=3) in screen-y
fn move_y(yt: &mut i32, yb: &mut i32, active: i32, d: i32) {
    match active {
        2 => *yt += d,
        3 => *yb += d,
        _ => {}
    }
}

fn reset_bar(xl: &mut i32, xr: &mut i32, yt: &mut i32, yb: &mut i32, active: i32) {
    match active {
        0 => *xl = (0.04 * W as f32) as i32,
        1 => *xr = (0.96 * W as f32) as i32,
        2 => *yt = (0.04 * H as f32) as i32,
        3 => *yb = (0.96 * H as f32) as i32,
        _ => {}
    }
}