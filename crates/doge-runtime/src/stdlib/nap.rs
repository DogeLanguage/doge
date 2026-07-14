//! `nap` — time and clocks. A wall clock (`now`) and a monotonic clock (`mono`),
//! both reported as Float seconds; a guarded sleep (`rest`); and whole-second UTC
//! date formatting/parsing (`stamp`/`parse`) in ISO-8601. The calendar conversion
//! is the public-domain `days_from_civil`/`civil_from_days` integer algorithm, so
//! `nap` needs no third-party dependency. Every fallible member returns a
//! catchable `DogeError` — a bad sleep duration or a malformed timestamp never
//! panics.

use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::error::{DogeError, DogeResult};
use crate::stdlib::str_arg;
use crate::value::Value;

/// Seconds in a day, the pivot between a date and a wall-clock time-of-day.
const SECS_PER_DAY: i64 = 86_400;

/// The origin the monotonic clock measures from, captured on first use. Shared
/// process-wide (like `env`'s argument slot) so every `nap.mono()` — on any
/// thread — reports seconds since the same instant.
static MONO_ORIGIN: OnceLock<Instant> = OnceLock::new();

/// A numeric argument as `f64`, or a catchable type error naming the member.
/// Shared by `rest`/`stamp`, which both accept an Int or a Float.
fn numeric(fname: &str, v: &Value) -> DogeResult<f64> {
    match v {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        _ => Err(DogeError::type_error(format!(
            "nap.{fname} needs a number, got {}",
            v.describe()
        ))),
    }
}

/// `nap.now()` — seconds since the Unix epoch (UTC), with sub-second precision.
/// Never fails: a system clock set before the epoch reads back as a negative
/// number rather than an error.
pub fn nap_now() -> DogeResult {
    let secs = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_secs_f64(),
        Err(before) => -before.duration().as_secs_f64(),
    };
    Ok(Value::Float(secs))
}

/// `nap.mono()` — seconds from a fixed process origin, for measuring elapsed time.
/// Only differences between two readings are meaningful; the origin itself is
/// arbitrary. Monotonic, so it never jumps when the wall clock is adjusted.
pub fn nap_mono() -> DogeResult {
    let origin = MONO_ORIGIN.get_or_init(Instant::now);
    Ok(Value::Float(origin.elapsed().as_secs_f64()))
}

/// `nap.rest(seconds)` — block for `seconds` (Int or Float). A negative,
/// non-finite, or absurdly large duration is a catchable `ValueError` rather than
/// the panic `Duration::from_secs_f64` would raise on such an input.
pub fn nap_rest(seconds: &Value) -> DogeResult {
    let secs = numeric("rest", seconds)?;
    if !secs.is_finite() || secs < 0.0 {
        return Err(DogeError::value_error(format!(
            "cannot rest for {secs} seconds — the duration must be finite and not negative"
        )));
    }
    if secs > Duration::MAX.as_secs_f64() {
        return Err(DogeError::value_error(
            "cannot rest that long — the duration is out of range",
        ));
    }
    std::thread::sleep(Duration::from_secs_f64(secs));
    Ok(Value::None)
}

/// `nap.stamp(secs)` — the ISO-8601 UTC string `"YYYY-MM-DDTHH:MM:SSZ"` for a unix
/// timestamp (Int or Float seconds, truncated to a whole second toward negative
/// infinity). A timestamp outside the representable Int range is a catchable
/// `ValueError`.
pub fn nap_stamp(secs: &Value) -> DogeResult {
    let secs = numeric("stamp", secs)?;
    if !secs.is_finite() || secs < i64::MIN as f64 || secs >= i64::MAX as f64 {
        return Err(DogeError::value_error(
            "that timestamp is outside the representable range",
        ));
    }
    let secs = secs.floor() as i64;
    let days = secs.div_euclid(SECS_PER_DAY);
    let time_of_day = secs.rem_euclid(SECS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;
    Ok(Value::str(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
    )))
}

/// `nap.parse(text)` — unix seconds (as a Float) for an ISO-8601 UTC timestamp
/// `"YYYY-MM-DDTHH:MM:SSZ"`. The trailing `Z` is optional. Anything that is not a
/// well-formed, in-range UTC timestamp is a catchable `ValueError` with a hint.
pub fn nap_parse(text: &Value) -> DogeResult {
    let text = str_arg("nap", "parse", text)?;
    let secs = parse_iso8601(text).ok_or_else(|| {
        DogeError::value_error(format!(
            "cannot read \"{text}\" as a timestamp — expected \"YYYY-MM-DDTHH:MM:SSZ\""
        ))
    })?;
    Ok(Value::Float(secs as f64))
}

/// Parse an ISO-8601 UTC timestamp to whole unix seconds, or `None` when the shape
/// or any field is invalid. Strict on layout (fixed widths, `-`/`:` separators, a
/// `T` between date and time) but tolerant of a missing trailing `Z`.
fn parse_iso8601(text: &str) -> Option<i64> {
    let text = text.strip_suffix('Z').unwrap_or(text);
    let (date, time) = text.split_once('T')?;

    let mut date_parts = date.splitn(3, '-');
    let year: i64 = signed_field(date_parts.next()?)?;
    let month: i64 = field(date_parts.next()?)?;
    let day: i64 = field(date_parts.next()?)?;
    if date_parts.next().is_some() {
        return None;
    }

    let mut time_parts = time.splitn(3, ':');
    let hour: i64 = field(time_parts.next()?)?;
    let minute: i64 = field(time_parts.next()?)?;
    let second: i64 = field(time_parts.next()?)?;
    if time_parts.next().is_some() {
        return None;
    }

    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=59).contains(&second)
    {
        return None;
    }

    let days = days_from_civil(year, month, day);
    Some(days * SECS_PER_DAY + hour * 3600 + minute * 60 + second)
}

/// A non-negative fixed-width numeric field (all ASCII digits), or `None`. Rejects
/// signs and whitespace, so `" 3"`, `"+3"`, and `"-3"` never slip through.
fn field(s: &str) -> Option<i64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// The year field, which may carry a leading `-` for years before 1 BCE-ish. Any
/// other sign or stray character is rejected.
fn signed_field(s: &str) -> Option<i64> {
    match s.strip_prefix('-') {
        Some(rest) => field(rest).map(|n| -n),
        None => field(s),
    }
}

/// Days since the Unix epoch (1970-01-01) for a proleptic-Gregorian civil date.
/// Howard Hinnant's public-domain `days_from_civil`; exact integer math, valid far
/// beyond any range a script will use.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date `(year, month, day)` for a count of days since the Unix epoch.
/// The inverse of [`days_from_civil`] (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    fn stamp(secs: i64) -> String {
        match nap_stamp(&Value::Int(secs)).unwrap() {
            Value::Str(s) => s.to_string(),
            other => panic!("expected a Str, got {other:?}"),
        }
    }

    fn parse(text: &str) -> f64 {
        match nap_parse(&Value::str(text)).unwrap() {
            Value::Float(f) => f,
            other => panic!("expected a Float, got {other:?}"),
        }
    }

    #[test]
    fn stamp_formats_known_timestamps() {
        assert_eq!(stamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(stamp(946_684_800), "2000-01-01T00:00:00Z");
        assert_eq!(stamp(1_609_459_199), "2020-12-31T23:59:59Z");
    }

    #[test]
    fn stamp_handles_pre_epoch_dates() {
        assert_eq!(stamp(-1), "1969-12-31T23:59:59Z");
        assert_eq!(stamp(-SECS_PER_DAY), "1969-12-31T00:00:00Z");
    }

    #[test]
    fn parse_is_the_inverse_of_stamp() {
        for secs in [0, 946_684_800, 1_609_459_199, -1, -SECS_PER_DAY] {
            assert_eq!(parse(&stamp(secs)), secs as f64);
        }
    }

    #[test]
    fn parse_tolerates_a_missing_z() {
        assert_eq!(parse("2000-01-01T00:00:00"), 946_684_800.0);
    }

    #[test]
    fn parse_rejects_malformed_input() {
        for bad in [
            "not a date",
            "2000-13-01T00:00:00Z",
            "2000-01-32T00:00:00Z",
            "2000-01-01T24:00:00Z",
            "2000-01-01 00:00:00Z",
            "2000/01/01T00:00:00Z",
            "2000-01-01T00:00:00+02:00",
            "",
        ] {
            assert_eq!(
                nap_parse(&Value::str(bad)).unwrap_err().kind,
                ErrorKind::ValueError,
                "{bad:?} should be a ValueError"
            );
        }
    }

    #[test]
    fn rest_rejects_bad_durations() {
        assert_eq!(
            nap_rest(&Value::Int(-1)).unwrap_err().kind,
            ErrorKind::ValueError
        );
        assert_eq!(
            nap_rest(&Value::Float(f64::NAN)).unwrap_err().kind,
            ErrorKind::ValueError
        );
        assert_eq!(
            nap_rest(&Value::Float(f64::INFINITY)).unwrap_err().kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn rest_zero_returns_none() {
        assert!(matches!(nap_rest(&Value::Int(0)).unwrap(), Value::None));
    }

    #[test]
    fn mono_never_goes_backwards() {
        let a = match nap_mono().unwrap() {
            Value::Float(f) => f,
            other => panic!("expected a Float, got {other:?}"),
        };
        let b = match nap_mono().unwrap() {
            Value::Float(f) => f,
            other => panic!("expected a Float, got {other:?}"),
        };
        assert!(b >= a);
    }

    #[test]
    fn now_is_after_the_epoch() {
        match nap_now().unwrap() {
            Value::Float(f) => assert!(f > 0.0),
            other => panic!("expected a Float, got {other:?}"),
        }
    }

    #[test]
    fn stamp_out_of_range_is_value_error() {
        assert_eq!(
            nap_stamp(&Value::Float(f64::INFINITY)).unwrap_err().kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn bad_arg_types_are_type_errors() {
        assert_eq!(
            nap_rest(&Value::str("soon")).unwrap_err().kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            nap_stamp(&Value::str("soon")).unwrap_err().kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            nap_parse(&Value::Int(0)).unwrap_err().kind,
            ErrorKind::TypeError
        );
    }
}
