#!/bin/bash

# Fail on the first error, and print every command as it is executed.
set -e

if [[ "${TRAVIS_RUST_VERSION}" != "nightly" ]]; then
  echo "Not fuzzing because we are not building on nightly."
  exit 0
fi

cargo install cargo-fuzz --vers 0.3.1 || true

# Pre-populate the corpus with the test samples, if they did not exist already.
mkdir -p fuzz/corpus
cp --update testsamples/*.flac fuzz/corpus
cp --update testsamples/fuzz/*.flac fuzz/corpus

echo "Corpus size: $(ls -A fuzz/corpus | wc -l)"

echo "Running fuzzer for ${FUZZ_SECONDS:-10} seconds ..."

# Disable leak detection, because when the fuzzer terminates after the set
# timeout, it might leak because it is in the middle of an iteration, but then
# the leak sanitizer will report that and exit with a nonzero exit code, while
# actually everything is fine.
export ASAN_OPTIONS="detect_leaks=0"

# Set max length to a small-ish number (in comparison to the test samples), as
# the coverage is similar (it is a lot harder to find a few elusive paths), but
# every iteration runs much faster. Warn about slow runs, as every iteration
# should execute in well below a second. Disable the leak sanitizer, otherwise
# it reports a leak when the fuzzer exits after the given total time.
cargo fuzz run decode_full -- \
  -max_len=2048 \
  -report_slow_units=1 \
  -max_total_time=${FUZZ_SECONDS:-10} \
  -print_final_stats=1 \
  -detect_leaks=0
