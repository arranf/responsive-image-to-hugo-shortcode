
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_longlong};

use crate::error::AppError;

extern "C" {
    fn MakeSVG(
        path: GoString,
        number_of_primitives: c_longlong,
        mode: c_longlong,
        alpha: c_longlong,
        workers: c_longlong,
    ) -> *const c_char;
}

/// See [here](http://blog.ralch.com/tutorial/golang-sharing-libraries/) for `GoString` struct layout
// See the generated header file: libsqif.h
#[repr(C)]
struct GoString {
    a: *const c_char,
    b: isize,
}

pub fn make_sqip(path: &str) -> Result<String, AppError> {
    let c_path = CString::new(path).expect("CString::new failed");
    let ptr = c_path.as_ptr();
    let go_string = GoString {
        a: ptr,
        b: c_path.as_bytes().len() as isize,
    };
    let number_of_primitives: c_longlong = 10;
    let mode: c_longlong = 0;
    let alpha: c_longlong = 128;
    let workers: c_longlong = num_cpus::get() as c_longlong;

    let result = unsafe { MakeSVG(go_string, number_of_primitives, mode, alpha, workers) };
    let c_str = unsafe { CStr::from_ptr(result) };
    let string = c_str.to_str().expect("Error translating SQIP from library");
    if string.is_empty() || string.starts_with("Error") {
        error!("Failed to get SQIP from SQIP library: {}", string);
        Err(AppError::SQIP {})
    } else {
        Ok(base64::encode(string))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::read_dir;
    use tempfile::tempdir;

    const IMAGE_FILE: &str = "./test/test.png";

    fn init() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    // TODO: make integration test
    #[test]
    fn test_make_sqip_happy() {
        init();
        let dest_dir = tempdir().unwrap();

        // Nothing there to begin with
        let paths = read_dir(dest_dir.path()).unwrap();
        assert_eq!(0, paths.count());

        let sqip = make_sqip(IMAGE_FILE);
        assert!(sqip.is_ok());

        dest_dir.close().unwrap();
    }
}
