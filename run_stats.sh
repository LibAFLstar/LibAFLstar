#!/usr/bin/env bash
set -euo pipefail

# go into the benchmark directory
cd ./benchmark || { echo "benchmark folder not found"; exit 1; }

# order of b_values
b_values=(sc-cy mcsm-cy mcmm-cy sc-oe mcsm-oe mcmm-oe)
# l_values in ascending order
l_values=(1 10 100 1000 10000 100000)
# fixed t_value
t_value="1h"

for case in */; do
  case_name=${case%/}
  echo "Case study: $case_name"
  
  for b in "${b_values[@]}"; do
    echo "$b"
    
    for l in "${l_values[@]}"; do
      dir="$case_name/${b}-${l}-${t_value}"
      stats="$dir/total_stats_info.txt"
      
      if [[ ! -f "$stats" ]]; then
        echo "  [missing] $l"
        continue
      fi
      
      # line 2: complete_coverage: 34% (308/896)
      cov_line=$(sed -n '2p' "$stats")
      # extract "308/896"
      frac=$(echo "$cov_line" | grep -oP '\(\K[0-9]+/[0-9]+(?=\))')
      num=${frac%%/*}
      den=${frac##*/}
      
      # calc percentage with two decimals, dot decimal
      pct=$(awk "BEGIN{printf \"%.2f\", ($num/$den)*100}")
      # convert to comma decimal
      pct=${pct/./,}%
      
      # line 4: total_executions: 2367196
      execs=$(sed -n '4p' "$stats" | awk '{print $2}')
      
      # print: l_value coverage executions
      printf "  %s %s %s\n" "$l" "$pct" "$execs"
    done

    echo  # blank line between b_values
  done

  echo "----------------------------------------"
done
