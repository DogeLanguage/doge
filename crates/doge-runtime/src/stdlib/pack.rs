//! `pack` — the concurrency stdlib module. `zoom` spawns a function onto its own
//! OS thread (a *pup*), `fetch` waits for its result, and a *bowl* (`bowl`/`drop`/
//! `sniff`) is a channel pups pass values over. Each pup is its own single-threaded
//! world: arguments, captures, and globals are deep-copied in, results copied back
//! (see [`crate::pack`]), so no two threads share a mutable cell — no locks, no
//! `unsafe`. Every misuse — a non-callable `zoom`, a double `fetch`, a wrong handle
//! type — is a catchable error rather than a panic.

use std::sync::mpsc::RecvError;

use crate::error::{DogeError, DogeResult};
use crate::pack::{pack_value, unpack_packed, BowlHandle, PackMode, Packed, PackedError, PupEntry};
use crate::value::{BowlData, PupState, Value};

/// The stack a pup's thread gets. Matches the large stack the CLI runs the
/// interpreter on: a pup must be able to nest calls up to the same recursion limit
/// the main thread allows, and that limit — not a stack overflow — is what stops
/// runaway recursion inside a pup.
const PUP_STACK_SIZE: usize = 256 * 1024 * 1024;

/// Spawn a job onto a fresh pup thread and hand back the pup value. The job
/// produces the packed result (or error) the pup's `fetch` will return. Shared by
/// both engines: the compiler passes a generated trampoline, the interpreter a
/// closure that runs a fresh interpreter. A thread the OS refuses to start is a
/// catchable IOError.
pub fn spawn_pup<F>(job: F) -> DogeResult
where
    F: FnOnce() -> Result<Packed, PackedError> + Send + 'static,
{
    std::thread::Builder::new()
        .stack_size(PUP_STACK_SIZE)
        .spawn(job)
        .map(Value::pup)
        .map_err(|err| DogeError::io_error(format!("cannot start a pup: {err}")))
}

/// `pack.zoom(f, args)` — spawn `f` onto a new pup, called with the List `args`.
/// The callee's captures and the caller's top-level variables are snapshotted in;
/// each argument is transferred in. `f` must be callable and `args` must be a List,
/// or it is a catchable type error. `entry` is the generated trampoline that
/// rebuilds a world inside the pup and runs the call.
pub fn pack_zoom(entry: PupEntry, globals: Packed, f: &Value, args: &Value) -> DogeResult {
    if !matches!(
        f,
        Value::Function(_) | Value::Class(_) | Value::BoundMethod(_)
    ) {
        return Err(DogeError::type_error(format!(
            "pack.zoom needs something callable to run, got {}",
            f.describe()
        )));
    }
    let items = match args {
        Value::List(items) => items.borrow(),
        _ => {
            return Err(DogeError::type_error(format!(
                "pack.zoom needs a List of arguments, got {}",
                args.describe()
            )))
        }
    };
    let packed_f = pack_value(f, PackMode::Snapshot)?;
    let mut packed_args = Vec::with_capacity(items.len());
    for item in items.iter() {
        packed_args.push(pack_value(item, PackMode::Transfer)?);
    }
    drop(items);
    spawn_pup(move || entry(globals, packed_f, packed_args))
}

/// `pack.fetch(pup)` — wait for a pup to finish and return its function's result,
/// or re-raise (catchably) the error the pup hit, with the pup's own file/line
/// preserved. Fetching a pup twice, or a value that is not a pup, is a catchable
/// error.
pub fn pack_fetch(pup: &Value) -> DogeResult {
    let pup = match pup {
        Value::Pup(pup) => pup,
        _ => {
            return Err(DogeError::type_error(format!(
                "pack.fetch needs a Pup, got {}",
                pup.describe()
            )))
        }
    };
    let handle = match pup.state.replace(PupState::Fetched) {
        PupState::Running(handle) => handle,
        PupState::Fetched => {
            return Err(DogeError::value_error(
                "this pup was already fetched — a pup's result can only be taken once",
            ))
        }
    };
    match handle.join() {
        Ok(Ok(value)) => Ok(unpack_packed(value)),
        Ok(Err(err)) => Err(err.into_error()),
        // The pup's thread unwound. Generated code and the interpreter are
        // panic-free by construction, so this is a Doge compiler bug surfaced as a
        // catchable error rather than a leaked Rust panic.
        Err(_) => Err(DogeError::io_error("a pup crashed unexpectedly")),
    }
}

/// `pack.bowl()` — open a fresh, empty bowl (channel).
pub fn pack_bowl() -> DogeResult {
    Ok(Value::bowl(BowlHandle::new()))
}

/// `pack.drop(bowl, value)` — send `value` into a bowl (transferring it, so a
/// socket moves along). Returns `none`. A non-bowl handle is a catchable type
/// error; a bowl whose every reader is gone is a catchable IOError rather than a
/// silent loss.
pub fn pack_drop(bowl: &Value, value: &Value) -> DogeResult {
    let bowl = bowl_arg("drop", bowl)?;
    let packed = pack_value(value, PackMode::Transfer)?;
    bowl.handle
        .sender
        .send(packed)
        .map(|()| Value::None)
        .map_err(|_| DogeError::io_error("this bowl has no readers left"))
}

/// `pack.sniff(bowl)` — block until a value arrives in the bowl, then return it.
/// A bowl that is empty and can never receive another value again (every writer is
/// gone) is a catchable IOError rather than a forever-block. A non-bowl handle is a
/// catchable type error.
pub fn pack_sniff(bowl: &Value) -> DogeResult {
    let bowl = bowl_arg("sniff", bowl)?;
    let receiver = bowl
        .handle
        .receiver
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    match receiver.recv() {
        Ok(value) => Ok(unpack_packed(value)),
        Err(RecvError) => Err(DogeError::io_error(
            "this bowl is empty and every writer is gone — nothing left to sniff",
        )),
    }
}

/// A bowl argument as its shared data, or a catchable type error naming the member.
fn bowl_arg<'a>(fname: &str, value: &'a Value) -> DogeResult<&'a BowlData> {
    match value {
        Value::Bowl(bowl) => Ok(bowl),
        _ => Err(DogeError::type_error(format!(
            "pack.{fname} needs a Bowl, got {}",
            value.describe()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use crate::pack::finish_pup;

    /// A stand-in trampoline: zoom's argument checks fire before it is ever
    /// invoked, so it only needs the [`PupEntry`] shape.
    fn stub_entry(_globals: Packed, _f: Packed, _args: Vec<Packed>) -> Result<Packed, PackedError> {
        finish_pup(Ok(Value::None))
    }

    #[test]
    fn a_pup_returns_its_value_through_fetch() {
        let pup = spawn_pup(|| finish_pup(Ok(Value::int(49)))).unwrap();
        assert!(crate::values_equal(
            &pack_fetch(&pup).unwrap(),
            &Value::int(49)
        ));
    }

    #[test]
    fn a_pups_error_is_re_raised_by_fetch() {
        let pup = spawn_pup(|| finish_pup(Err(DogeError::value_error("pup went bad")))).unwrap();
        let err = pack_fetch(&pup).unwrap_err();
        assert_eq!(err.kind, ErrorKind::ValueError);
        assert_eq!(err.message, "pup went bad");
    }

    #[test]
    fn fetching_twice_is_a_catchable_error() {
        let pup = spawn_pup(|| finish_pup(Ok(Value::None))).unwrap();
        pack_fetch(&pup).unwrap();
        assert_eq!(pack_fetch(&pup).unwrap_err().kind, ErrorKind::ValueError);
    }

    #[test]
    fn fetch_and_bowl_ops_reject_wrong_handles() {
        assert_eq!(
            pack_fetch(&Value::int(1)).unwrap_err().kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            pack_drop(&Value::int(1), &Value::None).unwrap_err().kind,
            ErrorKind::TypeError
        );
        assert_eq!(
            pack_sniff(&Value::int(1)).unwrap_err().kind,
            ErrorKind::TypeError
        );
    }

    #[test]
    fn a_bowl_carries_a_value_between_threads() {
        let bowl = pack_bowl().unwrap();
        // Only the channel handle (which is Send) crosses to the pup, exactly as a
        // real dropped/zoomed bowl does; the pup rebuilds its bowl value from it,
        // drops a value in, and the main thread sniffs it out of the same channel.
        let handle = match &bowl {
            Value::Bowl(bowl) => bowl.handle.clone(),
            _ => unreachable!(),
        };
        let pup = spawn_pup(move || {
            pack_drop(&Value::bowl(handle), &Value::str("treat")).ok();
            finish_pup(Ok(Value::None))
        })
        .unwrap();
        assert!(matches!(pack_sniff(&bowl).unwrap(), Value::Str(s) if &*s == "treat"));
        pack_fetch(&pup).unwrap();
    }

    #[test]
    fn zoom_rejects_a_non_callable_and_non_list() {
        assert_eq!(
            pack_zoom(
                stub_entry,
                Packed::None,
                &Value::int(3),
                &Value::list(vec![])
            )
            .unwrap_err()
            .kind,
            ErrorKind::TypeError
        );
        // A callable but a non-List argument bundle.
        let f = Value::function(0, "worker", vec![]);
        assert_eq!(
            pack_zoom(stub_entry, Packed::None, &f, &Value::int(3))
                .unwrap_err()
                .kind,
            ErrorKind::TypeError
        );
    }

    #[test]
    fn sending_a_pup_across_a_boundary_is_rejected() {
        let inner = spawn_pup(|| finish_pup(Ok(Value::None))).unwrap();
        // Dropping a pup into a bowl tries to pack it — a catchable type error.
        let bowl = pack_bowl().unwrap();
        assert_eq!(
            pack_drop(&bowl, &inner).unwrap_err().kind,
            ErrorKind::TypeError
        );
        pack_fetch(&inner).unwrap();
    }
}
