//! The interpreter's side of `pack.zoom`. Unlike the other pack members, which are
//! plain runtime calls the natives table reaches, spawning a pup needs interpreter
//! state: the pup runs a *fresh* interpreter over the same program on its own
//! thread. This mirrors the compiled trampoline — the callee, its arguments, and a
//! snapshot of the entry file's globals are deep-copied across, and the result (or
//! error) is packed back — so an interpreted `pack.zoom` behaves exactly like a
//! compiled one.

use std::sync::Arc;

use doge_compiler as dc;
use doge_runtime::{
    finish_pup, pack_snapshot, pack_value, spawn_pup, unpack_packed, DogeError, DogeResult,
    ErrorKind, PackMode, Packed, PackedError, Value,
};

use crate::{cell, Interp};

impl Interp {
    /// `pack.zoom(f, args)` in the interpreter: validate the callee and argument
    /// list, deep-copy them plus a snapshot of the entry globals, then spawn a pup
    /// that runs a fresh interpreter over the same program. A REPL session has no
    /// whole-program handle, so `pack.zoom` is not available there yet.
    pub(crate) fn interp_zoom(&mut self, mut args: Vec<Value>) -> DogeResult<Value> {
        if args.len() != 2 {
            return Err(doge_runtime::function_arity_error(
                "pack.zoom",
                2,
                Some(2),
                args.len(),
            ));
        }
        let job = args.pop().expect("checked length");
        let callee = args.pop().expect("checked length");
        if !matches!(
            callee,
            Value::Function(_) | Value::Class(_) | Value::BoundMethod(_)
        ) {
            return Err(DogeError::type_error(format!(
                "pack.zoom needs something callable to run, got {}",
                callee.describe()
            )));
        }
        let items = match &job {
            Value::List(items) => items.borrow().clone(),
            _ => {
                return Err(DogeError::type_error(format!(
                    "pack.zoom needs a List of arguments, got {}",
                    job.describe()
                )))
            }
        };
        let program = match &self.program {
            Some(program) => program.clone(),
            None => {
                return Err(DogeError::new(
                    ErrorKind::ValueError,
                    "pack.zoom needs a running program — it isn't available in the repl yet (run the file with doge bark <script>.doge)",
                ))
            }
        };

        let packed_callee = pack_value(&callee, PackMode::Snapshot)?;
        let mut packed_args = Vec::with_capacity(items.len());
        for item in &items {
            packed_args.push(pack_value(item, PackMode::Transfer)?);
        }
        let globals = self.snapshot_globals();

        spawn_pup(move || run_pup(program, globals, packed_callee, packed_args))
    }

    /// Snapshot the entry file's globals as a name→value map, so the pup can restore
    /// them by name into its own fresh scope. A binding that cannot be snapshotted
    /// (a live pup, say) arrives as `none`, exactly as the compiled snapshot does.
    fn snapshot_globals(&self) -> Packed {
        let globals = self.globals(0);
        let globals = globals.borrow();
        let pairs = globals
            .iter()
            .map(|(name, value)| (name.clone(), pack_snapshot(&value.borrow())))
            .collect();
        Packed::Dict(pairs)
    }

    /// Overwrite the entry file's globals with a snapshot taken on the spawning
    /// thread, so the pup starts from the same top-level state.
    fn restore_globals(&mut self, globals: Packed) {
        let Packed::Dict(pairs) = globals else {
            return;
        };
        let scope = self.globals(0);
        let mut scope = scope.borrow_mut();
        for (name, packed) in pairs {
            let value = unpack_packed(packed);
            match scope.get(&name) {
                Some(existing) => *existing.borrow_mut() = value,
                None => {
                    scope.insert(name, cell(value));
                }
            }
        }
    }
}

/// Run one pup on its own thread: build a fresh interpreter over the same program,
/// restore the spawning thread's globals, unpack the callee and its arguments, run
/// the call, and pack the result (or error) for the trip back to `pack.fetch`.
fn run_pup(
    program: Arc<dc::Program>,
    globals: Packed,
    callee: Packed,
    args: Vec<Packed>,
) -> Result<Packed, PackedError> {
    let mut interp = Interp::new();
    interp.integrate_program(&program);
    interp.program = Some(program);
    interp.restore_globals(globals);

    let callee = unpack_packed(callee);
    let args: Vec<Value> = args.into_iter().map(unpack_packed).collect();
    finish_pup(interp.call_value(callee, args, Vec::new()))
}
