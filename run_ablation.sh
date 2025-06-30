#!/usr/bin/env bash
set -euo pipefail

# Default parameter sets
b_values=(mcmm-cy mcmm-oe mcsm-oe mcsm-cy sc-cy sc-oe)
l_values=(1 10 100 1000)
t_value="1h"
CASE_STUDIES=""

usage() {
  cat <<EOF
Usage: $0 [options]

Options:
  -c case_studies  comma-separated list of case studies to run (default: all detected)
  -B binaries      comma-separated list of b_values to test (default: all: ${b_values[*]})
  -L loops         comma-separated list of l_values to test (default: ${l_values[*]})
  -T timeout       timeout value for -t (default: ${t_value})
  -h               show this help message and exit
EOF
}

# Parse command-line options
while getopts ":c:B:L:T:h" opt; do
  case "${opt}" in
    c) CASE_STUDIES="$OPTARG" ;;
    B) IFS=',' read -r -a b_values <<< "$OPTARG" ;;
    L) IFS=',' read -r -a l_values <<< "$OPTARG" ;;
    T) t_value="$OPTARG" ;;
    h) usage; exit 0 ;;
    \?) echo "Invalid option: -$OPTARG" >&2; usage; exit 1 ;;
    :) echo "Option -$OPTARG requires an argument." >&2; usage; exit 1 ;;
  esac
done
shift $((OPTIND -1))

# Build the "-c" argument if provided
C_FLAG=()
if [[ -n "$CASE_STUDIES" ]]; then
  C_FLAG=(-c "$CASE_STUDIES")
fi

# Loop over every combination
for b in "${b_values[@]}"; do
  for l in "${l_values[@]}"; do
    echo "=== Running: ./run_exp.sh -b ${b} -l ${l} -t ${t_value} ${C_FLAG[*]} ==="
    ./run_exp.sh -b "${b}" -l "${l}" -t "${t_value}" "${C_FLAG[@]}"
  done
done
