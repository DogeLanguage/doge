# Plan: <one-line task summary>

<!--
Written by Sol (planner). Consumed by Luna (executor) as .codex/plan.md.
Fill EVERY section. Luna executes against this spec and does not re-explore — a vague plan wastes
the frontier reasoning it cost. If a section genuinely does not apply, write "n/a" and say why.
-->

## Goal
What "done" means, in one or two sentences. The observable outcome.

## Design decisions (resolved)
Any language-design or trade-off calls settled during planning, with the reasoning. If a decision is
NOT settled, this plan is not ready — hand back to the user, don't guess (AGENTS.md "Ask, don't assume").

## Files to touch
Each with the exact change. Reference `path:line` where possible.
- `crates/<crate>/src/<file>` — <what changes and why>

## Reuse
Existing functions/helpers to call or extend instead of writing new ones (from the function-index skill).

## Steps
Ordered, each independently verifiable.
1. …
2. …

## Tests
- Unit tests to add/adjust (which module).
- `examples/*.doge` (+ `.out`) to add — required for any language feature (AGENTS.md rule 6).

## Docs to update
`docs/` files touched (rule 3) and whether the writing-doge skill needs refreshing.

## Verify
Commands Luna must run and expect green:
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

## Out of scope / hand-back conditions
What NOT to touch, and the conditions under which Luna must stop and return to Sol/the user.
