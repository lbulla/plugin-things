use i_slint_core::platform::Clipboard;
use i_slint_core::{
    platform::{Platform, PlatformError},
    window::WindowAdapter,
};
use std::cell::RefCell;
use std::rc::Rc;

use crate::window_adapter::PluginCanvasWindowAdapter;

#[derive(Default)]
pub struct PluginCanvasPlatform {
    clipboard: RefCell<Option<String>>,
}

impl Platform for PluginCanvasPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        PluginCanvasWindowAdapter::new()
    }

    fn set_clipboard_text(&self, text: &str, clipboard: Clipboard) {
        match clipboard {
            Clipboard::DefaultClipboard => {
                self.clipboard.replace(Some(text.into()));
            }
            _ => (),
        }
    }

    fn clipboard_text(&self, clipboard: Clipboard) -> Option<String> {
        match clipboard {
            Clipboard::DefaultClipboard => self.clipboard.borrow().clone(),
            _ => None,
        }
    }
}
