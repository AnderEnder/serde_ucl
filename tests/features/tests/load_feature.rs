//! The crate built without its `load` feature (spec §9.6; WORKLIST C3, C7).

use serde_ucl::UclValue;
use serde_ucl::parse::{MemoryLoader, Parser};

const LOAD: &str = ".load(key=\"k\") \"x\"";

#[test]
fn load_needs_its_feature() {
    // `.load` fails with the "unsupported" error, whether or not the file exists.
    let error = serde_ucl::parse::parse(LOAD.as_bytes()).unwrap_err();
    assert!(error.is_unsupported(), "{error}");

    let mut loader = MemoryLoader::new();
    loader.add_file("/x", "v");
    let mut parser = Parser::new();
    parser.set_loader(loader);
    let error = parser.parse(LOAD.as_bytes()).unwrap_err();
    assert!(error.is_unsupported(), "{error}");

    // The serde entry points report the same error.
    let error = serde_ucl::from_str::<UclValue>(LOAD).unwrap_err();
    assert!(
        error.parse_error().is_some_and(|e| e.is_unsupported()),
        "{error}"
    );
}
