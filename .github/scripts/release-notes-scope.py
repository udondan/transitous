"""Scopes the release notes to their crate and adds released dependencies.

Release Please lists a commit in the notes of every crate whose files it
changes and shows the scope only as a prefix, like "* **transitous-core:** add a
fixture". This removes such an entry from the notes of every other crate,
drops the prefix where it stays, and removes headings left without entries.
Entries without a scope, or with a scope that is no crate (like "deps"), stay
as they are.

A crate whose workspace dependency is released in the same pull request gets
an item for it under "Dependencies": the version, and for a dependency that
ships with the crate its entries, like

    * transitous-core 0.1.1
      * count cancellations as realtime data (#12) (03aa2af)

A development dependency gets the version only. Release Please releases the
crate in both cases but leaves its notes empty, because the crates inherit
the dependencies from [workspace.dependencies] without a version of their own.

    release-notes-scope.py body < body.md > processed.md
        The body of the release pull request, with one <details> section per
        crate.
    release-notes-scope.py changelogs < processed.md
        Replaces the section of each released version in the CHANGELOG.md of
        its crate with the one of the processed body.

The crates are the components of release-please-config.json in the current
directory, and their dependencies come from their Cargo.toml.
"""

import json
import re
import sys
import tomllib
from pathlib import Path

ENTRY = re.compile(r"^\* \*\*([A-Za-z0-9_.-]+):\*\* (.*)$")
SECTION = re.compile(r"(<details><summary>([A-Za-z0-9_.-]+): ([^<]*)</summary>)")
DEPENDENCIES = "### Dependencies"
DEV = " (development dependency)"


def packages():
    """Maps each component to the path of its crate."""
    config = json.loads(Path("release-please-config.json").read_text())
    return {p["component"]: path for path, p in config["packages"].items()}


def workspace_dependencies(path, crates):
    """Maps each workspace crate the crate at path depends on to its kind.

    The kind is "dev" for a dependency of the tests only and "runtime" for
    one that ships with the crate, which wins where a crate is both.
    """
    manifest = tomllib.loads((Path(path) / "Cargo.toml").read_text())
    tables = [manifest, *manifest.get("target", {}).values()]
    kinds = {}
    for table in tables:
        for key in ("dependencies", "build-dependencies", "dev-dependencies"):
            for name, spec in table.get(key, {}).items():
                if isinstance(spec, dict):
                    name = spec.get("package", name)
                if name not in crates:
                    continue
                kind = "dev" if key == "dev-dependencies" else "runtime"
                if kinds.get(name) != "runtime":
                    kinds[name] = kind
    return kinds


def filter_entries(lines, component, crates):
    """Drops or unprefixes the scoped entries of one crate's notes."""
    out = []
    dropping = False
    for line in lines:
        if dropping and is_continuation(line):
            continue
        dropping = False
        match = ENTRY.match(line)
        if match and match.group(1) in crates:
            if match.group(1) != component:
                dropping = True
                continue
            line = f"* {match.group(2)}"
        out.append(line)
    return drop_empty_headings(out)


def is_continuation(line):
    return line.startswith((" ", "\t")) and bool(line.strip())


def drop_generated(lines, crates):
    """Removes the items of an earlier run, so that a rerun gives the same."""
    names = "|".join(re.escape(c) for c in sorted(crates))
    generated = re.compile(rf"^\* ({names}) \d+\.\d+\.\d+\S*({re.escape(DEV)})?$")
    out = []
    dropping = False
    for line in lines:
        if dropping and is_continuation(line):
            continue
        dropping = bool(generated.match(line))
        if not dropping:
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


def entries(lines):
    """The entries of one crate's notes, each with its continuation lines."""
    return [
        line
        for line in lines
        if line.startswith("* ") or (is_continuation(line) and not line.startswith("#"))
    ]


def add_dependencies(lines, items):
    """Appends items to the "Dependencies" heading, which it adds if missing."""
    while lines and not lines[-1].strip():
        lines.pop()
    if DEPENDENCIES not in lines:
        return [*lines, "", "", DEPENDENCIES, "", *items, ""]
    start = lines.index(DEPENDENCIES) + 1
    end = start
    while end < len(lines) and not lines[end].startswith("#"):
        end += 1
    while end > start and not lines[end - 1].strip():
        end -= 1
    return [*lines[:end], *items, *lines[end:], ""]


def split_body(text):
    """Splits the body into its text before the sections and its sections.

    Each section is (summary, component, version, notes, rest), where notes
    are the lines up to "</details>" and rest is the text from there on.
    """
    parts = SECTION.split(text)
    sections = []
    for i in range(1, len(parts), 4):
        summary, component, version, body = parts[i : i + 4]
        notes, sep, rest = body.partition("</details>")
        sections.append((summary, component, version, notes.split("\n"), sep + rest))
    return parts[0], sections


def process_body(text, paths):
    crates = set(paths)
    head, sections = split_body(text)
    notes = {}
    for _, component, _, lines, _ in sections:
        lines = filter_entries(lines, component, crates)
        notes[component] = drop_generated(lines, crates)
    versions = {component: version for _, component, version, _, _ in sections}
    out = [head]
    for summary, component, _, _, rest in sections:
        lines = notes[component]
        items = []
        deps = workspace_dependencies(paths[component], crates)
        for dep in sorted((d for d in deps if d in versions), key=lambda d: (deps[d] == "dev", d)):
            if deps[dep] == "dev":
                items.append(f"* {dep} {versions[dep]}{DEV}")
                continue
            items.append(f"* {dep} {versions[dep]}")
            items.extend(f"  {line}" for line in entries(notes[dep]))
        if items:
            lines = add_dependencies(lines, items)
        out.append(summary + "\n".join(lines) + rest)
    return "".join(out)


def update_changelogs(text, paths):
    """Replaces the released sections of the changelogs with the body's."""
    _, sections = split_body(text)
    for _, component, _, lines, _ in sections:
        path = Path(paths[component]) / "CHANGELOG.md"
        section = "\n".join(lines).strip("\n")
        heading = section.split("\n", 1)[0]
        if not path.exists() or not heading.startswith("## "):
            continue
        old = path.read_text().split("\n")
        if heading not in old:
            continue
        start = old.index(heading)
        end = start + 1
        while end < len(old) and not old[end].startswith("## "):
            end += 1
        new = [*old[:start], *section.split("\n"), *([""] if end < len(old) else []), *old[end:]]
        path.write_text("\n".join(new))


def main():
    paths = packages()
    if sys.argv[1:] == ["body"]:
        sys.stdout.write(process_body(sys.stdin.read(), paths))
    elif sys.argv[1:] == ["changelogs"]:
        update_changelogs(sys.stdin.read(), paths)
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
