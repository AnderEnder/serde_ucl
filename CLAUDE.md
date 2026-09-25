# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Communication Style

**CRITICAL REQUIREMENTS:**
- Be direct, clear, and concise
- Write without emojis, reduntant phrased and filler words
- Maintain neutral emotional tone
- Avoid excessive optimism or enthusiasm
- Base all statements on facts and evidence
- Use technical precision over conversational language

## Token Efficiency

**MANDATORY:**
- Read files only when necessary for the specific task
- Use Grep with specific patterns instead of reading entire files for searches
- Use Glob to locate files before reading
- Avoid re-reading files already examined in the conversation
- When making edits, include only minimal context in old_string
- Batch related operations in single tool calls


## Project Overview

UCL (Universal Configuration Language) for Rust with serde: package `ucl-rust-lexer`, library
`ucl_lexer`, latest stable Rust (1.98 at this release). The crate reads and writes UCL as libucl,
the C library used by FreeBSD, does. Compatibility is defined by behaviour: the behaviour spec in
`docs/spec/` and the conformance suite in `tests/conformance/`, whose golden files come from
libucl. `docs/COMPATIBILITY.md` lists the deliberate differences and the libucl quirks the crate
reproduces.

- `src/parse/`: the parser. `mod.rs` (`Parser`: flags, variables, loader, base directory, search
  directories, input limit), `builder.rs` (`ParserBuilder`), `core.rs` (document structure),
  `number.rs`, `string.rs`, `vars.rs`, `comments.rs` (saved comments), `macros.rs` (macro syntax
  and the built-in macros), `include.rs` and `glob.rs` (`.include`, `.try_include`, `.load`),
  `loader.rs` (`FsLoader`, `MemoryLoader`), `inputs.rs` (several inputs into one parser,
  `Parser::inputs`, spec §13.1), `registered.rs` (macros the application registers, `MacroCall`,
  spec §13.2), `facts.rs` and `tree.rs` (what the output formats need to know about the parse),
  `error.rs` (`Error`, `ErrorKind`).
- `src/emit/`: libucl's output formats (config, JSON, compact JSON, YAML).
- `src/de.rs`, `src/de/`: serde deserialization; `src/ser/`: serde serialization;
  `src/handoff.rs`: moves a whole `UclValue` past serde's data model, so that deep values do not
  recurse.
- `src/value.rs`: the value model; `src/error.rs`: `UclError` and `Position`; `src/time.rs`: the
  `Duration` helper.
- `tests/conformance.rs` and `tests/conformance/`: the conformance runners and cases;
  `tests/common/oracle.rs`: running the crate as the oracle runs a case, and the comparison of
  dumps, shared by the conformance runner and the fuzzer; `tests/serde_roundtrip.rs` and
  `tests/serde_corpus/`: serde round trips; `tests/inputs_and_macros.rs`: several inputs and
  registered macros through the API; `tests/error_positions.rs`: positions of deserialization
  errors.
- `fuzz/`: the differential fuzzer `ucl-differential`, a package of its own outside `cargo test`
  (`fuzz/README.md`).
- `scripts/ci.sh`: what CI runs, and the `golden`, `pin` and `fuzz` modes;
  `scripts/regen-golden.sh` and `tools/ucl-dump/`: the oracle that produces the golden files.
- `.github/workflows/`: `ci.yml` (`scripts/ci.sh` on Linux and macOS with stable Rust),
  `golden.yml` (the nightly drift check), `pin-move.yml` (manual: the golden files at another
  libucl commit, published for review, nothing committed) and `fuzz.yml` (manual: the
  differential fuzzer).

## Clean-Room Rules

The implementation in `src/` must not be derived from libucl's source code. Tests may be.
Two teams: the spec team reads libucl source and writes the behaviour spec in `docs/spec/`, the
conformance cases and the oracle tooling; the implementation team has never read libucl source and
works only from released spec versions. The rules below bind the implementation team; the spec
team never edits `src/`.

- Do not read libucl source files (`*.c`, `*.h`, build files) anywhere, including the clone that
  `scripts/regen-golden.sh` makes under `target/libucl-oracle/`, or browse its source online.
- Do not read branches `quarantine/*`, or `REVIEW.md`, `PLAN.md` or `PROGRESS.md`.
- The old `src/lexer.rs` and `src/parser.rs` were deleted at the cut-over (C5b). Their history
  stays forbidden: no `git show`, `git log -p`, `git diff` or checkout of revisions that contain
  them. The same holds for the history of `src/` before commit `ef8007e` and the history of
  `CLAUDE.md`.
- Allowed inputs for implementation: the released spec (latest `spec-vN` tag) in `docs/spec/`,
  `docs/clean-room/`, libucl's public format documentation, the conformance cases and golden
  files, and running the oracle as a black box (`scripts/regen-golden.sh`,
  `target/libucl-oracle/ucl-dump`).
- Questions about the spec go to `docs/clean-room/QUESTIONS.md`; answers come as a new spec
  version.
- Record provenance in `docs/clean-room/LOG.md`, and start commit messages with the work-item ID.
  The full protocol is in `docs/clean-room/PROTOCOL.md`; the work items are in
  `docs/clean-room/WORKLIST.md`.

## Common Commands

### Building and Testing

```bash
# Build the project
cargo build

# Run all tests: unit, integration, conformance, and doc tests (the README examples included)
cargo test

# Run the conformance suite, with per-case detail
cargo test --test conformance
UCL_CONFORMANCE_REPORT=1 cargo test --test conformance -- --nocapture

# Run one test file, or tests by name
cargo test --test api_tests
cargo test <test_name>

# Run tests without capturing output
cargo test -- --nocapture

# Run benchmarks (see benches/README.md)
cargo bench
cargo bench --bench parse_benchmarks
```

### CI

```bash
# Everything CI runs on push and pull request
scripts/ci.sh

# The nightly drift check: rebuild libucl, regenerate every golden file, fail on any change
scripts/ci.sh golden

# What moving the libucl pin to another commit (a full SHA) would change: regenerates every
# golden file at it and writes a summary and a patch to target/pin-move/; commits nothing
scripts/ci.sh pin <commit>

# The differential fuzzer for N seconds (fuzz/README.md); fails if it finds a difference
scripts/ci.sh fuzz 600
```

### Running Examples

```bash
# List all examples (examples/README.md describes them)
ls examples/

# Run an example; each checks its results with assertions
cargo run --example basic_usage
cargo run --example complete_ucl_syntax
cargo run --example number_parsing
cargo run --example web_server_config
cargo run --example framework_integration
cargo run --example nginx_style_framework_integration
cargo run --example real_world_configurations
cargo run --example configuration_management
cargo run --example real_world_usage
cargo run --example advanced_features
```

### Development

```bash
# Check code without building
cargo check

# Format code
cargo fmt

# Run clippy lints as CI does
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy --lib --no-default-features -- -D warnings

# Build documentation
cargo doc --open
```


## Testing

- Conformance: `cargo test --test conformance` runs three tests. `libucl_conformance_new_core`
  compares the parse of every case in `tests/conformance/` with its `<case>.golden.json`;
  `libucl_conformance_emitters` compares the output of every parsed case in each format, byte for
  byte, with libucl's; `libucl_conformance_readback` parses each output again and compares the
  value, allowing only the losses spec §10.8 lists (an unlisted difference would go in
  `READBACK_PENDING` with its question; the list is empty). Known failures are listed in
  `tests/conformance/xfail-new.txt` and `xfail-emit.txt` with a reason; they hold only justified
  divergences, may only shrink, and a listed case that passes fails the run.
  `tests/conformance/README.md` describes the layout.
- Golden files come only from libucl, through `scripts/regen-golden.sh` (git, CMake and a C
  compiler; runs on macOS). Never edit them by hand. Cases and golden files belong to the spec
  team.
- serde: `cargo test --test serde_roundtrip` checks round trips and the corpus in
  `tests/serde_corpus/`; `UCL_SERDE_REGEN=1` regenerates the corpus and needs the oracle binary
  `target/libucl-oracle/ucl-dump`, which `scripts/regen-golden.sh` builds.
- No wall-clock thresholds in tests. `tests/scaling.rs` is the only timing-based test: it checks
  that time grows linearly by comparing time ratios within one run, which only time can show.
- `tests/stack_depth.rs` checks that no entry point overflows a 2 MiB stack at the deepest
  accepted input; `scripts/ci.sh` also runs it with the crate unoptimised (`cargo test` builds the
  crate with `opt-level = 2`, for speed only; the tests pass without it).
- `tests/features/` is a separate package that depends on the crate without the `load` feature
  (the crate's dev-dependency on itself turns `load` on for every other test build);
  `scripts/ci.sh` runs it.
- `fuzz/` is the differential fuzzer: it mutates the conformance cases, parses each input with
  the crate and with the oracle binary, compares them as the conformance runner does, and saves
  reduced differences under `target/fuzz-differential/findings/`. It needs
  `target/libucl-oracle/ucl-dump`; `scripts/ci.sh` only formats, lints and unit-tests it. A
  difference goes to `docs/clean-room/QUESTIONS.md`, or is fixed where the spec is clear.
- The README's code examples run as doctests (`ReadmeDoctests` in `src/lib.rs`).
- Unit tests live inline under `#[cfg(test)]`; integration tests in `tests/`; benchmarks in `benches/`.

## Task-Specific Workflows

**Bug Investigation:**
1. Grep for error message or function name
2. Read specific file sections only
3. Check related test files
4. Propose fix with minimal context

**Performance Optimization:**
1. Run benchmarks first: `cargo bench`
2. Identify bottleneck from benchmark output
3. Read only affected code sections
4. Apply optimization
5. Re-run specific benchmark only

**Adding Features:**
1. Grep existing similar functionality
2. Identify integration points
3. Write implementation
4. Add tests in same batch
5. Run affected tests only: `cargo test <test_name>`

## Autonomous Decision Guidelines

**When to use Grep vs Read:**
- Grep: Finding definitions, usages, patterns across codebase
- Read: Understanding specific implementation details after locating file

**When to run tests:**
- Always after lexer/parser changes
- Always after error handling changes
- Skip for documentation-only changes
- Use specific test names when possible

**When to use Agent tool:**
- Searching across 5+ files for patterns
- Complex refactoring requiring multiple file coordination
- Never for simple file reads or single-file edits


## Code Conventions

- Public functions return `Result<T, UclError>`. A parse error is a `parse::Error` with a
  `parse::ErrorKind`, a `Position` and, inside an included file, that file.
- Behaviour follows the released spec; doc comments cite the spec section a rule comes from
  (for example §9.4).
- Text input reads no files by default; only `from_file` and parsers given a file loader do
  (WORKLIST C5, decision 1).
- `cargo fmt`, and clippy clean with `-D warnings` on the latest stable Rust (1.98 at this
  release); CI runs stable only.
- Every Cargo feature (`fs`, `load`) is referenced by a `#[cfg]`.
- User-visible changes are recorded in `CHANGELOG.md`.
- Unit tests inline with `#[cfg(test)]`; integration tests in `tests/`.
- Profile with `cargo bench` before optimizing.

---

# Claude Code Configuration for Rust Projects

## CRITICAL: RUST PARALLEL EXECUTION PATTERNS

**MANDATORY RULE**: Rust projects require memory-safe coordination with Cargo parallel compilation and ownership management.

## CRITICAL: CONCURRENT EXECUTION FOR ALL RUST OPERATIONS

**ABSOLUTE RULE**: ALL Rust operations MUST be concurrent/parallel in a single message:

### MANDATORY CONCURRENT PATTERNS FOR RUST:

1. **Cargo Operations**: ALWAYS batch ALL cargo build/test/run commands
2. **Crate Management**: ALWAYS batch ALL dependency installations
3. **Testing**: ALWAYS run ALL test suites in parallel with cargo test
4. **Memory Safety**: ALWAYS batch ALL borrowing/ownership patterns
5. **Concurrency**: ALWAYS batch ALL async/threading implementations

### RUST GOLDEN RULE: "1 MESSAGE = ALL MEMORY-SAFE OPERATIONS"

**Examples of CORRECT Rust concurrent execution:**

```
// CORRECT: Everything in ONE message
[Single Message]:
  - TodoWrite { todos: [10+ todos with all Rust tasks] }
  - Task("You are Rust architect. Coordinate via hooks for ownership design...")
  - Task("You are Systems programmer. Coordinate via hooks for performance...")
  - Task("You are Safety engineer. Coordinate via hooks for memory safety...")
  - Bash("cargo new my-rust-app --bin")
  - Bash("cd my-rust-app && cargo add serde tokio reqwest")
  - Bash("cd my-rust-app && cargo add --dev proptest criterion")
  - Write("Cargo.toml", cargoConfiguration)
  - Write("src/main.rs", mainApplication)
  - Write("src/lib.rs", libraryModule)
  - Write("src/models/user.rs", userStruct)
  - Write("src/services/api.rs", apiService)
  - Write("src/utils/helpers.rs", utilityFunctions)
  - Write("tests/integration_test.rs", integrationTests)
  - Bash("cd my-rust-app && cargo build && cargo test && cargo run")
```

## RUST-SPECIFIC SWARM PATTERNS

### Cargo Project Coordination

**Rust Project Setup Strategy:**

```bash
# Always batch Cargo operations
cargo new my-app --bin
cargo add serde serde_json tokio
cargo add --dev proptest criterion
cargo build --release
cargo test
```

**Parallel Development Setup:**

```
// CORRECT: All setup in ONE message
[BatchTool]:
  - Bash("cargo new rust-project --bin")
  - Bash("cd rust-project && cargo add serde serde_json tokio reqwest")
  - Bash("cd rust-project && cargo add --dev proptest criterion mockall")
  - Write("Cargo.toml", optimizedCargoToml)
  - Write("src/main.rs", asyncMainFunction)
  - Write("src/lib.rs", libraryRoot)
  - Write("src/config.rs", configurationModule)
  - Write("src/error.rs", errorHandlingTypes)
  - Write("src/models/mod.rs", modelsModule)
  - Write("tests/common/mod.rs", testUtilities)
  - Bash("cd rust-project && cargo build && cargo clippy && cargo test")
```

### Rust Agent Specialization

**Agent Types for Rust Projects:**

1. **Systems Architect Agent** - Memory management, ownership patterns
2. **Performance Engineer Agent** - Zero-cost abstractions, optimization
3. **Safety Specialist Agent** - Borrow checker, lifetime management
4. **Concurrency Expert Agent** - Async/await, threading, channels
5. **Testing Agent** - Unit tests, integration tests, property testing
6. **Ecosystem Agent** - Crate selection, FFI, WebAssembly

### Memory Safety Coordination

**Ownership and Borrowing Patterns:**

```
// Memory safety coordination
[BatchTool]:
  - Write("src/ownership/smart_pointers.rs", smartPointerExamples)
  - Write("src/ownership/lifetimes.rs", lifetimePatterns)
  - Write("src/ownership/borrowing.rs", borrowingExamples)
  - Write("src/memory/allocator.rs", customAllocatorUsage)
  - Write("src/safety/invariants.rs", safetyInvariants)
  - Write("tests/memory_safety.rs", memorySafetyTests)
  - Bash("cargo build && cargo miri test")
```

### Async/Concurrency Coordination

**Tokio Async Runtime Setup:**

```
// Async coordination pattern
[BatchTool]:
  - Write("src/async/runtime.rs", tokioRuntimeConfig)
  - Write("src/async/tasks.rs", asyncTaskHandling)
  - Write("src/async/channels.rs", channelCommunication)
  - Write("src/async/streams.rs", asyncStreamProcessing)
  - Write("src/network/client.rs", asyncHttpClient)
  - Write("src/network/server.rs", asyncWebServer)
  - Write("tests/async_tests.rs", asyncTestCases)
  - Bash("cargo test --features async")
```

## RUST TESTING COORDINATION

### Comprehensive Testing Strategy

**Testing Setup:**

```
// Test coordination pattern
[BatchTool]:
  - Write("tests/integration_test.rs", integrationTests)
  - Write("tests/common/mod.rs", testUtilities)
  - Write("src/lib.rs", unitTestsInline)
  - Write("benches/benchmark.rs", criterionBenchmarks)
  - Write("proptest-regressions/", propertyTestRegressions)
  - Write("tests/property_tests.rs", proptestCases)
  - Bash("cargo test --all-features")
  - Bash("cargo bench")
  - Bash("cargo test --doc")
```

### Property Testing and Fuzzing

**Advanced Testing Coordination:**

```
[BatchTool]:
  - Write("fuzz/fuzz_targets/fuzz_parser.rs", fuzzingTargets)
  - Write("tests/quickcheck_tests.rs", quickcheckTests)
  - Write("tests/model_based_tests.rs", modelBasedTesting)
  - Bash("cargo fuzz run fuzz_parser")
  - Bash("cargo test --features property-testing")
```

## RUST PERFORMANCE COORDINATION

### Performance Optimization

**Performance Enhancement Batch:**

```
[BatchTool]:
  - Write("src/performance/simd.rs", simdOptimizations)
  - Write("src/performance/zero_copy.rs", zeroCopyPatterns)
  - Write("src/performance/cache_friendly.rs", cacheOptimization)
  - Write("src/performance/profiling.rs", profilingIntegration)
  - Write("benches/performance_bench.rs", performanceBenchmarks)
  - Write("Cargo.toml", releaseOptimizations)
  - Bash("cargo build --release")
  - Bash("cargo bench --all-features")
  - Bash("perf record cargo run --release")
```

### Parallel Processing

**Rayon Parallel Coordination:**

```
// Parallel processing batch
[BatchTool]:
  - Write("src/parallel/rayon_examples.rs", rayonParallelization)
  - Write("src/parallel/custom_threadpool.rs", customThreadPool)
  - Write("src/parallel/work_stealing.rs", workStealingQueues)
  - Write("src/data/parallel_processing.rs", parallelDataProcessing)
  - Bash("cargo add rayon crossbeam")
  - Bash("cargo test parallel_")
```

## RUST WEB DEVELOPMENT COORDINATION

### Web Framework Integration

**Axum/Warp Web Service Setup:**

```
// Web development coordination
[BatchTool]:
  - Write("src/web/server.rs", axumWebServer)
  - Write("src/web/handlers.rs", requestHandlers)
  - Write("src/web/middleware.rs", customMiddleware)
  - Write("src/web/routes.rs", routingConfiguration)
  - Write("src/database/connection.rs", databaseIntegration)
  - Write("src/models/schema.rs", databaseSchema)
  - Write("migrations/001_initial.sql", databaseMigrations)
  - Bash("cargo add axum tokio tower sqlx")
  - Bash("cargo run --bin server")
```

### Database Integration

**SQLx Database Coordination:**

```
// Database integration batch
[BatchTool]:
  - Write("src/database/models.rs", databaseModels)
  - Write("src/database/queries.rs", sqlQueries)
  - Write("src/database/migrations.rs", schemaMigrations)
  - Write("src/database/connection_pool.rs", connectionPooling)
  - Write("tests/database_tests.rs", databaseTests)
  - Bash("cargo add sqlx --features runtime-tokio-rustls,postgres")
  - Bash("sqlx migrate run")
```

## RUST SECURITY COORDINATION

### Security Best Practices

**Security Implementation Batch:**

```
[BatchTool]:
  - Write("src/security/crypto.rs", cryptographicOperations)
  - Write("src/security/validation.rs", inputValidation)
  - Write("src/security/auth.rs", authenticationLogic)
  - Write("src/security/sanitization.rs", dataSanitization)
  - Write("src/security/secrets.rs", secretsManagement)
  - Write("audit.toml", cargoAuditConfig)
  - Bash("cargo add ring argon2 jsonwebtoken")
  - Bash("cargo audit")
  - Bash("cargo deny check")
```

**Rust Security Checklist:**

* Memory safety by design
* Integer overflow protection
* Secure random number generation
* Constant-time cryptographic operations
* Input validation and sanitization
* Dependency vulnerability scanning
* Safe FFI interfaces
* Secure compilation flags

## RUST BUILD COORDINATION

### Cargo Advanced Configuration

**Advanced Cargo Setup:**

```
// Advanced build coordination
[BatchTool]:
  - Write("Cargo.toml", advancedCargoConfig)
  - Write(".cargo/config.toml", cargoLocalConfig)
  - Write("build.rs", buildScript)
  - Write("Cross.toml", crossCompilationConfig)
  - Write("Dockerfile", rustDockerfile)
  - Bash("cargo build --target x86_64-unknown-linux-musl")
  - Bash("cross build --target aarch64-unknown-linux-gnu")
```

### WebAssembly Coordination

**WASM Integration Setup:**

```
// WebAssembly coordination
[BatchTool]:
  - Write("src/wasm/lib.rs", wasmBindings)
  - Write("src/js/wasm_interface.js", jsWasmInterface)
  - Write("pkg/package.json", wasmPackageJson)
  - Write("webpack.config.js", wasmWebpackConfig)
  - Bash("cargo add wasm-bindgen web-sys js-sys")
  - Bash("wasm-pack build --target web")
  - Bash("npm run serve")
```

## RUST DEPLOYMENT COORDINATION

### Production Deployment

**Deployment Configuration:**

```
[BatchTool]:
  - Write("Dockerfile", optimizedRustDockerfile)
  - Write("docker-compose.yml", dockerComposeRust)
  - Write("k8s/deployment.yaml", kubernetesDeployment)
  - Write("scripts/deploy.sh", deploymentScript)
  - Write("systemd/rust-service.service", systemdService)
  - Bash("cargo build --release --target x86_64-unknown-linux-musl")
  - Bash("docker build -t rust-app:latest .")
  - Bash("kubectl apply -f k8s/")
```

### Distribution and Packaging

**Crate Publishing Coordination:**

```
[BatchTool]:
  - Write("README.md", crateDocumentation)
  - Write("CHANGELOG.md", versionHistory)
  - Write("LICENSE", licenseFile)
  - Write("src/lib.rs", publicApiDocumentation)
  - Write("examples/basic_usage.rs", usageExamples)
  - Bash("cargo doc --open")
  - Bash("cargo package --dry-run")
  - Bash("cargo publish --dry-run")
```

## RUST CODE QUALITY COORDINATION

### Code Quality Tools

**Quality Toolchain Batch:**

```
[BatchTool]:
  - Write("rustfmt.toml", rustfmtConfiguration)
  - Write("clippy.toml", clippyConfiguration)
  - Write(".gitignore", rustGitignore)
  - Write("deny.toml", cargoServerDenyConfig)
  - Write("rust-toolchain.toml", toolchainConfiguration)
  - Bash("cargo fmt --all")
  - Bash("cargo clippy --all-targets --all-features -- -D warnings")
  - Bash("cargo deny check")
```

### Documentation Coordination

**Documentation Generation:**

```
[BatchTool]:
  - Write("src/lib.rs", comprehensiveDocComments)
  - Write("examples/", codeExamples)
  - Bash("cargo doc --no-deps --open")
  - Bash("cargo test --doc")
```

## RUST CI/CD COORDINATION

### GitHub Actions for Rust

**CI/CD Pipeline Batch:**

```
[BatchTool]:
  - Write(".github/workflows/ci.yml", rustCI)
  - Write(".github/workflows/security.yml", securityWorkflow)
  - Write(".github/workflows/release.yml", releaseWorkflow)
  - Write("scripts/ci-test.sh", ciTestScript)
  - Write("scripts/security-audit.sh", securityAuditScript)
  - Bash("cargo test --all-features")
  - Bash("cargo clippy --all-targets -- -D warnings")
  - Bash("cargo audit")
```

## RUST BEST PRACTICES

### Code Design Principles

1. **Ownership Model**: Understand borrowing and lifetimes
2. **Zero-Cost Abstractions**: Write high-level code with low-level performance
3. **Error Handling**: Use Result and Option types effectively
4. **Memory Safety**: Eliminate data races and memory bugs
5. **Performance**: Leverage compiler optimizations
6. **Concurrency**: Safe parallel programming patterns

### Advanced Patterns

1. **Type System**: Leverage advanced type features
2. **Macros**: Write declarative and procedural macros
3. **Unsafe Code**: When and how to use unsafe blocks
4. **FFI**: Foreign function interface patterns
5. **Embedded**: Bare metal and embedded development
6. **WebAssembly**: Compile to WASM targets

## RUST LEARNING RESOURCES

### Recommended Topics

1. **Core Rust**: Ownership, borrowing, lifetimes
2. **Advanced Features**: Traits, generics, macros
3. **Async Programming**: Tokio, async/await patterns
4. **Systems Programming**: Low-level development
5. **Web Development**: Axum, Warp, Rocket frameworks
6. **Performance**: Profiling, optimization techniques

### Essential Tools

1. **Toolchain**: rustc, cargo, rustup, clippy
2. **IDEs**: VS Code with rust-analyzer, IntelliJ Rust
3. **Testing**: Built-in test framework, proptest, criterion
4. **Debugging**: gdb, lldb, rr (record and replay)
5. **Profiling**: perf, valgrind, cargo-flamegraph
6. **Cross-compilation**: cross, cargo-zigbuild

### Ecosystem Highlights

1. **Web Frameworks**: Axum, Actix-web, Warp, Rocket
2. **Async Runtime**: Tokio, async-std, smol
3. **Serialization**: Serde, bincode, postcard
4. **Databases**: SQLx, Diesel, sea-orm
5. **CLI Tools**: Clap, structopt, colored
6. **Graphics**: wgpu, bevy, ggez, nannou

---

**Remember**: Rust requires memory-safe coordination, parallel compilation, and zero-cost abstractions. Always batch cargo operations and leverage Rust's ownership system for safe, fast, concurrent applications.
