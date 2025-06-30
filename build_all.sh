#!/usr/bin/env bash
set -euo pipefail

# Require Bash 4+ for associative arrays
if ((BASH_VERSINFO[0] < 4)); then
  echo "This script requires Bash 4 or higher." >&2
  exit 1
fi

declare -A pid_to_dir
declare -a successes failures

# Kick off all builds in parallel, recording PIDs
for dir in case_studies/*/; do
  [ -d "$dir" ] || continue
  name=$(basename "$dir")
  (
    cd "$dir"
    docker build -t "libaflstar_${name}" .
  ) &
  pid_to_dir[$!]="$name"
done

# Wait for each build and collect results
for pid in "${!pid_to_dir[@]}"; do
  name=${pid_to_dir[$pid]}
  if wait "$pid"; then
    successes+=("$name")
  else
    failures+=("$name")
  fi
done

# Print summary
echo
if [ "${#successes[@]}" -gt 0 ]; then
  echo "✅ Successful builds:"
  for name in "${successes[@]}"; do
    echo "  - libaflstar_${name}"
  done
else
  echo "⚠️  No builds succeeded."
fi

if [ "${#failures[@]}" -gt 0 ]; then
  echo
  echo "❌ Failed builds (WIP):"
  for name in "${failures[@]}"; do
    echo "  - libaflstar_${name}"
  done
else
  echo
  echo "🎉 All builds succeeded!"
fi
