//! `wasm32-unknown-unknown` build of `gradius-extract`, in its own module so
//! the game's wasm stays small and this one is only fetched when the page has
//! no pack yet.
//!
//! # ABI
//!
//! | export | meaning |
//! | --- | --- |
//! | `alloc(len) -> *mut u8` / `dealloc(ptr, len)` | Buffer for the ROM |
//! | `extract(ptr, len) -> u32` | 0 with the pack ready, 1 with a message ready |
//! | `result_ptr()` / `result_len()` | The pack after a successful `extract` |
//! | `error_ptr()` / `error_len()` | UTF-8 message after a failed `extract` |
//!
//! Import `env.progress(step, done, total)`, called as the extraction moves;
//! `step` is the index of `gradius_extract::Step` (0 Tiles ... 8 Pack).

use std::cell::RefCell;

use gradius_extract::Step;

#[link(wasm_import_module = "env")]
extern "C" {
    fn progress(step: u32, done: u32, total: u32);
}

thread_local! {
    /// The pack or the error message of the last `extract`.
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    Box::into_raw(vec![0u8; len].into_boxed_slice()) as *mut u8
}

/// # Safety
///
/// `ptr` must come from [`alloc`] with this exact `len` and must not be used
/// afterwards.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
}

/// # Safety
///
/// `ptr..ptr + len` must be a buffer from [`alloc`].
#[no_mangle]
pub unsafe extern "C" fn extract(ptr: *const u8, len: usize) -> u32 {
    let rom = std::slice::from_raw_parts(ptr, len);
    let result = gradius_extract::extract(rom, &mut |step: Step, done, total| {
        // SAFETY: the import is provided by the page.
        unsafe { progress(step as u32, done, total) }
    });
    let (code, bytes) = match result {
        Ok(pack) => (0, pack),
        Err(error) => (1, error.to_string().into_bytes()),
    };
    OUTPUT.with(|out| *out.borrow_mut() = bytes);
    code
}

fn output(f: impl FnOnce(&[u8]) -> usize) -> usize {
    OUTPUT.with(|out| f(&out.borrow()))
}

#[no_mangle]
pub extern "C" fn result_ptr() -> *const u8 {
    output(|bytes| bytes.as_ptr() as usize) as *const u8
}

#[no_mangle]
pub extern "C" fn result_len() -> u32 {
    output(<[u8]>::len) as u32
}

#[no_mangle]
pub extern "C" fn error_ptr() -> *const u8 {
    result_ptr()
}

#[no_mangle]
pub extern "C" fn error_len() -> u32 {
    result_len()
}
