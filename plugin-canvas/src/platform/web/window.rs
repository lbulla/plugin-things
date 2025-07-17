use cursor_icon::CursorIcon;
use keyboard_types::Code;
use raw_window_handle::RawWindowHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use web_sys::wasm_bindgen::closure::Closure;
use web_sys::wasm_bindgen::convert::FromWasmAbi;
use web_sys::wasm_bindgen::{JsCast, JsValue};
use web_sys::{HtmlCanvasElement, Window, window};

use crate::drag_drop::{DropData, DropOperation};
use crate::error::Error;
use crate::event::{EventCallback, EventResponse, ScrollDelta};
use crate::keyboard::KeyboardModifiers;
use crate::platform::interface::{HtmlCanvasInterface, OsWindowInterface};
use crate::platform::os_window_handle::OsWindowHandle;
use crate::thread_bound::ThreadBound;
use crate::window::WindowAttributes;
use crate::{Event, LogicalPosition, LogicalSize, MouseButton, PhysicalPosition};

macro_rules! update_modifiers {
    ($inner:ident, $event:ident) => {
        let mut modifiers = KeyboardModifiers::empty();
        if $event.alt_key() {
            modifiers |= KeyboardModifiers::Alt;
        }
        if $event.ctrl_key() {
            modifiers |= KeyboardModifiers::Control;
        }
        if $event.meta_key() {
            modifiers |= KeyboardModifiers::Meta;
        }
        if $event.shift_key() {
            modifiers |= KeyboardModifiers::Shift;
        }
        $inner.send_event(Event::KeyboardModifiers { modifiers });
    };
}

macro_rules! event_position {
    ($inner:ident, $event:ident) => {{
        let rect = $inner.canvas.get_bounding_client_rect();
        PhysicalPosition {
            x: $event.x() - rect.left() as i32,
            y: $event.y() - rect.top() as i32,
        }
        .to_logical($inner.os_scale())
    }};
}

macro_rules! send_event {
    ($inner:ident, $web_event:expr, $event:expr) => {
        if $inner.send_event($event) == EventResponse::Handled {
            $web_event.prevent_default();
        }
    };
}

macro_rules! send_drag_event {
    ($inner:ident, $web_event:expr, $event:expr) => {
        match $inner.send_event($event) {
            EventResponse::Handled => $web_event.prevent_default(),
            EventResponse::Ignored => (),
            EventResponse::DropAccepted(op) => {
                if let Some(data) = $web_event.data_transfer() {
                    match op {
                        DropOperation::None => data.set_drop_effect("none"),
                        DropOperation::Copy => data.set_drop_effect("copy"),
                        DropOperation::Move => data.set_drop_effect("move"),
                        DropOperation::Link => data.set_drop_effect("link"),
                    }
                }
            }
        }
    };
}

pub struct OsWindow {
    inner: Rc<OsWindowInner>,
}

impl OsWindow {
    fn convert_key(web_event: &web_sys::KeyboardEvent) -> Option<(Code, String)> {
        let text = web_event.key();
        let key_code = match text.as_str() {
            "Backspace" => Code::Backspace,
            "Enter" => Code::Enter,
            "Delete" => Code::Delete,
            "ArrowUp" => Code::ArrowUp,
            "ArrowDown" => Code::ArrowDown,
            "ArrowLeft" => Code::ArrowLeft,
            "ArrowRight" => Code::ArrowRight,
            _ => {
                // Not a letter (e.g. modifier).
                if text.len() != 1 && text.is_ascii() {
                    return None;
                }
                Code::Unidentified
            }
        };
        Some((key_code, text))
    }

    fn convert_button(web_event: &web_sys::PointerEvent) -> MouseButton {
        match web_event.button() {
            0 => MouseButton::Left,
            1 => MouseButton::Middle,
            2 => MouseButton::Right,
            _ => MouseButton::Left,
        }
    }

    fn drop_date(web_event: &web_sys::DragEvent) -> DropData {
        if let Some(file_list) = web_event.data_transfer().and_then(|d| d.files()) {
            if file_list.length() == 0 {
                DropData::None
            } else {
                DropData::Files(
                    (0..file_list.length())
                        .filter_map(|i| file_list.item(i))
                        .collect(),
                )
            }
        } else {
            DropData::None
        }
    }
}

impl OsWindowInterface for OsWindow {
    fn open(
        parent_window_handle: RawWindowHandle,
        window_attributes: WindowAttributes,
        event_callback: Box<EventCallback>,
    ) -> Result<OsWindowHandle, Error> {
        let RawWindowHandle::Web(handle) = parent_window_handle else {
            return Err(Error::PlatformError("Not a web window".into()));
        };

        let window = window().ok_or(Error::PlatformError("No window found".into()))?;
        let document = window
            .document()
            .ok_or(Error::PlatformError("No document found".into()))?;

        let canvas = document
            .query_selector(format!("[data-raw-handle=\"{}\"]", handle.id).as_str())
            .map_err(|_| Error::PlatformError("Canvas search failed".into()))?;
        let canvas = canvas.ok_or(Error::PlatformError(
            format!("Canvas ID `{}` not found", handle.id).into(),
        ))?;
        let canvas = canvas.dyn_into::<HtmlCanvasElement>().map_err(|_| {
            Error::PlatformError("Canvas could not be casted to `HtmlCanvasElement`".into())
        })?;

        let inner = Rc::new(OsWindowInner {
            window,
            canvas,
            event_callback,
            closures: RefCell::new(None),
        });

        let closures = Closures {
            on_keydown: inner.add_event_listener_canvas("keydown", {
                let inner = inner.clone();
                move |web_event: web_sys::KeyboardEvent| {
                    update_modifiers!(inner, web_event);
                    if let Some((key_code, text)) = Self::convert_key(&web_event) {
                        send_event!(
                            inner,
                            web_event,
                            Event::KeyDown {
                                key_code,
                                text: Some(text),
                                repeat: web_event.repeat(),
                            }
                        );
                    }
                }
            }),

            on_keyup: inner.add_event_listener_canvas("keyup", {
                let inner = inner.clone();
                move |web_event: web_sys::KeyboardEvent| {
                    update_modifiers!(inner, web_event);
                    if let Some((key_code, text)) = Self::convert_key(&web_event) {
                        send_event!(
                            inner,
                            web_event,
                            Event::KeyUp {
                                key_code,
                                text: Some(text),
                            }
                        );
                    }
                }
            }),

            on_pointerdown: inner.add_event_listener_canvas("pointerdown", {
                let inner = inner.clone();
                move |web_event: web_sys::PointerEvent| {
                    update_modifiers!(inner, web_event);
                    send_event!(
                        inner,
                        web_event,
                        Event::MouseButtonDown {
                            button: Self::convert_button(&web_event),
                            position: event_position!(inner, web_event),
                        }
                    );
                }
            }),

            on_pointerup: inner.add_event_listener_window("pointerup", {
                let inner = inner.clone();
                move |web_event: web_sys::PointerEvent| {
                    update_modifiers!(inner, web_event);
                    send_event!(
                        inner,
                        web_event,
                        Event::MouseButtonUp {
                            button: Self::convert_button(&web_event),
                            position: event_position!(inner, web_event),
                        }
                    );
                }
            }),

            on_pointerleave: inner.add_event_listener_canvas("pointerleave", {
                let inner = inner.clone();
                move |web_event: web_sys::PointerEvent| {
                    update_modifiers!(inner, web_event);
                    send_event!(inner, web_event, Event::MouseExited);
                }
            }),

            on_pointermove: inner.add_event_listener_window("pointermove", {
                let inner = inner.clone();
                move |web_event: web_sys::PointerEvent| {
                    update_modifiers!(inner, web_event);
                    send_event!(
                        inner,
                        web_event,
                        Event::MouseMoved {
                            position: event_position!(inner, web_event),
                        }
                    );
                }
            }),

            on_contextmenu: inner.add_event_listener_canvas("contextmenu", {
                |web_event: web_sys::PointerEvent| {
                    web_event.prevent_default();
                }
            }),

            on_wheel: inner.add_event_listener_canvas("wheel", {
                let inner = inner.clone();
                move |web_event: web_sys::WheelEvent| {
                    update_modifiers!(inner, web_event);

                    let delta = if web_event.delta_mode() == web_sys::WheelEvent::DOM_DELTA_PIXEL {
                        ScrollDelta::PixelDelta(web_event.delta_x(), web_event.delta_y())
                    } else {
                        // TODO: Handle `DOM_DELTA_PAGE`.
                        ScrollDelta::LineDelta(web_event.delta_x(), web_event.delta_y())
                    };
                    send_event!(
                        inner,
                        web_event,
                        Event::MouseWheel {
                            position: event_position!(inner, web_event),
                            delta,
                        }
                    );
                }
            }),

            on_dragenter: inner.add_event_listener_window("dragenter", {
                let inner = inner.clone();
                move |web_event: web_sys::DragEvent| {
                    send_drag_event!(
                        inner,
                        web_event,
                        Event::DragEntered {
                            position: event_position!(inner, web_event),
                            data: Self::drop_date(&web_event),
                        }
                    );
                }
            }),

            on_dragleave: inner.add_event_listener_window("dragleave", {
                let inner = inner.clone();
                move |web_event: web_sys::DragEvent| {
                    send_drag_event!(inner, web_event, Event::DragExited);
                }
            }),

            on_dragover: inner.add_event_listener_window("drag", {
                let inner = inner.clone();
                move |web_event: web_sys::DragEvent| {
                    send_drag_event!(
                        inner,
                        web_event,
                        Event::DragMoved {
                            position: event_position!(inner, web_event),
                            data: Self::drop_date(&web_event),
                        }
                    );
                }
            }),

            on_dragend: inner.add_event_listener_window("dragleave", {
                let inner = inner.clone();
                move |web_event: web_sys::DragEvent| {
                    send_drag_event!(
                        inner,
                        web_event,
                        Event::DragDropped {
                            position: event_position!(inner, web_event),
                            data: Self::drop_date(&web_event),
                        }
                    );
                }
            }),

            on_animation: Closure::new({
                let inner = inner.clone();
                move |_timestamp: JsValue| {
                    inner.animation_frame();
                }
            }),
        };
        inner.closures.replace(Some(closures));

        let size = window_attributes
            .size
            .to_physical(window_attributes.scale * inner.os_scale());
        inner.canvas.set_width(size.width);
        inner.canvas.set_height(size.height);
        inner.animation_frame();

        Ok(OsWindowHandle::new(Arc::new(ThreadBound::new(Self {
            inner,
        }))))
    }

    fn os_scale(&self) -> f64 {
        self.inner.os_scale()
    }

    fn resized(&self, size: LogicalSize) {
        let size = size.to_physical(self.os_scale());
        self.inner.canvas.set_width(size.width);
        self.inner.canvas.set_height(size.height);
    }

    fn set_cursor(&self, cursor: Option<CursorIcon>) {
        self.inner
            .canvas
            .style()
            .set_property("cursor", cursor.map(|c| c.name()).unwrap_or("default"))
            .unwrap();
    }

    fn set_input_focus(&self, focus: bool) {
        if focus {
            self.inner.canvas.focus().unwrap();
        } else {
            self.inner.canvas.blur().unwrap();
        }
    }

    fn warp_mouse(&self, _position: LogicalPosition) {
        // TODO?
    }

    fn poll_events(&self) -> Result<(), Error> {
        Ok(())
    }
}

impl HtmlCanvasInterface for OsWindow {
    fn canvas(&self) -> HtmlCanvasElement {
        self.inner.canvas.clone()
    }
}

struct OsWindowInner {
    window: Window,
    canvas: HtmlCanvasElement,
    event_callback: Box<EventCallback>,
    closures: RefCell<Option<Closures>>,
}

impl OsWindowInner {
    fn os_scale(&self) -> f64 {
        self.window.device_pixel_ratio()
    }

    fn add_event_listener_canvas<F: Fn(A) + 'static, A: FromWasmAbi + 'static>(
        &self,
        name: &str,
        f: F,
    ) -> Closure<dyn Fn(A)> {
        let closure = Closure::<dyn Fn(A)>::new(f);
        self.canvas
            .add_event_listener_with_callback(name, closure.as_ref().unchecked_ref())
            .unwrap();
        closure
    }

    fn remove_event_listener_canvas<A: FromWasmAbi + 'static>(
        &self,
        name: &str,
        closure: &Closure<dyn Fn(A)>,
    ) {
        self.canvas
            .remove_event_listener_with_callback(name, closure.as_ref().unchecked_ref())
            .unwrap();
    }

    fn add_event_listener_window<F: Fn(A) + 'static, A: FromWasmAbi + 'static>(
        &self,
        name: &str,
        f: F,
    ) -> Closure<dyn Fn(A)> {
        let closure = Closure::<dyn Fn(A)>::new(f);
        self.window
            .add_event_listener_with_callback(name, closure.as_ref().unchecked_ref())
            .unwrap();
        closure
    }

    fn remove_event_listener_window<A: FromWasmAbi + 'static>(
        &self,
        name: &str,
        closure: &Closure<dyn Fn(A)>,
    ) {
        self.window
            .remove_event_listener_with_callback(name, closure.as_ref().unchecked_ref())
            .unwrap();
    }

    fn send_event(&self, event: Event) -> EventResponse {
        (self.event_callback)(event)
    }

    fn animation_frame(&self) {
        self.send_event(Event::Draw);
        self.window
            .request_animation_frame(
                self.closures
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .on_animation
                    .as_ref()
                    .unchecked_ref(),
            )
            .unwrap();
    }
}

impl Drop for OsWindowInner {
    fn drop(&mut self) {
        let closures = self.closures.borrow_mut().take().unwrap();

        self.remove_event_listener_canvas("keydown", &closures.on_keydown);
        self.remove_event_listener_canvas("keyup", &closures.on_keyup);

        self.remove_event_listener_canvas("pointerdown", &closures.on_pointerdown);
        self.remove_event_listener_window("pointerup", &closures.on_pointerup);
        self.remove_event_listener_canvas("pointerleave", &closures.on_pointerleave);
        self.remove_event_listener_window("pointermove", &closures.on_pointermove);
        self.remove_event_listener_window("contextmenu", &closures.on_contextmenu);

        self.remove_event_listener_canvas("wheel", &closures.on_wheel);

        self.remove_event_listener_canvas("dragenter", &closures.on_dragenter);
        self.remove_event_listener_canvas("dragleave", &closures.on_dragleave);
        self.remove_event_listener_canvas("dragover", &closures.on_dragover);
        self.remove_event_listener_canvas("dragend", &closures.on_dragend);
    }
}

struct Closures {
    on_keydown: Closure<dyn Fn(web_sys::KeyboardEvent)>,
    on_keyup: Closure<dyn Fn(web_sys::KeyboardEvent)>,

    on_pointerdown: Closure<dyn Fn(web_sys::PointerEvent)>,
    on_pointerup: Closure<dyn Fn(web_sys::PointerEvent)>,
    on_pointerleave: Closure<dyn Fn(web_sys::PointerEvent)>,
    on_pointermove: Closure<dyn Fn(web_sys::PointerEvent)>,
    on_contextmenu: Closure<dyn Fn(web_sys::PointerEvent)>,

    on_wheel: Closure<dyn Fn(web_sys::WheelEvent)>,

    on_dragenter: Closure<dyn Fn(web_sys::DragEvent)>,
    on_dragleave: Closure<dyn Fn(web_sys::DragEvent)>,
    on_dragover: Closure<dyn Fn(web_sys::DragEvent)>,
    on_dragend: Closure<dyn Fn(web_sys::DragEvent)>,

    on_animation: Closure<dyn Fn(JsValue)>,
}
