#!/bin/sh

# This script runs the "decode" example on the flac files in the
# "testsamples/extra" directory, and also the reference "flac -d" program.
# It measures the time it takes, and prints a comparison.

# Exit if any command fails.
set -e

# Disable automatic CPU frequency scaling to get lower variance measurements.
if ! grep -q "performance" /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor; then
    echo "Locking CPU clock speed to its maximum. This requires root access."
  echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor > /dev/null
fi

# Optimize for the current CPU specifically, and include debugging symbols.
export RUSTFLAGS="-C target-cpu=native -g"

# Compile the example decode program.
cargo build --release --example decode

rm -f /tmp/bench_times_claxon.dat
rm -f /tmp/bench_times_libflac.dat

for i in {1..11}; do
    echo -n "Benchmarking, round ${i}/11 "

    # Clean output files in between each run, otherwise we are prompted to
    # overwrite.
    rm -f testsamples/extra/*.wav
    echo -n "[libflac]"
    /bin/time --format="%e" --append --output /tmp/bench_times_libflac.dat \
        flac -d testsamples/extra/*.flac 2> /dev/null

    rm -f testsamples/extra/*.wav
    echo -en "\b\b\b\b\b\b\b\b\b[Claxon] \b"
    /bin/time --format="%e" --append --output /tmp/bench_times_claxon.dat \
        target/release/examples/decode testsamples/extra/*.flac > /dev/null

    echo -e "\b\b\b\b\b\b\b\b[done]  "
done
