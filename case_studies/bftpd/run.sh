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
  cargo run --release --bin "libaflstar-ftp-${BIN}" -- \
    --in-dir /corpus \
    --out-dir /output_bftpd \
    --target-port 21021 \
    --loops "${LOOPS}" \
    -t 300 \
    /bftpd/bftpd -D -c /basic.conf

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

cp /output_bftpd/stats.json "/results/${BIN}-${LOOPS}-${TIMEOUT}/"
cp /output_bftpd/total_stats_info.txt "/results/${BIN}-${LOOPS}-${TIMEOUT}/"
cp -r /output_bftpd/crashes "/results/${BIN}-${LOOPS}-${TIMEOUT}/"

# Make everything in the /results/${BIN}-${LOOPS}-${TIMEOUT}/ folder readable and writable by everyone
chmod -R 777 "/results/${BIN}-${LOOPS}-${TIMEOUT}/"