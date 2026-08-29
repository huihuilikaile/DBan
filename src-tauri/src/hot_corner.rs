//! 顶部热区呼出：轮询光标位置，在记忆窗口位置对应的屏幕顶部停留后弹出面板。
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::{Manager, PhysicalPosition};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

fn point_in_top_zone(pt: POINT, top: i32, zone_left: i32, zone_right: i32) -> bool {
    pt.x >= zone_left && pt.x < zone_right && pt.y >= top && pt.y < top + 8
}

fn is_in_top_zone(app: &tauri::AppHandle, pt: POINT) -> bool {
    let Some(win) = app.get_webview_window("main") else {
        return false;
    };
    let monitors = win.available_monitors().unwrap_or_default();
    let Some(monitor) = crate::monitor_at_point(&monitors, PhysicalPosition::new(pt.x, pt.y))
    else {
        return false;
    };
    let state = app.state::<crate::AppState>();
    if state.window_dragging.load(Ordering::Acquire) {
        return false;
    }
    let (size, position) = crate::remembered_geometry(crate::PANEL, &state, &monitor);
    let zone_width = (state.top_trigger_width.load(Ordering::Acquire) as f64
        * monitor.scale_factor())
    .round()
    .max(1.0) as i32;
    let center = position.x + size.width as i32 / 2;
    point_in_top_zone(
        pt,
        monitor.position().y,
        center - zone_width / 2,
        center - zone_width / 2 + zone_width,
    )
}

pub fn spawn(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut dwell = Instant::now();
        let mut parked = false; // 触发过一次后须等光标离开热角区，避免来回切换
        loop {
            std::thread::sleep(Duration::from_millis(60));

            let mut pt = POINT::default();
            unsafe {
                let _ = GetCursorPos(&mut pt);
            }
            let in_zone = is_in_top_zone(&app, pt);

            let mode = {
                let state = app.state::<crate::AppState>();
                let m = state.mode.lock().unwrap().clone();
                m
            };
            if mode == "panel" {
                parked = in_zone;
                dwell = Instant::now();
                continue;
            }

            if in_zone {
                let dwell_ms = app
                    .state::<crate::AppState>()
                    .top_trigger_dwell_ms
                    .load(Ordering::Acquire) as u64;
                if !parked && dwell.elapsed() >= Duration::from_millis(dwell_ms) {
                    parked = true;
                    let state = app.state::<crate::AppState>();
                    let _ = crate::apply_mode(&app, &state, "panel");
                }
            } else {
                parked = false;
                dwell = Instant::now();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::point_in_top_zone;
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
}
