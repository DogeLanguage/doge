---
name: function-index
description: >
  Scan the Doge codebase for all Rust function, struct, and enum definitions, build a compact
  index of name → file:line, then use it to (1) identify which files to read or change for the
  current task and (2) spot existing functions that could be reused or extended instead of
  reimplemented. When duplication risk is found, propose a shared helper before writing new
  code. Use this skill at the start of every new feature request, any time you are about to create
  a new runtime builtin, parser rule, or codegen arm, or when asked to "add", "implement",
  "build", "create", "extend", or "refactor" anything in the codebase. Also trigger when you
  notice yourself reaching for a new helper that might already exist.
---

# Function Index Skill

The most common source of duplication is writing a new function without knowing a similar one
already exists. This skill builds a full index once, then queries it — reason about reuse before a
single line of new code is written.

## Step 1 — Build the index (once per session)

```
grep -rn --include='*.rs' -E '^\s*(pub(\([a-z]+\))? )?(fn|struct|enum|trait|impl) ' crates/ > /tmp/doge-function-index.txt
```

Writes one line per definition (`path:line: signature`) across all crates. **Never cat or dump the
whole file into context** — query it with grep.

## Step 2 — Query the index against the task

Grep the index file case-insensitively for 2–4 keywords from the task, including doge keyword
names and their internal synonyms (e.g. `bark|print`, `pls|try|catch`, `bork|break`,
`such|declare|let`, `much|param|repeat`). Both name hits and path hits are signal. Iterate
keywords until you can name the 3–6 most relevant files to read — no more. This replaces a
blind crate scan.

## Step 3 — Classify reuse before reading any file

- **Direct reuse** — an existing function already does it: import and call it.
- **Near-reuse** — it does 70–90%: add an optional parameter with a default so existing callers
  are unaffected, instead of writing a filtered/variant copy.
- **Duplication risk** — the new function would share a non-trivial body with an existing one:
  extract a shared helper instead of copying.
  1. Identify the parameters that differ between the two use cases.
  2. Put the helper in the lowest shared location — same module if both callers live there;
     otherwise the lowest crate both can reach (`doge-runtime` for value behaviour,
     `doge-compiler` for compilation helpers).
  3. Refactor the **existing** function to delegate to the helper (same external signature, no
     callers broken) and verify its tests still pass before continuing.
  4. Implement the new function on the helper.

Be specific in the response: "`value_add` (crates/doge-runtime/src/ops.rs:45) already handles X;
the new function only differs in Y — parameterize Y instead of copying."

## Step 4 — Confirm plan with the user (only when refactoring existing code)

If Step 3 requires changing an **existing** function's signature or extracting it into a shared
helper, state the plan (what gets extracted, which files and tests it touches) and confirm before
touching anything. This is the only gate — once confirmed, execute without further pauses.

## What this skill does NOT do

- It does not read full file bodies — that happens after Step 2 names the relevant files.
- It does not refactor unrelated code found in the index — flag observed duplication, scope the
  change to the current task.
