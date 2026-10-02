# C15 Stage A independent specification review

Date: 2026-10-02. Role: fresh independent spec reviewer. No implementation authored.

Recommendation: approve the reviewed Stage A behavior contract for specification
release. No remaining blocking content or declaration findings were identified.
This is a release recommendation, not an implementation authorization: the candidate
is uncommitted and untagged, and PROTOCOL.md requires a committed `spec-vN` release
before it reaches implementers. Counsel review remains a separate release control.

## Reviewed inputs

- Current `CLAUDE.md` and `docs/clean-room/PROTOCOL.md`.
- Initial `docs/spec/drafts/c15/temporary-spec.md`, `FUNCTIONS.md`, `api.json`,
  and declaration-only `include/ucl.h`.
- Final `docs/spec/drafts/c15/stage-a/README.md`, `api.json`, `cases.json`,
  and declaration-only `include/ucl.h`.
- Public-interface conformance input `tests/conformance/capi/stage-a/probe.c`
  and its ten `golden/darwin-arm64/*.txt` snapshots, including the final mixed-chain
  iteration cases, all four retained-container emission formats, and the final
  precise/tiny/integral-float/fractional-time conversion examples.
- `docs/clean-room/LOG.md` for session provenance.

The spec-team implementation plan, broad research README, upstream header/source,
`tools/`, forbidden history and session memory were not consulted.

## Findings resolved before approval

The initial temporary proposal left parser diagnostics, public field semantics,
iterator modes, filesystem behavior and retained-result evidence unresolved, and
contained implementation prescriptions. The Stage A replacement specifies observable
behavior by feature, cites public-interface cases, supplies a bounded caller domain,
and omits implementation design and plan links.

The final inventory marks exactly 43 functions provided. Independent comparison
found exactly those 43 external function declarations in the Stage A header, with
no deferred external declarations. The missing supported `ucl_iterate_object` alias
was restored and exercised by the conformance case. The supported caller domain now
explicitly permits the optional NULL iterator error output used by that macro.

Mixed scalar/object/array chains were added to the iteration evidence. The final
EXPLICIT rule describes the observable scalar-prefix/first-container behavior, with
both scalar-to-container transitions and a longer scalar prefix evidenced by the
snapshot. IMPLICIT/BOTH and exhaustion/reset behavior are separately stated.

The retained child fixture now emits all four formats after parser and parent
release. Its CONFIG snapshot preserves single quoting. The contract explicitly
excludes independent duplicate-chain sibling ownership and unsafe old iterator-end
uses, rather than extrapolating from ordinary retained scalar/container behavior.

## Checks and limits

Independently checked declaration inventory, ABI/metadata text against snapshots,
parser results and positions, conversion outcomes and preserved failure outputs,
embedded NUL lengths, traversal sequences, references, emission bytes and filesystem
observations. The shipping probe passes C11 syntax checks with warnings as errors;
a C++11 translation unit using the public iteration alias also passes.

The spec author reports all ten release-oracle cases matching both header/library
combinations, all 43 signature/link checks passing, and final ASan runs matching
snapshots. Those runtime and sanitizer executions were not independently rerun by
this reviewer; no black-box runner command was supplied. This review does not
validate a replacement library or claim any target beyond Darwin arm64 LP64.

The forced-numeric precision follow-up was resolved before final approval: the
contract now states C locale, signed-decimal integers, integral FLOAT/TIME trailing
`.0`, and fractional fixed-six formatting. Additional oracle snapshots evidence
precision rounding, tiny negative values, an integral float and fractional time.
Any remaining implementation uncertainty should use the written QUESTIONS.md/new-spec
process rather than implementation inference.

Project-defined second-submission rejection is explicitly distinguished from oracle
behavior. Allocation failure, invalid enums/pointers, non-UTF-8 values and unsafe
ownership uses remain outside the verified compatibility domain. The unsupported
UCL_EMIT_MAX call and its output were removed from the acceptance probe and golden;
the contract explicitly excludes unsupported selectors and binary emission from
candidate acceptance requirements. This acceptance issue is resolved and the release
recommendation is unchanged. Delivery still requires package/link documentation, memory/stack checks
appropriate to the implementation, and unchanged Rust behavior.

No commits or tags were produced. No prohibited executable source, control-flow
pseudocode, or source-mirroring organization was found in the reviewed contract.
