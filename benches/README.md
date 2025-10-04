# UCL Rust Lexer Benchmarks

This directory contains comprehensive benchmarks for the UCL Rust lexer implementation, focusing on performance optimization verification and comparison with other parsing approaches.

## Benchmark Categories

### 1. Lexer Benchmarks (`lexer_benchmarks.rs`)

Tests the core lexical analysis performance:

- **Tokenization**: Basic token parsing across different input sizes (small ~1KB, medium ~10KB, large ~100KB)
- **String Parsing**: Performance of different string formats (JSON, single-quoted, heredoc) with various escape patterns
- **Number Parsing**: Numeric literal parsing including suffixes and special values
- **Comment Handling**: Single-line and multi-line comment processing with and without preservation
- **Character Classification**: Character table lookup performance

### 2. Parser Benchmarks (`parser_benchmarks.rs`)

Tests the parsing and serde integration performance:

- **Basic Parsing**: Simple object and document parsing
- **Nested Structures**: Performance with deeply nested objects at various depths and breadths
- **Variable Expansion**: Impact of variable resolution with different handler types
- **Serde Deserialization**: Integration with serde for type-safe deserialization
- **Configuration Impact**: Performance effects of different parser configuration options
- **Error Handling**: Performance characteristics when parsing invalid input

### 3. Zero-Copy Benchmarks (`zero_copy_benchmarks.rs`)

Verifies zero-copy optimization effectiveness:

- **String Handling**: Comparison between allocating and zero-copy string processing
- **Memory Allocation**: Measurement of allocation patterns and their impact
- **Cow Usage**: Performance characteristics of `Cow<str>` in different scenarios
- **String Format Optimization**: Efficiency of format detection and optimization
- **Size Impact**: How zero-copy benefits scale with string size
- **Effectiveness Measurement**: Actual zero-copy usage rates in realistic scenarios

## Running Benchmarks

### Prerequisites

Ensure you have the benchmark dependencies installed:

```bash
cargo install criterion
```

### Running Individual Benchmark Suites

```bash
# Run lexer benchmarks
cargo bench --bench lexer_benchmarks

# Run parser benchmarks  
cargo bench --bench parser_benchmarks

# Run zero-copy benchmarks
cargo bench --bench zero_copy_benchmarks
```

### Running All Benchmarks

```bash
cargo bench
```

### Generating HTML Reports

Criterion automatically generates HTML reports in `target/criterion/`. Open `target/criterion/report/index.html` in a browser to view detailed results with graphs and statistics.

## Benchmark Results Interpretation

### Key Metrics

- **Throughput**: Bytes processed per second (higher is better)
- **Latency**: Time per operation (lower is better)
- **Memory Usage**: Allocation patterns and zero-copy effectiveness
- **Scalability**: Performance characteristics across different input sizes

### Expected Performance Characteristics

1. **Zero-Copy Benefits**: Should show significant performance improvements for simple strings without escapes or variables
2. **Streaming Performance**: Should maintain consistent memory usage regardless of input size
3. **Character Table Lookups**: Should demonstrate O(1) performance for character classification
4. **Parser Scaling**: Should scale reasonably with nesting depth and structure complexity

### Performance Targets

- **Small Files (< 1KB)**: Sub-millisecond parsing
- **Medium Files (< 10KB)**: Single-digit millisecond parsing
- **Large Files (< 100KB)**: Sub-100ms parsing with streaming support
- **Zero-Copy Effectiveness**: > 80% borrowed strings for simple content
- **Memory Efficiency**: Constant memory usage for streaming parser

## Comparison Baselines

The benchmarks include comparisons between:

- Regular vs zero-copy lexer modes
- Different parser configurations
- Various string formats and complexity levels
- Streaming vs in-memory parsing approaches

## Continuous Performance Monitoring

These benchmarks should be run regularly to:

1. Detect performance regressions
2. Validate optimization effectiveness
3. Guide future performance improvements
4. Compare against alternative implementations

## Adding New Benchmarks

When adding new benchmarks:

1. Follow the existing naming conventions
2. Include appropriate throughput measurements
3. Test across multiple input sizes/complexities
4. Document expected performance characteristics
5. Add comparison baselines where relevant

## Environment Considerations

Benchmark results can vary based on:

- CPU architecture and speed
- Available memory
- System load
- Rust compiler version and optimization flags
- Input data characteristics

For consistent results, run benchmarks on a quiet system with consistent configuration.