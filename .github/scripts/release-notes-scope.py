"""Keeps entries with a crate as scope in the notes of that crate only.

Release Please lists a commit in the notes of every crate whose files it
changes and shows the scope only as a prefix, like "* **transitous-core:** add a
fixture". This removes such an entry from the notes of every other crate,
drops the prefix where it stays, and removes headings left without entries.
Entries without a scope, or with a scope that is no crate (like "deps"), stay
as they are.

    release-notes-scope.py body < body.md > filtered.md
        The body of the release pull request, with one <details> section per
        crate.
    release-notes-scope.py changelog <component> <CHANGELOG.md>
        The changelog of one crate, rewritten in place.

The crates are the components of release-please-config.json.
"""

import json
import re
import sys
from pathlib import Path

CONFIG = Path(__file__).resolve().parents[2] / "release-please-config.json"
ENTRY = re.compile(r"^\* \*\*([A-Za-z0-9_.-]+):\*\* (.*)$")
SECTION = re.compile(r"(<details><summary>([A-Za-z0-9_.-]+): [^<]*</summary>)")


def components():
    packages = json.loads(CONFIG.read_text())["packages"].values()
    return {p["component"] for p in packages}


def filter_entries(lines, component, crates):
    """Drops or unprefixes the scoped entries of one crate's notes."""
    out = []
    dropping = False
    for line in lines:
        if dropping and line.startswith((" ", "\t")) and line.strip():
            continue  # continuation of a dropped entry
        dropping = False
        match = ENTRY.match(line)
        if match and match.group(1) in crates:
            if match.group(1) != component:
                dropping = True
                continue
            line = f"* {match.group(2)}"
        out.append(line)
    return drop_empty_headings(out)


def drop_empty_headings(lines):
    """Removes "### Heading" lines whose entries were all dropped."""
    out = []
    i = 0
    while i < len(lines):
        if lines[i].startswith("### "):
            j = i + 1
            while j < len(lines) and not lines[j].strip():
                j += 1
            end = j >= len(lines) or lines[j].startswith(("#", "</details>"))
            if end:
                i = j
                continue
        out.append(lines[i])
        i += 1
    return out


def filter_body(text, crates):
    parts = SECTION.split(text)
    out = [parts[0]]
    for i in range(1, len(parts), 3):
        summary, component, rest = parts[i : i + 3]
        lines = filter_entries(rest.split("\n"), component, crates)
        out.append(summary + "\n".join(lines))
    return "".join(out)


def main():
    crates = components()
    if sys.argv[1:2] == ["body"]:
        sys.stdout.write(filter_body(sys.stdin.read(), crates))
    elif sys.argv[1:2] == ["changelog"] and len(sys.argv) == 4:
        path = Path(sys.argv[3])
        lines = filter_entries(path.read_text().split("\n"), sys.argv[2], crates)
        path.write_text("\n".join(lines))
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
