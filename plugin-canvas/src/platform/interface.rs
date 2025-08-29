use cursor_icon::CursorIcon;
use raw_window_handle::RawWindowHandle;

use crate::{
    LogicalPosition, LogicalSize, error::Error, event::EventCallback, window::WindowAttributes,
};

use super::os_window_handle::OsWindowHandle;

pub(crate) trait OsWindowInterface: Sized {
    fn open(
        parent_window_handle: RawWindowHandle,
        window_attributes: WindowAttributes,
        event_callback: Box<EventCallback>,
    ) -> Result<OsWindowHandle, Error>;

    fn os_scale(&self) -> f64;

    fn resized(&self, size: LogicalSize);

    fn set_cursor(&self, cursor: Option<CursorIcon>);
    fn set_input_focus(&self, focus: bool);
    fn warp_mouse(&self, position: LogicalPosition);

    fn poll_events(&self) -> Result<(), Error>;
}

#[cfg(target_arch = "wasm32")]
pub trait HtmlCanvasInterface {
    fn canvas(&self) -> web_sys::HtmlCanvasElement;
}
