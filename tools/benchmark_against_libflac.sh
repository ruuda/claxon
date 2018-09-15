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
# Since Rust 1.24, the number of codegen units is more than 1 by default. This
# improves compile time, but causes a 36% regression in Claxon performance, so
# we need to set the number of codegen units to 1.
export RUSTFLAGS="-C target-cpu=native -C codegen-units=1 -g"

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
    env time --format="%e" --append --output /tmp/bench_times_libflac.dat \
        flac -d testsamples/extra/*.flac 2> /dev/null

    rm -f testsamples/extra/*.wav
    echo -en "\b\b\b\b\b\b\b\b\b[Claxon] \b"
    env time --format="%e" --append --output /tmp/bench_times_claxon.dat \
        target/release/examples/decode testsamples/extra/*.flac > /dev/null

    echo -e "\b\b\b\b\b\b\b\b[done]  "
done

# Compute statistics with R.

Rscript - << EOF
claxon  <- read.table('/tmp/bench_times_claxon.dat')[[1]]
libflac <- read.table('/tmp/bench_times_libflac.dat')[[1]]

# Estimates the absolute error in (a ± aErr) / (b ± bErr).
# See also https://en.wikipedia.org/wiki/Propagation_of_uncertainty.
propErr <- function(a, b, aErr, bErr)
{
  # The new relative variance is the sum of the relative variances of a and b.
  relA2 <- (aErr / a) * (aErr / a)
  relB2 <- (bErr / b) * (bErr / b)
  return(abs(b / a) * sqrt(relA2 + relB2))
}

claxonMean <- mean(claxon)  / mean(libflac)
refMean    <- mean(libflac) / mean(libflac)

claxonSd <- propErr(mean(claxon),  mean(libflac), sd(claxon), sd(libflac))
refSd    <- propErr(mean(libflac), mean(libflac), sd(libflac), sd(libflac))

cat(sprintf('Claxon:  %3.2f ± %2.2f\n', claxonMean, claxonSd))
cat(sprintf('libflac: %3.2f ± %2.2f\n', refMean, refSd))
EOF

rm /tmp/bench_times_claxon.dat
rm /tmp/bench_times_libflac.dat
