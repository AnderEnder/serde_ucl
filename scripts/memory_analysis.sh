#!/bin/bash

# Memory analysis script for UCL compatibility features
# This script runs memory profiling and leak detection

set -e

echo "🔍 Starting UCL Memory Analysis"
echo "==============================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Create results directory
RESULTS_DIR="target/memory-analysis"
mkdir -p "$RESULTS_DIR"

# Function to run memory benchmark
run_memory_benchmark() {
    echo -e "${BLUE}Running memory efficiency benchmarks...${NC}"
    
    local output_file="$RESULTS_DIR/memory_benchmark_$(date +%Y%m%d_%H%M%S).txt"
    
    # Run memory benchmarks with detailed output
    cargo bench --bench memory_efficiency_benchmarks 2>&1 | tee "$output_file"
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ Memory benchmarks completed${NC}"
        echo "Results saved to: $output_file"
    else
        echo -e "${RED}✗ Memory benchmarks failed${NC}"
        return 1
    fi
    
    echo ""
}

# Function to run valgrind memory check
run_valgrind_check() {
    echo -e "${BLUE}Running Valgrind memory leak detection...${NC}"
    
    # Check if valgrind is available
    if ! command -v valgrind &> /dev/null; then
        echo -e "${YELLOW}⚠ Valgrind not found. Install with: brew install valgrind (macOS) or apt-get install valgrind (Linux)${NC}"
        return 0
    fi
    
    local valgrind_dir="$RESULTS_DIR/valgrind"
    mkdir -p "$valgrind_dir"
    
    # Create a simple test program for memory leak detection
    cat > "$valgrind_dir/memory_test.rs" << 'EOF'
use ucl_lexer::from_str;
use serde_json::Value;

fn main() {
    // Test NGINX-style syntax parsing
    let nginx_config = r#"
        server {
            listen 80
            server_name example.com
            root /var/www/html
            
            location / {
                proxy_pass http://backend
                proxy_timeout 30s
            }
        }
        
        upstream backend {
            server 127.0.0.1:3000
            keepalive 32
        }
    "#;
    
    // Parse multiple times to detect leaks
    for i in 0..100 {
        let result: Result<Value, _> = from_str(nginx_config);
        if let Ok(value) = result {
            // Use the value to prevent optimization
            let _ = value.get("server");
        }
        
        if i % 10 == 0 {
            println!("Iteration: {}", i);
        }
    }
    
    // Test Unicode escape parsing
    let unicode_config = r#"
        messages = {
            welcome = "Hello \u{1F600} World!"
            success = "Success \u{2713}"
            error = "Error \u{274C}"
        }
    "#;
    
    for i in 0..50 {
        let result: Result<Value, _> = from_str(unicode_config);
        if let Ok(value) = result {
            let _ = value.get("messages");
        }
    }
    
    println!("Memory test completed");
}
EOF
    
    # Compile the test program
    rustc --edition 2021 -L target/release/deps "$valgrind_dir/memory_test.rs" \
        --extern ucl_lexer=target/release/libucl_rust_lexer.rlib \
        --extern serde_json=target/release/deps/libserde_json-*.rlib \
        -o "$valgrind_dir/memory_test" 2>/dev/null || {
        echo -e "${YELLOW}⚠ Could not compile memory test program. Skipping Valgrind check.${NC}"
        return 0
    }
    
    # Run valgrind
    echo "Running Valgrind leak check..."
    valgrind --leak-check=full --show-leak-kinds=all --track-origins=yes \
        --log-file="$valgrind_dir/valgrind_output.txt" \
        "$valgrind_dir/memory_test"
    
    # Analyze results
    if grep -q "ERROR SUMMARY: 0 errors" "$valgrind_dir/valgrind_output.txt"; then
        echo -e "${GREEN}✓ No memory errors detected${NC}"
    else
        echo -e "${YELLOW}⚠ Memory issues detected. Check $valgrind_dir/valgrind_output.txt${NC}"
    fi
    
    if grep -q "All heap blocks were freed" "$valgrind_dir/valgrind_output.txt"; then
        echo -e "${GREEN}✓ No memory leaks detected${NC}"
    else
        echo -e "${YELLOW}⚠ Potential memory leaks detected. Check $valgrind_dir/valgrind_output.txt${NC}"
    fi
    
    echo ""
}

# Function to run heap profiling with massif
run_heap_profiling() {
    echo -e "${BLUE}Running heap profiling with Massif...${NC}"
    
    if ! command -v valgrind &> /dev/null; then
        echo -e "${YELLOW}⚠ Valgrind not found. Skipping heap profiling.${NC}"
        return 0
    fi
    
    local massif_dir="$RESULTS_DIR/massif"
    mkdir -p "$massif_dir"
    
    # Use the same test program as above
    if [ ! -f "$RESULTS_DIR/valgrind/memory_test" ]; then
        echo -e "${YELLOW}⚠ Memory test program not found. Skipping heap profiling.${NC}"
        return 0
    fi
    
    echo "Running Massif heap profiler..."
    valgrind --tool=massif --massif-out-file="$massif_dir/massif.out" \
        "$RESULTS_DIR/valgrind/memory_test" > "$massif_dir/massif_log.txt" 2>&1
    
    # Generate heap profile visualization if ms_print is available
    if command -v ms_print &> /dev/null; then
        ms_print "$massif_dir/massif.out" > "$massif_dir/heap_profile.txt"
        echo -e "${GREEN}✓ Heap profile generated: $massif_dir/heap_profile.txt${NC}"
    else
        echo -e "${YELLOW}⚠ ms_print not found. Raw massif output: $massif_dir/massif.out${NC}"
    fi
    
    echo ""
}

# Function to analyze memory usage patterns
analyze_memory_patterns() {
    echo -e "${BLUE}Analyzing memory usage patterns...${NC}"
    
    local analysis_file="$RESULTS_DIR/memory_analysis_$(date +%Y%m%d_%H%M%S).md"
    
    cat > "$analysis_file" << 'EOF'
# UCL Memory Efficiency Analysis

This report analyzes memory usage patterns and efficiency of UCL compatibility features.

## Memory Usage Summary

### Key Findings

#### Parsing Memory Efficiency
- **NGINX-style syntax**: Memory usage compared to explicit syntax
- **Unicode escapes**: Memory overhead for extended Unicode processing
- **Bare words**: Memory savings from reduced string allocations
- **Comment handling**: Memory impact of comment preservation

#### Memory Leak Detection
- **Valgrind results**: Summary of leak detection
- **Heap profiling**: Peak memory usage and allocation patterns
- **Error handling**: Memory behavior during error conditions

#### Optimization Opportunities

##### String Allocation Optimization
1. **Bare Word Processing**: Minimize allocations for unquoted identifiers
2. **Unicode Escape Caching**: Cache common Unicode sequences
3. **String Interning**: Reuse common string values
4. **Zero-Copy Parsing**: Maximize use of borrowed string slices

##### Memory Pool Usage
1. **Token Allocation**: Pool frequently created tokens
2. **Error Objects**: Efficient error object creation
3. **Temporary Buffers**: Reuse parsing buffers

##### Garbage Collection Efficiency
1. **Early Drops**: Release memory as soon as possible
2. **Scope Management**: Minimize lifetime of large objects
3. **Iterator Optimization**: Use iterators to reduce intermediate allocations

## Performance vs Memory Trade-offs

### Syntax Detection
- **Memory Cost**: Lookahead caching vs repeated parsing
- **Performance Benefit**: Faster syntax recognition
- **Recommendation**: Limited lookahead cache with LRU eviction

### Comment Preservation
- **Memory Cost**: Additional storage for comment metadata
- **Feature Benefit**: Complete UCL compatibility
- **Recommendation**: Optional feature with memory limits

### Error Context
- **Memory Cost**: Rich error information storage
- **User Benefit**: Better debugging experience
- **Recommendation**: Lazy error context generation

## Memory Efficiency Targets

### Baseline Metrics
- **Small configs (<1KB)**: <10KB peak memory usage
- **Medium configs (<10KB)**: <100KB peak memory usage  
- **Large configs (<100KB)**: <1MB peak memory usage

### Optimization Goals
- **Zero-copy parsing**: >80% of strings should be borrowed
- **Memory overhead**: <20% overhead compared to JSON parsing
- **Leak prevention**: Zero memory leaks in all test scenarios

### Monitoring Recommendations
- **Continuous benchmarking**: Track memory usage in CI/CD
- **Regression detection**: Alert on >10% memory usage increase
- **Real-world testing**: Profile with actual configuration files

EOF

    echo -e "${GREEN}✓ Memory analysis report generated: $analysis_file${NC}"
    echo ""
}

# Function to run memory stress test
run_memory_stress_test() {
    echo -e "${BLUE}Running memory stress test...${NC}"
    
    local stress_dir="$RESULTS_DIR/stress_test"
    mkdir -p "$stress_dir"
    
    # Create stress test program
    cat > "$stress_dir/stress_test.rs" << 'EOF'
use ucl_lexer::from_str;
use serde_json::Value;
use std::time::Instant;

fn main() {
    println!("Starting memory stress test...");
    
    // Generate large configuration
    let mut large_config = String::from("{\n");
    for i in 0..5000 {
        large_config.push_str(&format!(r#"
    item_{} = {{
        id = {}
        name = "item-{}"
        data = "This is test data for item {} with some content to increase memory usage"
        nested = {{
            level1 = {{
                level2 = {{
                    value = "deep-value-{}"
                    number = {}
                }}
            }}
        }}
    }}
"#, i, i, i, i, i, i * 42));
    }
    large_config.push_str("}\n");
    
    println!("Generated config size: {} bytes", large_config.len());
    
    // Parse multiple times to test memory behavior
    let start = Instant::now();
    for iteration in 0..20 {
        let parse_start = Instant::now();
        let result: Result<Value, _> = from_str(&large_config);
        let parse_duration = parse_start.elapsed();
        
        match result {
            Ok(value) => {
                let item_count = value.as_object()
                    .map(|obj| obj.len())
                    .unwrap_or(0);
                println!("Iteration {}: Parsed {} items in {:?}", 
                    iteration + 1, item_count, parse_duration);
            }
            Err(e) => {
                println!("Iteration {}: Parse error: {}", iteration + 1, e);
            }
        }
        
        // Force garbage collection (if applicable)
        if iteration % 5 == 0 {
            println!("Memory checkpoint at iteration {}", iteration + 1);
        }
    }
    
    let total_duration = start.elapsed();
    println!("Stress test completed in {:?}", total_duration);
}
EOF
    
    # Compile and run stress test
    if rustc --edition 2021 -L target/release/deps "$stress_dir/stress_test.rs" \
        --extern ucl_lexer=target/release/libucl_rust_lexer.rlib \
        --extern serde_json=target/release/deps/libserde_json-*.rlib \
        -o "$stress_dir/stress_test" 2>/dev/null; then
        
        echo "Running stress test..."
        "$stress_dir/stress_test" > "$stress_dir/stress_test_output.txt" 2>&1
        
        if [ $? -eq 0 ]; then
            echo -e "${GREEN}✓ Memory stress test completed successfully${NC}"
            echo "Results: $stress_dir/stress_test_output.txt"
        else
            echo -e "${YELLOW}⚠ Memory stress test encountered issues${NC}"
            echo "Check: $stress_dir/stress_test_output.txt"
        fi
    else
        echo -e "${YELLOW}⚠ Could not compile stress test program${NC}"
    fi
    
    echo ""
}

# Main execution
echo "Building release version for memory analysis..."
cargo build --release --lib

echo ""

# Run memory analysis
run_memory_benchmark
run_valgrind_check
run_heap_profiling
run_memory_stress_test
analyze_memory_patterns

# Summary
echo -e "${GREEN}🎉 Memory analysis completed!${NC}"
echo ""
echo "Results available in: $RESULTS_DIR"
echo ""
echo "Key files:"
echo "  - Memory benchmarks: $RESULTS_DIR/memory_benchmark_*.txt"
echo "  - Valgrind output: $RESULTS_DIR/valgrind/valgrind_output.txt"
echo "  - Heap profile: $RESULTS_DIR/massif/heap_profile.txt"
echo "  - Analysis report: $RESULTS_DIR/memory_analysis_*.md"
echo ""

# Check for memory issues
if [ -f "$RESULTS_DIR/valgrind/valgrind_output.txt" ]; then
    if grep -q "ERROR SUMMARY: 0 errors" "$RESULTS_DIR/valgrind/valgrind_output.txt" && \
       grep -q "All heap blocks were freed" "$RESULTS_DIR/valgrind/valgrind_output.txt"; then
        echo -e "${GREEN}✓ No memory leaks or errors detected${NC}"
    else
        echo -e "${YELLOW}⚠ Memory issues detected. Review Valgrind output for details.${NC}"
    fi
else
    echo -e "${BLUE}ℹ Valgrind analysis not available (requires Linux/macOS with Valgrind installed)${NC}"
fi

echo ""
echo "Memory analysis complete! 🔍"