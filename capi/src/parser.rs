use crate::boundary;
use crate::object::{UclObject, release, retain, tree};
use serde_ucl::parse::{ErrorKind, FsLoader, Input, Parser};
use serde_ucl::value::ParserFlags;
use std::ffi::{CStr, CString, c_char, c_int};
use std::ptr;

pub struct UclParser {
    parser: Parser,
    submitted: bool,
    root: *mut UclObject,
    error: Option<CString>,
    code: c_int,
    line: u32,
    column: u32,
}
impl Drop for UclParser {
    fn drop(&mut self) {
        unsafe {
            release(self.root);
        }
    }
}

fn submit(p: &mut UclParser, input: Input<'_>) -> bool {
    if p.submitted {
        p.code = 3;
        p.error = Some(c"Stage A accepts only one input submission".into());
        return false;
    }
    p.submitted = true;
    let observation = p.parser.observe_c_input(input);
    p.line = observation.cursor.line as u32;
    p.column = observation.cursor.column as u32;
    if let Some(root) = observation.root {
        p.root = tree(root, observation.facts);
    }
    match observation.error {
        None => true,
        Some(e) => {
            p.code = match e.kind() {
                ErrorKind::UnterminatedObject | ErrorKind::UnterminatedArray => 5,
                ErrorKind::NestingTooDeep { .. } => 4,
                ErrorKind::UnknownMacro { .. }
                | ErrorKind::FileNotFound { .. }
                | ErrorKind::NotAFile { .. }
                | ErrorKind::Io { .. }
                | ErrorKind::Stopped { .. } => 0,
                _ => 1,
            };
            if !e.is_stopped() {
                p.error = Some(
                    CString::new(e.to_string().replace('\0', "\\0")).expect("escaped diagnostic"),
                );
            }
            false
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_parser_new(flags: c_int) -> *mut UclParser {
    boundary(|| {
        let mut parser = Parser::with_flags(ParserFlags::from_bits_truncate(flags as u32));
        parser.set_loader(FsLoader::new());
        if let Ok(cwd) = std::env::current_dir() {
            parser.set_base_dir(cwd);
        }
        Box::into_raw(Box::new(UclParser {
            parser,
            submitted: false,
            root: ptr::null_mut(),
            error: None,
            code: 0,
            line: 0,
            column: 0,
        }))
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_parser_register_variable(
    p: *mut UclParser,
    name: *const c_char,
    value: *const c_char,
) {
    boundary(|| {
        let p = unsafe { &mut *p };
        let name = unsafe { CStr::from_ptr(name) }
            .to_str()
            .expect("UTF-8 name");
        let value = unsafe { CStr::from_ptr(value) }
            .to_str()
            .expect("UTF-8 value");
        p.parser.register_variable(name, value);
    });
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_parser_add_chunk(
    p: *mut UclParser,
    data: *const u8,
    len: usize,
) -> bool {
    boundary(|| {
        let p = unsafe { &mut *p };
        if p.submitted {
            return submit(p, Input::bytes(b""));
        }
        let bytes = if len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(data, len) }
        };
        submit(p, Input::bytes(bytes))
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_parser_add_string(
    p: *mut UclParser,
    data: *const c_char,
    len: usize,
) -> bool {
    boundary(|| {
        if unsafe { (*p).submitted } {
            return submit(unsafe { &mut *p }, Input::bytes(b""));
        }
        let len = if len == 0 {
            unsafe { CStr::from_ptr(data) }.to_bytes().len()
        } else {
            len
        };
        unsafe { ucl_parser_add_chunk(p, data.cast(), len) }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_parser_add_file(p: *mut UclParser, filename: *const c_char) -> bool {
    boundary(|| {
        let p = unsafe { &mut *p };
        if p.submitted {
            return submit(p, Input::bytes(b""));
        }
        let name = unsafe { CStr::from_ptr(filename) }
            .to_str()
            .expect("UTF-8 filename");
        submit(p, Input::file(name))
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_parser_get_object(p: *mut UclParser) -> *mut UclObject {
    boundary(|| unsafe { retain((*p).root) })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_parser_get_error(p: *mut UclParser) -> *const c_char {
    boundary(|| unsafe { (*p).error.as_ref() }.map_or(ptr::null(), |e| e.as_ptr()))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_parser_get_error_code(p: *mut UclParser) -> c_int {
    boundary(|| unsafe { (*p).code })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_parser_get_column(p: *mut UclParser) -> u32 {
    boundary(|| unsafe { (*p).column })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_parser_get_linenum(p: *mut UclParser) -> u32 {
    boundary(|| unsafe { (*p).line })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_parser_free(p: *mut UclParser) {
    boundary(|| unsafe {
        drop(Box::from_raw(p));
    });
}
