//! 顶部热区呼出：停留后露出面板底边，点击露出区域后再展开完整面板。
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::{Manager, PhysicalPosition};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

const POLL_INTERVAL: Duration = Duration::from_millis(40);
const PEEK_LEAVE_DELAY: Duration = Duration::from_millis(420);
const RETRY_INTERVAL: Duration = Duration::from_millis(180);

#[derive(Default)]
struct CursorHit {
    top_zone: bool,
    peek_surface: bool,
}

fn point_in_top_zone(pt: POINT, top: i32, zone_left: i32, zone_right: i32) -> bool {
    pt.x >= zone_left && pt.x < zone_right && pt.y >= top && pt.y < top + 8
}

fn point_in_peek_surface(pt: POINT, top: i32, left: i32, width: u32, height: i32) -> bool {
    pt.x >= left && pt.x < left + width as i32 && pt.y >= top && pt.y < top + height.max(1)
}

fn cursor_hit(app: &tauri::AppHandle, pt: POINT) -> CursorHit {
    let Some(win) = app.get_webview_window("main") else {
        return CursorHit::default();
    };
    let monitors = win.available_monitors().unwrap_or_default();
    let Some(monitor) = crate::monitor_at_point(&monitors, PhysicalPosition::new(pt.x, pt.y))
    else {
        return CursorHit::default();
    };
    let state = app.state::<crate::AppState>();
    if state.window_dragging.load(Ordering::Acquire) {
        return CursorHit::default();
    }
    let (size, position) = crate::remembered_geometry(crate::PANEL, &state, &monitor);
    let monitor_top = monitor.position().y;
    let peek_height = (crate::PEEK_HEIGHT * monitor.scale_factor())
        .round()
        .max(1.0) as i32;
    let zone_width = (state.top_trigger_width.load(Ordering::Acquire) as f64
        * monitor.scale_factor())
    .round()
    .max(1.0) as i32;
    let center = position.x + size.width as i32 / 2;
    CursorHit {
        top_zone: point_in_top_zone(
            pt,
            monitor_top,
            center - zone_width / 2,
            center - zone_width / 2 + zone_width,
        ),
        peek_surface: point_in_peek_surface(pt, monitor_top, position.x, size.width, peek_height),
    }
}

pub fn spawn(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut dwell_started: Option<Instant> = None;
        let mut leave_started: Option<Instant> = None;
        let mut last_attempt: Option<Instant> = None;
        loop {
            std::thread::sleep(POLL_INTERVAL);

            let mut pt = POINT::default();
            unsafe {
                let _ = GetCursorPos(&mut pt);
            }
            let hit = cursor_hit(&app, pt);

            let mode = {
                let state = app.state::<crate::AppState>();
                let m = state.mode.lock().unwrap().clone();
                m
            };
            match mode.as_str() {
                "hidden" => {
                    leave_started = None;
                    if !hit.top_zone {
                        dwell_started = None;
                        last_attempt = None;
                        continue;
                    }

                    let now = Instant::now();
                    let started = *dwell_started.get_or_insert(now);
                    let dwell_ms = app
                        .state::<crate::AppState>()
                        .top_trigger_dwell_ms
                        .load(Ordering::Acquire) as u64;
                    let retry_ready = last_attempt
                        .is_none_or(|attempt| now.duration_since(attempt) >= RETRY_INTERVAL);
                    if now.duration_since(started) >= Duration::from_millis(dwell_ms) && retry_ready
                    {
                        last_attempt = Some(now);
                        let state = app.state::<crate::AppState>();
                        if crate::apply_mode(&app, &state, "peek").is_ok() {
                            dwell_started = None;
                            last_attempt = None;
                        }
                    }
                }
                "peek" => {
                    dwell_started = None;
                    last_attempt = None;
                    if hit.top_zone || hit.peek_surface {
                        leave_started = None;
                        continue;
                    }

                    let now = Instant::now();
                    let left_at = *leave_started.get_or_insert(now);
                    if now.duration_since(left_at) >= PEEK_LEAVE_DELAY {
                        let state = app.state::<crate::AppState>();
                        if crate::apply_mode(&app, &state, "hidden").is_ok() {
                            leave_started = None;
                        }
                    }
                }
                _ => {
                    dwell_started = None;
                    leave_started = None;
                    last_attempt = None;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{point_in_peek_surface, point_in_top_zone};
    use windows::Win32::Foundation::POINT;

    #[test]
    fn detects_remembered_zone_on_negative_origin_monitor() {
        assert!(point_in_top_zone(POINT { x: -500, y: 0 }, 0, -600, -200));
        assert!(point_in_top_zone(POINT { x: -201, y: 7 }, 0, -600, -200));
        assert!(!point_in_top_zone(POINT { x: -601, y: 7 }, 0, -600, -200));
        assert!(!point_in_top_zone(POINT { x: -500, y: 8 }, 0, -600, -200));
    }

    #[test]
    fn respects_monitor_vertical_origin() {
        assert!(point_in_top_zone(
            POINT { x: 2559, y: -200 },
            -200,
            2200,
            2600
        ));
        assert!(!point_in_top_zone(
            POINT { x: 2559, y: -201 },
            -200,
            2200,
            2600
        ));
    }

    #[test]
    fn supports_configurable_zone_width() {
        assert!(point_in_top_zone(POINT { x: 400, y: 3 }, 0, 200, 600));
        assert!(!point_in_top_zone(POINT { x: 199, y: 3 }, 0, 200, 600));
        assert!(!point_in_top_zone(POINT { x: 600, y: 3 }, 0, 200, 600));
    }

    #[test]
    fn detects_pointer_over_visible_peek_surface() {
        assert!(point_in_peek_surface(
            POINT { x: 1600, y: 15 },
            0,
            1500,
            320,
            16,
        ));
        assert!(!point_in_peek_surface(
            POINT { x: 1600, y: 16 },
            0,
            1500,
            320,
            16,
        ));
        assert!(!point_in_peek_surface(
            POINT { x: 1499, y: 8 },
            0,
            1500,
            320,
            16,
        ));
    }
}
