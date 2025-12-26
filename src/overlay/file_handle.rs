use parking_lot::Mutex;
use std::cell::RefCell;
use std::fs::File;
use std::sync::Arc;

thread_local! {
    pub(crate) static READ_BUFFER: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(128 * 1024));
}

#[derive(Debug)]
pub(crate) struct FileHandle {
    pub file: Arc<Mutex<File>>,
}
