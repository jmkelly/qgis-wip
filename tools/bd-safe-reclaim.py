#!/usr/bin/env python3
"""Reclaim stale beads leases without stealing work from a live agent.

    python3 tools/bd-safe-reclaim.py                    # recover what is safe
    python3 tools/bd-safe-reclaim.py --dry-run          # report only
    python3 tools/bd-safe-reclaim.py --older-than 90m   # wider grace window
    python3 tools/bd-safe-reclaim.py --heartbeat <id>   # worker: renew my lease
    python3 tools/bd-safe-reclaim.py --db <path>        # another queue entirely

`bd reclaim` is the right primitive and the wrong tool for a swarm. It keys on
lease age alone: a lease goes stale when its holder stops heartbeating, and a
long-running worker does not heartbeat, so "expired" only means "this agent has
not run `bd heartbeat` lately". One coordinator tick (2026-09-28) reclaimed
eight leases that paseo still reported as running, dropping live work back into
`bd ready` where the next tick would double-claim it (SpatialEngine-u2x.30).

So recovery here has two inputs: beads says when a lease *could* be reaped, and
paseo says whether the agent working that bead's branch is *actually* alive. A
bead is reclaimed only when both agree. A bead whose lease is stale but whose
agent is running (or merely idle) is reported and left in place — the safe
outcome is to leave in-flight state alone and let the human look, since the
recoverable case here is a dead worker, not a fast one.

Liveness is matched two ways, both of which the runbook already produces:

  * the bead's worktree, resolved through `paseo workspace ls` to the cwd whose
    directory slug is this bead's, compared against the cwd of every live
    agent. A live agent's cwd is that worktree, so the bead is held. The
    fallback when the workspace is not listed matches the worktree directory
    slug `bd-<id-with-dots-as-dashes>` as the final path segment, allowing the
    numeric suffix paseo appends when the name is already taken
    (`bd-qgis-5gj-1`). Only digits may follow the slug, so
    `bd-spatialengine-u2x-2` never matches a bead id of `SpatialEngine-u2x.21`,
    and `bd-qgis-5gj-extra` never matches `qgis-5gj`.
  * the agent id the coordinator recorded in the bead's notes (`agent <id>`),
    which narrows the same worktree test to the agent that claimed this bead
    rather than to any agent id the notes happen to carry.

Anything else is reported, not guessed: the beads side is the authority on
staleness, and this tool never extends a lease it did not read as expired.

Every `bd` call goes through one place, `bd_args`, so `--db` can point the
whole run — reads and the reclaim alike — at a queue that is not the
repository's shared `.beads` database. That is how a test or a rehearsal runs
this tool without touching in-flight work (SpatialEngine-k0p).
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from datetime import datetime, timedelta, timezone
from pathlib import PurePosixPath

# Mirrors `bd reclaim`'s own default: a lease only counts as reaped once it has
# been expired for at least this long, so a GC pause or clock skew is not a death.
DEFAULT_OLDER_THAN = timedelta(minutes=10)

# A paseo agent that is running or idle is alive and still owns its bead; only
# `closed` (and anything unknown, via the fallback below) counts as gone.
LIVE_STATUSES = ("running", "idle")

# The coordinator records the agent id at claim time so a reclaim stays
# reversible. Matched against paseo's id and shortId, so a short id is fine.
AGENT_ID_IN_NOTES = re.compile(
    r"\bagent\s+(?P<id>[0-9a-fA-F][0-9a-fA-F-]{5,35})\b")


class Completed:
    """The slice of subprocess.CompletedProcess this tool reads."""

    def __init__(self, returncode, stdout, stderr):
        self.returncode = returncode
        self.stdout = stdout
        self.stderr = stderr


class Decision:
    """One bead's recovery verdict, and the reason a human can audit."""

    def __init__(self, bead_id, reclaim, reason):
        self.bead_id = bead_id
        self.reclaim = reclaim
        self.reason = reason


def parse_timestamp(value):
    """Parse a beads RFC3339 timestamp, or None when there is not one."""
    if not value:
        return None
    text = str(value).strip()
    if text.endswith("Z"):
        text = text[:-1] + "+00:00"
    try:
        parsed = datetime.fromisoformat(text)
    except ValueError:
        return None
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc)


def parse_duration(value):
    """Parse `90m` / `2h` / `30s` / a bare number of minutes."""
    text = str(value).strip()
    match = re.fullmatch(r"(?P<amount>\d+(?:\.\d+)?)(?P<unit>[smh])?", text)
    if not match:
        raise ValueError(f"not a duration: {value!r}")
    amount = float(match.group("amount"))
    unit = match.group("unit") or "m"
    seconds = {"s": 1, "m": 60, "h": 3600}[unit]
    return timedelta(seconds=amount * seconds)


def is_stale(bead, now, older_than):
    """Whether `bd reclaim` would consider this lease reapable.

    A bead with no lease was never claimed, and an unexpired one is not stale
    whatever the agent is doing; both are left alone.
    """
    expires = parse_timestamp(bead.get("lease_expires_at"))
    if expires is None:
        return False
    return now - expires >= older_than


def live_agents(agents):
    """The agents that still hold what they claimed."""
    return [a for a in agents
            if str(a.get("status", "")).lower() in LIVE_STATUSES]


def worktree_slug(bead_id):
    """`SpatialEngine-u2x.9.1` -> `bd-spatialengine-u2x-9-1`."""
    slug = re.sub(r"[^0-9a-zA-Z]+", "-", str(bead_id).lower()).strip("-")
    return f"bd-{slug}"


def branch_name(bead_id):
    """The branch the runbook gives a bead's worktree."""
    return f"bd/{bead_id}"


def _path_of(value):
    """Normalise a path or a `~`-prefixed one to comparable absolute-ish form."""
    return os.path.expanduser(str(value or "")).rstrip("/")


def _last_segment(value):
    text = _path_of(value)
    if not text:
        return ""
    return PurePosixPath(text).name


def _segment_matches_slug(segment, slug):
    """Whether a worktree directory name is this bead's, suffix included.

    paseo disambiguates a taken worktree name with a numeric suffix, so the
    live worker for `qgis-5gj` sits in `bd-qgis-5gj-1` and an exact segment
    match misses it. A slug followed by `-<digits>` is the same bead; any
    other suffix (`bd-qgis-5gj-extra`) is a different worktree.
    """
    if not segment or not slug:
        return False
    if segment == slug:
        return True
    prefix = slug + "-"
    return segment.startswith(prefix) and segment[len(prefix):].isdigit()


def branch_worktree(bead_id, workspaces):
    """The worktree `paseo` has for this bead, if it knows one.

    paseo names a worktree workspace with a human title (e.g. "qgis-5gj
    headless driver"), not the branch, so matching `name` against `bd/<id>`
    alone never fires. Match the workspace directory slug instead, which is
    what the worktree is actually named on disk.
    """
    branch = branch_name(bead_id)
    slug = worktree_slug(bead_id)
    for workspace in workspaces:
        if _path_of(workspace.get("name")) == branch:
            return _path_of(workspace.get("cwd"))
    for workspace in workspaces:
        cwd = _path_of(workspace.get("cwd"))
        if _segment_matches_slug(_last_segment(cwd), slug):
            return cwd
    return None


def _agent_covers_cwd(agent, cwd, slug):
    """Whether this agent's worktree is `cwd` (or the bead's slug)."""
    agent_cwd = _path_of(agent.get("cwd"))
    if not agent_cwd:
        return False
    if cwd and agent_cwd == cwd:
        return True
    # Fallback for a worktree paseo no longer lists: compare the final path
    # segment to the slug, allowing the numeric disambiguation suffix. Only
    # digits may follow the slug, so `...-u2x-2` is never read as `...-u2x-21`.
    return _segment_matches_slug(_last_segment(agent_cwd), slug)


def _notes_agent_ids(bead):
    return {match.group("id").lower()
            for match in AGENT_ID_IN_NOTES.finditer(bead.get("notes") or "")}


def _agent_matches_id(agent, wanted):
    return any(wanted == str(agent.get("id", "")).lower()
               or wanted == str(agent.get("shortId", "")).lower()
               for _ in [0])


def protect_reason(bead, agents, workspaces, notes_only=False):
    """Why this bead must not be reclaimed, or None when nothing holds it.

    `notes_only` loosens the second arm to the notes' agent id alone, for the
    reporting-only reader: `parked_lease_holds()` in tools/beads_gate.py prints
    what it finds rather than failing it, so a parent epic whose notes are the
    swarm's coordination log is worth a reader's attention there and nowhere
    else (SpatialEngine-nwo).

    Raises nothing: a bead this cannot vouch for is reported as unprotected so
    a human sees it, rather than silently skipped for ever.
    """
    alive = live_agents(agents)
    if not alive:
        return None
    bead_id = bead.get("id", "")
    cwd = branch_worktree(bead_id, workspaces)
    slug = worktree_slug(bead_id)
    for agent in alive:
        if _agent_covers_cwd(agent, cwd, slug):
            return (f"paseo agent {agent.get('id')} is {agent.get('status')} "
                    f"in {_path_of(agent.get('cwd'))}")
    recorded = _notes_agent_ids(bead)
    for agent in alive:
        if not any(_agent_matches_id(agent, wanted) for wanted in recorded):
            continue
        # The notes name *which* agent claimed this bead; the worktree is what
        # makes it *this* bead. A bare agent id in the notes is not a hold:
        # a parent epic's notes are the swarm's coordination log and name every
        # agent that ever worked one of its children, so matching the id alone
        # fires whenever any named agent is mid-turn anywhere (SpatialEngine-nwo).
        if notes_only or _agent_covers_cwd(agent, cwd, slug):
            return (f"paseo agent {agent.get('id')} is {agent.get('status')} "
                    f"in {_path_of(agent.get('cwd'))}, and this bead's notes "
                    f"name it as the holder")
    return None


def plan(beads, agents, workspaces, now, older_than):
    """Decide, per bead, whether to reap it — the whole reviewable step."""
    decisions = []
    for bead in beads:
        labels = [str(x).lower() for x in (bead.get("labels") or [])]
        if "human" in labels:
            decisions.append(Decision(bead["id"], False,
                                    "labelled human; never reclaimed"))
            continue
        notes = str(bead.get("notes") or "").lower()
        if "coordinator trim" in notes or "trimmed by coordinator" in notes:
            decisions.append(Decision(bead["id"], False,
                                    "stopped by coordinator trim; never reclaimed"))
            continue
        if not is_stale(bead, now, older_than):
            continue
        reason = protect_reason(bead, agents, workspaces)
        if reason:
            decisions.append(
                Decision(bead["id"], False,
                         f"lease stale but {reason}; left in place"))
        else:
            decisions.append(
                Decision(bead["id"], True,
                         "lease stale and no live agent on its branch"))
    return decisions


def _default_run(cmd, cwd=None):
    return subprocess.run(cmd, capture_output=True, text=True, cwd=cwd)


def _json_output(result, what):
    if result.returncode != 0:
        raise RuntimeError(
            f"{what} failed ({result.returncode}): "
            f"{result.stderr.strip() or result.stdout.strip()}")
    return json.loads(result.stdout or "[]")


def _format_duration(delta):
    minutes = int(delta.total_seconds() // 60)
    return f"{minutes}m"


def _bd_args(args, command):
    """The `bd` command line, with the queue override on it when asked for.

    The override is appended rather than inserted so the subcommand stays
    first: `bd` takes `--db` as a global flag, and every call this tool makes
    names the same queue, including the reclaim that writes.
    """
    command = ["bd"] + list(command)
    if args.db:
        command += ["--db", args.db]
    return command


def _reclaim(run, args, ids, verbose):
    """Hand the verified-dead ids to `bd reclaim`, one call."""
    command = ["reclaim"]
    for bead_id in ids:
        command += ["--id", bead_id]
    if verbose:
        command.append("-v")
    return run(_bd_args(args, command))


def main(argv=None, run=None, now=None, older_than=None, out=None):
    run = run or _default_run
    now = now or datetime.now(timezone.utc)
    if older_than is None:
        older_than = DEFAULT_OLDER_THAN
    emit = (lambda line: print(line, file=out or sys.stdout))

    parser = argparse.ArgumentParser(
        prog="bd-safe-reclaim",
        description="Reclaim only the beads leases no live agent holds.")
    parser.add_argument("--dry-run", action="store_true",
                        help="report the verdicts, reap nothing")
    parser.add_argument("--older-than", default=None,
                        help="grace window past lease expiry (default 10m)")
    parser.add_argument("--heartbeat", metavar="ID", default=None,
                        help="renew this bead's lease and exit (a worker turn)")
    parser.add_argument("-v", "--verbose", action="store_true",
                        help="ask bd for per-lease detail")
    parser.add_argument("--db", metavar="PATH", default=None,
                        help="run against this beads database, not the one "
                             "`bd` resolves from the current directory "
                             "(rehearsals and tests)")
    args = parser.parse_args(argv)

    if args.heartbeat:
        return run(_bd_args(args, ["heartbeat", args.heartbeat])).returncode

    if args.older_than is not None:
        try:
            older_than = parse_duration(args.older_than)
        except ValueError as error:
            emit(f"bd-safe-reclaim: {error}")
            return 2

    try:
        beads = _json_output(
            run(_bd_args(args, ["list", "--status", "in_progress",
                                "--json", "-n", "0"])),
            "bd list")
        agents = _json_output(run(["paseo", "ls", "--json"]), "paseo ls")
        workspaces = _json_output(
            run(["paseo", "workspace", "ls", "--json"]),
            "paseo workspace ls")
    except (RuntimeError, json.JSONDecodeError) as error:
        emit(f"bd-safe-reclaim: {error}")
        return 2

    decisions = plan(beads, agents, workspaces, now, older_than)
    if not decisions:
        emit("no stale leases; nothing to recover (KEEP all)")
        return 0

    for decision in decisions:
        verb = "RECLAIM" if decision.reclaim else "KEEP   "
        if decision.reclaim and args.dry_run:
            verb = "WOULD  "
        emit(f"{verb} {decision.bead_id}  {decision.reason}")

    to_reclaim = [d for d in decisions if d.reclaim]
    kept = [d for d in decisions if not d.reclaim]
    if args.dry_run:
        emit(f"dry run: would reclaim {len(to_reclaim)} of {len(decisions)}, "
             f"keeping {len(kept)}")
        return 0
    if not to_reclaim:
        emit(f"reclaimed 0; kept {len(kept)} lease(s) held by live agents")
        return 0

    result = _reclaim(run, args, [d.bead_id for d in to_reclaim], args.verbose)
    if result.returncode != 0:
        emit("bd-safe-reclaim: bd reclaim failed: "
             f"{result.stderr.strip() or result.stdout.strip()}")
        return 1
    emit(f"reclaimed {len(to_reclaim)}, kept {len(kept)} "
         f"lease(s) held by live agents")
    return 0


if __name__ == "__main__":
    sys.exit(main())
