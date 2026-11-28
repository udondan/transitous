"""Gives the changelogs the notes of the release pull request.

Replaces the section of each released version in the CHANGELOG.md of its
crate with the one of the body, so that the changelogs carry the notes as
release-notes-scope.py and release-notes-dependencies.py made them.

    release-notes-changelogs.py < notes.md
"""

import sys
from pathlib import Path

from release_notes import packages, split_body


def replace(path, section):
    """Replaces the section with the same heading as section in path."""
    heading = section[0]
    old = path.read_text().split("\n") if path.exists() else []
    if not heading.startswith("## ") or heading not in old:
        return
    start = old.index(heading)
    end = start + 1
    while end < len(old) and not old[end].startswith("## "):
        end += 1
    gap = [""] if end < len(old) else []
    path.write_text("\n".join([*old[:start], *section, *gap, *old[end:]]))


def main():
    if len(sys.argv) > 1:
        sys.exit(__doc__)
    paths = packages()
    _, sections = split_body(sys.stdin.read())
    for _, component, _, lines, _ in sections:
        section = "\n".join(lines).strip("\n").split("\n")
        replace(Path(paths[component]) / "CHANGELOG.md", section)


if __name__ == "__main__":
    main()
