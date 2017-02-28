# Fuzzing

Claxon can be fuzzed with cargo-fuzz, which can be installed with Cargo.
As 0.2.2, the latest at the time of writing, is broken, we opt for 0.2.1:

    cargo install --vers 0.2.1 cargo-fuzz

Copy the test samples into the fuzzing corpus to have some initial files:

    mkdir -p fuzz/corpus
    cp testsamples/*.flac fuzz/corpus

Then start the fuzzer for a moment:

    cargo fuzz --fuzz-target decode-full

Exit it with Ctrl+C, it will use a far too large input by default, which results
in a low number of iterations per second. By invoking the binary directly, we
can pass arguments to libfuzzer:

    cd fuzz
    target/debug/decode-full -max_len=2048 corpus

You can also run with `-help=1` to get the full list of options.
