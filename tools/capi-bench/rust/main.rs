//! Spec-owned caller of the public Rust API, for comparison with the C callers.
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(
        args.len(),
        4,
        "usage: rust-api-baseline FILE parse|dump ITERATIONS"
    );
    let input = std::fs::read(&args[1]).expect("read input");
    let iterations: usize = args[3].parse().expect("iterations");
    assert!(iterations > 0);
    match args[2].as_str() {
        "parse" => {
            let start = Instant::now();
            for _ in 0..iterations {
                let value = serde_ucl::parse::parse(black_box(&input)).expect("parse input");
                drop(black_box(value));
            }
            println!("{:.12}", start.elapsed().as_secs_f64());
        }
        "dump" => {
            let mut parser = serde_ucl::parse::Parser::new();
            let value = parser.parse(&input).expect("parse input");
            print!(
                "{}",
                parser
                    .emitter(serde_ucl::emit::Format::JsonCompact)
                    .emit(&value)
            );
        }
        _ => panic!("invalid mode"),
    }
}
