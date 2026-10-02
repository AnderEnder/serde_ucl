//! Read-only C15 Stage A ABI. The caller domain is documented in capi/README.md.
//!
//! Every exported function uses the released declarations. Panics are caught at the boundary;
//! allocation failure and invalid pointers remain outside the supported caller domain.
#![allow(clippy::missing_safety_doc)]
mod iteration;
mod object;
mod parser;
pub use iteration::*;
pub use object::*;
pub use parser::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

fn boundary<T: Default>(f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_default()
}
