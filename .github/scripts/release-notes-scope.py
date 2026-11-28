"""Keeps entries with a crate as scope in the notes of that crate only.

Release Please lists a commit in the notes of every crate whose files it
changes and shows the scope only as a prefix, like "* **transitous-core:** add a
fixture". This removes such an entry from the notes of every other crate,
drops the prefix where it stays, and removes headings left without entries.
Entries without a scope, or with a scope that is no crate (like "deps"), stay
as they are.

    release-notes-scope.py < body.md > scoped.md

Reads the body of the release pull request, with one <details> section per
crate.
"""

import re
import sys

from release_notes import drop_entries, join_body, packages, split_body

ENTRY = re.compile(r"^\* \*\*([A-Za-z0-9_.-]+):\*\* (.*)$")


def scope(lines, component, crates):
    """Drops or unprefixes the scoped entries of one crate's notes."""

    def other_crate(line):
        match = ENTRY.match(line)
        return bool(match) and match.group(1) in crates and match.group(1) != component

    def unprefix(line):
        match = ENTRY.match(line)
        return f"* {match.group(2)}" if match and match.group(1) == component else line

    return [unprefix(line) for line in drop_entries(lines, other_crate)]


def main():
    if len(sys.argv) > 1:
        sys.exit(__doc__)
    crates = set(packages())
    head, sections = split_body(sys.stdin.read())
    sections = [
        (summary, component, version, scope(lines, component, crates), rest)
        for summary, component, version, lines, rest in sections
    ]
    sys.stdout.write(join_body(head, sections))


if __name__ == "__main__":
    main()
