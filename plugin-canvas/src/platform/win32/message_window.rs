use std::{
    mem,
    ptr::{null, null_mut},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use uuid::Uuid;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::HBRUSH,
        UI::{
            Input::KeyboardAndMouse::{SetFocus, VIRTUAL_KEY},
            WindowsAndMessaging::{
                CS_OWNDC, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
                GWLP_USERDATA, GetMessageW, GetWindowLongPtrW, HCURSOR, HICON, PostMessageW,
                RegisterClassW, SetWindowLongPtrW, TranslateMessage, UnregisterClassW, WM_CHAR,
                WM_KEYDOWN, WM_KEYUP, WNDCLASSW, WS_CHILD, WS_EX_NOACTIVATE,
            },
        },
    },
    core::PCWSTR,
};
use windows_core::BOOL;

use crate::error::Error;

use super::{
    PLUGIN_HINSTANCE, WM_USER_CHAR, WM_USER_KEY_DOWN, WM_USER_KEY_UP,
    keyboard::virtual_key_to_keycode, to_wstr,
};

pub struct MessageWindow {
    hwnd: usize,
    main_window_hwnd: usize,
    window_class: u16,
}

impl MessageWindow {
    pub fn new(main_window_hwnd: HWND) -> Result<Self, Error> {
        let class_name = to_wstr(
            "plugin-canvas-message-window-".to_string() + &Uuid::new_v4().simple().to_string(),
        );
        let window_name = to_wstr("Message window");

        let window_class_attributes = WNDCLASSW {
            style: CS_OWNDC,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: PLUGIN_HINSTANCE.with(|hinstance| *hinstance),
            hIcon: HICON(null_mut()),
            hCursor: HCURSOR(null_mut()),
            hbrBackground: HBRUSH(null_mut()),
            lpszMenuName: PCWSTR(null()),
            lpszClassName: PCWSTR(class_name.as_ptr()),
        };

        let window_class = unsafe { RegisterClassW(&window_class_attributes) };
        if window_class == 0 {
            return Err(Error::PlatformError(
                "Failed to register window class".into(),
            ));
        }

        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE,
                PCWSTR(window_class as _),
                PCWSTR(window_name.as_ptr() as _),
                WS_CHILD,
                0,
                0,
                0,
                0,
                Some(main_window_hwnd),
                None,
                Some(PLUGIN_HINSTANCE.with(|hinstance| *hinstance)),
                None,
            )
            .unwrap()
        };

        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, main_window_hwnd.0 as _) };

        Ok(Self {
            hwnd: hwnd.0 as _,
            main_window_hwnd: main_window_hwnd.0 as _,
            window_class,
        })
    }

    pub fn run(&self, running: Arc<AtomicBool>) {
        unsafe {
            let hwnd = HWND(self.hwnd as _);
            let mut msg = mem::zeroed();

            while running.load(Ordering::Acquire) {
                match GetMessageW(&mut msg, Some(hwnd), 0, 0) {
                    BOOL(-1) => {
                        panic!()
                    }

                    BOOL(0) => {
                        return;
                    }

                    _ => {}
                }

                // We can ignore the return value
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    pub fn set_focus(&self, focus: bool) {
        let hwnd = HWND(if focus {
            self.hwnd
        } else {
            self.main_window_hwnd
        } as _);

        unsafe {
            SetFocus(Some(hwnd)).unwrap();
        }
    }
}

impl Drop for MessageWindow {
    fn drop(&mut self) {
        unsafe {
            // It's ok if this fails; window might already be deleted if our parent window was deleted
            DestroyWindow(HWND(self.hwnd as _)).ok();
            UnregisterClassW(
                PCWSTR(self.window_class as _),
                Some(PLUGIN_HINSTANCE.with(|hinstance| *hinstance)),
            )
            .unwrap();
        }
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let main_window_hwnd = unsafe { HWND(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as _) };

    match msg {
        WM_CHAR => {
            unsafe { PostMessageW(Some(main_window_hwnd), WM_USER_CHAR, wparam, lparam).unwrap() };
            LRESULT(0)
        }

        WM_KEYDOWN => {
            let keycode = virtual_key_to_keycode(VIRTUAL_KEY(wparam.0 as _));
            unsafe {
                PostMessageW(
                    Some(main_window_hwnd),
                    WM_USER_KEY_DOWN,
                    WPARAM(keycode as _),
                    lparam,
                )
                .unwrap()
            };

            LRESULT(0)
        }

        WM_KEYUP => {
            let keycode = virtual_key_to_keycode(VIRTUAL_KEY(wparam.0 as _));
            unsafe {
                PostMessageW(
                    Some(main_window_hwnd),
                    WM_USER_KEY_UP,
                    WPARAM(keycode as _),
                    LPARAM(0),
                )
                .unwrap()
            };

            LRESULT(0)
        }

        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
