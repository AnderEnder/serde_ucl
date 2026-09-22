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

This rule belongs to `append` only. Under the other strategies (§8.4) an inherited value is
treated like any other value:

| Strategy | `d { a = 1 }⏎e { .inherit "d"; a = 2 }` gives `e` … | Cases |
| --- | --- | --- |
| `merge` | `{ a: ⟨int 1 \| int 2⟩ }`: equal priorities add; a higher priority replaces, a lower one is dropped; objects merge | `inherit_merge_equal_adds`, `inherit_merge_higher_replaces`, `inherit_merge_lower_dropped`, `inherit_merge_objects` |
| `rewrite` | `{ a: int 2 }` | `inherit_strategy_rewrite` |
| `error` | **error** | `inherit_strategy_error` |

With `no-implicit-arrays` (§12.4) the `append` rule still applies: the inherited value is
replaced, not collected (`nia_inherited_replaced`).

A value stays inherited when a `merge` goes into it, or when the §8.4 scalar quirk replaces it, so
a later explicit value under `append` still replaces it:
`d { a { x = 1 } }⏎e { .inherit "d"; <merge include of a = 5>; a = 7 }` → `e: { a: int 7 }`, and
with a merge include of `a { y = 2 }` followed by `a { z = 3 }` → `e: { a: { z: int 3 } }`
(`inherit_kept_after_merge_scalar`, `inherit_kept_after_merge_object`). It stops being inherited
once another value replaces it (a higher priority, `rewrite`, or this rule).

## 8.4 Other strategies

An input unit may use another strategy. The main document can be given one (`strategy:` in
`.flags`), and an included file gets one through `.include(duplicate=…)` (§9.4, `libucl/basic/19`).

**rewrite**: a repeated key replaces the whole entry, whatever the priorities:
`a = 1⏎a = 2` → `{ a: int 2 }`; `b { x = 1 }⏎b { y = 2 }` → `{ b: { y: int 2 } }` (`strategy_rewrite`).

**error**: any repeated key is an error (`strategy_error`); a document without repeats parses
normally (`strategy_error_no_duplicates`).

**merge**: the result depends on the existing **first** value. When the entry already holds
several values, only the first takes part; the others stay after it, unchanged. With `a { x = 1 }⏎a { y = 2 }`
in the main document and a merge include of `a { z = 3 }`, `a` becomes
`⟨{x: int 1, z: int 3} | {y: int 2}⟩`; with a merge include of `a = 5`, it becomes
`⟨int 5 | {y: int 2}⟩`, and with two arrays and a merge include of `a = [9]`,
`⟨[int 1, int 9] | [int 2]⟩` (`merge_include_first_value_only`,
`merge_include_first_value_scalar`, `merge_include_first_array_only`). A scalar first value
followed by an object adds the object as a further value: `a = 1⏎a = 2⏎a { y = 2 }` under `merge`
→ `a: ⟨int 1 | int 2 | {y: int 2}⟩` (`strategy_merge_first_scalar_of_several`).

- **An object**: the new object's entries go into the existing object, each by the same rules, so
  repeated scalars inside append: `a { x = 1; z = 3 }⏎a { x = 2; y = 4 }` →
  `{ a: { x: ⟨int 1 | int 2⟩, z: int 3, y: int 4 } }` (`strategy_merge_objects`). Priorities are
  not compared for the objects themselves; the merge happens whether the new unit's priority is
  higher or lower, and the existing object keeps its priority
  (`cases/spec/09-macros/include_merge_ignores_priority`,
  `cases/spec/09-macros/include_merge_lower_priority_still_merges`). Each entry that goes in
  carries the new unit's priority and is compared with what is already there by these same rules:
  `.priority 2⏎a { x = 1 }` followed by a merge include at priority 5 of `a { x = 9 }` →
  `a: { x: int 9 @5 } @2`; the same at priorities 5 and 2 → `a: { x: int 1 @5 } @5`
  (`merge_nested_priority_higher`, `merge_nested_priority_lower`).
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

Priorities are compared first, as in §8.3, and only a repeat that would be added is collected:
`a = 1⏎.priority 3⏎a = 2⏎.priority 1⏎a = 3⏎.priority 3⏎a = 4` → `{ a: [int 2 @3, int 4 @3] }`
(`nia_priorities_compared_first`).

**Quirk.** The collected array has priority 0, whatever the priorities of the values in it, and
later repeats are compared with that 0. A repeat at priority 0 is appended; a repeat at any higher
priority replaces the whole array. So with a non-zero priority at most two values are ever
collected:

| Input (`no-implicit-arrays`) | Result | Case |
| --- | --- | --- |
| `.priority 3⏎a = 1⏎a = 2⏎a = 3` | `{ a: int 3 @3 }` | `nia_collection_has_priority_0` |
| `.priority 3⏎a = 1⏎a = 2⏎.priority 1⏎a = 3` | `{ a: int 3 @1 }` | `nia_collection_replaced_lower_value` |
| `a = 1⏎a = 2⏎.priority 2⏎a = 3` | `{ a: int 3 @2 }` | `nia_collection_replaced_higher_value` |
| `a = 1⏎a = 2⏎a = 3`, main document at priority 3 | `{ a: int 3 @3 }` | `nia_chunk_priority` |

**With `merge`.** Under `merge`, a collected array counts as an array (§8.4): a scalar repeat
replaces it (the scalar quirk), and an array repeat appends its elements:
`a = 1⏎a = 2⏎a = 3` → `{ a: int 3 }`; `b = 1⏎b = 2⏎b = [3]` → `{ b: [int 1, int 2, int 3] }`
(`nia_merge_scalar_replaces_collection`, `nia_merge_array_extends_collection`). The same happens
when the repeat comes from a merge include (`nia_include_merge_scalar`: `b = 1⏎b = 2` and a merge
include of `b = 5` → `{ b: int 5 }`; `nia_include_merge_array_then_repeat`: after a merge include
of `b = [3]`, a later `b = 4` gives `[int 1, int 2, int 3, int 4]`).

**Quirk.** When a collected array has been replaced by a scalar in this way, the next repeat of
that key at priority 0 is an **error**, whatever its type and strategy. A repeat at a higher
priority replaces the value normally, and a replacement by `rewrite` clears the condition:

| Input (`no-implicit-arrays`) | Result | Case |
| --- | --- | --- |
| `a = 1⏎a = 2⏎a = 3⏎a = 4`, strategy `merge` | **error** | `nia_merge_quirk_then_repeat_error` |
| `b = 1⏎b = 2`, merge include of `b = 5`, then `b = 6` | **error** | `nia_include_merge_scalar_then_repeat_error` |
| the same, then `b { x = 6 }` | **error** | `nia_include_merge_scalar_then_object_error` |
| the same, then `.priority 2⏎b = 6` | `{ b: int 6 @2 }` | `nia_include_merge_scalar_then_higher` |
| the same, then a rewrite include of `b = 7`, then `b = 6` | `{ b: [int 7, int 6] }` | `nia_include_merge_scalar_then_rewrite` |

Without `no-implicit-arrays` there is no such error: `b = [1]`, a merge include of `b = 5`, then
`b = 6` → `{ b: ⟨int 5 | int 6⟩ }` (`include_merge_scalar_quirk_default_mode`).

## 8.6 Key comparison

Keys are compared byte for byte. With `key-lowercase` set, keys are lowercased first (ASCII only),
so `A` and `a` are the same key (`cases/spec/12-flags/key_lowercase_merges_case`).

## 8.7 Priorities that have no effect

Priorities matter only for the values held directly under a key, where §8.3 to §8.5 compare them.
Two other priorities appear in the conformance dumps but have **no observable effect**: they are
never compared, never change a result, and never appear in any output format (§10):

- the priority of the document's root object (`chunk_priority`, where the root carries priority 5);
- the priority of each element of an explicit array. Elements keep the priority of the unit they
  came from (`priority_applies_to_containers`, `strategy_merge_arrays_ignore_priority`,
  `nia_priorities_compared_first`), but merging into an array (§8.4) and collecting under
  `no-implicit-arrays` (§8.5) never look at them, and the output formats do not show them
  (`cases/spec/10-output/priority_not_emitted`).

An implementation need not represent either. The conformance runner ignores both when it compares
a result with its golden file.
