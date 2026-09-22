//! Serde helper for [`std::time::Duration`] fields (PLAN.md §2.3).
//!
//! ```
//! use serde::Deserialize;
//! use std::time::Duration;
//!
//! #[derive(Deserialize)]
//! struct Config {
//!     #[serde(with = "ucl_lexer::time")]
//!     timeout: Duration,
//! }
//!
//! let config: Config = ucl_lexer::from_str("timeout = 90s").unwrap();
//! assert_eq!(config.timeout, Duration::from_secs(90));
//! ```
//!
//! Any number is read as seconds: a UCL time (`30s`, `10ms`, `2h`), a float or an integer. The
//! time type does not survive emission in libucl either (`1s` is written back as `1.0`, upstream
//! `tests/basic/2.res`), so a stricter rule could not read libucl's own output. Negative,
//! non-finite and out-of-range values are errors.

use serde::de::{self, Deserializer, Visitor};
use std::fmt;
use std::time::Duration;

/// Deserializes a number of seconds into a [`Duration`].
pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_f64(SecondsVisitor)
}

struct SecondsVisitor;

impl SecondsVisitor {
    fn from_secs<E: de::Error>(secs: f64) -> Result<Duration, E> {
        Duration::try_from_secs_f64(secs)
            .map_err(|_| E::invalid_value(de::Unexpected::Float(secs), &Self))
    }
}

impl<'de> Visitor<'de> for SecondsVisitor {
    type Value = Duration;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a non-negative number of seconds")
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Duration, E> {
        Self::from_secs(v)
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Duration, E> {
        Ok(Duration::from_secs(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Duration, E> {
        u64::try_from(v)
            .map(Duration::from_secs)
            .map_err(|_| E::invalid_value(de::Unexpected::Signed(v), &self))
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use std::time::Duration;

    #[derive(Debug, Deserialize)]
    struct Config {
        #[serde(with = "crate::time")]
        t: Duration,
    }

    fn parse(input: &str) -> Result<Duration, crate::UclError> {
        crate::from_str::<Config>(input).map(|c| c.t)
    }

    #[test]
    fn test_time_float_and_integer_are_seconds() {
        assert_eq!(parse("t = 30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse("t = 10min").unwrap(), Duration::from_secs(600));
        assert_eq!(parse("t = 1.5").unwrap(), Duration::from_millis(1500));
        assert_eq!(parse("t = 2").unwrap(), Duration::from_secs(2));
    }

    #[test]
    fn test_negative_non_finite_and_non_numbers_are_errors() {
        for input in [
            "t = -1",
            "t = -1.5",
            "t = inf",
            "t = nan",
            "t = \"30s\"",
            "t = true",
        ] {
            assert!(parse(input).is_err(), "{input:?}");
        }
    }
}
