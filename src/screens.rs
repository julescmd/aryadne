//! Les écrans branchés. Sert aux fenêtres détachées : une fenêtre rangée sur un écran qui n'est plus là
//! revient sur celui de la fenêtre principale. egui ne donne pas la liste des écrans : on la demande
//! au système (Xinerama sous Linux, user32 sous Windows). Ailleurs, la liste est inconnue et rien ne bouge.

use eframe::egui::{pos2, vec2, Pos2, Rect, Vec2};

/// Les écrans, en pixels du bureau. None : ce système ne le dit pas.
#[cfg(target_os = "linux")]
fn query() -> Option<Vec<Rect>> {
    use std::ffi::{c_char, c_int, c_void};
    #[repr(C)]
    struct Info {
        screen: c_int,
        x: i16,
        y: i16,
        w: i16,
        h: i16,
    }
    // Les bibliothèques restent chargées : on les rouvre à chaque lecture sinon
    static LIBS: std::sync::LazyLock<Option<(libloading::Library, Option<libloading::Library>)>> = std::sync::LazyLock::new(|| unsafe {
        let x11 = libloading::Library::new("libX11.so.6").ok()?;
        Some((x11, libloading::Library::new("libXinerama.so.1").ok()))
    });
    let (x11, xin) = LIBS.as_ref()?;
    unsafe {
        let open: libloading::Symbol<unsafe extern "C" fn(*const c_char) -> *mut c_void> = x11.get(b"XOpenDisplay\0").ok()?;
        let close: libloading::Symbol<unsafe extern "C" fn(*mut c_void) -> c_int> = x11.get(b"XCloseDisplay\0").ok()?;
        let free: libloading::Symbol<unsafe extern "C" fn(*mut c_void) -> c_int> = x11.get(b"XFree\0").ok()?;
        let dpy = open(std::ptr::null());
        if dpy.is_null() {
            return None;
        }
        let mut out = Vec::new();
        if let Some(xin) = xin {
            let active: Option<libloading::Symbol<unsafe extern "C" fn(*mut c_void) -> c_int>> = xin.get(b"XineramaIsActive\0").ok();
            let list: Option<libloading::Symbol<unsafe extern "C" fn(*mut c_void, *mut c_int) -> *mut Info>> = xin.get(b"XineramaQueryScreens\0").ok();
            if let (Some(active), Some(list)) = (active, list) {
                if active(dpy) != 0 {
                    let mut n: c_int = 0;
                    let p = list(dpy, &mut n);
                    if !p.is_null() {
                        for i in 0..n.max(0) as usize {
                            let s = &*p.add(i);
                            out.push(Rect::from_min_size(pos2(s.x as f32, s.y as f32), vec2(s.w as f32, s.h as f32)));
                        }
                        free(p as *mut c_void);
                    }
                }
            }
        }
        if out.is_empty() {
            // Pas de Xinerama : un seul écran, de la taille du bureau
            let screen: Option<libloading::Symbol<unsafe extern "C" fn(*mut c_void) -> c_int>> = x11.get(b"XDefaultScreen\0").ok();
            let w: Option<libloading::Symbol<unsafe extern "C" fn(*mut c_void, c_int) -> c_int>> = x11.get(b"XDisplayWidth\0").ok();
            let h: Option<libloading::Symbol<unsafe extern "C" fn(*mut c_void, c_int) -> c_int>> = x11.get(b"XDisplayHeight\0").ok();
            if let (Some(screen), Some(w), Some(h)) = (screen, w, h) {
                let s = screen(dpy);
                out.push(Rect::from_min_size(Pos2::ZERO, vec2(w(dpy, s) as f32, h(dpy, s) as f32)));
            }
        }
        close(dpy);
        (!out.is_empty()).then_some(out)
    }
}

#[cfg(target_os = "windows")]
fn query() -> Option<Vec<Rect>> {
    use std::ffi::c_void;
    #[repr(C)]
    struct WinRect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    type Callback = unsafe extern "system" fn(*mut c_void, *mut c_void, *mut WinRect, isize) -> i32;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumDisplayMonitors(hdc: *mut c_void, clip: *const WinRect, callback: Option<Callback>, data: isize) -> i32;
    }
    unsafe extern "system" fn each(_monitor: *mut c_void, _hdc: *mut c_void, r: *mut WinRect, data: isize) -> i32 {
        unsafe {
            let out = &mut *(data as *mut Vec<Rect>);
            if !r.is_null() {
                let r = &*r;
                out.push(Rect::from_min_max(pos2(r.left as f32, r.top as f32), pos2(r.right as f32, r.bottom as f32)));
            }
        }
        1
    }
    let mut out: Vec<Rect> = Vec::new();
    unsafe {
        EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(each), &mut out as *mut Vec<Rect> as isize);
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn query() -> Option<Vec<Rect>> {
    None
}

/// Les écrans, en points d'interface (`ppp` : pixels par point). Relus au plus une fois par seconde.
pub fn list(ppp: f32) -> Option<Vec<Rect>> {
    static CACHE: std::sync::Mutex<Option<(std::time::Instant, Option<Vec<Rect>>)>> = std::sync::Mutex::new(None);
    let mut g = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if g.as_ref().is_none_or(|(t, _)| t.elapsed().as_secs_f32() > 1.0) {
        *g = Some((std::time::Instant::now(), query()));
    }
    let ppp = ppp.max(0.1);
    g.as_ref().and_then(|(_, s)| s.as_ref().map(|v| v.iter().map(|r| Rect::from_min_max((r.min.to_vec2() / ppp).to_pos2(), (r.max.to_vec2() / ppp).to_pos2())).collect()))
}

/// La barre de titre d'une fenêtre (en haut de `pos`, large de `size`) reste-t-elle attrapable sur un écran ?
pub fn reachable(pos: Pos2, size: Vec2, screens: &[Rect]) -> bool {
    let bar = Rect::from_min_size(pos, vec2(size.x.max(1.0), 30.0));
    screens.iter().any(|s| {
        let i = s.intersect(bar);
        i.width() >= 60.0f32.min(bar.width()) && i.height() >= 12.0
    })
}

/// Où remettre une fenêtre dont l'écran a disparu : sur l'écran de la fenêtre principale (`home`), décalée
/// de `n` crans pour que plusieurs fenêtres ramenées ne se cachent pas. None : elle est bien où elle est.
pub fn rescue(pos: Pos2, size: Vec2, screens: &[Rect], home: Rect, n: usize) -> Option<Pos2> {
    if screens.is_empty() || reachable(pos, size, screens) {
        return None;
    }
    // L'écran principal : celui qui montre le plus la fenêtre principale, sinon le premier
    let area = |r: Rect| if r.is_positive() { r.width() * r.height() } else { 0.0 };
    let main = screens.iter().copied().max_by(|a, b| area(a.intersect(home)).total_cmp(&area(b.intersect(home)))).filter(|s| area(s.intersect(home)) > 0.0).unwrap_or(screens[0]);
    let step = 28.0 * (n % 8) as f32;
    let want = if area(main.intersect(home)) > 0.0 { home.min + vec2(60.0 + step, 60.0 + step) } else { main.min + vec2(60.0 + step, 60.0 + step) };
    // Toute la fenêtre tient dans l'écran si elle peut
    let x = want.x.min(main.right() - size.x).max(main.left());
    let y = want.y.min(main.bottom() - size.y - 30.0).max(main.top());
    Some(pos2(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(pos2(x, y), vec2(w, h))
    }

    #[test]
    fn fenetre_sur_un_ecran_reste() {
        let two = [r(0.0, 0.0, 1920.0, 1080.0), r(1920.0, 0.0, 1920.0, 1080.0)];
        let home = r(100.0, 100.0, 1200.0, 800.0);
        // Sur le deuxième écran : on n'y touche pas
        assert_eq!(rescue(pos2(2200.0, 200.0), vec2(400.0, 300.0), &two, home, 0), None);
        // À cheval sur les deux, ou qui dépasse en bas : toujours attrapable
        assert_eq!(rescue(pos2(1800.0, 900.0), vec2(400.0, 300.0), &two, home, 0), None);
        // Écran à gauche du principal (coordonnées négatives)
        let left = [r(-1280.0, 0.0, 1280.0, 1024.0), r(0.0, 0.0, 1920.0, 1080.0)];
        assert_eq!(rescue(pos2(-900.0, 300.0), vec2(400.0, 300.0), &left, home, 0), None);
    }

    #[test]
    fn ecran_disparu_ramene_sur_le_principal() {
        let one = [r(0.0, 0.0, 1920.0, 1080.0)];
        let home = r(100.0, 100.0, 1200.0, 800.0);
        // La fenêtre était sur un deuxième écran, débranché depuis
        let p = rescue(pos2(2200.0, 200.0), vec2(400.0, 300.0), &one, home, 0).unwrap();
        assert!(one[0].contains_rect(Rect::from_min_size(p, vec2(400.0, 300.0))));
        assert!(reachable(p, vec2(400.0, 300.0), &one));
        // Plusieurs fenêtres ramenées : décalées l'une de l'autre
        let q = rescue(pos2(2300.0, 300.0), vec2(400.0, 300.0), &one, home, 1).unwrap();
        assert!(p != q);
        // Presque sortie par le haut ou la gauche : barre de titre perdue, on la ramène aussi
        assert!(rescue(pos2(300.0, -200.0), vec2(400.0, 300.0), &one, home, 0).is_some());
        assert!(rescue(pos2(-380.0, 300.0), vec2(400.0, 300.0), &one, home, 0).is_some());
        // Une fenêtre plus grande que l'écran reste accrochée à son coin
        let big = rescue(pos2(5000.0, 5000.0), vec2(3000.0, 2000.0), &one, home, 0).unwrap();
        assert_eq!(big, pos2(0.0, 0.0));
    }

    #[test]
    fn principale_hors_champ_prend_le_premier_ecran() {
        let one = [r(0.0, 0.0, 1920.0, 1080.0)];
        let p = rescue(pos2(9000.0, 100.0), vec2(400.0, 300.0), &one, r(9000.0, 0.0, 800.0, 600.0), 0).unwrap();
        assert!(one[0].contains(p));
        // Liste inconnue : on ne bouge rien
        assert_eq!(rescue(pos2(9000.0, 100.0), vec2(400.0, 300.0), &[], r(0.0, 0.0, 800.0, 600.0), 0), None);
    }
}
