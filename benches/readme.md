# Benchmarks

Claxon includes an extensive benchmark to ensure that changes do not regress
performance, and to validate that optimizations are worthwhile. The benchmark
is a custom measurement program with a separate Python program for analysis.

TODO: Add table of contents.
TODO: Explain how to run benchmarks.

## Alternatives

Claxon does not use a standard benchmark harness like [`std::test`][stdbench]
or [Criterion.rs][criterion], because they are unreliable. For `std::test`, I
have regularly observed means further than two standard deviations away of the
previous mean for repeated benchmark runs, and Criterion.rs reports a
significant change in performance, with high confidence, on a repeated benchmark
run far more frequently than it should. Claxon's benchmark aims to eliminate
noise, and quantify the remaining noise in a statistically sound manner to give
realistic confidence bounds, for reproducible and accurate benchmarks.

## Approach

The program [`bench_decode`][benchdecode] decodes the files provided on the
command line a number of times. Per file, every frame is decoded one by one,
and the times are recorded. The time per sample (time to decode the frame
divided by the length of the frame) is recorded. This is the raw data that the
program outputs: for every iteration, for every frame, the time per sample.

Although the time per sample can vary across frames (some frames are more
expensive to decode than others), the time to decode the same frame should be
constant. We assume that the times we observe are of the for `t + x`, where `t`
is the absolute minimum time it takes to decode the frame, and `x` is noise on
top (which can have many causes, such as interrupts, or cache misses due to a
different process trashing the caches). From the observations in every
iteration, we can make an estimate of `t`, with confidence interval.

The reported time per sample is the mean time per sample over all frames. The
confidence interval is based on the individual confidence intervals. This means
that we can shrink the confidence interval by decoding for more iterations, not
by decoding more frames.

To compare two benchmark runs, we look at the difference between the estimated
time per sample for each frame. If the runs are indistinguishable, the
differences should be distributed like the difference of two independent noise
samples, which has a zero mean. We take this zero mean as null hypothesis, and
perform a statistical test to see if the difference is significant. Because we
compare differences per frame, we can get a test with a higher power by decoding
more frames. The number of iterations plays a role too, for increasing the
number of iterations decreases variance, which enables observing smaller
differences.

## Preparation

TODO: Mention some variance reduction techniques:

 * Disable network and peripherals. (Reduce interrupts.)
 * Lock the scaling governor.
 * Disable the clock interrupt (`nohz_full`).
 * Pin to a core.
 * Don't type on the keyboard (keystrokes cause interrupts).
 * Lock the screen and darken monitor (no need to drive the GPU).

TODO: Add graphs to show that the effect is real.
Note that you can still do benchmarks if you don't follow this advice,
the confidence bounds will just be wider.

## Analysis

TODO: Explain the plots.

[stdbench]:    https://doc.rust-lang.org/test/struct.Bencher.html
[criterion]:   https://crates.rs/crates/criterion
[benchdecode]: ../examples/bench_decode.rs
