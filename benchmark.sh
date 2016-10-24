#!/bin/sh

# Exit if any command fails.
set -e

if [ -z "$1" ]; then
  echo "You must provide a basename for the file to write the results to."
  exit 1
fi

# Disable automatic CPU frequency scaling to get lower variance measurements.
if ! grep -q "performance" /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor; then
    echo "Locking CPU clock speed to its maximum. This requires root access."
  echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor > /dev/null
fi

# Optimize for Skylake CPUs specifically.
export RUSTFLAGS="-C target-cpu=skylake"

# Compile the benchmarking program.
cargo build --verbose --release --example bench_decode

# Run the benchmarks with "taskset" to lock them to the same CPU core for the
# entire program, to lower variance in the measurements.
echo "Benchmarking sample file 0 ..."
taskset -c 1 target/release/examples/bench_decode 16 testsamples/b0_daft_punk_one_more_time.flac > "$1_b0_16.dat"
taskset -c 1 target/release/examples/bench_decode 32 testsamples/b0_daft_punk_one_more_time.flac > "$1_b0_32.dat"

echo "Benchmarking sample file 1 ..."
taskset -c 1 target/release/examples/bench_decode 16 testsamples/b1_deadmau5_i_remember.flac > "$1_b1_16.dat"
taskset -c 1 target/release/examples/bench_decode 32 testsamples/b1_deadmau5_i_remember.flac > "$1_b1_32.dat"

echo "Benchmarking sample file 2 ..."
taskset -c 1 target/release/examples/bench_decode 16 testsamples/b2_massive_attack_unfinished_sympathy.flac > "$1_b2_16.dat"
taskset -c 1 target/release/examples/bench_decode 32 testsamples/b2_massive_attack_unfinished_sympathy.flac > "$1_b2_32.dat"

echo "Benchmarking sample file 3 ..."
taskset -c 1 target/release/examples/bench_decode 16 testsamples/b3_muse_starlight.flac > "$1_b3_16.dat"
taskset -c 1 target/release/examples/bench_decode 32 testsamples/b3_muse_starlight.flac > "$1_b3_32.dat"

echo "Benchmarking sample file 4 ..."
taskset -c 1 target/release/examples/bench_decode 16 testsamples/b4_u2_sunday_bloody_sunday.flac > "$1_b4_16.dat"
taskset -c 1 target/release/examples/bench_decode 32 testsamples/b4_u2_sunday_bloody_sunday.flac > "$1_b4_32.dat"
