#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DropOperation {
    None,
    Copy,
    Move,
    Link,
}

#[derive(Clone, Debug, Default)]
pub enum DropData {
    #[default]
    None,
    #[cfg(not(target_arch = "wasm32"))]
    Files(Vec<std::path::PathBuf>),
    #[cfg(target_arch = "wasm32")]
    Files(Vec<web_sys::File>),
}
