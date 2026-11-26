#!/usr/bin/env python3
"""Checks that every changed line is covered, like the patch status of Codecov.

Usage: patch-coverage.py LCOV [BASE]

Compares the working tree with the merge base of BASE (default
`origin/main`) and reports every added or changed line under `crates/` that
the LCOV report counts as executable but never ran. Lines the report does not
list, like comments and declarations, count as covered, as on Codecov, and so
do the files that `codecov.yml` ignores.
"""

import re
import subprocess
import sys
from pathlib import Path

# The same files as `ignore` in codecov.yml.
IGNORED: set[str] = set()


def git(*args):
    return subprocess.run(
        ["git", *args], check=True, capture_output=True, text=True
    ).stdout


def changed_lines(base):
    """The added and changed lines of the Rust files, by file."""
    merge_base = git("merge-base", base, "HEAD").strip()
    diff = git("diff", "--unified=0", "--no-color", merge_base, "--", "crates/*.rs")
    lines, path = {}, None
    for line in diff.splitlines():
        if line.startswith("+++ "):
            path = None if line == "+++ /dev/null" else line[len("+++ b/") :]
        elif line.startswith("@@") and path and path not in IGNORED:
            match = re.match(r"@@ -\S+ \+(\d+)(?:,(\d+))? @@", line)
            start, count = int(match[1]), int(match[2] or 1)
            lines.setdefault(path, set()).update(range(start, start + count))
    return lines


def missed_lines(lcov, root):
    """The executable lines that never ran, by file relative to `root`."""
    missed, path = {}, None
    for line in Path(lcov).read_text().splitlines():
        if line.startswith("SF:"):
            source = Path(line[3:])
            path = source.relative_to(root).as_posix() if source.is_relative_to(root) else None
        elif line.startswith("DA:") and path:
            number, hits = line[3:].split(",")[:2]
            if int(hits) == 0:
                missed.setdefault(path, set()).add(int(number))
    return missed


def main():
    if len(sys.argv) not in (2, 3):
        sys.exit(__doc__.strip())
    lcov = sys.argv[1]
    base = sys.argv[2] if len(sys.argv) == 3 else "origin/main"
    root = Path(git("rev-parse", "--show-toplevel").strip()).resolve()
    changed = changed_lines(base)
    missed = missed_lines(lcov, root)
    uncovered = 0
    for path in sorted(changed):
        for number in sorted(changed[path] & missed.get(path, set())):
            print(f"{path}:{number}: not covered by any test", file=sys.stderr)
            uncovered += 1
    if uncovered:
        sys.exit(f"{uncovered} changed line(s) are not covered, Codecov needs 100%")
    total = sum(len(lines) for lines in changed.values())
    print(f"all changed lines are covered ({total} changed lines checked)")


if __name__ == "__main__":
    main()
