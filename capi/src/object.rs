use crate::boundary;
use serde_ucl::emit::{Emitter, Format, key_needs_quoting};
use serde_ucl::parse::{FactsCursor, OutputFacts};
use serde_ucl::value::{UclValue, Value};
use std::cell::{OnceCell, RefCell};
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

// The model and all public addresses remain stable until the last owned node is released.
// Keeping the original immutable Rust value also preserves retained-subtree emission facts.
struct Model {
    root: Rc<UclValue>,
    facts: OutputFacts,
    nodes: Vec<Node>,
    links: Vec<*mut UclObject>,
    bytes: Vec<u8>,
    external: usize,
    forced: RefCell<Vec<Vec<u8>>>,
}

#[repr(C)]
pub(crate) struct Node {
    pub public: UclObject,
    facts: FactsCursor,
    forced: OnceCell<std::ptr::NonNull<c_char>>,
    links: *const *mut UclObject,
}

impl Node {
    pub fn children(&self) -> &[*mut UclObject] {
        let count = if self.public.r#type < 2 {
            self.public.len as usize
        } else {
            0
        };
        // The private immutable link buffer outlives every node and includes this exact range.
        unsafe { std::slice::from_raw_parts(self.links, count) }
    }
    pub fn heads(&self) -> &[*mut UclObject] {
        let count = unsafe { &*self.value() }
            .as_object()
            .map_or(0, |object| object.len());
        let heads = if self.public.r#type == 0 {
            unsafe { self.public.value.ov.cast::<*mut UclObject>() }
        } else {
            self.links.cast_mut()
        };
        unsafe { std::slice::from_raw_parts(heads, count) }
    }
    fn model(&self) -> *mut Model {
        self.public.trash_stack[0].cast()
    }
    fn value(&self) -> *const UclValue {
        self.public.trash_stack[1].cast()
    }
}

pub(crate) unsafe fn node<'a>(p: *const UclObject) -> &'a Node {
    unsafe { &*p.cast::<Node>() }
}

fn terminated(s: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(s.len() + 1);
    v.extend_from_slice(s);
    v.push(0);
    v
}

impl Model {
    fn text(&mut self, s: &str) -> *const c_char {
        let start = self.bytes.len();
        self.bytes.extend_from_slice(s.as_bytes());
        self.bytes.push(0);
        unsafe { self.bytes.as_ptr().add(start).cast() }
    }

    fn allocate(
        &mut self,
        value: &UclValue,
        facts: FactsCursor,
        key: Option<&str>,
        flags: u16,
    ) -> *mut UclObject {
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
            Value::String(s) => (4, s.len(), UclUnion { sv: self.text(s) }),
            Value::Boolean(b) => (5, 0, UclUnion { iv: i64::from(*b) }),
            Value::Time(t) => (6, 0, UclUnion { dv: *t }),
            Value::Null => (8, 0, UclUnion { iv: 0 }),
        };
        let observed = facts.get(&self.facts);
        let spelling = observed.and_then(|f| f.key_spelling.as_deref()).or(key);
        let key_escape = spelling.is_some_and(|k| {
            observed
                .and_then(|f| f.key_quoted)
                .unwrap_or_else(|| key_needs_quoting(k))
        });
        let mut flags = flags | if key_escape { 4 } else { 0 };
        if let Some(f) = observed {
            flags |= if f.single_quoted { 256 } else { 0 };
            flags |= if f.multiline { 16 } else { 0 };
        }
        let key_ptr = key.map_or(ptr::null(), |key| self.text(key));
        let mut public = UclObject {
            value: union,
            key: key_ptr,
            next: ptr::null_mut(),
            prev: ptr::null_mut(),
            keylen: key.map_or(0, |s| s.len() as u32),
            len: len as u32,
            r#ref: 1,
            flags,
            r#type: kind,
            trash_stack: [
                (self as *mut Model).cast(),
                (value as *const UclValue).cast_mut().cast(),
            ],
        };
        let p = unsafe { self.nodes.as_mut_ptr().add(self.nodes.len()) }.cast::<UclObject>();
        public.prev = p;
        self.nodes.push(Node {
            public,
            facts,
            forced: OnceCell::new(),
            links: self.links.as_ptr(),
        });
        p
    }

    fn links(&self, start: usize) -> *const *mut UclObject {
        // Exact pre-counting ensures links and nodes never reallocate during construction.
        unsafe { self.links.as_ptr().add(start) }
    }
}

pub(crate) fn tree(root: UclValue, facts: OutputFacts) -> *mut UclObject {
    let mut count = 0;
    let mut link_count = 0;
    let mut byte_count = 0;
    let mut work = vec![&root];
    while let Some(value) = work.pop() {
        count += 1;
        match value {
            Value::Object(o) => {
                let values = o.entries().map(|entry| entry.len()).sum::<usize>();
                link_count += values + if values == o.len() { 0 } else { o.len() };
                for (key, entry) in o.iter() {
                    byte_count += (key.len() + 1) * entry.len();
                    work.extend(entry.slots().iter().map(|slot| slot.value()));
                }
            }
            Value::Array(a) => {
                link_count += a.len();
                work.extend(a.iter());
            }
            Value::String(s) => byte_count += s.len() + 1,
            _ => {}
        }
    }
    let mut model = Box::new(Model {
        root: Rc::new(root),
        facts,
        nodes: Vec::with_capacity(count),
        links: Vec::with_capacity(link_count),
        bytes: Vec::with_capacity(byte_count),
        external: 1,
        forced: RefCell::new(Vec::new()),
    });
    let value = &*model.root as *const UclValue;
    let top = model.allocate(unsafe { &*value }, FactsCursor::root(), None, 0);
    let mut index = 0;
    // Appended children are visited in order. No recursive calls or per-container work lists.
    while index < model.nodes.len() {
        let value = model.nodes[index].value();
        let facts = model.nodes[index].facts;
        let start = model.links.len();
        match unsafe { &*value } {
            Value::Object(o) => {
                for (entry_index, (key, entry)) in o.iter().enumerate() {
                    let mut head: *mut UclObject = ptr::null_mut();
                    let mut previous: *mut UclObject = ptr::null_mut();
                    for (slot_index, slot) in entry.slots().iter().enumerate() {
                        let cursor = facts.entry(&model.facts, entry_index, key, slot_index);
                        let flags = (u16::from(slot.priority()) << 12)
                            | if slot.is_inherited() { 64 } else { 0 };
                        let child = model.allocate(slot.value(), cursor, Some(key), flags);
                        model.links.push(child);
                        if head.is_null() {
                            head = child;
                        } else {
                            unsafe {
                                (*previous).next = child;
                                (*child).prev = previous;
                            }
                        }
                        previous = child;
                    }
                    unsafe {
                        (*head).prev = previous;
                    }
                    if entry.len() > 1 && unsafe { (*head).r#type > 1 } {
                        unsafe {
                            (*head).flags |= 32;
                        }
                    }
                }
                let end = model.links.len();
                // A singleton entry's child is also its head; share the complete range.
                let heads = if end - start == o.len() {
                    start
                } else {
                    for child in start..end {
                        let p = model.links[child];
                        if child == start || unsafe { (*model.links[child - 1]).next != p } {
                            model.links.push(p);
                        }
                    }
                    end
                };
                model.nodes[index].links = model.links(start);
                model.nodes[index].public.value.ov = model.links(heads).cast_mut().cast();
            }
            Value::Array(a) => {
                for (i, value) in a.iter().enumerate() {
                    let cursor = facts.element(&model.facts, i);
                    let child = model.allocate(value, cursor, None, 0);
                    model.links.push(child);
                }
                model.nodes[index].links = model.links(start);
            }
            _ => {}
        }
        index += 1;
    }
    debug_assert_eq!(model.nodes.len(), count);
    debug_assert_eq!(model.links.len(), link_count);
    debug_assert_eq!(model.bytes.len(), byte_count);
    let _ = Box::into_raw(model);
    top
}

pub(crate) unsafe fn retain(p: *const UclObject) -> *mut UclObject {
    let p = p.cast_mut();
    if !p.is_null() {
        unsafe {
            (*p).r#ref += 1;
            (*node(p).model()).external += 1;
        }
    }
    p
}

pub(crate) unsafe fn release(p: *mut UclObject) {
    if p.is_null() {
        return;
    }
    let model = unsafe { node(p).model() };
    unsafe {
        (*model).external -= 1;
    }
    // With no parser or separately granted reference left, no caller can observe the
    // headers again. Drop the complete arena without visiting its owning child edges.
    if unsafe { (*model).external == 0 } {
        drop(unsafe { Box::from_raw(model) });
        return;
    }
    unsafe {
        (*p).r#ref -= 1;
    }
    if unsafe { (*p).r#ref != 0 } {
        return;
    }
    let mut pending = p;
    // Reserved opaque storage is an intrusive stack for dead nodes. Public sibling links
    // and all retained nodes remain untouched; destruction requires no allocations.
    unsafe {
        (*pending).trash_stack[0] = ptr::null_mut();
    }
    while !pending.is_null() {
        let current = pending;
        pending = unsafe { (*current).trash_stack[0].cast() };
        for &child in unsafe { node(current) }.children() {
            unsafe {
                (*child).r#ref -= 1;
            }
            if unsafe { (*child).r#ref == 0 } {
                unsafe {
                    (*child).trash_stack[0] = pending.cast();
                }
                pending = child;
            }
        }
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
                .children()
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
                let text = match unsafe { &*n.value() } {
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
                let bytes = terminated(text.as_bytes());
                let pointer = std::ptr::NonNull::new(bytes.as_ptr().cast_mut().cast())
                    .expect("terminated storage");
                unsafe { &(*n.model()).forced }.borrow_mut().push(bytes);
                pointer
            })
            .as_ptr()
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
        unsafe { &*node(p).value() }
            .as_object()
            .and_then(|object| object.index_of(s))
            .and_then(|index| unsafe { node(p) }.heads().get(index).copied())
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
            .with_facts_cursor(unsafe { &(*n.model()).facts }, n.facts)
            .emit(unsafe { &*n.value() });
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
