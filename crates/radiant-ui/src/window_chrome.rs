//! Custom title bar glue. The window has no Windows frame (`no-frame` in main.slint), so dragging, edge resizing
//! and the minimize / maximize / close buttons are handled here. Drag and resize use winit through Slint.

use crate::MainWindow;
use slint::winit_030::{winit, WinitWindowAccessor};
use slint::ComponentHandle;
use winit::window::ResizeDirection;

pub fn wire(ui: &MainWindow) {
    let weak = ui.as_weak();

    ui.on_window_minimize({
        let weak = weak.clone();
        move || {
            if let Some(ui) = weak.upgrade() {
                ui.window().set_minimized(true);
            }
        }
    });

    ui.on_window_toggle_maximize({
        let weak = weak.clone();
        move || {
            if let Some(ui) = weak.upgrade() {
                let maximize = !ui.window().is_maximized();
                ui.window().set_maximized(maximize);
                ui.set_window_maximized(maximize);
            }
        }
    });

    // Same path as the X of a normal window (the close request), so nothing is skipped on exit.
    ui.on_window_close({
        let weak = weak.clone();
        move || {
            if let Some(ui) = weak.upgrade() {
                ui.window().dispatch_event(slint::platform::WindowEvent::CloseRequested);
            }
        }
    });

    // Native window move: works with Win+arrows snapping and restores a maximized window under the cursor.
    ui.on_window_drag({
        let weak = weak.clone();
        move || {
            if let Some(ui) = weak.upgrade() {
                ui.window().with_winit_window(|w| {
                    let _ = w.drag_window();
                });
            }
        }
    });

    // 0 N, 1 S, 2 W, 3 E, 4 NW, 5 NE, 6 SW, 7 SE (the order of the edges in main.slint).
    ui.on_window_resize({
        let weak = weak.clone();
        move |dir| {
            let direction = match dir {
                0 => ResizeDirection::North,
                1 => ResizeDirection::South,
                2 => ResizeDirection::West,
                3 => ResizeDirection::East,
                4 => ResizeDirection::NorthWest,
                5 => ResizeDirection::NorthEast,
                6 => ResizeDirection::SouthWest,
                _ => ResizeDirection::SouthEast,
            };
            if let Some(ui) = weak.upgrade() {
                ui.window().with_winit_window(|w| {
                    let _ = w.drag_resize_window(direction);
                });
            }
        }
    });

    // Maximized by Win+Up or by snapping: the icon and the resize edges catch up when the pointer is on the title bar.
    ui.on_window_state_changed({
        let weak = weak.clone();
        move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_window_maximized(ui.window().is_maximized());
            }
        }
    });
}
