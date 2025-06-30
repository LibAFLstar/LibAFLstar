#!/usr/bin/env bash
set -euo pipefail

# Default parameters (can be overridden with flags)
BIN="${BIN:-mcmm-cy}"
LOOPS="${LOOPS:-1000}"
TIMEOUT="${TIMEOUT:-1m}"
CASE_STUDIES=""

usage() {
  cat <<EOF
Usage: $0 [options]

Options:
  -b bin           which libaflstar configuration to run; one of: mcmm-cy, mcmm-oe, mcsm-cy, mcsm-oe, sc-cy, sc-oe (default: ${BIN})
  -l loops         number of loops to pass to --loops (default: ${LOOPS})
  -t timeout       timeout duration for 'timeout' (default: ${TIMEOUT})
  -c case_studies  comma-separated list of case studies to run (default: all detected)
  -h               show this help message and exit
EOF
}

# Parse command-line options
while getopts ":b:l:t:c:h" opt; do
  case ${opt} in
    b) BIN="$OPTARG" ;;
    l) LOOPS="$OPTARG" ;;
    t) TIMEOUT="$OPTARG" ;;
    c) CASE_STUDIES="$OPTARG" ;;
    h) usage; exit 0 ;;
    \?) echo "Invalid option: -$OPTARG" >&2; usage; exit 1 ;;
    :) echo "Option -$OPTARG requires an argument." >&2; usage; exit 1 ;;
  esac
done
shift $((OPTIND-1))

# Build the list of images into an array
declare -a images_arr

if [[ -n "$CASE_STUDIES" ]]; then
  # User specified a comma-separated list
  IFS=',' read -r -a SELECTED <<< "$CASE_STUDIES"
  for cs in "${SELECTED[@]}"; do
    image="libaflstar_${cs}"
    if docker image inspect "$image" &>/dev/null; then
      images_arr+=("$image")
    else
      echo "Warning: Docker image '$image' not found; skipping." >&2
    fi
  done
else
  # Auto-detect all libaflstar_ images
  # mapfile will strip the trailing newline from each line
  mapfile -t images_arr < <(docker images --format "{{.Repository}}" | grep '^libaflstar_')
fi

if (( ${#images_arr[@]} == 0 )); then
  echo "No Docker images found for the specified case studies." >&2
  exit 1
fi

echo "Starting experiments for the following case studies:"
printf '  %s\n' "${images_arr[@]}"

# Launch each container
for image in "${images_arr[@]}"; do
  case_study="${image#libaflstar_}"
  results_dir="$(pwd)/benchmark/${case_study}"
  echo "Launching $image → case study '$case_study'"
  docker run -d \
    -v "$results_dir":/results \
    --rm \
    "$image" \
    -b "$BIN" \
    -l "$LOOPS" \
    -t "$TIMEOUT"
done

echo "All experiments launched."
