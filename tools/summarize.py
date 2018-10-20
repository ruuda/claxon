#!/usr/bin/env python3

"""
Prints a summary of sample times recorded by benchmark.sh.

Usage:
    tools/summarize.py measurements/basename_sha.dat
"""

import numpy as np
import sys
from typing import NamedTuple

import matplotlib
matplotlib.use('gtk3cairo')

import matplotlib.pyplot as plt

class Stats(NamedTuple):
    # Number of blocks for which we measured the time per sample.
    num_blocks: int

    # Number of times we measured each block.
    num_iters: int

    # Mean time per sample (in nanoseconds) over all blocks,
    # one element per iteration.
    iter_means: np.array

    # Time above which the iteration is considered an outlier for noise analysis
    # purposes.
    iter_outlier_threshold: float

    # Minimum time per sample (in nanoseconds) over all iterations, one element
    # per block. Excludes outlier blocks that are very fast to decode and
    # therefore not interesting.
    block_mins: np.array

    # Delays on top of the minimal time per sample for a block. Filtered to
    # remove outlier iterations.
    noise: np.array


def load(fname) -> Stats:
    data = np.genfromtxt(sys.argv[1])

    # For every decoded block, we have num_samples measurements.
    num_blocks, num_iters = data.shape

    # Take the minimum time of all of the recorded times that we have for a
    # block. This is the time undisturbed by noise, and the time we will report.
    mins = np.min(data, axis=1)

    # Remove a few outliers from the measurements. These outliers are legitimite
    # measurements (after taking the min), some blocks really are that fast to
    # decode. We're just not interested in them. This happens mostly for the few
    # blocks of silence at the beginning or end of a song; these are trivial to
    # decode at less than 1 ns per sample. These samples drive up the variance,
    # so we exclude them.
    outlier_threshold = np.mean(mins) * 0.75
    assert np.quantile(mins, 0.002) > outlier_threshold, 'Unexpected distribution'
    ok_mins = mins[mins > outlier_threshold]

    # For analysis of the noise, we also want to remove outliers, but this time
    # outliers where the entire iteration was slow. This can happen; especially
    # the first few iterations can be much slower than later ones, but then
    # still, sometimes there are plateaus where one run of the benchmark program
    # (which spans multiple iterations) is much slower than others. Perhaps it
    # got unlucky with ASLR and that caused more TLB misses? Single iterations
    # can be unexpectedly slow too, perhaps because some daemon had to do a
    # periodic task. Whatever the cause, for the purpose of analyzing the noise
    # distribution, these severe outliers cause a fat tail, so we exclude those
    # iterations entirely. They are still used for the minimum time per block,
    # just not for analyzing the noise.
    iter_means = np.mean(data, axis=0)
    iter_outlier_threshold = np.quantile(iter_means, 0.5)

    # For every block, we have a number of measurements, that should be of the
    # form t + x, where t is the true time, and x is noise on top. We estimate t
    # as the minimum over all samples. Then for every iteration that was below
    # the threshold, we get the noise delay x, and by collecting all of the
    # delays we can quantify the noise and report its properties.
    # NOTE: Looks lognormal.
    diffs = np.transpose(data) - mins
    ok_diffs = diffs[iter_means < iter_outlier_threshold]

    # We now have the noise per block, but the particular block is not that
    # interesting, so flatten the matrix. Also exclude zeros, those are not
    # noise.
    noise = np.reshape(ok_diffs, -1)
    noise = noise[noise > 0.0]

    return Stats(
        num_blocks = num_blocks,
        num_iters = num_iters,
        iter_means = iter_means,
        iter_outlier_threshold = iter_outlier_threshold,
        block_mins = ok_mins,
        noise = noise,
    )


def plot(stats: Stats) -> None:
    plt.plot(np.arange(0, stats.num_iters), stats.iter_means)
    plt.axhline(np.mean(stats.iter_outlier_threshold))
    plt.show()


def report(stats: Stats) -> None:
    # Report the mean time per sample (average the time per sample over the
    # blocks). What we are interested in for optimization purposes is the total
    # running time, not the median or an other quantile, because you always
    # decode all blocks of a file. So we take the mean.
    t_mean = np.mean(stats.block_mins)

    mean_noise = np.mean(stats.noise)
    print(mean_noise)

    # TODO: These new bounds look far too tight. What's up?
    conf_bounds = np.array([0.95, 0.95 * 0.5, 0.0])
    qs = -np.log(1.0 - conf_bounds) / stats.num_blocks
    noise_offset_bounds = qs * mean_noise

    # Recall that we assume times to be of the form t + x where x is noise. We
    # now have a 95% confidence interval for x, and by taking the mean over all
    # timings for a block, we get an estimate of t + x. Combined we get a 95%
    # confidence interval for t. It's a bit of a hack, and there should be a
    # better statistic to report, but for now this will do.
    t_bounds = t_mean - noise_offset_bounds

    mid_interval = 0.5 * (t_bounds[0] + t_bounds[2])
    plm_interval = 0.5 * (t_bounds[2] - t_bounds[0])

    print(f'Mean time per sample:    {t_mean:6.3f} ns')
    print(f'95% confidence interval: {mid_interval:6.3f} ± {plm_interval:.3f} ns')


if len(sys.argv) < 2:
    print(__doc__)
    sys.exit(1)

for fname in sys.argv[1:]:
    stats = load(fname)
    plot(stats)
    report(stats)
