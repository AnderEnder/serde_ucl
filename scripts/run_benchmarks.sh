#!/bin/bash

# UCL Rust Lexer Benchmark Runner
# This script runs all benchmarks and generates reports

set -e

echo "🚀 Running UCL Rust Lexer Benchmarks"
echo "======================================"

# Check if criterion is available
if ! command -v cargo &> /dev/null; then
    echo "❌ Cargo not found. Please install Rust and Cargo."
    exit 1
fi

# Create output directory
mkdir -p benchmark_results
cd "$(dirname "$0")/.."

echo ""
echo "📊 Running Lexer Benchmarks..."
echo "------------------------------"
cargo bench --bench lexer_benchmarks

echo ""
echo "🔍 Running Parser Benchmarks..."
echo "-------------------------------"
cargo bench --bench parser_benchmarks

echo ""
echo "⚡ Running Zero-Copy Benchmarks..."
echo "---------------------------------"
cargo bench --bench zero_copy_benchmarks

echo ""
echo "✅ All benchmarks completed!"
echo ""
echo "📈 Results are available in:"
echo "   - HTML Report: target/criterion/report/index.html"
echo "   - Raw Data: target/criterion/"
echo ""
echo "🔧 To run specific benchmarks:"
echo "   cargo bench --bench lexer_benchmarks"
echo "   cargo bench --bench parser_benchmarks"
echo "   cargo bench --bench zero_copy_benchmarks"
echo ""
echo "📋 To run with specific filters:"
echo "   cargo bench -- string_parsing"
echo "   cargo bench -- zero_copy"
echo ""

# Check if HTML report exists and offer to open it
if command -v open &> /dev/null && [ -f "target/criterion/report/index.html" ]; then
    echo "🌐 Open HTML report? (y/n)"
    read -r response
    if [[ "$response" =~ ^[Yy]$ ]]; then
        open target/criterion/report/index.html
    fi
elif command -v xdg-open &> /dev/null && [ -f "target/criterion/report/index.html" ]; then
    echo "🌐 Open HTML report? (y/n)"
    read -r response
    if [[ "$response" =~ ^[Yy]$ ]]; then
        xdg-open target/criterion/report/index.html
    fi
fi

echo "🎉 Benchmark run complete!"