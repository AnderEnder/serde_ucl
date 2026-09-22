# 8. Repeated keys, priorities and duplicate strategies

Cases: `tests/conformance/cases/spec/08-duplicates/`.

## 8.1 The value model

Each object maps keys to **one or more** values:

- Keys keep the order of their first appearance. When a key appears again, its entry stays where it
  first appeared (`keeps_first_position`).
- A key holding several values is a **multi-value entry**, libucl's "implicit array". It is not the
  same thing as an explicit array (`[…]`), and the two are never merged into each other:
  `a = [1, 2]⏎a = 3` → `{ a: ⟨[int 1, int 2] | int 3⟩ }` (`explicit_array_not_flattened`,
  `cases/additions/v02_explicit_then_scalar`).
- The values of one entry may have different types (`mixed_types`, `libucl/basic/1`, `libucl/basic/2`).
- Every value, containers included, carries a **priority** from 0 to 15 (§8.3).

Repeated keys are detected within a single object only. The same key in different objects is
unrelated (`repeated_inside_array_object`).

## 8.2 Default behaviour: append

With the default strategy (`append`) and equal priorities, a repeated key adds its value to the
existing entry, in order:

- `a = 1⏎a = 2⏎a = 3` → `{ a: ⟨int 1 | int 2 | int 3⟩ }` (`scalar_repeated`, `cases/review/17_multikey`)
- Objects are **not** merged: `a { x = 1 }⏎a { y = 2 }` → `{ a: ⟨{x: int 1} | {y: int 2}⟩ }`
  (`objects_repeated_not_merged`, `libucl/basic/issue312`, `libucl/basic/comments`)
- The same holds for named sections (§3.4). The outermost name receives another value, and nothing
  deeper is merged: `a b { x = 1 }⏎a c { y = 2 }⏎a b { z = 3 }` → `a` holds three objects
  (`sections_repeated_not_merged`, `repeated_inside_braces`, `cases/review/10_dupnested`,
  `cases/additions/a27_nested_dup_in_braces`, `libucl/basic/8`, `libucl/basic/10`).

## 8.3 Priorities

Values from the main document have priority 0, unless the input unit is given another priority
(`chunk_priority`, where the root object itself carries priority 5). A `.priority` macro changes the
priority of the values that follow it in the same input unit (§9.5), and `.include(priority=N)`
sets it for an included file (§9.4). Containers carry the priority too (`priority_applies_to_containers`).

Priority values are kept modulo 16: `.priority 20` → 4, `.priority 16` → 0, `.priority -1` → 15
(`priority_modulo_16`, `priority_16_is_0`, `priority_negative`, `priority_15`).

Under `append`, a repeated key compares the new value's priority with the priority of the
**first** value already held:

| New priority is … | Effect | Cases |
| --- | --- | --- |
| equal | the value is added to the entry | `priority_equal_appends` |
| higher | the entry is replaced by the new value alone | `priority_higher_replaces`, `libucl/basic/13` |
| lower | the new value is discarded | `priority_lower_ignored`, `libucl/basic/15`, `libucl/basic/16` |

An **inherited** value (§9.7) is always replaced by a later explicit value of the same key,
whatever the priorities (`cases/spec/09-macros/inherit_basic`,
`cases/spec/09-macros/inherit_replaced_whatever_priority`, `libucl/basic/18`).

## 8.4 Other strategies

An input unit may use another strategy. The main document can be given one (`strategy:` in
`.flags`), and an included file gets one through `.include(duplicate=…)` (§9.4, `libucl/basic/19`).

**rewrite**: a repeated key replaces the whole entry, whatever the priorities:
`a = 1⏎a = 2` → `{ a: int 2 }`; `b { x = 1 }⏎b { y = 2 }` → `{ b: { y: int 2 } }` (`strategy_rewrite`).

**error**: any repeated key is an error (`strategy_error`); a document without repeats parses
normally (`strategy_error_no_duplicates`).

**merge**: the result depends on the existing first value.

- **An object**: the new object's entries go into the existing object, each by the same rules, so
  repeated scalars inside append: `a { x = 1; z = 3 }⏎a { x = 2; y = 4 }` →
  `{ a: { x: ⟨int 1 | int 2⟩, z: int 3, y: int 4 } }` (`strategy_merge_objects`). Priorities are
  not compared for this; the merge happens whether the new unit's priority is higher or lower
  (`cases/spec/09-macros/include_merge_ignores_priority`,
  `cases/spec/09-macros/include_merge_lower_priority_still_merges`).
- **An array**: a new array's elements are appended to it, whatever the priorities. Each element
  keeps its own priority, and the array keeps the existing array's priority: `a = [1]⏎a = [2]` →
  `{ a: [int 1, int 2] }`; with `.priority 3` before the first and `.priority 1` before the second,
  `{ a: [int 1 @3, int 2 @1] @3 }` (`strategy_merge_arrays`, `strategy_merge_arrays_ignore_priority`).
- **An array, with an object as the new value**, and **an object, with an array as the new
  value**: error (`strategy_merge_array_then_object_error`, `strategy_merge_object_then_array_error`).
- **Quirk. An object or array, with a scalar as the new value**: the container is replaced by the
  scalar, whatever the priorities, and the result keeps the container's priority:
  `a { x = 1 }⏎a = 2` → `{ a: int 2 }`; `a = [1]⏎a = 2` → `{ a: int 2 }`; an object at priority 3
  followed by `a = 2` at priority 1 → `a: int 2 @3`, and at priorities 1 and 3 → `a: int 2 @1`
  (`strategy_merge_object_then_scalar`, `strategy_merge_array_then_scalar`,
  `strategy_merge_scalar_keeps_container_priority`).
- **A scalar**: behaves like `append`, priorities included: `a = 1⏎a = 2` → `⟨int 1 | int 2⟩`;
  `a = 1⏎a { y = 2 }` → `⟨int 1 | {y: int 2}⟩`; a lower priority is dropped and a higher one
  replaces (`strategy_merge_scalars_append`, `strategy_merge_scalar_then_object`,
  `cases/spec/09-macros/include_merge_scalar_lower_priority_dropped`,
  `cases/spec/09-macros/include_merge_scalar_higher_priority_replaces`).

## 8.5 Without implicit arrays

With `no-implicit-arrays` set (§12.4), repeated values are collected into an **explicit array**
instead of a multi-value entry. The first repeat turns the entry into an array of the old and new
values, and later repeats are appended. Existing arrays are elements, not flattened:

- `a = 1⏎a = 2⏎a = 3` → `{ a: [int 1, int 2, int 3] }` (`no_implicit_arrays_scalars`)
- `a = [1]⏎a = 2` → `{ a: [[int 1], int 2] }`; `b = 1⏎b = [2]` → `{ b: [int 1, [int 2]] }`
  (`no_implicit_arrays_with_arrays`)
- `a { x = 1 }⏎a { y = 2 }` → `{ a: [{x: int 1}, {y: int 2}] }` (`no_implicit_arrays_objects`)

## 8.6 Key comparison

Keys are compared byte for byte. With `key-lowercase` set, keys are lowercased first (ASCII only),
so `A` and `a` are the same key (`cases/spec/12-flags/key_lowercase_merges_case`).
