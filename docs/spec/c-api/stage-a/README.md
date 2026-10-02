# C15 Stage A: initial public C API contract

Status: released in `spec-v22`, 2026-10-02, after independent review
([review report](../../../clean-room/reviews/c15-stage-a.md)). This document supersedes the temporary
proposal for Stage A. It contains public behavior, without implementation design.

## 1. Scope and evidence

Stage A provides exactly the 43 external functions marked `provided` in
[api.json](api.json), with the signatures and public types in
[include/ucl.h](include/ucl.h). Other inventory entries are deferred. Header aliases
for provided functions are available; aliases are not additional external symbols.
The parser accepts one complete input, and returned values are read-only. Stateful
multiple submissions, constructors, mutation, callbacks, comment access and binary
emission belong to later C15 stages. This is a documented subset of the libucl C API.

The reference is libucl commit `24c8b399062ae4691168c243e3b7345ef7f31956`.
Evidence is [the public-interface cases](../../../../tests/conformance/capi/stage-a/probe.c)
and [oracle snapshots](../../../../tests/conformance/capi/stage-a/golden/darwin-arm64).
Case names below refer to that program's argument. Snapshot lengths count bytes;
hexadecimal output records exact bytes. Addresses are compared by identity, never
serialized. Snapshots come exclusively from the reference. Format behavior remains
that of the released UCL format specification and its existing conformance cases.

Initial verified target: Darwin arm64, LP64 C ABI. Other targets require their own
ABI snapshots and reference checks before compatibility is claimed. C11 and C++11
translation units must accept the public header. All 43 symbols must link using
both this header and the pinned reference's public header. (`abi` and signature checks)

## 2. Supported caller domain

Inputs, keys, registered names and values are valid UTF-8, including embedded NUL
when a byte length is supplied. NUL-terminated arguments must have a terminator.
Nonzero lengths describe accessible bytes, not character counts. Writable output
arguments are valid non-NULL pointers, except the optional old-iterator error
output may be NULL (as supplied by the public iterate macro). Parsers and iterator handles are live and
non-NULL. Objects originate from this library; callers never construct objects,
write public fields, mutate containers or mix libraries' handles. Reference counts
are balanced. Concurrent access to a parser, iterator or object's reference state
requires caller synchronization. (`strings`, `lifetime`, `iteration`)

For ZEROCOPY input, callers keep the input storage alive and unchanged until all
objects derived from it are released. Borrowed child pointers remain valid while
the containing tree is retained. A separately retained child with no implicit
siblings, including a container and its descendants, survives its parent's
release. For duplicate-value chains, keep the parent tree alive until all separate
references to chain elements have been released; retaining the head alone does
not establish independent ownership of its siblings. Traversing links after
parent release is outside Stage A. (`metadata`, `lifetime`)

Conversions from floating/time values to integers require finite values within
int64's representable range. Integer-to-double rounding follows C's binary64
conversion. NaN, infinity, out-of-range casts, invalid enums, allocation exhaustion,
invalid pointers and refcount overflow are outside the verified compatibility
domain. Only emitter selectors 0 through 3 are supported. `UCL_EMIT_MAX` and binary emission are not candidate acceptance requirements.
(`conversions`, `emitters`)

NULL object arguments are supported for type queries, conversions other than
`tostring_forced`, lookups, array queries and emission, with the results below.
No claim is made for other NULL arguments. (`conversions`, `strings`, `emitters`)

## 3. Public ABI and object observations

Declarations, constant values, signedness, const qualifiers and field order are
normative. `ucl_object_t` has size 64 and alignment 8 on the verified target:

| Field | Offset | Meaning callers may observe |
| --- | ---: | --- |
| value | 0 | int/bool use iv; float/time use dv; string uses sv |
| key | 8 | borrowed key bytes, NUL terminated; NULL for unkeyed values |
| next | 16 | next implicit sibling, or NULL |
| prev | 24 | previous sibling; head points to tail; singleton points to itself |
| keylen | 32 | key byte length, excluding terminator |
| len | 36 | string byte length; array element count; object value count |
| ref | 40 | current reference count |
| flags | 44 | public flags and priority bits |
| type | 46 | numeric ucl_type_t value |
| trash_stack | 48 | reserved opaque storage; callers neither inspect nor modify it |

Container union pointers are opaque and cannot be dereferenced. Unused union
members and allocation addresses have no promised contents. Boolean iv is 0 or 1;
time dv is seconds. A root returned once has ref=2 while its parser is alive;
children initially have ref=1. The metadata fixture has 11 distinct keys and 13
values, so its root len=13, not 11. Array element count excludes descendants.
Scalar nonstrings have len=0. These observations hold for default and ZEROCOPY
parsing in the fixture. (`abi`, `metadata`, `lifetime`)

The metadata fixture shows flags: first head of a scalar duplicate chain 32,
single quoted string 256, heredoc 16, a key requiring escaping 4, inherited value
64, priority-three value 12288. Other tested plain values have flags=0. Priority
occupies the upper four bits; 0..15 are the declared bounds. Public allocated-key
and allocated-value flags are declared but no caller can require allocation
addresses or storage strategies. Default root flags/keylen are zero; root prev
is itself and next is NULL. Explicit array elements are unkeyed singletons.
(`abi`, `metadata`)

## 4. Parser lifecycle, inputs and diagnostics

`new(flags)` owns a parser; `free` releases its ownership of the result and parser
strings. Before any submission, get_object returns NULL, error code is EOK,
error string is NULL, line and column are zero. A successful get_object grants
an owned reference to the same root on every call. The parser retains its own
reference independently. Registered variables are configured before submission;
registering a name again replaces its value. Unknown variables retain their
spelling. (`parser`, `lifetime`)

Chunk length is exact, including zero. String length zero selects strlen; nonzero
selects the given bytes. Empty input succeeds with an empty object. A submission
attempt consumes the single permitted input even if it fails or silently stops.
A second top-level add_chunk/add_string/add_file returns false, sets ESTATE and a
nonempty error string, and preserves any first result; it reads no second input.
This last boundary is project-defined, not an oracle assertion. After submission,
register_variable is outside the supported domain. (`parser`; Stage A boundary)

Error-code success and error-string absence are separate observations. Error text
must be a meaningful NUL-terminated diagnostic, owned by the parser and valid
until parser destruction; exact wording is unspecified. Positions match the C
API's cursor, rather than assuming the Rust error's coordinates. For these inputs:

| Input | Return | Code | Error string | Line:column | Result |
| --- | --- | --- | --- | --- | --- |
| empty | true | 0 | NULL | 1:0 | {} |
| one space | true | 0 | NULL | 1:1 | {} |
| a=1 | true | 0 | NULL | 1:3 | a=1 |
| a=1 LF b=2 | true | 0 | NULL | 2:3 | a=1,b=2 |
| {a=1} trailing ignored | true | 0 | NULL | 1:4 | a=1 |
| a="open | false | 1 | present | 1:7 | NULL |
| a={ | false | 5 | present | 1:3 | a={} |
| a=[ | false | 5 | present | 1:3 | a=[] |
| .unknown x | false | 0 | present | 1:8 | NULL |
| .try_include "missing" LF a=2 | false | 0 | NULL | 2:0 | {} |
| .include "missing.ucl" | false | 0 | present | 1:22 | {} |

The exact input spellings, including the missing-file names, are in `parser`.
Both string and chunk entry points produce these observations. Failed submission
can expose a partial object: get_object does not mean the input succeeded.
The silent-stop example exposes the partial root without an ordinary error.
The bounded three-byte chunk of `a=1garbage` parses a=1. (`parser`)

All seven parser flags have the released format specification's behavior. The
`flags` fixture additionally checks lowercase key collisions, ZEROCOPY output,
NO_TIME preserving "2s", NO_IMPLICIT_ARRAYS's duplicate output, SAVE_COMMENTS's
unchanged default output, DISABLE_MACRO suppressing variable expansion, and
NO_FILEVARS preserving "$FILENAME" while ordinary registered variables expand.
Without NO_FILEVARS a memory input substitutes "undef" for FILENAME. Comments
cannot be retrieved or edited through Stage A. (`flags`, `metadata`)

File parsing and built-in include/load remain supported format features. Memory
includes and includes in a file both resolve against process cwd. File input
FILENAME is the canonical absolute filename, and CURDIR is its canonical parent;
CURDIR does not change the directory used for includes. A missing main file
returns false with code=0, error string present, position 0:0 and no root. The
relative-file fixture succeeds with position 2:0. (`filesystem`)

## 5. Reading, lookup, references and conversions

Lookup returns a borrowed entry head, preserving pointer identity with iteration.
Length lookup compares exactly the supplied key bytes; zero key length is not a
strlen request. Wrong container type, absent key, out-of-range index and NULL
object return NULL; array_size on a nonarray or NULL is zero. key/keyl return
borrowed NUL-terminated storage, and keyl supplies its byte length. Embedded NUL
keys and values remain addressable through length APIs; C-string consumers see
only their first segment. UTF-8 e-acute occupies two bytes. (`strings`, `iteration`)

ref returns the same object and increments ref; unref releases one ownership.
Borrowed lookups, conversions and iteration do not grant references. A retained
non-chain child container remains readable, traversable and emittable after
parser and root destruction; reference counts in `lifetime` are normative.

NULL type query returns UCL_NULL. Type names for values 0..8 respectively are
object, array, integer, number, string, boolean, number, userdata, null.
string_to_type accepts object, array, integer, number, string, boolean, userdata,
null case-insensitively. number maps to FLOAT. It rejects int, float, time, str,
bool, unknown and empty; failure leaves the output unchanged. (`conversions`)

| Conversion | Accepted types | Success | Failure |
| --- | --- | --- | --- |
| toint_safe | INT,FLOAT,TIME | integer; fractional part truncated toward zero | false, output unchanged |
| todouble_safe | INT,FLOAT,TIME | binary64 numeric value | false, output unchanged |
| toboolean_safe | BOOLEAN | its boolean value | false, output unchanged |
| tostring_safe | STRING | borrowed string storage | false, output unchanged |
| tolstring_safe | STRING | borrowed storage and exact byte length | false, both outputs unchanged |

Unsafe numeric/bool forms return zero/false on type mismatch or NULL.
Unsafe string forms return NULL on mismatch or NULL; callers should use the
safe length form when they need preserved failure outputs. String storage is
NUL terminated even when its payload contains embedded NUL. (`conversions`, `strings`)

Forced-string conversion returns borrowed NUL-terminated storage. The verified
locale is C. Integers use signed decimal without leading zeroes; FLOAT/TIME with
an integral value use decimal with a trailing .0; fractional values use fixed
notation with six fractional digits, rounded as C %f formatting. Thus tiny
negative values can give "-0.000000". For strings it returns their contents.
Integer 42 gives "42"; float +/-1.75 gives "1.750000" /
"-1.750000"; time 2s gives "2.0"; booleans give "true"/"false"; NULL-type object
"null"; array "array"; object "object". Its pointer remains valid until that
object is released. Calling it with a NULL pointer is outside the domain.
(`conversions`)

## 6. Iteration

Old iteration starts with a NULL handle. Do not share a handle between objects or
modes. With expand=true, an object yields its entry heads in insertion order;
an explicit array yields elements. Otherwise it follows the implicit sibling
chain, including a scalar singleton or the container itself. Expanded iteration
of a container-chain head expands only its first container. Results are borrowed.
Successful expanded object traversal sets the error output to zero; scalar,
array and unexpanded traversal leave the caller's error output unchanged in the
fixture. Expanded object exhaustion clears the handle; scalar/chain exhaustion
can retain a non-NULL handle. Reuse after exhaustion is outside old iteration's
domain; start a new handle for a new traversal. (`iteration`)

iterate_end is supported for an expanded object traversal, including early
abandonment, and clears its handle. It must not be used for unexpanded object
traversal or as universal cleanup for scalar/array traversal. The latter handles
need no separate cleanup. The conformance fixture uses this restricted domain.
(`iteration`, sanitizer validation)

new owns a safe iterator handle; reset returns a handle reset to the supplied
object, including an exhausted handle. free releases the handle, even after
exhaustion. safe(expand) has the corresponding old traversal choices. Full modes
are EXPLICIT=1, IMPLICIT=2, BOTH=3. On a homogeneous scalar chain all modes yield each scalar.
On an explicit array all modes yield its elements. On a chain of containers,
EXPLICIT expands the first container only; IMPLICIT/BOTH expand all containers
in chain order. On a root object, all modes return entry heads. These are shallow
traversals, not recursive descendant walks. For a mixed chain, EXPLICIT yields its scalar prefix, then the first container's
children, and stops before later siblings. IMPLICIT/BOTH visit each scalar or
each container's children in chain order. The matrix covers all three head types,
both scalar-to-container transitions and a longer scalar prefix.
(`iteration`)

Full EXPLICIT object traversal can restart on a further call after returning
NULL; IMPLICIT/BOTH stay exhausted in the fixtures. Empty containers remain
exhausted. The exact sequence/exhaustion matrix in the snapshot is normative;
callers should stop at the first NULL unless deliberately restarting. Iterator
handles never retain their target: keep its parent tree alive through traversal.
(`iteration`)

ABI quirk: although chk_excpn is declared with ucl_object_iter_t *, its valid
argument is the handle cast to that pointer type, as in the fixture, rather than
&handle. It returns false in the successful fixture traversals. Passing &handle
is outside the supported domain. (`iteration`, sanitizer validation)

## 7. Text emission and packaging

emit and emit_len return independently allocated, C-free-compatible buffers,
NUL terminated. emit_len reports bytes excluding the trailing NUL. For selectors
JSON=0, JSON_COMPACT=1, CONFIG=2, YAML=3 the two calls yield identical bytes.
Whitespace, order, quoting, numeric formatting and embedded-NUL replacement match
the released format specification and snapshots. Emitting an entry-chain head
alone emits that first value; a parent object emits its duplicate values using
the format's array/repeated-key convention. NULL input returns NULL and leaves
emit_len's length output unchanged. (`emitters`)

The package provides an installable ucl.h and static/shared C-linkable libraries,
with the 43 symbols and ABI above. Public Rust parser defaults and APIs retain
their existing behavior; C entry points follow this C environment contract.
No Rust panic unwinds across the C boundary. Static link dependencies and shared
library installation/link instructions must be documented and demonstrated by
compiling the public conformance program. The subset and single-submission
boundary must be stated in the package's public documentation. (Stage A delivery)
