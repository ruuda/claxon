#!/usr/bin/env python3

"""
Prints a summary of sample times recorded by benchmark.sh.

Usage:
    tools/summarize.py measurements/basename_sha.dat
"""

import numpy as np
import sys
from scipy.stats import gamma, erlang
from typing import Callable, NamedTuple, Tuple

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

    # Width of a 95% confidence interval starting at 0, for the noise delay that
    # is still present in the minima that we observed.
    noise_min_q95: float


def quantile(p: float, lower: float, upper: float, cdf: Callable[[float], float]) -> float:
    """
    Performs a binary search to find q_p such that cdf(q_p) = p. A lower and
    upper bound for q_p must be provided as "low" and "high". The cdf must be
    monotonically increasing (as all CDFs are).
    """
    qp_lo, p_lo = lower, 0.0
    qp_hi, p_hi = upper, 1.0

    for _ in range(0, 60):
        qp_mid = 0.5 * qp_lo + 0.5 * qp_hi
        p_mid = cdf(qp_mid)

        if p_mid > p:
            qp_hi, p_hi = qp_mid, p_mid
        else:
            qp_lo, p_lo = qp_mid, p_mid

    return 0.5 * qp_lo + 0.5 * qp_hi


def quantile_min(p: float, n: int, xs: np.array) -> float:
    """
    Return the p-th quantile of the distribution of min(x1, x2, ..., xn),
    where the xi are n independent random variables drawn from the empirical
    distribution given by xs.
    """
    # The empirical CDF for the data given by xs.
    ecdf = lambda x: np.sum(xs < x) / len(xs)

    # Given a cumulative distribution function F, we can construct G that
    # describes the distribution min(x1, x2, ..., xn) where the xi are drawn
    # from the distribution described by F, and G(x) = 1 - (1 - F(x))^n.
    ecdf_min = lambda x: 1.0 - np.power(1.0 - ecdf(x), n)

    return quantile(p, np.min(xs), np.max(xs), ecdf_min)


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
    # delays we can quantify the noise and plot its distribution. On my system
    # it often looks like an Erlang distribution with k=3, which would mean that
    # the delay is the sum of 3 exponentially distributed noise sources.
    diffs = np.transpose(data) - mins

    # We now have the noise per block, but the particular block is not that
    # interesting, so flatten the matrix. Also exclude zeros, those are not
    # delays, those are the actual minimum times.
    noise = np.reshape(diffs, -1)
    noise = noise[noise > 0.0]

    # From the empirical noise distribution, we can get a distribution of
    # min(x1, x2, ..., xn), where xi are drawn from that distribution. With
    # this, we can build a 95% confidence interval for the delay in our observed
    # minima. We could take quantile bounds (0.025, 0.975) as is customary for
    # symmetric distributions, but that would mean that the maximum likelihood
    # estimate of the minimum is not in the interval, so we take (0.0, 0.95) as
    # bounds instead, such that the maximum likelihood estimate of the minimum
    # is one of the endpoints.
    noise_min_q95 = quantile_min(0.95, num_iters, noise)

    return Stats(
        num_blocks = num_blocks,
        num_iters = num_iters,
        iter_means = iter_means,
        iter_outlier_threshold = iter_outlier_threshold,
        block_mins = ok_mins,
        noise = noise,
        noise_min_q95 = noise_min_q95,
    )


def plogn(mu: float, sigma: float, x: np.array) -> np.array:
    """Return log-normal probability density at x."""
    denom = x * sigma * np.sqrt(np.pi * 2.0)
    exponent = np.square(np.log(x) - mu) / (2.0 * sigma * sigma)
    return np.exp(-exponent) / denom


def pexp(lam: float, x: np.array) -> np.array:
    """Return exponential probability density at x."""
    return lam * np.exp(-lam * x)


def perl(lam: float, k: int, x: np.array) -> np.array:
    """Return Erlang probability density at x."""
    k_minus_1_fact = np.product(range(1, k))
    return np.power(lam, k) * np.power(x, k - 1) * np.exp(-lam * x) / k_minus_1_fact


def plot(stats: Stats) -> None:
    plt.rcParams['font.family'] = 'Source Serif Pro'
    fig, (ax1, ax2, ax3) = plt.subplots(3, 1)

    ax1.axhline(np.mean(stats.iter_outlier_threshold), color='red')
    ax1.plot(
        np.arange(0, stats.num_iters),
        stats.iter_means,
        color='black',
        linewidth=1,
    )
    ax1.set_xlabel('iteration')
    ax1.set_ylabel('time per sample (ns)')

    noise_max_bin = min(
        np.quantile(stats.noise, 0.98),
        np.quantile(stats.noise, 0.66) * 4.0,
    )
    bins = np.arange(0.0, noise_max_bin, noise_max_bin / 200.0);
    ax2.hist(stats.noise, bins=bins, density=True, color='#bbbbbb')
    ax2.set_xlabel('noise delay (ns)')
    ax2.set_ylabel('density')

    plt.show()


def report(stats: Stats) -> None:
    # Report the mean time per sample (average the time per sample over the
    # blocks). What we are interested in for optimization purposes is the total
    # running time, not the median or an other quantile, because you always
    # decode all blocks of a file. So we take the mean.
    t_mean = np.mean(stats.block_mins)

    # Recall that we assume times to be of the form t + x where x is noise. We
    # have min(t + x1, t + x2, ...), and a 95% confidence interval for the noise
    # that is still present in that measurement, which means we have a 95%
    # confidence interval for t.
    mid_interval = t_mean - stats.noise_min_q95 * 0.5
    plm_interval = stats.noise_min_q95 * 0.5

    print(f'Mean time per sample:    {t_mean:6.3f} ns')
    print(f'95% confidence interval: {mid_interval:6.3f} ± {plm_interval:.3f} ns')


if len(sys.argv) < 2:
    print(__doc__)
    sys.exit(1)

for fname in sys.argv[1:]:
    stats = load(fname)
    plot(stats)
    report(stats)
