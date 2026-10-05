use crate::boundary;
use crate::object::{UclObject, node};
use std::ffi::{c_int, c_void};
use std::ptr;

struct OldObject {
    index: usize,
}
struct Iterator {
    current: *const UclObject,
    index: usize,
    mode: Option<c_int>,
    restart: *const UclObject,
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
            let out = unsafe { node(p) }.head(cursor.index);
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
            current: p,
            index: 0,
            mode: None,
            restart: ptr::null(),
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
        i.current = p;
        i.index = 0;
        i.mode = None;
        i.restart = ptr::null();
        it
    })
}
unsafe fn next(it: *mut c_void, mode: c_int) -> *const UclObject {
    let i = unsafe { &mut *it.cast::<Iterator>() };
    // The first call selects the traversal until reset, as in the released matrix.
    let mode = *i.mode.get_or_insert(mode);
    if mode == 0 {
        let current = i.current;
        if current.is_null() {
            return ptr::null();
        }
        let kind = unsafe { (*current).r#type };
        if kind > 1 {
            i.current = unsafe { (*current).next };
            return current;
        }
        let container = unsafe { node(current) };
        let child = if kind == 0 {
            container.head(i.index)
        } else {
            container.children().get(i.index)
        };
        if let Some(child) = child {
            i.index += 1;
            return child.cast_const();
        }
        i.current = ptr::null();
        i.index = 0;
        return ptr::null();
    }
    loop {
        let current = i.current;
        if current.is_null() {
            // EXPLICIT restarts only the first nonempty object's children, after NULL.
            // A scalar prefix is not replayed; arrays and empty objects stay exhausted.
            i.current = i.restart;
            i.index = 0;
            return ptr::null();
        }
        let kind = unsafe { (*current).r#type };
        if mode == -1 || kind > 1 {
            i.current = unsafe { (*current).next };
            return current;
        }
        let container = unsafe { node(current) };
        if mode == 1 && kind == 0 && container.head_count() != 0 {
            i.restart = current;
        }
        let child = if kind == 0 {
            container.head(i.index)
        } else {
            container.children().get(i.index)
        };
        if let Some(child) = child {
            i.index += 1;
            return child.cast_const();
        }
        i.index = 0;
        i.current = if mode == 0 || mode == 1 {
            ptr::null()
        } else {
            unsafe { (*current).next }
        };
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
