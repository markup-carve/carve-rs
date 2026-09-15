//! Minimal C ABI used by the Echo language binding.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;
use std::str;

const STATUS_OK: i32 = 0;
const STATUS_INVALID_POINTER: i32 = 1;
const STATUS_INVALID_UTF8: i32 = 2;
const STATUS_PANIC: i32 = 3;

/// Render UTF-8 Carve source to UTF-8 HTML.
///
/// Returns zero on success, one for invalid pointers, two for invalid UTF-8,
/// and three if the Rust implementation panics. On success, the caller owns
/// `out_data` and must release it with [`carve_html_free`].
///
/// # Safety
///
/// `input` must address `input_len` readable bytes. `out_data` and `out_len`
/// must be valid writable pointers.
#[no_mangle]
pub unsafe extern "C" fn carve_to_html(
    input: *const u8,
    input_len: usize,
    out_data: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    if out_data.is_null() || out_len.is_null() {
        return STATUS_INVALID_POINTER;
    }

    ptr::write(out_data, ptr::null_mut());
    ptr::write(out_len, 0);

    if input.is_null() && input_len != 0 {
        return STATUS_INVALID_POINTER;
    }

    let bytes = if input_len == 0 {
        &[]
    } else {
        slice::from_raw_parts(input, input_len)
    };
    let source = match str::from_utf8(bytes) {
        Ok(source) => source,
        Err(_) => return STATUS_INVALID_UTF8,
    };

    match catch_unwind(AssertUnwindSafe(|| carve::to_html(source))) {
        Ok(html) if html.is_empty() => STATUS_OK,
        Ok(html) => {
            let mut bytes = html.into_bytes().into_boxed_slice();
            ptr::write(out_data, bytes.as_mut_ptr());
            ptr::write(out_len, bytes.len());
            std::mem::forget(bytes);
            STATUS_OK
        }
        Err(_) => STATUS_PANIC,
    }
}

/// Release a buffer returned by [`carve_to_html`].
///
/// # Safety
///
/// The pointer and length must be an unchanged pair returned by
/// [`carve_to_html`], and the pair must be freed at most once.
#[no_mangle]
pub unsafe extern "C" fn carve_html_free(data: *mut u8, len: usize) {
    if data.is_null() {
        return;
    }
    drop(Box::from_raw(ptr::slice_from_raw_parts_mut(data, len)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_and_releases_html() {
        let source = b"# Hello\n\n/italic/ and *bold*.";
        let mut data = ptr::null_mut();
        let mut len = 0;
        let status = unsafe { carve_to_html(source.as_ptr(), source.len(), &mut data, &mut len) };
        assert_eq!(status, STATUS_OK);
        let html = unsafe { str::from_utf8(slice::from_raw_parts(data, len)).unwrap() };
        assert_eq!(
            html,
            "<section id=\"Hello\">\n  <h1>Hello</h1>\n  <p><em>italic</em> and <strong>bold</strong>.</p>\n</section>"
        );
        unsafe { carve_html_free(data, len) };
    }

    #[test]
    fn rejects_invalid_utf8_without_allocating() {
        let source = [0xff];
        let mut data = ptr::null_mut();
        let mut len = 0;
        let status = unsafe { carve_to_html(source.as_ptr(), source.len(), &mut data, &mut len) };
        assert_eq!(status, STATUS_INVALID_UTF8);
        assert!(data.is_null());
        assert_eq!(len, 0);
    }

    #[test]
    fn rejects_a_null_nonempty_input_without_allocating() {
        let mut data = ptr::NonNull::<u8>::dangling().as_ptr();
        let mut len = 99;
        let status = unsafe { carve_to_html(ptr::null(), 1, &mut data, &mut len) };
        assert_eq!(status, STATUS_INVALID_POINTER);
        assert!(data.is_null());
        assert_eq!(len, 0);
    }

    #[test]
    fn accepts_an_empty_input() {
        let mut data = ptr::NonNull::<u8>::dangling().as_ptr();
        let mut len = 99;
        let status = unsafe { carve_to_html(ptr::null(), 0, &mut data, &mut len) };
        assert_eq!(status, STATUS_OK);
        assert!(data.is_null());
        assert_eq!(len, 0);
    }
}
