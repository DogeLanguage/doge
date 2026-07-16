# Codex setup for Doge

This repo is configured for [Codex](https://developers.openai.com/codex/) with a two-model flow:
**Sol plans, Luna executes.** All shared project rules live in the root [`AGENTS.md`](../AGENTS.md);
this file is just the runbook.

## What's here

| File | Role |
| --- | --- |
| `../AGENTS.md` | Single source of project instructions. Codex loads it every session (root + any `AGENTS.md` from cwd up to root); nested `crates/*/AGENTS.md` load when you work in that crate. |
| `config.toml` | Project-local defaults and sandbox policy. |
| `user-config.template.toml` | The `planner` (Sol) and `executor` (Luna) profiles to merge into the user-level Codex config. |
| `PLAN.template.md` | The shape Sol fills in when producing a plan. |
| `plan.md` | The handoff artifact Sol writes and Luna reads. Gitignored — it's per-task scratch. |
| `../.agents/skills/` | Project skills Codex auto-triggers: `function-index`, `writing-doge`, `maintaining-agents`. |

## One-time setup

1. **Trust the project** so `.codex/config.toml` and the project skills load. On your first `codex` run in
   this directory, choose to trust it when prompted — or add to `~/.codex/config.toml`:

   ```toml
   [projects."/home/szasadny/Documents/Codings/Doge/doge"]
   trust_level = "trusted"
   ```

2. **Install the profiles** — copy the two `[profiles.*]` tables from
   `.codex/user-config.template.toml` into `~/.codex/config.toml`. Codex deliberately ignores
   profiles in project-local configuration.

3. **Confirm the setup:** `codex -p planner --version` and `codex -p executor --version` run without a
   config error. `codex doctor` should show no unsupported project-local configuration warning.

## The loop

```bash
# 1. Sol plans. High-reasoning frontier model. Writes .codex/plan.md, no source edits.
codex -p planner "add a `bytes.hex()` method mirroring the existing b64() round-trip"

# 2. Review .codex/plan.md yourself. It's the contract — fix it here, not mid-execution.

# 3. Luna executes the plan. High-reasoning executor, runs the verify commands.
codex -p executor "execute .codex/plan.md"
```

- Plain `codex "<task>"` starts with Sol (`gpt-5.6-sol`, high) as the planner and orchestrator.
  Use the `executor` profile for Luna (`gpt-5.6-luna`, high) when delegating implementation work.
- Non-interactive / scripted: `codex exec -p executor "execute .codex/plan.md"`.
- Reviews: `codex review` (or the `/codex:review` Claude Code plugin command).

## Why the split

Sol's frontier reasoning is spent on planning and orchestration. Luna executes against a precise spec at
high reasoning effort and never re-explores. A vague plan erases the benefit, so `PLAN.template.md`
forces every section (files, reuse, tests, verify, hand-back conditions). If Luna finds the plan wrong
or incomplete, it stops and hands back rather than improvising.

## Relationship to Claude Code

`AGENTS.md` is the single source of truth. `.claude/CLAUDE.md` imports it, so Claude Code and Codex read
the same rules — edit `AGENTS.md`, never fork the two. Claude Code's skills stay in `.claude/skills/`;
Codex's mirror them in `.agents/skills/`. When a skill changes, update both copies (they're small).
