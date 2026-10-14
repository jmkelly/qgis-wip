---
name: qgis-worker-start
description: Bootstrap and editing conventions for worker agents in the QGIS Rust workspace. Use at the start of every bead or worker session to initialize context, select crate-scoped verification, and follow safe file-editing and git worktree practices.
agent: opencode
---

# qgis-worker-start — Worker Bootstrap & Editing Conventions

**Source of truth for worker prompt:** [`eng/runbook.md`](../../../eng/runbook.md) § "Worker prompt (verbatim template)"

This skill covers the editing conventions and worktree practices that are
unique to worker agents. For the full worker prompt template (bootstrap,
gates, handoff), see `eng/runbook.md`.

## When to use

Load this skill at the start of any worker task (bead) in the QGIS Rust
workspace before touching source files or running builds.

## 1. Bootstrap

The full worker prompt template in `eng/runbook.md` covers the bootstrap
sequence (`bd show`, `bd prime`, read `AGENTS.md`). Follow that prompt.

## 2. Editing Conventions (Prefer the edit tool for Rust)

Use the dedicated file tools instead of shell text-manipulation tricks:

- **Prefer the edit tool for Rust source files (`.rs`)** and existing markdown/config files. Use `write` only when creating new files or intentionally replacing an entire file.
- **Never use `cat >>` appends, `sed -i`, or `python3 - <<'EOF'` heredocs to patch Rust source code.** Shell appends and ad-hoc script replacements frequently corrupt brace matching, duplicate items, or mangle indentation.
- **Reserve Python heredocs (`python3 -c` / `python3 - <<'EOF'`) strictly for**:
  - Structured `Cargo.toml` updates across multiple workspace members
  - Mechanical bulk renames across many files where single-file `edit` calls would be excessive
- Always read the target section of a `.rs` file with the `read` tool (or locate symbols with `grep`) immediately before applying `edit`.

## 3. Keep CWD Stable in Worktrees (`git -C <worktree>`)

Worker sessions already start inside their assigned worktree directory (e.g., `/home/james/.paseo/worktrees/.../bd-<id>`). Churning on `cd ~/.paseo/worktrees/...` prefixes in every shell command wastes tokens and risks path mismatches.

- **Keep the session CWD stable**: Rely on the tool harness `workdir` (which defaults to your worktree root) and relative paths for `read`, `edit`, `write`, `./verify`, and `bd` commands.
- **Use `git -C <worktree>` when running git commands against an explicit worktree path**:

  ```bash
  git -C <worktree> status
  git -C <worktree> diff
  git -C <worktree> log -n 5 --oneline
  ```

- Avoid chaining `cd /path/to/worktree && ...` in shell commands.

## 4. Verification

Drive verification through `./verify` — see the [`verify-qgis` skill](../verify-qgis/SKILL.md)
for the full command reference and feature maps.

## 5. Handoff

The full handoff procedure is in the worker prompt template in `eng/runbook.md`.
