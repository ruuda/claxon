#!/bin/sh

# Exit if any command fails.
set -e

# Optimize for Skylake CPUs specifically.
export RUSTFLAGS="-C target-cpu=skylake"

# Compile the benchmarking program.
cargo build --verbose --release --example bench_decode

time target/release/examples/bench_decode 16 testsamples/b0_daft_punk_one_more_time.flac
time target/release/examples/bench_decode 32 testsamples/b0_daft_punk_one_more_time.flac

time target/release/examples/bench_decode 16 testsamples/b1_deadmau5_i_remember.flac
time target/release/examples/bench_decode 32 testsamples/b1_deadmau5_i_remember.flac

time target/release/examples/bench_decode 16 testsamples/b2_massive_attack_unfinished_sympathy.flac
time target/release/examples/bench_decode 32 testsamples/b2_massive_attack_unfinished_sympathy.flac

time target/release/examples/bench_decode 16 testsamples/b3_muse_starlight.flac
time target/release/examples/bench_decode 32 testsamples/b3_muse_starlight.flac

time target/release/examples/bench_decode 16 testsamples/b4_u2_sunday_bloody_sunday.flac
time target/release/examples/bench_decode 32 testsamples/b4_u2_sunday_bloody_sunday.flac
