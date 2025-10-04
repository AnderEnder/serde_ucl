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

A high-performance UCL (Universal Configuration Language) lexer and parser with seamless serde integration for Rust. UCL is a human-readable configuration format combining features from JSON, YAML, and NGINX configuration syntax.

This Rust implementation follows the design of the C libucl library, implementing a collection of specialized parsing functions that directly construct objects during parsing rather than producing a traditional token stream.

## Quick Reference: File Locations

**Core Implementation:**
- Lexer: `src/lexer.rs` (tokenization, character classification, number parsing)
- Parser: `src/parser.rs` (UclValue construction, variable expansion, plugins)
- Deserializer: `src/deserializer.rs` (serde integration)
- Error handling: `src/error.rs` (UclError, Position, Span)

**Testing:**
- Integration tests: `tests/*.rs`
- Benchmarks: `benches/*.rs`
- Examples: `examples/*.rs`

**Configuration:**
- Dependencies: `Cargo.toml`
- Feature flags: std, zero-copy, save-comments, strict-unicode

## Common Commands

### Building and Testing

```bash
# Build the project
cargo build

# Run all tests (unit + integration)
cargo test

# Run specific test file
cargo test --test integration_tests

# Run specific test by name
cargo test <test_name>

# Run tests without capturing output
cargo test -- --nocapture

# Run benchmarks
cargo bench

# Run specific benchmark
cargo bench <benchmark_name>
```

### Running Examples

```bash
# List all examples
ls examples/

# Run specific example
cargo run --example basic_usage
cargo run --example web_server_config
cargo run --example advanced_features
cargo run --example performance_comparison
cargo run --example number_parsing
```

### Development

```bash
# Check code without building
cargo check

# Format code
cargo fmt

# Run clippy lints
cargo clippy

# Build documentation
cargo doc --open
```

## Architecture

### Core Components

The codebase is organized into 4 main modules that form a processing pipeline:

1. **Lexer** (`src/lexer.rs`) - Tokenizes UCL text into tokens
   - Handles 3 string formats: JSON-style (`"..."`), single-quoted (`'...'`), and heredoc (`<<EOF...EOF`)
   - Supports rich number parsing with suffixes (512mb, 30s, 2gb)
   - Implements character classification using bitfield flags for performance
   - Provides both regular and streaming lexer implementations

2. **Parser** (`src/parser.rs`) - Converts tokens into structured `UclValue` tree
   - Builds `UclValue` enum with variants: String, Integer, Float, Time, Boolean, Null, Object, Array
   - Uses `IndexMap` for preserving key insertion order in objects
   - Implements variable expansion with circular reference detection
   - Supports custom hooks: `NumberSuffixHandler`, `StringPostProcessor`, `ValidationHook`
   - Plugin system via `UclPlugin` trait and `PluginRegistry`

3. **Deserializer** (`src/deserializer.rs`) - Bridges parser to serde
   - Implements `serde::Deserializer` trait to enable `#[derive(Deserialize)]`
   - Public API: `from_str()`, `from_str_with_variables()`
   - Handles variable expansion through `VariableHandler` implementations

4. **Error Handling** (`src/error.rs`) - Comprehensive error types
   - `UclError` enum: Lex, Parse, Serde variants
   - Position tracking with `Position` (line, column) and `Span` (start, end positions)
   - All errors implement `std::error::Error` via `thiserror`

### Data Flow

```
Input String → UclLexer → Token Stream → UclParser → UclValue Tree → UclDeserializer → Rust Struct
                                             ↑
                                      VariableHandler (optional)
                                      ParsingHooks (optional)
                                      Plugins (optional)
```

### Key Design Patterns

**Zero-Copy Parsing**: When enabled via `LexerConfig.zero_copy = true`, strings use `Cow<'a, str>` to reference original input instead of allocating.

**Streaming Support**: `StreamingUclLexer` processes large files with constant memory by reading from `BufReader`.

**Variable Expansion**:
- `VariableHandler` trait allows custom variable resolution
- Built-in handlers: `EnvironmentVariableHandler`, `MapVariableHandler`, `ChainedVariableHandler`
- Context tracking via `VariableContext` with expansion stack for circular reference detection

**Extensibility**:
- Hook system: `ParsingHooks` struct with `NumberSuffixHandler`, `StringPostProcessor`, `ValidationHook`
- Plugin system: Implement `UclPlugin` trait, register via `PluginRegistry`, build parser with `UclParserBuilder`
- Example plugins in parser module: `CssUnitsPlugin`, `PathProcessingPlugin`, `ConfigValidationPlugin`

## Feature Flags

- `std` (default): Standard library support
- `zero-copy`: Zero-copy string parsing optimizations
- `save-comments`: Preserve comments during parsing
- `strict-unicode`: Enforce strict Unicode validation

## Testing Strategy

- Unit tests: Embedded in source files (e.g., `src/error_tests.rs`)
- Integration tests: `tests/integration_tests.rs`, `tests/compatibility_tests.rs`, `tests/extensibility_tests.rs`, `tests/performance_tests.rs`
- Benchmarks: `benches/lexer_benchmarks.rs`, `benches/parser_benchmarks.rs`, `benches/zero_copy_benchmarks.rs`
- Examples serve as both documentation and functional tests

## Number Suffix Parsing

The lexer supports rich number formats:
- Size suffixes (binary 1024-based): `kb`, `mb`, `gb`, `tb`
- Size suffixes (decimal 1000-based): `kbps`, `mbps`, `gbps`
- Time suffixes: `ms`, `s`, `min`, `h`, `d`
- Special values: `inf`, `nan`
- Number bases: hex (`0xFF`), binary (`0b1010`), octal (`0o755`)

## String Format Handling

Three distinct string formats with different escape semantics:
1. **JSON-style (`"..."`)**: Full escape sequences including `\n`, `\t`, `\uXXXX`. Detects variable expansion during lexing but expands during parsing.
2. **Single-quoted (`'...'`)**: Literal strings with only two escapes: `\'` for single quote and `\<newline>` for line continuation.
3. **Heredoc (`<<EOF...EOF`)**: Multiline with uppercase-only terminator. Terminator must be on its own line. Preserves all whitespace.

**Key Implementation Detail**: String unescaping is done in-place to avoid allocations (destination pointer starts at same position as source).

## Variable Expansion

Two-pass algorithm from C implementation:
1. **Pass 1**: Scan for variables, calculate total expanded length
2. **Pass 2**: Allocate buffer with exact size, expand variables into new buffer

Variable formats:
- **Braced** (`${VARNAME}`): Strict matching, stops at `}`
- **Unbraced** (`$VARNAME`): Greedy matching of alphanumeric + `_`
- **Escape**: `$$` → literal `$`
- **Not found**: Preserve original (e.g., `${UNKNOWN}` stays as-is)

Variable resolution order:
1. Registered variables (exact/prefix match depending on format)
2. Custom variable handler callback
3. Not found → preserve original text

## Comment Handling

**Single-line**: `#` to end of line
**Multi-line**: `/* ... */` with nesting support

Nesting algorithm tracks depth counter:
- `/*` increments depth
- `*/` decrements depth
- Comment ends when depth reaches 0

When `save-comments` feature is enabled, comments are accumulated in parser and attached to the next parsed object.

## Lexer Implementation Notes

**Character Classification**: Uses 256-entry lookup table with bitfield flags for O(1) classification:
```rust
CharacterFlags {
    WHITESPACE: 1 << 0,          // space, tab
    WHITESPACE_UNSAFE: 1 << 1,   // newline, carriage return (affects line counting)
    KEY_START: 1 << 2,           // [A-Za-z_/]
    KEY: 1 << 3,                 // [A-Za-z0-9_/-]
    VALUE_END: 1 << 4,           // Terminates unquoted values: , ; } ] # newline
    VALUE_DIGIT: 1 << 5,         // [0-9]
    ESCAPE: 1 << 6,              // Requires escaping: " \
    JSON_UNSAFE: 1 << 7,         // Control characters requiring JSON escaping
}
```

**Number Parsing State Machine**:
1. **START**: Handle optional sign `-`
2. **INTEGER**: Check for `0x` (hex) or parse decimal digits
3. **HEX_DIGITS**: Parse hexadecimal (sets `allow_double = false`)
4. **DECIMAL_DIGITS**: Parse decimal, check for `.` or `e`/`E`
5. **FRACTION**: Parse digits after `.`, check for `e`/`E`
6. **EXPONENT**: Parse `e`/`E` with optional sign and digits
7. **SUFFIX**: Parse size/time suffixes:
   - Size (binary 1024): `kb`, `mb`, `gb` (with 'b')
   - Size (decimal 1000): `k`, `m`, `g` (without 'b')
   - Time: `ms` (×0.001), `s` (×1), `min` (×60), `h` (×3600), `d` (×86400), `w` (×604800), `y` (×31536000)
8. **TYPE DETERMINATION**:
   - Time suffix + `allow_time` → `UCL_TIME`
   - Decimal point or exponent → `UCL_FLOAT`
   - Otherwise → `UCL_INT`

**Atom Termination**: Unquoted values end at characters with `VALUE_END` flag: whitespace, `,`, `;`, `}`, `]`, `#`, or newline.

**Performance Optimizations**:
- Character table lookup instead of multiple conditionals
- In-place string unescaping (destination pointer = source pointer, write result back to same buffer)
- Zero-copy mode: strings reference input buffer directly with `Cow<'a, str>` (requires input to remain valid)
- Inline small functions for hot paths

**Error Handling**:
All lexer functions return `Result` with position information:
- `UnterminatedString { position }` - No closing quote
- `InvalidEscape { sequence, position }` - Bad escape like `\q`
- `InvalidUnicodeEscape { sequence, position }` - Bad `\uXXXX`
- `UnterminatedComment { position }` - Unmatched `/*`
- `InvalidNumber { message, position }` - Malformed number

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

**Error Handling:**
- All public functions return `Result<T, UclError>`
- Include position information in all errors
- Use `thiserror` derive macro

**Testing:**
- Unit tests inline with `#[cfg(test)]`
- Integration tests in `tests/` directory
- Benchmark comparisons in `benches/`

**Performance:**
- Prefer in-place operations over allocations
- Use `Cow<'a, str>` for zero-copy mode
- Character classification via lookup table, not conditionals
- Profile with `cargo bench` before optimizing

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
