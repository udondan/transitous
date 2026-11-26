#!/usr/bin/env bash
# Prints the title of the release pull request with the planned versions,
# like "chore: release transitous 0.2.0, motis 0.1.2". Reads the body of the pull
# request on stdin, which has a summary "<component>: <version>" for every
# release, transitous first.
#
# Release Please parses the title again when the pull request is merged: it
# must start with "chore: release <component> <version>". With more than one
# release, it takes the versions from the body.
set -euo pipefail

first=()
rest=()
pattern='<summary>([A-Za-z0-9_.-]+): ([^<]+)</summary>'
while IFS= read -r line; do
  if [[ $line =~ $pattern ]]; then
    name=${BASH_REMATCH[1]}
    version=${BASH_REMATCH[2]}
    case $name in
      transitous) first+=("$name $version") ;;
      *) rest+=("$name $version") ;;
    esac
  fi
done

parts=("${first[@]}" "${rest[@]}")
if [[ ${#parts[@]} -eq 0 ]]; then
  echo "no release in the body" >&2
  exit 1
fi
title="chore: release ${parts[0]}"
for part in "${parts[@]:1}"; do
  title+=", $part"
done
echo "$title"
