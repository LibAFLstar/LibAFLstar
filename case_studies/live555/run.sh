#!/usr/bin/env bash
# Default values
BIN="mcmm-cy"
LOOPS=1000
TIMEOUT="24h"

usage() {
  cat <<EOF
Usage: $0 [-b bin] [-l loops] [-t timeout]

  -b bin       which libaflstar-ftp binary to run; one of:
               mcmm-cy, mcmm-oe, mcsm-cy, mcsm-oe, sc-cy, sc-oe
               (default: ${BIN})
  -l loops     number of loops to pass to --loops
               (default: ${LOOPS})
  -t timeout   timeout duration for 'timeout'
               (default: ${TIMEOUT})
  -h           show this help message and exit
EOF
  exit 1
}

# Parse options
while getopts ":b:l:t:h" opt; do
  case ${opt} in
    b) BIN="$OPTARG" ;;
    l) LOOPS="$OPTARG" ;;
    t) TIMEOUT="$OPTARG" ;;
    h) usage ;;
    \?) echo "Error: Invalid option -$OPTARG" >&2; usage ;;
    :) echo "Error: Option -$OPTARG requires an argument." >&2; usage ;;
  esac
done

# Validate BIN choice
case "${BIN}" in
  mcmm-cy|mcmm-oe|mcsm-cy|mcsm-oe|sc-cy|sc-oe) ;;
  *)
    echo "Error: invalid bin '${BIN}'. Must be one of: mcmm-cy, mcmm-oe, mcsm-cy, mcsm-oe, sc-cy, sc-oe." >&2
    usage
    ;;
esac

shift $((OPTIND -1))

# Run
timeout "${TIMEOUT}" \
  cargo run --release --bin "libaflstar-rtsp-${BIN}" -- \
    --in-dir /corpus \
    --out-dir /output_live555 \
    --target-port 8554 \
    --loops "${LOOPS}" \
    -t 300 \
    /live/testProgs/testOnDemandRTSPServer 8554
rc=$?

if [ "$rc" -ne 124 ]; then
  # anything other than “124 = killed by timeout” → exit here
  echo "Exiting: fuzzing failed before timeout"
  exit "$rc"
fi

# Copy the stats.json file, the total_stats_info.txt file and the crashes folder into the /results/${BIN}-${LOOPS}-${TIMEOUT}/ folder
# If the folder exists, add the current date to the folder name
if [ ! -d "/results/${BIN}-${LOOPS}-${TIMEOUT}/" ]; then
  mkdir -p "/results/${BIN}-${LOOPS}-${TIMEOUT}/"
else
  SUFFIX=$(date)
  mkdir -p "/results/${BIN}-${LOOPS}-${TIMEOUT}-${SUFFIX}/"
fi

cp /output_live555/stats.json "/results/${BIN}-${LOOPS}-${TIMEOUT}/"
cp /output_live555/total_stats_info.txt "/results/${BIN}-${LOOPS}-${TIMEOUT}/"
cp -r /output_live555/crashes "/results/${BIN}-${LOOPS}-${TIMEOUT}/"

# Make everything in the /results/${BIN}-${LOOPS}-${TIMEOUT}/ folder readable and writable by everyone
chmod -R 777 "/results/${BIN}-${LOOPS}-${TIMEOUT}/"