use crate::boundary;
use serde_ucl::emit::{Emitter, Format, key_needs_quoting};
use serde_ucl::parse::{OutputFacts, PathSegment};
use serde_ucl::value::{UclValue, Value};
use std::cell::OnceCell;
use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr;
use std::rc::Rc;

#[repr(C)]
#[derive(Clone, Copy)]
pub union UclUnion {
    pub iv: i64,
    pub sv: *const c_char,
    pub dv: f64,
    pub av: *mut c_void,
    pub ov: *mut c_void,
    pub ud: *mut c_void,
}

/// Exact LP64 public layout from released spec-v22, C API §3.
#[repr(C)]
pub struct UclObject {
    pub value: UclUnion,
    pub key: *const c_char,
    pub next: *mut UclObject,
    pub prev: *mut UclObject,
    pub keylen: u32,
    pub len: u32,
    pub r#ref: u32,
    pub flags: u16,
    pub r#type: u16,
    pub trash_stack: [*mut u8; 2],
}

struct Model {
    root: UclValue,
    facts: OutputFacts,
}

// The public header is the first field, so every library object can be recovered as a Node.
// Children are owning edges; next/prev and lookup/iterator pointers are borrowed edges.
#[repr(C)]
pub(crate) struct Node {
    pub public: UclObject,
    model: Rc<Model>,
    value: *const UclValue,
    path: Vec<PathSegment>,
    key: Vec<u8>,
    string: Vec<u8>,
    forced: OnceCell<Vec<u8>>,
    pub children: Vec<*mut UclObject>,
    pub heads: Vec<*mut UclObject>,
    lookup: HashMap<String, *mut UclObject>,
}

pub(crate) unsafe fn node<'a>(p: *const UclObject) -> &'a Node {
    unsafe { &*p.cast::<Node>() }
}

fn terminated(s: &[u8]) -> Vec<u8> {
    let mut v = s.to_vec();
    v.push(0);
    v
}

fn allocate(
    model: &Rc<Model>,
    value: &UclValue,
    path: Vec<PathSegment>,
    key: Option<&str>,
    flags: u16,
) -> *mut UclObject {
    let string = value
        .as_str()
        .map_or_else(Vec::new, |s| terminated(s.as_bytes()));
    let key_bytes = key.map_or_else(Vec::new, |k| terminated(k.as_bytes()));
    let (kind, len, union) = match value {
        Value::Object(o) => (
            0,
            o.entries().map(|e| e.len()).sum(),
            UclUnion {
                ov: ptr::null_mut(),
            },
        ),
        Value::Array(a) => (
            1,
            a.len(),
            UclUnion {
                av: ptr::null_mut(),
            },
        ),
        Value::Integer(i) => (2, 0, UclUnion { iv: *i }),
        Value::Float(f) => (3, 0, UclUnion { dv: *f }),
        Value::String(s) => (
            4,
            s.len(),
            UclUnion {
                sv: string.as_ptr().cast(),
            },
        ),
        Value::Boolean(b) => (5, 0, UclUnion { iv: i64::from(*b) }),
        Value::Time(t) => (6, 0, UclUnion { dv: *t }),
        Value::Null => (8, 0, UclUnion { iv: 0 }),
    };
    let facts = model.facts.get(&path);
    let spelling = facts.and_then(|f| f.key_spelling.as_deref()).or(key);
    let key_escape = spelling.is_some_and(|k| {
        facts
            .and_then(|f| f.key_quoted)
            .unwrap_or_else(|| key_needs_quoting(k))
    });
    let mut flags = flags | if key_escape { 4 } else { 0 };
    if let Some(f) = facts {
        flags |= if f.single_quoted { 256 } else { 0 };
        flags |= if f.multiline { 16 } else { 0 };
    }
    let mut n = Box::new(Node {
        public: UclObject {
            value: union,
            key: if key.is_some() {
                key_bytes.as_ptr().cast()
            } else {
                ptr::null()
            },
            next: ptr::null_mut(),
            prev: ptr::null_mut(),
            keylen: key.map_or(0, |s| s.len() as u32),
            len: len as u32,
            r#ref: 1,
            flags,
            r#type: kind,
            trash_stack: [ptr::null_mut(); 2],
        },
        model: model.clone(),
        value,
        path,
        key: key_bytes,
        string,
        forced: OnceCell::new(),
        children: Vec::new(),
        heads: Vec::new(),
        lookup: HashMap::new(),
    });
    let p = &mut n.public as *mut UclObject;
    n.public.prev = p;
    Box::into_raw(n).cast()
}

pub(crate) fn tree(root: UclValue, facts: OutputFacts) -> *mut UclObject {
    let model = Rc::new(Model { root, facts });
    let top = allocate(&model, &model.root, Vec::new(), None, 0);
    let mut work = vec![top];
    while let Some(p) = work.pop() {
        // The Rc model remains alive in every node; its values never move after construction.
        let n = unsafe { &mut *p.cast::<Node>() };
        match unsafe { &*n.value } {
            Value::Object(o) => {
                for (key, entry) in o.iter() {
                    let mut chain = Vec::new();
                    for (index, slot) in entry.slots().iter().enumerate() {
                        let mut path = n.path.clone();
                        path.push(PathSegment::Key {
                            key: key.to_string(),
                            index,
                        });
                        let flags = (u16::from(slot.priority()) << 12)
                            | if slot.is_inherited() { 64 } else { 0 };
                        let child = allocate(&model, slot.value(), path, Some(key), flags);
                        n.children.push(child);
                        chain.push(child);
                        work.push(child);
                    }
                    let head = chain[0];
                    for i in 0..chain.len() {
                        unsafe {
                            (*chain[i]).prev = chain[if i == 0 { chain.len() - 1 } else { i - 1 }];
                            (*chain[i]).next = chain.get(i + 1).copied().unwrap_or(ptr::null_mut());
                        }
                    }
                    // Stage A's multivalue observation applies to scalar heads.
                    if chain.len() > 1 && unsafe { (*head).r#type > 1 } {
                        unsafe {
                            (*head).flags |= 32;
                        }
                    }
                    n.heads.push(head);
                    n.lookup.insert(key.to_string(), head);
                }
            }
            Value::Array(a) => {
                for (index, value) in a.iter().enumerate() {
                    let mut path = n.path.clone();
                    path.push(PathSegment::Index(index));
                    let child = allocate(&model, value, path, None, 0);
                    n.children.push(child);
                    work.push(child);
                }
            }
            _ => {}
        }
    }
    top
}

pub(crate) unsafe fn retain(p: *const UclObject) -> *mut UclObject {
    let p = p.cast_mut();
    if !p.is_null() {
        unsafe {
            (*p).r#ref += 1;
        }
    }
    p
}

pub(crate) unsafe fn release(p: *mut UclObject) {
    let mut work = vec![p];
    while let Some(p) = work.pop() {
        if p.is_null() {
            continue;
        }
        unsafe {
            (*p).r#ref -= 1;
        }
        if unsafe { (*p).r#ref != 0 } {
            continue;
        }
        let mut n = unsafe { Box::from_raw(p.cast::<Node>()) };
        work.append(&mut n.children);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_type(p: *const UclObject) -> c_int {
    boundary(|| {
        if p.is_null() {
            8
        } else {
            unsafe { (*p).r#type.into() }
        }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_type_to_string(t: c_int) -> *const c_char {
    boundary(|| match t {
        0 => c"object".as_ptr(),
        1 => c"array".as_ptr(),
        2 => c"integer".as_ptr(),
        3 | 6 => c"number".as_ptr(),
        4 => c"string".as_ptr(),
        5 => c"boolean".as_ptr(),
        7 => c"userdata".as_ptr(),
        8 => c"null".as_ptr(),
        _ => ptr::null(),
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_string_to_type(s: *const c_char, out: *mut c_int) -> bool {
    boundary(|| {
        let s = unsafe { CStr::from_ptr(s) }.to_bytes();
        let names: [&[u8]; 9] = [
            b"object",
            b"array",
            b"integer",
            b"number",
            b"string",
            b"boolean",
            b"",
            b"userdata",
            b"null",
        ];
        if let Some(i) = names
            .iter()
            .position(|n| !n.is_empty() && s.eq_ignore_ascii_case(n))
        {
            unsafe {
                *out = i as c_int;
            }
            true
        } else {
            false
        }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_array_size(p: *const UclObject) -> u32 {
    boundary(|| {
        if !p.is_null() && unsafe { (*p).r#type == 1 } {
            unsafe { (*p).len }
        } else {
            0
        }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_array_find_index(p: *const UclObject, index: u32) -> *const UclObject {
    boundary(|| {
        if !p.is_null() && unsafe { (*p).r#type == 1 } {
            unsafe { node(p) }
                .children
                .get(index as usize)
                .copied()
                .unwrap_or(ptr::null_mut())
                .cast_const()
        } else {
            ptr::null()
        }
    })
}

unsafe fn numeric(p: *const UclObject) -> Option<f64> {
    if p.is_null() {
        return None;
    }
    match unsafe { (*p).r#type } {
        2 => Some(unsafe { (*p).value.iv as f64 }),
        3 | 6 => Some(unsafe { (*p).value.dv }),
        _ => None,
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_todouble_safe(p: *const UclObject, out: *mut f64) -> bool {
    boundary(|| {
        if let Some(v) = unsafe { numeric(p) } {
            unsafe {
                *out = v;
            }
            true
        } else {
            false
        }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_todouble(p: *const UclObject) -> f64 {
    boundary(|| unsafe { numeric(p) }.unwrap_or(0.0))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_toint_safe(p: *const UclObject, out: *mut i64) -> bool {
    boundary(|| {
        if !p.is_null() && unsafe { (*p).r#type == 2 } {
            unsafe {
                *out = (*p).value.iv;
            }
            true
        } else if let Some(v) = unsafe { numeric(p) } {
            unsafe {
                *out = v as i64;
            }
            true
        } else {
            false
        }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_toint(p: *const UclObject) -> i64 {
    boundary(|| {
        let mut v = 0;
        unsafe {
            ucl_object_toint_safe(p, &mut v);
        }
        v
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_toboolean_safe(p: *const UclObject, out: *mut bool) -> bool {
    boundary(|| {
        if !p.is_null() && unsafe { (*p).r#type == 5 } {
            unsafe {
                *out = (*p).value.iv != 0;
            }
            true
        } else {
            false
        }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_toboolean(p: *const UclObject) -> bool {
    boundary(|| !p.is_null() && unsafe { (*p).r#type == 5 && (*p).value.iv != 0 })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_tostring_safe(
    p: *const UclObject,
    out: *mut *const c_char,
) -> bool {
    boundary(|| {
        if !p.is_null() && unsafe { (*p).r#type == 4 } {
            unsafe {
                *out = (*p).value.sv;
            }
            true
        } else {
            false
        }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_tostring(p: *const UclObject) -> *const c_char {
    boundary(|| {
        if !p.is_null() && unsafe { (*p).r#type == 4 } {
            unsafe { (*p).value.sv }
        } else {
            ptr::null()
        }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_tolstring_safe(
    p: *const UclObject,
    out: *mut *const c_char,
    len: *mut usize,
) -> bool {
    boundary(|| {
        if unsafe { ucl_object_tostring_safe(p, out) } {
            unsafe {
                *len = (*p).len as usize;
            }
            true
        } else {
            false
        }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_tolstring(
    p: *const UclObject,
    len: *mut usize,
) -> *const c_char {
    boundary(|| {
        let mut out = ptr::null();
        unsafe {
            ucl_object_tolstring_safe(p, &mut out, len);
        }
        out
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_tostring_forced(p: *const UclObject) -> *const c_char {
    boundary(|| {
        let n = unsafe { node(p) };
        if n.public.r#type == 4 {
            return n.public.value_sv();
        }
        n.forced
            .get_or_init(|| {
                let text = match unsafe { &*n.value } {
                    Value::Integer(i) => i.to_string(),
                    Value::Float(f) | Value::Time(f) => {
                        if f.fract() == 0.0 {
                            format!("{f:.1}")
                        } else {
                            format!("{f:.6}")
                        }
                    }
                    Value::Boolean(b) => b.to_string(),
                    Value::Null => "null".into(),
                    Value::Array(_) => "array".into(),
                    Value::Object(_) => "object".into(),
                    Value::String(_) => unreachable!(),
                };
                terminated(text.as_bytes())
            })
            .as_ptr()
            .cast()
    })
}
impl UclObject {
    fn value_sv(&self) -> *const c_char {
        unsafe { self.value.sv }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_lookup(
    p: *const UclObject,
    key: *const c_char,
) -> *const UclObject {
    boundary(|| {
        if p.is_null() {
            return ptr::null();
        }
        let bytes = unsafe { CStr::from_ptr(key) }.to_bytes();
        unsafe { ucl_object_lookup_len(p, key, bytes.len()) }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_lookup_len(
    p: *const UclObject,
    key: *const c_char,
    len: usize,
) -> *const UclObject {
    boundary(|| {
        if p.is_null() || unsafe { (*p).r#type != 0 } {
            return ptr::null();
        }
        let bytes = if len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(key.cast(), len) }
        };
        let Ok(s) = std::str::from_utf8(bytes) else {
            return ptr::null();
        };
        unsafe { node(p) }
            .lookup
            .get(s)
            .copied()
            .unwrap_or(ptr::null_mut())
            .cast_const()
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_key(p: *const UclObject) -> *const c_char {
    boundary(|| {
        if p.is_null() {
            ptr::null()
        } else {
            unsafe { (*p).key }
        }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_keyl(p: *const UclObject, len: *mut usize) -> *const c_char {
    boundary(|| unsafe {
        *len = if p.is_null() { 0 } else { (*p).keylen as usize };
        ucl_object_key(p)
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_ref(p: *const UclObject) -> *mut UclObject {
    boundary(|| unsafe { retain(p) })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_unref(p: *mut UclObject) {
    boundary(|| unsafe { release(p) });
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_emit(p: *const UclObject, format: c_int) -> *mut u8 {
    boundary(|| {
        let mut len = 0;
        unsafe { ucl_object_emit_len(p, format, &mut len) }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ucl_object_emit_len(
    p: *const UclObject,
    format: c_int,
    len: *mut usize,
) -> *mut u8 {
    boundary(|| {
        if p.is_null() {
            return ptr::null_mut();
        }
        let format = match format {
            0 => Format::Json,
            1 => Format::JsonCompact,
            2 => Format::Config,
            3 => Format::Yaml,
            _ => return ptr::null_mut(),
        };
        let n = unsafe { node(p) };
        let output = Emitter::new(format)
            .with_facts_path(&n.model.facts, &n.path)
            .emit(unsafe { &*n.value });
        let out = unsafe { libc::malloc(output.len() + 1) }.cast::<u8>();
        if out.is_null() {
            return out;
        }
        unsafe {
            ptr::copy_nonoverlapping(output.as_ptr(), out, output.len());
            *out.add(output.len()) = 0;
            *len = output.len();
        }
        out
    })
}
