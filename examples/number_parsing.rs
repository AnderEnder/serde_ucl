//! Number Parsing Example
//!
//! This example demonstrates UCL's rich number parsing capabilities including
//! size suffixes, time suffixes, and special numeric values.

use serde::Deserialize;
use ucl_lexer::from_str;

#[derive(Debug, Deserialize)]
struct NumberConfig {
    // Basic numbers
    integer: i64,
    float: f64,
    negative: i64,

    // Size suffixes (binary)
    memory_limit: u64, // Will parse "512mb" as bytes
    cache_size: u64,   // Will parse "2gb" as bytes
    buffer_size: u64,  // Will parse "64kb" as bytes

    // Size suffixes (decimal)
    disk_space: u64,        // Will parse "1tb" as bytes
    network_bandwidth: u64, // Will parse "100mbps" as bits per second

    // Time suffixes
    timeout: f64,          // Will parse "30s" as seconds
    retry_delay: f64,      // Will parse "500ms" as seconds
    session_duration: f64, // Will parse "2h" as seconds
    backup_interval: f64,  // Will parse "1d" as seconds

    // Hexadecimal numbers
    hex_value: u64, // Will parse "0xFF" as 255

    // Special values
    infinity: f64,     // Will parse "inf"
    not_a_number: f64, // Will parse "nan"

    // Arrays of numbers with mixed formats
    timeouts: Vec<f64>,
    sizes: Vec<u64>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("UCL Number Parsing Example");
    println!("==========================\n");

    // Example 1: Basic number formats
    demo_basic_numbers()?;

    // Example 2: Size suffixes
    demo_size_suffixes()?;

    // Example 3: Time suffixes
    demo_time_suffixes()?;

    // Example 4: Special values and hex
    demo_special_values()?;

    // Example 5: Mixed number arrays
    demo_number_arrays()?;

    // Example 6: Full number configuration
    demo_full_number_config()?;

    Ok(())
}

fn demo_basic_numbers() -> Result<(), Box<dyn std::error::Error>> {
    println!("1. Basic Number Formats");
    println!("-----------------------");

    let config_text = r#"
        # Basic integer and float parsing
        integer = 42
        float = 3.14159
        negative = -123
        scientific = 1.23e-4
        large_number = 1000000
        zero = 0
    "#;

    #[derive(Debug, Deserialize)]
    struct BasicNumbers {
        integer: i64,
        float: f64,
        negative: i64,
        scientific: f64,
        large_number: u64,
        zero: i32,
    }

    let config: BasicNumbers = from_str(config_text)?;

    println!("Parsed basic numbers:");
    println!("  Integer: {}", config.integer);
    println!("  Float: {}", config.float);
    println!("  Negative: {}", config.negative);
    println!("  Scientific: {}", config.scientific);
    println!("  Large number: {}", config.large_number);
    println!("  Zero: {}", config.zero);

    println!();
    Ok(())
}

fn demo_size_suffixes() -> Result<(), Box<dyn std::error::Error>> {
    println!("2. Size Suffixes");
    println!("----------------");

    let config_text = r#"
        # Binary size suffixes (1024-based)
        small_buffer = 64kb      # 64 * 1024 bytes
        medium_cache = 512mb     # 512 * 1024 * 1024 bytes
        large_memory = 2gb       # 2 * 1024^3 bytes
        huge_storage = 1tb       # 1 * 1024^4 bytes
        
        # Decimal size suffixes (1000-based)
        network_speed = 100mbps  # 100 * 1000 * 1000 bits per second
        disk_capacity = 500gb    # 500 * 1000^3 bytes
        
        # Byte suffixes
        packet_size = 1500b      # 1500 bytes
        header_size = 64bytes    # 64 bytes
        
        # Without suffix (raw bytes)
        raw_size = 1048576       # 1MB in bytes
    "#;

    #[derive(Debug, Deserialize)]
    struct SizeConfig {
        small_buffer: u64,
        medium_cache: u64,
        large_memory: u64,
        huge_storage: u64,
        network_speed: u64,
        disk_capacity: u64,
        packet_size: u64,
        header_size: u64,
        raw_size: u64,
    }

    let config: SizeConfig = from_str(config_text)?;

    println!("Parsed size values:");
    println!(
        "  Small buffer: {} bytes ({} KB)",
        config.small_buffer,
        config.small_buffer / 1024
    );
    println!(
        "  Medium cache: {} bytes ({} MB)",
        config.medium_cache,
        config.medium_cache / (1024 * 1024)
    );
    println!(
        "  Large memory: {} bytes ({} GB)",
        config.large_memory,
        config.large_memory / (1024 * 1024 * 1024)
    );
    println!(
        "  Huge storage: {} bytes ({} TB)",
        config.huge_storage,
        config.huge_storage / (1024_u64.pow(4))
    );
    println!(
        "  Network speed: {} bps ({} Mbps)",
        config.network_speed,
        config.network_speed / (1000 * 1000)
    );
    println!(
        "  Disk capacity: {} bytes ({} GB)",
        config.disk_capacity,
        config.disk_capacity / (1000 * 1000 * 1000)
    );
    println!("  Packet size: {} bytes", config.packet_size);
    println!("  Header size: {} bytes", config.header_size);
    println!("  Raw size: {} bytes", config.raw_size);

    println!();
    Ok(())
}

fn demo_time_suffixes() -> Result<(), Box<dyn std::error::Error>> {
    println!("3. Time Suffixes");
    println!("----------------");

    let config_text = r#"
        # Time suffixes (all converted to seconds)
        quick_timeout = 500ms    # 0.5 seconds
        normal_timeout = 30s     # 30 seconds
        long_timeout = 5min      # 300 seconds
        session_timeout = 2h     # 7200 seconds
        backup_interval = 1d     # 86400 seconds
        archive_period = 1w      # 604800 seconds
        retention_period = 1y    # 31536000 seconds (365 days)
        
        # Fractional time values
        precise_delay = 1.5s     # 1.5 seconds
        sub_second = 0.1s        # 0.1 seconds
        
        # Multiple units (if supported)
        complex_duration = 1h30min  # 5400 seconds
        
        # Without suffix (raw seconds)
        raw_seconds = 3600       # 1 hour in seconds
    "#;

    #[derive(Debug, Deserialize)]
    struct TimeConfig {
        quick_timeout: f64,
        normal_timeout: f64,
        long_timeout: f64,
        session_timeout: f64,
        backup_interval: f64,
        archive_period: f64,
        retention_period: f64,
        precise_delay: f64,
        sub_second: f64,
        // complex_duration: f64,  // May not be supported yet
        raw_seconds: f64,
    }

    let config: TimeConfig = from_str(config_text)?;

    println!("Parsed time values (all in seconds):");
    println!(
        "  Quick timeout: {}s ({}ms)",
        config.quick_timeout,
        config.quick_timeout * 1000.0
    );
    println!("  Normal timeout: {}s", config.normal_timeout);
    println!(
        "  Long timeout: {}s ({}min)",
        config.long_timeout,
        config.long_timeout / 60.0
    );
    println!(
        "  Session timeout: {}s ({}h)",
        config.session_timeout,
        config.session_timeout / 3600.0
    );
    println!(
        "  Backup interval: {}s ({}d)",
        config.backup_interval,
        config.backup_interval / 86400.0
    );
    println!(
        "  Archive period: {}s ({}w)",
        config.archive_period,
        config.archive_period / 604800.0
    );
    println!(
        "  Retention period: {}s ({}y)",
        config.retention_period,
        config.retention_period / 31536000.0
    );
    println!("  Precise delay: {}s", config.precise_delay);
    println!("  Sub-second: {}s", config.sub_second);
    println!("  Raw seconds: {}s", config.raw_seconds);

    println!();
    Ok(())
}

fn demo_special_values() -> Result<(), Box<dyn std::error::Error>> {
    println!("4. Special Values and Hexadecimal");
    println!("----------------------------------");

    let config_text = r#"
        # Hexadecimal numbers
        hex_color = 0xFF0000     # Red color (16711680)
        hex_mask = 0xFFFF        # Bit mask (65535)
        hex_address = 0x1000     # Memory address (4096)
        
        # Binary numbers (if supported)
        binary_flags = 0b11010101  # Binary representation (213)
        
        # Octal numbers (if supported)
        octal_permissions = 0o755  # File permissions (493)
        
        # Special floating-point values
        positive_infinity = inf
        negative_infinity = -inf
        not_a_number = nan
        
        # Very large and small numbers
        very_large = 1e308
        very_small = 1e-308
        
        # Zero variations
        positive_zero = +0
        negative_zero = -0
    "#;

    #[derive(Debug, Deserialize)]
    struct SpecialNumbers {
        hex_color: u32,
        hex_mask: u16,
        hex_address: u32,
        // binary_flags: u8,      // May not be supported yet
        // octal_permissions: u16, // May not be supported yet
        positive_infinity: f64,
        negative_infinity: f64,
        not_a_number: f64,
        very_large: f64,
        very_small: f64,
        positive_zero: f64,
        negative_zero: f64,
    }

    let config: SpecialNumbers = from_str(config_text)?;

    println!("Parsed special values:");
    println!(
        "  Hex color: {} (0x{:X})",
        config.hex_color, config.hex_color
    );
    println!("  Hex mask: {} (0x{:X})", config.hex_mask, config.hex_mask);
    println!(
        "  Hex address: {} (0x{:X})",
        config.hex_address, config.hex_address
    );
    println!("  Positive infinity: {}", config.positive_infinity);
    println!("  Negative infinity: {}", config.negative_infinity);
    println!("  Not a number: {}", config.not_a_number);
    println!("  Very large: {}", config.very_large);
    println!("  Very small: {}", config.very_small);
    println!("  Positive zero: {}", config.positive_zero);
    println!("  Negative zero: {}", config.negative_zero);

    // Demonstrate special value checks
    println!("\nSpecial value checks:");
    println!(
        "  Is positive infinity infinite? {}",
        config.positive_infinity.is_infinite()
    );
    println!(
        "  Is negative infinity infinite? {}",
        config.negative_infinity.is_infinite()
    );
    println!("  Is NaN actually NaN? {}", config.not_a_number.is_nan());

    println!();
    Ok(())
}

fn demo_number_arrays() -> Result<(), Box<dyn std::error::Error>> {
    println!("5. Mixed Number Arrays");
    println!("----------------------");

    let config_text = r#"
        # Arrays with mixed number formats
        timeouts = [1s, 5s, 30s, 2min, 1h]
        
        memory_sizes = [64kb, 512mb, 2gb, 1tb]
        
        mixed_integers = [42, 0xFF, 0b1010, 1000]
        
        mixed_floats = [3.14, 1e-3, inf, -inf, nan]
        
        # Nested arrays with numbers
        timeout_matrix = [
            [1s, 2s, 3s],
            [10s, 20s, 30s],
            [1min, 2min, 5min]
        ]
        
        # Object with number values
        performance_thresholds {
            cpu_warning = 80.0
            cpu_critical = 95.0
            memory_warning = 85.0
            memory_critical = 95.0
            disk_warning = 90.0
            disk_critical = 98.0
            response_time_warning = 2s
            response_time_critical = 5s
        }
    "#;

    #[derive(Debug, Deserialize)]
    struct ArrayConfig {
        timeouts: Vec<f64>,
        memory_sizes: Vec<u64>,
        mixed_integers: Vec<i64>,
        mixed_floats: Vec<f64>,
        timeout_matrix: Vec<Vec<f64>>,
        performance_thresholds: PerformanceThresholds,
    }

    #[derive(Debug, Deserialize)]
    struct PerformanceThresholds {
        cpu_warning: f64,
        cpu_critical: f64,
        memory_warning: f64,
        memory_critical: f64,
        disk_warning: f64,
        disk_critical: f64,
        response_time_warning: f64,
        response_time_critical: f64,
    }

    let config: ArrayConfig = from_str(config_text)?;

    println!("Parsed number arrays:");

    println!("  Timeouts: {:?}", config.timeouts);
    println!(
        "    In minutes: {:?}",
        config
            .timeouts
            .iter()
            .map(|&t| t / 60.0)
            .collect::<Vec<_>>()
    );

    println!("  Memory sizes: {:?}", config.memory_sizes);
    println!(
        "    In MB: {:?}",
        config
            .memory_sizes
            .iter()
            .map(|&s| s / (1024 * 1024))
            .collect::<Vec<_>>()
    );

    println!("  Mixed integers: {:?}", config.mixed_integers);

    println!("  Mixed floats: {:?}", config.mixed_floats);

    println!("  Timeout matrix:");
    for (i, row) in config.timeout_matrix.iter().enumerate() {
        println!("    Row {}: {:?}", i, row);
    }

    println!("  Performance thresholds:");
    println!(
        "    CPU: {}% warning, {}% critical",
        config.performance_thresholds.cpu_warning, config.performance_thresholds.cpu_critical
    );
    println!(
        "    Memory: {}% warning, {}% critical",
        config.performance_thresholds.memory_warning, config.performance_thresholds.memory_critical
    );
    println!(
        "    Disk: {}% warning, {}% critical",
        config.performance_thresholds.disk_warning, config.performance_thresholds.disk_critical
    );
    println!(
        "    Response time: {}s warning, {}s critical",
        config.performance_thresholds.response_time_warning,
        config.performance_thresholds.response_time_critical
    );

    println!();
    Ok(())
}

fn demo_full_number_config() -> Result<(), Box<dyn std::error::Error>> {
    println!("6. Full Number Configuration");
    println!("----------------------------");

    let config_text = r#"
        integer = 42
        float = 3.14
        negative = -7
        memory_limit = 512mb
        cache_size = 2gb
        buffer_size = 64kb
        disk_space = 1tb
        network_bandwidth = 100mbps
        timeout = 30s
        retry_delay = 500ms
        session_duration = 2h
        backup_interval = 1d
        hex_value = 0xFF
        infinity = inf
        not_a_number = nan
        timeouts = [1s, 5s, 10s]
        sizes = [128kb, 1mb, 16mb]
    "#;

    let config: NumberConfig = from_str(config_text)?;

    println!("Parsed full number configuration:");
    println!(
        "  Integer: {} Float: {} Negative: {}",
        config.integer, config.float, config.negative
    );
    println!(
        "  Memory: {} Cache: {} Buffer: {}",
        config.memory_limit, config.cache_size, config.buffer_size
    );
    println!(
        "  Disk: {} Bandwidth: {}",
        config.disk_space, config.network_bandwidth
    );
    println!(
        "  Timeout: {}s Retry: {}s Session: {}s Backup: {}s",
        config.timeout, config.retry_delay, config.session_duration, config.backup_interval
    );
    println!("  Hex: {}", config.hex_value);
    println!(
        "  Infinity: {} NaN: {}",
        config.infinity.is_infinite(),
        config.not_a_number.is_nan()
    );
    println!("  Timeouts: {:?}", config.timeouts);
    println!("  Sizes: {:?}", config.sizes);

    println!();
    Ok(())
}
