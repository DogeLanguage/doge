//! `roll` — random numbers and sampling. A DnD-flavored RNG: `int` rolls a whole
//! number in an inclusive range, `float` lands in `[0, 1)`, and `choice`/`shuffle`/
//! `sample` draw from a List. `seed` fixes the sequence for reproducible runs.
//!
//! The generator is `xoshiro256**` seeded through `SplitMix64` — both are
//! public-domain algorithms, so `roll` needs no third-party dependency (like
//! `nap`'s calendar math). State is thread-local and lazily seeded from the system
//! clock, so each `pack` pup gets its own independent stream; `roll.seed` reseeds
//! only the calling thread. Every fallible member returns a catchable `DogeError`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{DogeError, DogeResult};
use crate::stdlib::int_arg;
use crate::value::Value;

/// `2^53`, the number of representable Floats in `[0, 1)` — the divisor that turns
/// 53 random bits into a uniform Float without ever reaching `1.0`.
const FLOAT_DIVISOR: f64 = (1u64 << 53) as f64;

/// Bumped once per lazy seed so two threads seeding in the same clock tick still
/// get distinct streams. Process-global, and the only shared state in the module.
static SEED_COUNTER: AtomicU64 = AtomicU64::new(0);

thread_local! {
    /// This thread's generator, lazily seeded from the clock on first use and
    /// replaced wholesale by `roll.seed`.
    static RNG: RefCell<Xoshiro256ss> = RefCell::new(Xoshiro256ss::from_entropy());
}

/// SplitMix64 — expands a single 64-bit seed into the four well-mixed words
/// `xoshiro256**` needs, so even a seed of `0` produces a healthy state.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// xoshiro256** — a fast, good-quality generator over four 64-bit words.
struct Xoshiro256ss {
    s: [u64; 4],
}

impl Xoshiro256ss {
    /// Seed the state from a single number via SplitMix64.
    fn from_seed(seed: u64) -> Self {
        let mut sm = SplitMix64(seed);
        Xoshiro256ss {
            s: [sm.next(), sm.next(), sm.next(), sm.next()],
        }
    }

    /// Seed from the system clock, mixed with a per-thread counter so concurrent
    /// pups never share a stream.
    fn from_entropy() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let count = SEED_COUNTER.fetch_add(1, Ordering::Relaxed);
        Xoshiro256ss::from_seed(nanos ^ count.wrapping_mul(0x9E37_79B9_7F4A_7C15))
    }

    fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// A uniform integer in `[0, bound)` with no modulo bias (Lemire's method:
    /// reject the low zone that would skew the mapping). `bound` is always
    /// non-zero at the call sites.
    fn below(&mut self, bound: u64) -> u64 {
        let mut x = self.next_u64();
        let mut m = (x as u128).wrapping_mul(bound as u128);
        let mut low = m as u64;
        if low < bound {
            let threshold = bound.wrapping_neg() % bound;
            while low < threshold {
                x = self.next_u64();
                m = (x as u128).wrapping_mul(bound as u128);
                low = m as u64;
            }
        }
        (m >> 64) as u64
    }

    /// A uniform Float in `[0, 1)` from the top 53 bits.
    fn next_float(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / FLOAT_DIVISOR
    }
}

/// Run `f` against this thread's generator.
fn with_rng<T>(f: impl FnOnce(&mut Xoshiro256ss) -> T) -> T {
    RNG.with(|rng| f(&mut rng.borrow_mut()))
}

/// A List argument as its shared backing vector, or a catchable type error naming
/// the member. The `roll` counterpart of [`crate::stdlib::str_arg`]/`int_arg`.
fn list_arg<'a>(fname: &str, v: &'a Value) -> DogeResult<&'a Rc<RefCell<Vec<Value>>>> {
    match v {
        Value::List(items) => Ok(items),
        _ => Err(DogeError::type_error(format!(
            "roll.{fname} needs a List, got {}",
            v.describe()
        ))),
    }
}

/// `roll.seed(n)` — reseed this thread's generator so the following draws repeat
/// exactly on the next run. Returns `none`.
pub fn roll_seed(n: &Value) -> DogeResult {
    let seed = int_arg("roll", "seed", n)?;
    with_rng(|rng| *rng = Xoshiro256ss::from_seed(seed as u64));
    Ok(Value::None)
}

/// `roll.int(low, high)` — a uniform Int in the inclusive range `[low, high]`. A
/// `low` above `high` is a catchable `ValueError`, not an empty range.
pub fn roll_int(low: &Value, high: &Value) -> DogeResult {
    let low = int_arg("roll", "int", low)?;
    let high = int_arg("roll", "int", high)?;
    if low > high {
        return Err(DogeError::value_error(format!(
            "roll.int needs low <= high, but {low} > {high} — swap the bounds"
        )));
    }
    let span = (high as i128 - low as i128 + 1) as u128;
    let draw = if span > u64::MAX as u128 {
        with_rng(|rng| rng.next_u64())
    } else {
        with_rng(|rng| rng.below(span as u64))
    };
    Ok(Value::int((low as i128 + draw as i128) as i64))
}

/// `roll.float()` — a uniform Float in `0.0 <= x < 1.0`.
pub fn roll_float() -> DogeResult {
    Ok(Value::Float(with_rng(|rng| rng.next_float())))
}

/// `roll.choice(list)` — one random element of a non-empty List. An empty List is
/// a catchable `ValueError`.
pub fn roll_choice(list: &Value) -> DogeResult {
    let items = list_arg("choice", list)?.borrow();
    if items.is_empty() {
        return Err(DogeError::value_error(
            "roll.choice needs a non-empty List — there is nothing to choose from",
        ));
    }
    let index = with_rng(|rng| rng.below(items.len() as u64)) as usize;
    Ok(items[index].clone())
}

/// `roll.shuffle(list)` — a new List holding the same elements in random order.
/// The argument is left untouched (module functions are pure; only list *methods*
/// mutate in place).
pub fn roll_shuffle(list: &Value) -> DogeResult {
    let mut items: Vec<Value> = list_arg("shuffle", list)?.borrow().clone();
    let len = items.len();
    with_rng(|rng| fisher_yates(rng, &mut items, len));
    Ok(Value::list(items))
}

/// `roll.sample(list, k)` — a new List of `k` distinct elements drawn from the
/// List (by position, so duplicate values may appear if the List holds them). A
/// `k` below zero or above the List's length is a catchable `ValueError`.
pub fn roll_sample(list: &Value, k: &Value) -> DogeResult {
    let items: Vec<Value> = list_arg("sample", list)?.borrow().clone();
    let k = int_arg("roll", "sample", k)?;
    if k < 0 || k as u128 > items.len() as u128 {
        return Err(DogeError::value_error(format!(
            "roll.sample needs 0 <= k <= {}, got {k}",
            items.len()
        )));
    }
    let mut items = items;
    let k = k as usize;
    with_rng(|rng| fisher_yates(rng, &mut items, k));
    items.truncate(k);
    Ok(Value::list(items))
}

/// Partial Fisher–Yates: shuffle the first `count` positions of `items` into a
/// uniformly random selection, drawing each from the unshuffled remainder. With
/// `count == items.len()` this is a full shuffle.
fn fisher_yates(rng: &mut Xoshiro256ss, items: &mut [Value], count: usize) {
    let len = items.len();
    for i in 0..count.min(len) {
        let j = i + rng.below((len - i) as u64) as usize;
        items.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use bigdecimal::ToPrimitive;

    fn seed(n: i64) {
        roll_seed(&Value::int(n)).unwrap();
    }

    fn as_int(v: Value) -> i64 {
        match v {
            Value::Int(n) => n.to_i64().unwrap(),
            other => panic!("expected an Int, got {other:?}"),
        }
    }

    fn as_list(v: Value) -> Vec<Value> {
        match v {
            Value::List(items) => items.borrow().clone(),
            other => panic!("expected a List, got {other:?}"),
        }
    }

    #[test]
    fn same_seed_reproduces_the_sequence() {
        seed(1234);
        let first: Vec<i64> = (0..10)
            .map(|_| as_int(roll_int(&Value::int(0), &Value::int(1_000_000)).unwrap()))
            .collect();
        seed(1234);
        let second: Vec<i64> = (0..10)
            .map(|_| as_int(roll_int(&Value::int(0), &Value::int(1_000_000)).unwrap()))
            .collect();
        assert_eq!(first, second);
    }

    #[test]
    fn int_stays_within_the_inclusive_range() {
        seed(7);
        for _ in 0..1000 {
            let n = as_int(roll_int(&Value::int(-3), &Value::int(3)).unwrap());
            assert!((-3..=3).contains(&n), "{n} out of range");
        }
    }

    #[test]
    fn int_with_equal_bounds_is_that_bound() {
        seed(7);
        assert_eq!(as_int(roll_int(&Value::int(5), &Value::int(5)).unwrap()), 5);
    }

    #[test]
    fn int_low_above_high_is_value_error() {
        assert_eq!(
            roll_int(&Value::int(5), &Value::int(1)).unwrap_err().kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn float_stays_in_the_unit_interval() {
        seed(99);
        for _ in 0..1000 {
            let f = match roll_float().unwrap() {
                Value::Float(f) => f,
                other => panic!("expected a Float, got {other:?}"),
            };
            assert!((0.0..1.0).contains(&f), "{f} out of range");
        }
    }

    #[test]
    fn choice_returns_a_member_and_rejects_empty() {
        seed(42);
        let list = Value::list(vec![Value::int(10), Value::int(20), Value::int(30)]);
        for _ in 0..100 {
            let n = as_int(roll_choice(&list).unwrap());
            assert!([10, 20, 30].contains(&n));
        }
        assert_eq!(
            roll_choice(&Value::list(vec![])).unwrap_err().kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn shuffle_is_a_permutation_that_leaves_the_input_untouched() {
        seed(2024);
        let list = Value::list((0..8).map(Value::int).collect());
        let shuffled = as_list(roll_shuffle(&list).unwrap());
        // The source List is unchanged (module functions are pure).
        let after: Vec<i64> = as_list(list).into_iter().map(as_int).collect();
        assert_eq!(after, (0..8).collect::<Vec<_>>());
        // Same multiset, just reordered.
        let mut got: Vec<i64> = shuffled.into_iter().map(as_int).collect();
        got.sort_unstable();
        assert_eq!(got, (0..8).collect::<Vec<_>>());
    }

    #[test]
    fn sample_draws_k_distinct_positions() {
        seed(2024);
        let list = Value::list((0..10).map(Value::int).collect());
        let picked = as_list(roll_sample(&list, &Value::int(4)).unwrap());
        assert_eq!(picked.len(), 4);
        let mut values: Vec<i64> = picked.into_iter().map(as_int).collect();
        values.sort_unstable();
        values.dedup();
        assert_eq!(values.len(), 4, "sampled positions should be distinct");
    }

    #[test]
    fn sample_bounds_are_value_errors() {
        let list = Value::list((0..3).map(Value::int).collect());
        assert_eq!(
            roll_sample(&list, &Value::int(4)).unwrap_err().kind,
            ErrorKind::ValueError
        );
        assert_eq!(
            roll_sample(&list, &Value::int(-1)).unwrap_err().kind,
            ErrorKind::ValueError
        );
    }

    #[test]
    fn sample_of_zero_is_empty_and_of_all_is_a_permutation() {
        seed(1);
        let list = Value::list((0..5).map(Value::int).collect());
        assert!(as_list(roll_sample(&list, &Value::int(0)).unwrap()).is_empty());
        let all = as_list(roll_sample(&list, &Value::int(5)).unwrap());
        let mut values: Vec<i64> = all.into_iter().map(as_int).collect();
        values.sort_unstable();
        assert_eq!(values, (0..5).collect::<Vec<_>>());
    }

    #[test]
    fn wrong_arg_types_are_type_errors() {
        assert_eq!(
            roll_seed(&Value::str("x")).unwrap_err().kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            roll_int(&Value::str("x"), &Value::int(1)).unwrap_err().kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            roll_choice(&Value::int(1)).unwrap_err().kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            roll_shuffle(&Value::int(1)).unwrap_err().kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            roll_sample(&Value::int(1), &Value::int(1))
                .unwrap_err()
                .kind,
            ErrorKind::TypeError
        );
    }
}
