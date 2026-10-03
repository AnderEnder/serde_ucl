# C15 Stage A public-interface cases

These spec-owned cases use only the 43 initial symbols and supported header aliases.
Expected output is generated exclusively from pinned libucl
`24c8b399062ae4691168c243e3b7345ef7f31956`, with C locale and UTC timezone.
The scope and behavior are defined by the released [Stage A specification](../../../../docs/spec/c-api/stage-a/README.md) in `spec-v22`.

The probe uses public declarations and observable results. It contains no upstream
function bodies. Each argument in `cases.json` selects one independent case;
filesystem fixtures are created and removed in the process working directory.
Golden output records bytes as hexadecimal and pointer comparisons as booleans.
Darwin arm64 snapshots are present; other platforms need spec-team oracle snapshots.

After the spec release, an implementer can compile this clean case program against
the shipping header/library and compare its stdout with the corresponding snapshot:

```sh
cc -std=c11 -D_POSIX_C_SOURCE=200809L -Wall -Wextra -Werror \
  -I capi/include tests/conformance/capi/stage-a/probe.c \
  capi/target/release/libucl.a -lm -o target/c15-stage-a-probe
LC_ALL=C TZ=UTC target/c15-stage-a-probe iteration
```

Run filesystem cases in a fresh writable working directory. Repeat linkage against
the shared library with its documented runtime search path. Use the candidate's
actual documented static dependencies if additional link options are required.

The one-submission boundary is project-defined and needs a separate implementation
test: a second add operation returns false/ESTATE with a nonempty diagnostic,
retains the first root, and does not access the second input. It is deliberately
not assigned an oracle snapshot. Unsupported pointer usages, nonfinite/out-of-range
casts and invalid emitter selectors are not compatibility requirements.
