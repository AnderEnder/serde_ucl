use crate::boundary;
use crate::object::{UclObject, node};
use std::ffi::{c_int, c_void};
use std::ptr;

struct OldObject {
    index: usize,
}
struct Iterator {
    target: *const UclObject,
    sequence: Vec<*const UclObject>,
    index: usize,
    mode: Option<c_int>,
    restart: Option<usize>,
}

unsafe fn children(p: *const UclObject) -> Vec<*const UclObject> {
    let n = unsafe { node(p) };
    match n.public.r#type {
        0 => n.heads().iter().map(|p| p.cast_const()).collect(),
        1 => n.children().iter().map(|p| p.cast_const()).collect(),
        _ => Vec::new(),
    }
}

unsafe fn sequence(p: *const UclObject, mode: c_int) -> Vec<*const UclObject> {
    let mut out = Vec::new();
    let mut current = p;
    while !current.is_null() {
        let kind = unsafe { (*current).r#type };
        if kind < 2 {
            out.extend(unsafe { children(current) });
            if mode == 1 || mode == 0 {
                break;
            }
        } else {
            out.push(current);
        }
        current = unsafe { (*current).next };
    }
    out
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_iterate_with_error(
    p: *const UclObject,
    it: *mut *mut c_void,
    expand: bool,
    error: *mut c_int,
) -> *const UclObject {
    boundary(|| {
        let handle = unsafe { *it };
        if expand && unsafe { (*p).r#type == 0 } {
            if !error.is_null() {
                unsafe {
                    *error = 0;
                }
            }
            let handle = if handle.is_null() {
                Box::into_raw(Box::new(OldObject { index: 0 }))
            } else {
                handle.cast::<OldObject>()
            };
            let cursor = unsafe { &mut *handle };
            let out = unsafe { node(p) }.heads().get(cursor.index).copied();
            if let Some(out) = out {
                cursor.index += 1;
                unsafe {
                    *it = handle.cast();
                }
                out.cast_const()
            } else {
                unsafe {
                    drop(Box::from_raw(handle));
                    *it = ptr::null_mut();
                }
                ptr::null()
            }
        } else if expand && unsafe { (*p).r#type == 1 } {
            let index = handle as usize;
            if let Some(out) = unsafe { node(p) }.children().get(index) {
                unsafe {
                    *it = (index + 1) as *mut c_void;
                }
                out.cast_const()
            } else {
                ptr::null()
            }
        } else {
            let out = if handle.is_null() {
                p
            } else {
                unsafe { (*handle.cast::<UclObject>()).next }.cast_const()
            };
            if !out.is_null() {
                unsafe {
                    *it = out.cast_mut().cast();
                }
            }
            out
        }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_iterate_end(_p: *const UclObject, it: *mut *mut c_void) {
    boundary(|| {
        if unsafe { !(*it).is_null() } {
            unsafe {
                drop(Box::from_raw((*it).cast::<OldObject>()));
                *it = ptr::null_mut();
            }
        }
    });
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_iterate_new(p: *const UclObject) -> *mut c_void {
    boundary(|| {
        Box::into_raw(Box::new(Iterator {
            target: p,
            sequence: Vec::new(),
            index: 0,
            mode: None,
            restart: None,
        }))
        .cast()
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_iter_chk_excpn(_it: *mut *mut c_void) -> bool {
    false
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_iterate_reset(
    it: *mut c_void,
    p: *const UclObject,
) -> *mut c_void {
    boundary(|| {
        let i = unsafe { &mut *it.cast::<Iterator>() };
        i.target = p;
        i.sequence.clear();
        i.index = 0;
        i.mode = None;
        i.restart = None;
        it
    })
}
unsafe fn next(it: *mut c_void, mode: c_int) -> *const UclObject {
    let i = unsafe { &mut *it.cast::<Iterator>() };
    if i.mode.is_none() {
        i.mode = Some(mode);
        i.sequence = if mode == -1 {
            let mut out = Vec::new();
            let mut p = i.target;
            while !p.is_null() {
                out.push(p);
                p = unsafe { (*p).next };
            }
            out
        } else {
            unsafe { sequence(i.target, mode) }
        };
        // EXPLICIT's first object expansion may restart even after a scalar prefix
        // (released iteration document 6). Arrays and scalar-only chains stay exhausted.
        if mode == 1 {
            let mut p = i.target;
            let mut prefix = 0;
            while !p.is_null() && unsafe { (*p).r#type > 1 } {
                prefix += 1;
                p = unsafe { (*p).next };
            }
            if !p.is_null() && unsafe { (*p).r#type == 0 } && i.sequence.len() > prefix {
                i.restart = Some(prefix);
            }
        }
    }
    if let Some(out) = i.sequence.get(i.index).copied() {
        i.index += 1;
        out
    } else {
        if let Some(index) = i.restart {
            i.index = index;
        }
        ptr::null()
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_iterate_safe(
    it: *mut c_void,
    expand: bool,
) -> *const UclObject {
    boundary(|| unsafe { next(it, if expand { 0 } else { -1 }) })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_iterate_full(it: *mut c_void, mode: c_int) -> *const UclObject {
    boundary(|| unsafe { next(it, mode) })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_iterate_free(it: *mut c_void) {
    boundary(|| unsafe {
        drop(Box::from_raw(it.cast::<Iterator>()));
    });
}
