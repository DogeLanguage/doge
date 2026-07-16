---
name: maintaining-agents
description: Sync the AGENTS.md files (root + per-crate) with the current codebase state. Use after adding or removing a crate or folder, after a crate/folder's purpose changes, or when the crate map feels stale. AGENTS.md is the single source of project instructions (.claude/CLAUDE.md imports it), so keeping it accurate keeps both Codex and Claude Code correct.
---

# Maintaining AGENTS.md

Keep the AGENTS.md files accurate and lean. Every line is loaded into future prompts — nothing stale,
nothing missing, nothing redundant. The root `AGENTS.md` is loaded every session and is byte-capped, so
push crate-internal detail down into the crate's own `AGENTS.md`, not the root.

## When to run

- A **crate** was added or removed in `crates/`.
- A **folder** was added/removed inside a crate's `src/`, or in the repo root (`examples/`, `docs/`, …).
- A crate or folder's purpose changed significantly.
- The crate map or a per-crate layout feels out of sync with reality.

Adding a file inside an existing folder does **not** require an update.

## Workflow

1. **Read** the root `AGENTS.md` (the "Crate map" table) and the relevant `crates/*/AGENTS.md`.
2. **Diff against reality** — glob only the folder layer, never individual files:

   ```
   crates/*/            (which crates exist)
   crates/*/src/*/      (submodule folders inside each crate)
   */                   (repo root: examples/, docs/, …)
   ```

   Identify: new crates/folders not documented; deleted/renamed entries still listed; changed purpose.

3. **Update the right file:**
   - A new/removed **crate**, or a repo-root folder → the root `AGENTS.md` "Crate map" table (one line,
     ≤ ~12 words per entry).
   - A changed **submodule layout inside a crate** → that crate's `crates/<crate>/AGENTS.md` "Layout"
     section. Keep the root's one-liner unchanged unless the crate's *purpose* changed.
   - New crate added → also create `crates/<crate>/AGENTS.md` following the shape of an existing one
     (Layout + crate-specific rules).

4. **Report** a compact summary: crates/folders added/removed, purposes changed, files touched. Do not
   dump a full diff.

## Rules

- **Folder-level only** in the root crate map — never list individual filenames there.
- **Never add changelogs or task notes** — git tracks history.
- **Don't restructure** — update the specific lines that drifted; leave the rest intact.
- If you touch a shared rule (not just the structure), remember `.claude/CLAUDE.md` imports the root
  `AGENTS.md`, so the change reaches Claude Code automatically — do not duplicate it into CLAUDE.md.
