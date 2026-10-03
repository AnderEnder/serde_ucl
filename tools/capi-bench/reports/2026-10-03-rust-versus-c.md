# Rust API versus C API parsing

Same inputs, release profiles, and machine as the C comparison. All three
callers emit byte-identical compact JSON before timings are accepted.
The Rust caller times `parse::parse` and drops its result every iteration,
matching the API used in the existing README comparison. C callers include
parser creation, submission, retained-root retrieval, parser free and unref.

5 rotated rounds × 9 samples, calibrated to 25ms.

| Input | Rust API µs | Our C API µs | libucl µs | C API / Rust API |
| --- | ---: | ---: | ---: | ---: |
| tiny-ucl | 0.55 | 10.18 | 7.93 | 18.50× |
| tiny-json | 0.56 | 10.23 | 7.91 | 18.13× |
| flat-1000 | 129.12 | 666.51 | 199.58 | 5.16× |
| floats-1000 | 143.42 | 686.62 | 220.08 | 4.79× |
| strings-1000 | 390.31 | 1133.91 | 402.17 | 2.91× |
| records-1000 | 618.96 | 4038.57 | 1086.62 | 6.52× |
| records-10000 | 6303.27 | 42589.00 | 11072.33 | 6.76× |
| rspamd-rbl | 55.07 | 309.42 | 87.49 | 5.62× |

The C/Rust difference covers all differences in the public API paths;
this experiment alone does not isolate one internal allocation or copy.
Raw results and binary hashes: local `target/c-bench/rust-baseline.json`.

## Why the C path costs more

A clean implementation participant reviewed the current implementation (without
benchmark or reference source inputs). The findings are implementation facts;
individual shares of the measured overhead remain unprofiled:

- The observed-input success path unconditionally constructs a fallback snapshot
  (`src/parse/inputs.rs:414`, `src/parse/core.rs:254`), then turns both the snapshot
  and successful tree into owned representations and discards the unused snapshot.
- ABI conversion retains the Rust tree and constructs another C object graph
  (`capi/src/object.rs:68,151`). Nodes have separate allocation, shared model
  ownership, copied NUL-terminated keys/strings, cloned ancestor paths, and
  container lookup structures.
- Output facts are cloned for the adapter and looked up by each node's full path
  (`src/parse/core.rs:281`, `src/parse/facts.rs:220`). Wide fact-bearing objects
  can incur repeated linear sibling scans; this is a potential hotspot whose
  actual contribution has not been measured.
- Final unref frees the C nodes and subsequently the retained Rust tree/facts
  (`capi/src/object.rs:217`).

The public-call phase measurements put roughly 75% of record-input time into
`ucl_parser_add_chunk` and 25% into tree destruction. The slowdown is in the
complete adapter path; the Rust value API remains faster than libucl on the
matching inputs. No implementation optimization was made in this investigation.
