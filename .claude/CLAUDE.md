# Doge — project instructions

The project instructions live in the repo-root **[`AGENTS.md`](../AGENTS.md)** — the single source of
truth for both Claude Code and Codex. It is imported below, so everything in it applies here verbatim;
do not duplicate rules into this file. Edit `AGENTS.md`, never fork the two.

@../AGENTS.md

## Claude Code specifics

- Skills live in `.claude/skills/` (`function-index`, `writing-doge`, `maintaining-claude`,
  `skill-creator`) and are invoked with the Skill tool or `/<name>`. Codex mirrors the first three under
  `.agents/skills/`; when you change one, update both copies (they're small).
- `maintaining-claude` keeps this repo's structure docs in sync from the Claude side; the Codex mirror is
  `maintaining-agents`. Both edit the same `AGENTS.md` — the single source.
- The Sol/Luna planner→executor workflow in `AGENTS.md` is a Codex flow. In Claude Code, do the planning
  and execution in-thread as usual; the `.codex/plan.md` artifact is optional here.
