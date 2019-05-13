#!/bin/bash

# Fail on the first error.
set -e

if [[ "${TRAVIS_RUST_VERSION}" != "nightly" ]]; then
  echo "Not fuzzing because we are not building on nightly."
  exit 0
fi

cargo install cargo-fuzz --vers 0.5.2 || true

# Pre-populate the corpus with the test samples, if they did not exist already.
# Note that we do not cache the corpus directly on Travis, due to this bug:
# https://bugs.llvm.org//show_bug.cgi?id=25991.
mkdir -p fuzz/corpus/decode_full
mkdir -p fuzz_corpus
cp --no-clobber testsamples/*.flac fuzz_corpus
cp --no-clobber testsamples/fuzz/*.flac fuzz_corpus
cp --no-clobber fuzz_corpus/* fuzz/corpus/decode_full

echo "Corpus size: $(ls -A fuzz/corpus/decode_full | wc -l)"

echo "Running fuzzer for ${FUZZ_SECONDS:-10} seconds ..."

# Disable leak detection, because when the fuzzer terminates after the set
# timeout, it might leak because it is in the middle of an iteration, but then
# the leak sanitizer will report that and exit with a nonzero exit code, while
# actually everything is fine.
export ASAN_OPTIONS="detect_leaks=0"

# Set max length to the size of a minimal flac file, as the coverage is similar
# (it is a lot harder to find a few elusive paths), but every iteration runs
# much faster. Warn about slow runs, as every iteration should execute in well
# below a second. Disable the leak sanitizer, otherwise it reports a leak when
# the fuzzer exits after the given total time.
cargo fuzz run decode_full -- \
  -max_len=8192 \
  -report_slow_units=1 \
  -max_total_time=${FUZZ_SECONDS:-10} \
  -print_final_stats=1 \
  -detect_leaks=0

# Copy back any new discoveries, so Travis can cache them. This step is not
# reached when fuzzing finds a crash, but that is ok, because in that case we
# should reproduce manually and add a regression test anyway.
cp --no-clobber fuzz/corpus/decode_full/* fuzz_corpus
