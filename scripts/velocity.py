#!/usr/bin/env python3
"""Compute velocity metrics from the beads database.

Queries the Dolt database via `bd list --json` and computes:
- Per-epic cycle time (created to closed) in hours
- Closures per session (grouped by UTC date)
- Cycle time vs touch time (estimated) separation

Usage:
    python3 scripts/velocity.py              # Full report
    python3 scripts/velocity.py --epic <id>  # Single epic detail
"""

import json
import subprocess
import sys
from collections import defaultdict
from datetime import datetime, timezone


def fetch_issues():
    """Fetch all issues from the beads database via bd CLI."""
    result = subprocess.run(
        ["bd", "list", "--all", "--json"],
        capture_output=True,
        text=True,
        cwd="/home/james/Projects/qgis",
    )
    if result.returncode != 0:
        print(f"Error running bd list: {result.stderr}", file=sys.stderr)
        sys.exit(1)
    return json.loads(result.stdout)


def parse_timestamp(ts):
    """Parse ISO timestamp to datetime."""
    if not ts:
        return None
    for fmt in ("%Y-%m-%dT%H:%M:%SZ", "%Y-%m-%dT%H:%M:%S.%fZ"):
        try:
            return datetime.strptime(ts, fmt).replace(tzinfo=timezone.utc)
        except ValueError:
            continue
    return None


def compute_cycle_time(issue):
    """Compute cycle time in hours from created to closed."""
    created = parse_timestamp(issue.get("created_at"))
    updated = parse_timestamp(issue.get("updated_at"))
    status = issue.get("status", "")

    if not created:
        return None

    if status == "closed" and updated:
        return (updated - created).total_seconds() / 3600
    return None


def compute_touch_time(issue):
    """Estimate touch time from estimate or default."""
    est = issue.get("estimated_minutes")
    if est:
        return est / 60
    return 2.0


def get_session_date(issue):
    """Get the UTC date string for session grouping."""
    updated = parse_timestamp(issue.get("updated_at"))
    if updated:
        return updated.strftime("%Y-%m-%d")
    created = parse_timestamp(issue.get("created_at"))
    if created:
        return created.strftime("%Y-%m-%d")
    return "unknown"


def main():
    issues = fetch_issues()

    closed = [i for i in issues if i.get("status") == "closed" and i.get("created_at")]

    epic_metrics = defaultdict(lambda: {"total_cycle_hours": 0, "count": 0, "touch_hours": 0})
    session_closures = defaultdict(int)

    for issue in closed:
        cycle = compute_cycle_time(issue)
        if cycle is not None:
            epic = issue.get("parent", "no-parent")
            epic_metrics[epic]["total_cycle_hours"] += cycle
            epic_metrics[epic]["count"] += 1

        touch = compute_touch_time(issue)
        epic = issue.get("parent", "no-parent")
        epic_metrics[epic]["touch_hours"] += touch

        session = get_session_date(issue)
        session_closures[session] += 1

    print("=" * 70)
    print("VELOCITY REPORT")
    print("=" * 70)
    print()

    print("PER-EPIC CYCLE TIME (created to closed)")
    print("-" * 70)
    print(f"{'Epic':<25} {'Tasks':>8} {'Cycle(h)':>10} {'Avg(h)':>8} {'Touch(h)':>10}")
    print("-" * 70)

    for epic in sorted(epic_metrics.keys()):
        m = epic_metrics[epic]
        avg = m["total_cycle_hours"] / m["count"] if m["count"] > 0 else 0
        print(f"{epic:<25} {m['count']:>8} {m['total_cycle_hours']:>10.1f} {avg:>8.1f} {m['touch_hours']:>10.1f}")

    print()

    print("CLOSURES PER SESSION (by UTC date)")
    print("-" * 70)
    for session in sorted(session_closures.keys()):
        print(f"  {session}: {session_closures[session]} closures")

    print()

    total_closed = len(closed)
    total_cycle = sum(m["total_cycle_hours"] for m in epic_metrics.values())
    total_touch = sum(m["touch_hours"] for m in epic_metrics.values())

    print("SUMMARY")
    print("-" * 70)
    print(f"  Total closed tasks: {total_closed}")
    print(f"  Total cycle time:   {total_cycle:.1f} hours")
    print(f"  Total touch time:   {total_touch:.1f} hours (estimated)")
    if total_closed > 0:
        print(f"  Avg cycle time:     {total_cycle / total_closed:.1f} hours/task")
    print()
    print("=" * 70)


if __name__ == "__main__":
    main()
