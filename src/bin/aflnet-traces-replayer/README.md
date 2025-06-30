# Trace Replayer

Replay AFLnet output traces on LibAFLstar binaries, to obtain reliable coverage comparisons.

Takes the `relayable-queue` (so not `queue`) from an AFLnet output directory, and replays them on the binary. 

View the `total_stats_info.txt` for the result.

## Details

- Make sure to use the same settings as used in the experiment you want to compare it to.