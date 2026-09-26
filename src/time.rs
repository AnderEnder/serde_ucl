//! Serde helper for [`std::time::Duration`] fields.
//!
//! ```
//! use serde::{Deserialize, Serialize};
//! use std::time::Duration;
//!
//! #[derive(Serialize, Deserialize)]
//! struct Config {
//!     #[serde(with = "serde_ucl::time")]
//!     timeout: Duration,
//! }
//!
//! let value = serde_ucl::parse::parse(b"timeout = 90s").unwrap();
//! let config: Config = serde_ucl::from_value(value).unwrap();
//! assert_eq!(config.timeout, Duration::from_secs(90));
//! assert_eq!(serde_ucl::to_string(&config).unwrap(), "timeout = 90.0s;\n");
//! ```
//!
//! Any number is read as seconds: a UCL time (`30s`, `10ms`, `2h`), a float or an integer. The
//! time type does not survive emission in libucl either (`1s` is written back as `1.0`, upstream
//! `tests/basic/2.res`), so a stricter rule could not read libucl's own output. Negative,
//! non-finite and out-of-range values are errors.
//!
//! A `Duration` is written as a UCL time, its seconds followed by `s` ([`crate::ser`]), except in
//! the crate's JSON output, which is valid JSON and writes the number of seconds alone; it reads
//! back as the same `Duration` either way. Other serializers see the number of seconds as an
//! `f64`.

use serde::de::{self, Deserializer, Visitor};
use serde::ser::{self as ser, Serializer};
use std::fmt;
use std::time::Duration;

/// Serializes a [`Duration`] as a UCL time: its number of seconds as an `f64`, which the crate's
/// serializer writes with the suffix `s` (spec §5.4, §10.8), in JSON as a plain number, and other
/// serializers as a plain number too.
///
/// UCL times are 64-bit floats, which cannot hold every `Duration` to the nanosecond, for
/// example `Duration::new(1_000_000_000, 1)`. Such a duration is an error rather than a value
/// that reads back as another duration.
pub fn serialize<S: Serializer>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
    let seconds = duration.as_secs_f64();
    if Duration::try_from_secs_f64(seconds).ok() != Some(*duration) {
        return Err(<S::Error as ser::Error>::custom(format!(
            "the duration {duration:?}: a UCL time is a 64-bit float of seconds, and none reads \
             back as exactly this duration"
        )));
    }
    serializer.serialize_newtype_struct(crate::ser::marker::TIME, &seconds)
}

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
    use serde::{Deserialize, Serialize};
    use std::time::Duration;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Config {
        #[serde(with = "crate::time")]
        t: Duration,
    }

    /// Serializes, reads the config text back with the new core, and deserializes.
    fn round_trip(t: Duration) -> Result<Duration, crate::UclError> {
        let text = crate::to_string(&Config { t })?;
        let value = crate::parse::parse(text.as_bytes())?;
        crate::from_value::<Config>(value).map(|c| c.t)
    }

    #[test]
    fn test_durations_round_trip_as_times() {
        for t in [
            Duration::ZERO,
            Duration::from_secs(90),
            Duration::from_millis(1500),
            Duration::from_nanos(1),
            Duration::from_nanos(300_000_000),
            Duration::new(4_000_000, 123_456_789),
            Duration::from_secs(u64::MAX >> 11),
        ] {
            assert_eq!(round_trip(t).unwrap(), t, "{t:?}");
        }
        assert_eq!(
            crate::to_string(&Config {
                t: Duration::from_millis(1500)
            })
            .unwrap(),
            "t = 1.5s;\n"
        );
        // The crate's JSON output writes the seconds alone, and they read back as the duration.
        let config = Config {
            t: Duration::new(4_000_000, 123_456_789),
        };
        let json = crate::to_json_string_compact(&config).unwrap();
        assert_eq!(json, r#"{"t":4000000.123456789}"#);
        let value = crate::parse::parse(json.as_bytes()).unwrap();
        assert_eq!(crate::from_value::<Config>(value).unwrap(), config);
        let value = crate::to_value(&Config {
            t: Duration::from_secs(2),
        })
        .unwrap();
        assert_eq!(value.as_object().unwrap()["t"], crate::UclValue::Time(2.0));
        // Other serializers see the seconds.
        let json = serde_json::to_string(&Config {
            t: Duration::from_millis(250),
        })
        .unwrap();
        assert_eq!(json, r#"{"t":0.25}"#);
    }

    #[test]
    fn test_durations_without_an_exact_float_are_errors() {
        for t in [Duration::new(1_000_000_000, 1), Duration::MAX] {
            let err = crate::to_string(&Config { t }).unwrap_err();
            assert!(
                matches!(
                    err,
                    crate::UclError::Serde(crate::error::SerdeError::Custom(_))
                ),
                "{t:?}: {err:?}"
            );
        }
    }

    fn parse(input: &str) -> Result<Duration, crate::UclError> {
        let value = crate::parse::parse(input.as_bytes())?;
        crate::from_value::<Config>(value).map(|c| c.t)
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
