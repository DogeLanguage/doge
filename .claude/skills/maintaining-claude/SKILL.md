---
name: maintaining-claude
description: Sync CLAUDE.md with the current codebase state. Use after adding or removing crates or folders, or when CLAUDE.md feels stale. Triggered by /maintaining-claude.
tools: Read, Glob, Grep, Edit
---

# Maintaining CLAUDE.md

Keep CLAUDE.md accurate and lean. Every line must earn its place in every future prompt — nothing stale, nothing missing, nothing redundant.

## When to run

- A **crate** was added or removed in `crates/`
- A new **folder** was added or removed inside a crate's `src/`, or in the repo root (e.g. `examples/`, `docs/`)
- A crate or folder's purpose changed significantly
- A roadmap milestone landed (the "Project status" line at the top must reflect reality)
- CLAUDE.md feels out of sync with reality

Adding a file inside an existing folder does **not** require a CLAUDE.md update.

## Workflow

### 1. Read current CLAUDE.md

Read `.claude/CLAUDE.md` in full. Note what the Project Structure section claims exists and what the "Project status" line says.

### 2. Diff against actual structure

Glob only the folder layer — do not list individual files:

```
crates/*/               (doge-cli, doge-compiler, doge-runtime, …)
crates/*/src/*/         (submodule folders inside each crate)
*/                      (repo root: examples/, docs/, …)
```

Identify:
- **New crates/folders** not in the Project Structure section
- **Deleted or renamed** entries still listed
- **Changed purpose** — a crate/folder that was repurposed
- **Stale status line** — milestones completed since the last update

### 3. Update Project Structure

Edit only the `## Project Structure` section and the "Project status" line in `.claude/CLAUDE.md`:
- Add new crates/folders with a one-line role comment (≤8 words)
- Remove deleted/renamed entries
- Update comments if a role changed

**Format rule:** crate and folder paths only — no individual filenames inside them. The comment describes what kind of code lives there, not which specific files.

### 4. Check for stale situational .md files

List `.claude/*.md` files (excluding the permanent CLAUDE.md). For each one referenced in CLAUDE.md:
- Is it still relevant? If not, remove the file and its reference line from CLAUDE.md.

### 5. Report changes

Output a compact summary:
- Crates/folders added to Project Structure
- Crates/folders removed from Project Structure
- Status line updated (old → new)
- Situational .md files added/removed

Do not output a full diff of CLAUDE.md.

## Rules

- **Folder-level only** — never list individual files in the Project Structure section
- **Never add changelogs or task notes** — git tracks history
- **One-line folder comments max** — describe the kind of files, not the specific files
- **Do not restructure CLAUDE.md** — only update the Project Structure section, the status line, and add/remove specific lines; leave all other sections intact
- **Situational .md files** are for temporary context only — delete when no longer relevant
