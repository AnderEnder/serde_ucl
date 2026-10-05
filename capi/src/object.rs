use crate::boundary;
use serde_ucl::emit::{Emitter, Format, key_needs_quoting};
use serde_ucl::parse::{FactsCursor, OutputFacts};
use serde_ucl::value::{UclValue, Value};
use std::cell::RefCell;
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

/// Owns the adapter's immutable memory input without an exclusive Box reference
/// being recreated while its parsed strings borrow the allocation.
///
/// The owner moves into Model before publication and is its last field to drop.
pub(crate) struct OwnedInput(std::ptr::NonNull<[u8]>);

impl OwnedInput {
    pub(crate) fn copy(bytes: &[u8]) -> Self {
        Self(Box::into_non_null(Box::<[u8]>::from(bytes)))
    }

    /// The tree may use the artificial static lifetime only inside its Model.
    /// The caller must transfer this owner into that Model, whose value drops
    /// before this backing allocation. No borrowed Rust value may escape it.
    pub(crate) unsafe fn text(&self) -> &'static [u8] {
        unsafe { self.0.as_ref() }
    }
}

impl Drop for OwnedInput {
    fn drop(&mut self) {
        // All borrowed values have already been destroyed. Recover exactly the
        // Box ownership transferred by into_non_null, rather than unleaking.
        drop(unsafe { Box::from_non_null(self.0) });
    }
}

// The model and all public addresses remain stable until the last owned node is released.
// Keeping the original immutable Rust value also preserves retained-subtree emission facts.
struct Model {
    root: Rc<UclValue>,
    facts: OutputFacts,
    nodes: Vec<Node>,
    containers: Vec<Container>,
    string_facts: [FactsCursor; 4],
    links: Vec<*mut UclObject>,
    bytes: Vec<u8>,
    external: usize,
    forced: RefCell<HashMap<usize, Vec<u8>>>,
    // Field drop order is significant: root and every parsed view precede backing text.
    _input: Option<OwnedInput>,
}

#[repr(C)]
pub(crate) struct Node {
    pub public: UclObject,
}

// Only containers need child ranges, entry-head metadata and subtree fact cursors.
struct Container {
    children: *mut Node,
    heads: *const *mut UclObject,
    head_count: usize,
    facts: FactsCursor,
}

const _: () = assert!(std::mem::size_of::<Node>() == std::mem::size_of::<UclObject>());

/// An immutable range of arena addresses. No Node references are created: ownership
/// release may update child headers while traversing the range.
#[derive(Clone, Copy)]
pub(crate) struct NodeRange {
    start: *mut Node,
    len: usize,
}

impl NodeRange {
    pub fn get(self, index: usize) -> Option<*mut UclObject> {
        (index < self.len).then(|| unsafe { self.start.add(index).cast() })
    }

    pub fn iter(self) -> impl Iterator<Item = *mut UclObject> {
        (0..self.len).map(move |index| unsafe { self.start.add(index).cast() })
    }
}

impl Node {
    fn container(&self) -> *mut Container {
        unsafe { self.public.value.ov.cast() }
    }

    #[inline]
    pub fn children(&self) -> NodeRange {
        if self.public.r#type < 2 {
            NodeRange {
                start: unsafe { (*self.container()).children },
                len: self.public.len as usize,
            }
        } else {
            NodeRange {
                start: ptr::null_mut(),
                len: 0,
            }
        }
    }

    #[inline]
    pub fn head_count(&self) -> usize {
        unsafe { (*self.container()).head_count }
    }

    #[inline]
    pub fn head(&self, index: usize) -> Option<*mut UclObject> {
        let container = unsafe { &*self.container() };
        if index >= container.head_count {
            None
        } else if container.heads.is_null() {
            Some(unsafe { container.children.add(index).cast() })
        } else {
            Some(unsafe { *container.heads.add(index) })
        }
    }

    fn facts(&self) -> FactsCursor {
        if self.public.r#type < 2 {
            unsafe { (*self.container()).facts }
        } else if self.public.r#type == 4 {
            // A scalar emission has no key or descendants. Its string facts depend
            // only on these two public flags, so any cursor with the same facts works.
            let index = usize::from(self.public.flags & 256 != 0)
                | (usize::from(self.public.flags & 16 != 0) << 1);
            unsafe { (*self.model()).string_facts[index] }
        } else {
            FactsCursor::empty()
        }
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
        key_ptr: *const c_char,
        flags: u16,
    ) -> *mut UclObject {
        let p = unsafe { self.nodes.as_mut_ptr().add(self.nodes.len()) }.cast::<UclObject>();
        let (kind, len, mut union) = match value {
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
        if kind < 2 {
            let container = unsafe { self.containers.as_mut_ptr().add(self.containers.len()) };
            self.containers.push(Container {
                // During construction this field queues this container's own node.
                // Once processed it holds the first child; no read sees the queue.
                children: p.cast(),
                heads: ptr::null(),
                head_count: value.as_object().map_or(len, |object| object.len()),
                facts,
            });
            union.ov = container.cast();
        } else if kind == 4 {
            let index = usize::from(flags & 256 != 0) | (usize::from(flags & 16 != 0) << 1);
            if index != 0 {
                self.string_facts[index] = facts;
            }
        }
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
                ptr::null_mut(),
                (value as *const UclValue).cast_mut().cast(),
            ],
        };
        public.prev = p;
        self.nodes.push(Node { public });
        p
    }

    fn links(&self, start: usize) -> *const *mut UclObject {
        // Exact pre-counting ensures links and nodes never reallocate during construction.
        unsafe { self.links.as_ptr().add(start) }
    }
}

pub(crate) fn tree(
    root: UclValue,
    facts: OutputFacts,
    input: Option<OwnedInput>,
) -> *mut UclObject {
    let mut count = 1;
    let mut container_count = 0;
    let mut link_count = 0;
    let mut byte_count = 0;
    let mut work = vec![&root];
    while let Some(value) = work.pop() {
        match value {
            Value::Object(o) => {
                container_count += 1;
                let mut values = 0;
                for (key, entry) in o.iter() {
                    byte_count += key.len() + 1;
                    values += entry.len();
                    for slot in entry.slots() {
                        match slot.value() {
                            Value::Object(_) | Value::Array(_) => work.push(slot.value()),
                            Value::String(s) => byte_count += s.len() + 1,
                            _ => {}
                        }
                    }
                }
                count += values;
                link_count += if values == o.len() { 0 } else { o.len() };
            }
            Value::Array(a) => {
                container_count += 1;
                count += a.len();
                for value in a.iter() {
                    match value {
                        Value::Object(_) | Value::Array(_) => work.push(value),
                        Value::String(s) => byte_count += s.len() + 1,
                        _ => {}
                    }
                }
            }
            Value::String(s) => byte_count += s.len() + 1,
            _ => {}
        }
    }
    let mut model = Box::new(Model {
        root: Rc::new(root),
        facts,
        nodes: Vec::with_capacity(count),
        containers: Vec::with_capacity(container_count),
        string_facts: [FactsCursor::empty(); 4],
        links: Vec::with_capacity(link_count),
        bytes: Vec::with_capacity(byte_count),
        external: 1,
        forced: RefCell::new(HashMap::new()),
        _input: input,
    });
    let value = &*model.root as *const UclValue;
    let top = model.allocate(
        unsafe { &*value },
        FactsCursor::root(),
        None,
        ptr::null(),
        0,
    );
    let mut index = 0;
    // Container metadata doubles as the construction queue. Scalar headers need
    // no further work and are never visited a second time.
    while index < model.containers.len() {
        let container = unsafe { model.containers.as_mut_ptr().add(index) };
        let current = unsafe { (*container).children };
        let value = unsafe { (*current).value() };
        let facts = unsafe { (*container).facts };
        let start = model.nodes.len();
        match unsafe { &*value } {
            Value::Object(o) => {
                let heads_start = model.links.len();
                let has_duplicates = unsafe { (*current).public.len as usize } != o.len();
                for (entry_index, (key, entry)) in o.iter().enumerate() {
                    let key_ptr = model.text(key);
                    let mut head: *mut UclObject = ptr::null_mut();
                    let mut previous: *mut UclObject = ptr::null_mut();
                    for (slot_index, slot) in entry.slots().iter().enumerate() {
                        let cursor = facts.entry(&model.facts, entry_index, key, slot_index);
                        let flags = (u16::from(slot.priority()) << 12)
                            | if slot.is_inherited() { 64 } else { 0 };
                        let child = model.allocate(slot.value(), cursor, Some(key), key_ptr, flags);
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
                    if has_duplicates {
                        model.links.push(head);
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
                unsafe {
                    (*(*current).container()).children = model.nodes.as_mut_ptr().add(start);
                    if has_duplicates {
                        (*(*current).container()).heads = model.links(heads_start);
                    }
                }
            }
            Value::Array(a) => {
                for (i, value) in a.iter().enumerate() {
                    let cursor = facts.element(&model.facts, i);
                    model.allocate(value, cursor, None, ptr::null(), 0);
                }
                unsafe {
                    (*(*current).container()).children = model.nodes.as_mut_ptr().add(start);
                }
            }
            _ => {}
        }
        index += 1;
    }
    debug_assert_eq!(model.nodes.len(), count);
    debug_assert_eq!(model.links.len(), link_count);
    debug_assert_eq!(model.bytes.len(), byte_count);
    // A pointer derived from allocate(&mut self) would be invalidated by the next
    // exclusive borrow of Model. Publish only the final Box::into_raw provenance,
    // after construction; Model is subsequently accessed through raw field pointers.
    let model = Box::into_raw(model);
    let nodes = unsafe { (*model).nodes.as_mut_ptr() };
    for index in 0..count {
        unsafe {
            (*nodes.add(index)).public.trash_stack[0] = model.cast();
        }
    }
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
        for child in unsafe { node(current) }.children().iter() {
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
        let mut forced = unsafe { &(*n.model()).forced }.borrow_mut();
        let bytes = forced.entry(p.addr()).or_insert_with(|| {
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
            terminated(text.as_bytes())
        });
        bytes.as_ptr().cast()
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
            .and_then(|index| unsafe { node(p) }.head(index))
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
            .with_facts_cursor(unsafe { &(*n.model()).facts }, n.facts())
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
