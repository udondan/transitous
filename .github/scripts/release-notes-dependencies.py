"""Adds the workspace dependencies released in the same pull request.

A crate whose workspace dependency ships with it and is released in the same
pull request gets an item for it under "Dependencies", with the version and
the entries of the dependency, like

    * transitous-core 0.1.1
      * count cancellations as realtime data (#12) (03aa2af)

Release Please releases the crate but leaves its notes empty, because the
crates inherit the dependencies from [workspace.dependencies] without a
version of their own. Development dependencies do not matter to the users of
a crate and are left out.
The items go under an existing "Dependencies" heading or a new one, and a
second run gives the same result.

    release-notes-dependencies.py < scoped.md > notes.md

Reads the body of the release pull request after release-notes-scope.py.
"""

import re
import sys

from release_notes import (
    drop_entries,
    is_continuation,
    join_body,
    packages,
    split_body,
    workspace_dependencies,
)

HEADING = "### Dependencies"
# Earlier runs also listed development dependencies, with this suffix.
DEV = " (development dependency)"


def entries(lines):
    """The entries of one crate's notes, each with its continuation lines."""
    return [line for line in lines if line.startswith("* ") or is_continuation(line)]


def items(deps, versions, notes):
    """The items for the dependencies released in the same pull request."""
    out = []
    for dep in sorted(d for d in deps if d in versions and deps[d] == "runtime"):
        out.append(f"* {dep} {versions[dep]}")
        out.extend(f"  {line}" for line in entries(notes[dep]))
    return out


def add(lines, new):
    """Appends items to the "Dependencies" heading, which it adds if missing.

    Also ends the notes with a single line break, which dropped items of an
    earlier run could leave doubled.
    """
    while lines and not lines[-1].strip():
        lines = lines[:-1]
    if not new:
        return [*lines, ""]
    if HEADING not in lines:
        return [*lines, "", "", HEADING, "", *new, ""]
    end = lines.index(HEADING) + 1
    while end < len(lines) and not lines[end].startswith("#"):
        end += 1
    while not lines[end - 1].strip():
        end -= 1
    return [*lines[:end], *new, *lines[end:], ""]


def main():
    if len(sys.argv) > 1:
        sys.exit(__doc__)
    paths = packages()
    names = "|".join(re.escape(c) for c in sorted(paths))
    generated = re.compile(rf"^\* ({names}) \d+\.\d+\.\d+\S*({re.escape(DEV)})?$")
    head, sections = split_body(sys.stdin.read())
    # Without the items of an earlier run, so that a rerun gives the same.
    notes = {s[1]: drop_entries(s[3], generated.match) for s in sections}
    versions = {s[1]: s[2] for s in sections}
    out = []
    for summary, component, version, _, rest in sections:
        lines = notes[component]
        new = items(workspace_dependencies(paths[component], set(paths)), versions, notes)
        out.append((summary, component, version, add(lines, new), rest))
    sys.stdout.write(join_body(head, out))


if __name__ == "__main__":
    main()
