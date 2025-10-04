#!/bin/bash

# Performance profiling script for UCL compatibility features
# This script runs comprehensive benchmarks and generates performance reports

set -e

echo "🚀 Starting UCL Compatibility Performance Profiling"
echo "=================================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Create results directory
RESULTS_DIR="target/benchmark-results"
mkdir -p "$RESULTS_DIR"

# Function to run benchmark and capture results
run_benchmark() {
    local bench_name=$1
    local output_file="$RESULTS_DIR/${bench_name}_$(date +%Y%m%d_%H%M%S).txt"
    
    echo -e "${BLUE}Running $bench_name benchmark...${NC}"
    
    # Run benchmark with detailed output
    cargo bench --bench "$bench_name" -- --output-format verbose 2>&1 | tee "$output_file"
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ $bench_name completed successfully${NC}"
        echo "Results saved to: $output_file"
    else
        echo -e "${RED}✗ $bench_name failed${NC}"
        return 1
    fi
    
    echo ""
}

# Function to run flamegraph profiling
run_flamegraph() {
    local bench_name=$1
    local flamegraph_dir="$RESULTS_DIR/flamegraphs"
    mkdir -p "$flamegraph_dir"
    
    echo -e "${BLUE}Generating flamegraph for $bench_name...${NC}"
    
    # Check if flamegraph is available
    if ! command -v flamegraph &> /dev/null; then
        echo -e "${YELLOW}⚠ flamegraph not found. Install with: cargo install flamegraph${NC}"
        return 0
    fi
    
    # Generate flamegraph
    flamegraph -o "$flamegraph_dir/${bench_name}_flamegraph.svg" -- \
        cargo bench --bench "$bench_name" -- --profile-time 10
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ Flamegraph generated: $flamegraph_dir/${bench_name}_flamegraph.svg${NC}"
    else
        echo -e "${YELLOW}⚠ Flamegraph generation failed for $bench_name${NC}"
    fi
    
    echo ""
}

# Function to run memory profiling with valgrind (if available)
run_memory_profile() {
    local bench_name=$1
    local memory_dir="$RESULTS_DIR/memory"
    mkdir -p "$memory_dir"
    
    echo -e "${BLUE}Running memory profile for $bench_name...${NC}"
    
    # Check if valgrind is available
    if ! command -v valgrind &> /dev/null; then
        echo -e "${YELLOW}⚠ valgrind not found. Skipping memory profiling.${NC}"
        return 0
    fi
    
    # Run with valgrind massif for heap profiling
    valgrind --tool=massif --massif-out-file="$memory_dir/${bench_name}_massif.out" \
        cargo bench --bench "$bench_name" -- --bench 2>&1 | \
        tee "$memory_dir/${bench_name}_memory.txt"
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ Memory profile completed: $memory_dir/${bench_name}_massif.out${NC}"
    else
        echo -e "${YELLOW}⚠ Memory profiling failed for $bench_name${NC}"
    fi
    
    echo ""
}

# Function to generate performance comparison report
generate_comparison_report() {
    local report_file="$RESULTS_DIR/performance_comparison_$(date +%Y%m%d_%H%M%S).md"
    
    echo -e "${BLUE}Generating performance comparison report...${NC}"
    
    cat > "$report_file" << 'EOF'
# UCL Compatibility Features Performance Report

This report compares the performance impact of UCL compatibility features.

## Benchmark Results Summary

### NGINX-Style Syntax Parsing
- **Impact**: Measures the performance difference between NGINX-style implicit syntax and explicit syntax
- **Key Metrics**: Throughput (bytes/sec), latency per operation
- **Expected**: Minimal overhead for syntax detection

### Mixed Syntax Parsing  
- **Impact**: Performance when mixing implicit and explicit syntax styles
- **Key Metrics**: Parse time, memory usage
- **Expected**: Comparable to pure syntax styles

### C++ Comment Parsing
- **Impact**: Performance difference between C++ comments (//) and hash comments (#)
- **Key Metrics**: Lexing speed, comment processing overhead
- **Expected**: Negligible difference in comment processing

### Unicode Escape Parsing
- **Impact**: Extended Unicode escape sequences (\u{...}) vs fixed-length (\uXXXX)
- **Key Metrics**: String parsing time, Unicode validation overhead
- **Expected**: Slight increase for variable-length parsing

### Bare Word Parsing
- **Impact**: Unquoted identifiers vs quoted strings
- **Key Metrics**: Token recognition speed, keyword lookup time
- **Expected**: Faster for simple identifiers, overhead for keyword detection

### Syntax Detection Overhead
- **Impact**: Cost of detecting syntax style (explicit vs implicit)
- **Key Metrics**: Lookahead performance, decision tree traversal
- **Expected**: Minimal overhead with efficient lookahead caching

## Performance Optimization Opportunities

### Hot Path Optimizations
1. **Character Classification**: Use lookup tables for character type detection
2. **Token Lookahead**: Cache lookahead results to avoid re-parsing
3. **String Interning**: Reuse common string values (keywords, operators)
4. **Memory Pool**: Reduce allocations with object pooling

### Syntax Detection Optimizations
1. **Fast Path Detection**: Quick identification of common patterns
2. **Lookahead Limits**: Minimize lookahead distance for syntax decisions
3. **Context Caching**: Cache syntax context to avoid repeated analysis

### Memory Optimizations
1. **Zero-Copy Parsing**: Minimize string allocations where possible
2. **Incremental Parsing**: Process large files in chunks
3. **Efficient Error Handling**: Minimize allocation in error paths

## Recommendations

### Performance Targets
- **Parsing Speed**: Maintain >90% of baseline performance for pure explicit syntax
- **Memory Usage**: Keep memory overhead <10% for new syntax features
- **Error Handling**: Error path performance should not degrade significantly

### Monitoring
- Set up continuous benchmarking in CI/CD pipeline
- Monitor performance regressions with each feature addition
- Profile real-world configuration files regularly

EOF

    echo -e "${GREEN}✓ Performance report generated: $report_file${NC}"
    echo ""
}

# Main execution
echo "Starting benchmark suite..."
echo ""

# Build in release mode first
echo -e "${BLUE}Building in release mode...${NC}"
cargo build --release --benches
echo ""

# Run all benchmarks
echo "Running UCL compatibility benchmarks..."
run_benchmark "ucl_compatibility_benchmarks"

echo "Running lexer benchmarks..."
run_benchmark "lexer_benchmarks"

echo "Running parser benchmarks..."
run_benchmark "parser_benchmarks"

# Optional: Run flamegraph profiling if requested
if [ "$1" = "--flamegraph" ]; then
    echo "Running flamegraph profiling..."
    run_flamegraph "ucl_compatibility_benchmarks"
fi

# Optional: Run memory profiling if requested
if [ "$1" = "--memory" ] || [ "$2" = "--memory" ]; then
    echo "Running memory profiling..."
    run_memory_profile "ucl_compatibility_benchmarks"
fi

# Generate comparison report
generate_comparison_report

# Summary
echo -e "${GREEN}🎉 Performance profiling completed!${NC}"
echo ""
echo "Results available in: $RESULTS_DIR"
echo ""
echo "To view HTML reports (if generated):"
echo "  open target/criterion/report/index.html"
echo ""
echo "To run with additional profiling:"
echo "  $0 --flamegraph    # Generate flamegraphs"
echo "  $0 --memory        # Run memory profiling"
echo "  $0 --flamegraph --memory  # Run both"
echo ""

# Check for performance regressions (basic check)
echo -e "${BLUE}Checking for potential performance issues...${NC}"

# Look for any benchmark failures in the results
if grep -r "FAILED" "$RESULTS_DIR" > /dev/null 2>&1; then
    echo -e "${RED}⚠ Some benchmarks failed. Check the results for details.${NC}"
else
    echo -e "${GREEN}✓ All benchmarks completed successfully.${NC}"
fi

# Look for significant performance differences (this is a simplified check)
if grep -r "change.*-[5-9][0-9]%" "$RESULTS_DIR" > /dev/null 2>&1; then
    echo -e "${YELLOW}⚠ Potential performance regression detected (>50% slower).${NC}"
    echo "  Review the benchmark results for details."
elif grep -r "change.*+[5-9][0-9]%" "$RESULTS_DIR" > /dev/null 2>&1; then
    echo -e "${GREEN}✓ Performance improvements detected!${NC}"
else
    echo -e "${GREEN}✓ Performance appears stable.${NC}"
fi

echo ""
echo "Performance profiling complete! 🚀"