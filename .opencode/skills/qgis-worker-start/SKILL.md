---
name: qgis-worker-start
description: Bootstrap and editing conventions for worker agents in the QGIS Rust workspace. Use at the start of every bead or worker session to initialize context, select crate-scoped verification, and follow safe file-editing and git worktree practices.
agent: opencode
---

# qgis-worker-start — Worker Bootstrap & Editing Conventions

This skill codifies the standard bootstrap sequence, crate-scoped verification workflow, file-editing rules, and worktree stability practices for worker agents in the QGIS Rust workspace.

## When to use

Load or follow this skill at the start of any worker task (bead) in the QGIS Rust workspace before touching source files or running builds.

## 1. Bootstrap Sequence (`bd show` + `bd prime` + read `AGENTS.md`)

Start every session with a single parallel batch to gather full task and repository context:

1. **Inspect the bead**: Run `bd show <id>` to read the task description, notes, branch/worktree metadata, and exact acceptance criteria.
2. **Load Beads workflow context**: Run `bd prime` if Beads workflow context is missing or stale.
3. **Read project instructions**: Read `AGENTS.md` in the workspace root for project conventions, non-interactive shell flags, verification rules, and session completion policies.

```bash
bd show <id>
bd prime
```

Do not guess requirements from the title alone—always check the `ACCEPTANCE CRITERIA` and `NOTES` fields in `bd show <id>`.

## 2. Confirm the Affected Crate & Scoped Verification

Before editing code, identify which workspace crate(s) your bead targets (e.g., `geometry`, `crs`, `expression`, `providers`, `analysis`, `raster`, `mesh`, `pointcloud`, `core`, `style`, `spatial-index`, `server`, `gui`, `pyo3`, `oracle`, `fixtures`, `qgis-bench`).

- Check the crate's feature map in `.opencode/skills/verify-qgis/features/` if one exists.
- Drive verification through `./verify` scoped to that crate rather than running full-workspace builds during iteration or chaining raw `cargo` commands:

```bash
./verify test --crate <crate>      # run tests for <crate> and its dependents
./verify clippy --crate <crate>    # run clippy -D warnings scoped to <crate>
```

Only escalate to `./verify all` or `./verify oracle` when your change crosses crate boundaries, updates golden fixtures, or when required by final acceptance gates.

## 3. Editing Conventions (Prefer the edit tool for Rust)

Use the dedicated file tools instead of shell text-manipulation tricks:

- **Prefer the edit tool for Rust source files (`.rs`)** and existing markdown/config files. Use `write` only when creating new files or intentionally replacing an entire file.
- **Never use `cat >>` appends, `sed -i`, or `python3 - <<'EOF'` heredocs to patch Rust source code.** Shell appends and ad-hoc script replacements frequently corrupt brace matching, duplicate items, or mangle indentation.
- **Reserve Python heredocs (`python3 -c` / `python3 - <<'EOF'`) strictly for**:
  - Structured `Cargo.toml` updates across multiple workspace members
  - Mechanical bulk renames across many files where single-file `edit` calls would be excessive
- Always read the target section of a `.rs` file with the `read` tool (or locate symbols with `grep`) immediately before applying `edit`.

## 4. Keep CWD Stable in Worktrees (`git -C <worktree>`)

Worker sessions already start inside their assigned worktree directory (e.g., `/home/james/.paseo/worktrees/.../bd-<id>`). Churning on `cd ~/.paseo/worktrees/...` prefixes in every shell command wastes tokens and risks path mismatches.

- **Keep the session CWD stable**: Rely on the tool harness `workdir` (which defaults to your worktree root) and relative paths for `read`, `edit`, `write`, `./verify`, and `bd` commands.
- **Use `git -C <worktree>` when running git commands against an explicit worktree path**:

```bash
git -C <worktree> status
git -C <worktree> diff
git -C <worktree> log -n 5 --oneline
```

- Avoid chaining `cd /path/to/worktree && ...` in shell commands.

## 5. Handoff & Completion Checklist

1. Run the exact acceptance gate command from `bd show <id>`.
2. Run `./verify test --crate <crate>` and `./verify clippy --crate <crate>` (when Rust code changed).
3. Follow the prompt's git and bead instructions (typically do **not** commit to `master` or push; mark the bead with `bd update <id> --add-label needs-merge --append-notes "Done. Branch: bd/<id>"`).
