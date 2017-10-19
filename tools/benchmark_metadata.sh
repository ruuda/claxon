#!/bin/sh

# This script runs the bench_metadata program, which reads at mst 1024 flac
# files in the testsamples/extra directory, and collects the results. It expects
# a basename for the output files. It is useful to use a directory plus a short
# identifier, e.g. "measurements/baseline". Then after making a change, run this
# script with "measurements/after" as basename. Results can be compared with the
# compare_benches.r script.

# Exit if any command fails.
set -e

if [ -z "$1" ]; then
  echo "You must provide a basename for the file to write the results to."
  exit 1
fi

# Put the Git commit in the basename so I can cross-reference later.
bname="$1_$(git rev-parse @ | cut -c 1-7)"

# Disable automatic CPU frequency scaling to get lower variance measurements.
if ! grep -q "performance" /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor; then
    echo "Locking CPU clock speed to its maximum. This requires root access."
  echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor > /dev/null
fi

# Optimize for the current CPU specifically, and include debugging symbols.
export RUSTFLAGS="-C target-cpu=native -g"

# Compile the benchmarking program.
cargo build --release --example bench_decode

for i in {1..10}; do
  echo "[$i/10] Benchmarking  ..."
  # Run the benchmarks with "taskset" to lock them to the same CPU core for the
  # entire program, to lower variance in the measurements.
  taskset -c 1 target/release/examples/bench_metadata > "${bname}_${i}.dat"
done

# Merge the output files.
rm -f "${bname}_all.dat"
cat ${bname}_*.dat > "${bname}_all.dat"
