"""Shared parts of the scripts that post-process the release notes.

The crates are the components of release-please-config.json in the current
directory, and their dependencies come from their Cargo.toml.
"""

import json
import re
import tomllib
from pathlib import Path

SECTION = re.compile(r"(<details><summary>([A-Za-z0-9_.-]+): ([^<]*)</summary>)")


def packages():
    """Maps each component to the path of its crate."""
    config = json.loads(Path("release-please-config.json").read_text())
    return {p["component"]: path for path, p in config["packages"].items()}


def dependency_tables(path):
    """Yields (kind, table) for each dependency table of a crate's manifest."""
    manifest = tomllib.loads((Path(path) / "Cargo.toml").read_text())
    for table in [manifest, *manifest.get("target", {}).values()]:
        yield "runtime", table.get("dependencies", {})
        yield "runtime", table.get("build-dependencies", {})
        yield "dev", table.get("dev-dependencies", {})


def workspace_dependencies(path, crates):
    """Maps each workspace crate the crate at path depends on to its kind.

    The kind is "dev" for a dependency of the tests only and "runtime" for
    one that ships with the crate, which wins where a crate is both.
    """
    kinds = {}
    for kind, table in dependency_tables(path):
        for name, spec in table.items():
            if isinstance(spec, dict):
                name = spec.get("package", name)
            if name in crates and kinds.get(name) != "runtime":
                kinds[name] = kind
    return kinds


def is_continuation(line):
    return line.startswith((" ", "\t")) and bool(line.strip())


def drop_entries(lines, matches):
    """Drops the entries for which matches(line) is true, with their
    continuation lines, and the headings left without entries."""
    out = []
    dropping = False
    for line in lines:
        if not (dropping and is_continuation(line)):
            dropping = matches(line)
            if not dropping:
                out.append(line)
    return drop_empty_headings(out)


def drop_empty_headings(lines):
    """Removes "### Heading" lines whose entries were all dropped."""
    out = []
    i = 0
    while i < len(lines):
        j = i + 1
        while j < len(lines) and not lines[j].strip():
            j += 1
        end = j >= len(lines) or lines[j].startswith(("#", "</details>"))
        if lines[i].startswith("### ") and end:
            i = j
            continue
        out.append(lines[i])
        i += 1
    return out


def split_body(text):
    """Splits the body of the release pull request into its text before the
    sections and its sections.

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


def join_body(head, sections):
    out = [head]
    for summary, _, _, lines, rest in sections:
        out.append(summary + "\n".join(lines) + rest)
    return "".join(out)
