use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use crate::error::AppError;

extern "C" {
    fn MakeSVG(path: GoString) -> *const c_char;
}

/// See [here](http://blog.ralch.com/tutorial/golang-sharing-libraries/) for `GoString` struct layout
#[repr(C)]
struct GoString {
    a: *const c_char,
    b: i64,
}

pub fn make_sqip(path: &str) -> Result<String, AppError> {
    let c_path = CString::new(path).expect("CString::new failed");
    let ptr = c_path.as_ptr();
    let go_string = GoString {
        a: ptr,
        b: c_path.as_bytes().len() as i64,
    };
    let result = unsafe { MakeSVG(go_string) };
    let c_str = unsafe { CStr::from_ptr(result) };
    let string = c_str.to_str().expect("Error translating SQIP from library");
    if string.is_empty() || string.starts_with("Error") {
        error!("Failed to get SQIP from SQIP library: {}", string);
        Err(AppError::SQIP {})
    } else {
        Ok(base64::encode(&string))
    }
}
