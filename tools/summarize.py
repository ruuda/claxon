#!/usr/bin/env python3

"""
Prints a summary of sample times recorded by benchmark.sh.

Usage:
    tools/summarize.py measurements/basename_sha.dat
"""

import numpy as np
import sys

import matplotlib.pyplot as plt

data = np.genfromtxt(sys.argv[1])

# For every decoded block, we have num_samples measurements.
num_blocks, num_samples = data.shape

# Take the minimum time of all of the recorded times that we have for a block.
# This is the time undisturbed by noise, and the time we will report.
mins = np.min(data, axis=1)

# We also report a confidence interval that is based on properties of the noise,
# and for this we use the means instead.
means = np.mean(data, axis=1)

# Remove a few outliers from the measurements. These outliers are legitimite
# measurements (after taking the min), some samples really are that fast to
# decode. We're just not interested in them. This happens mostly for the few
# blocks of silence at the beginning or end of a song; these are trivial to
# decode at less than 1 ns per sample. These samples drive up the variance, so
# we exclude them.
outlier_threshold = np.mean(mins) * 0.75
assert np.quantile(mins, 0.002) > outlier_threshold, 'Unexpected distribution'
ok_mins = mins[mins > outlier_threshold]
ok_means = means[means > outlier_threshold]

# Report the mean time per sample (average the time per sample of the blocks).
# What we are interested in for optimization purposes is the total running time,
# not the median or an other quantile, because you always decode all blocks of a
# file. So we take the mean.
t_mean = np.mean(ok_mins)

# For every block, we have a number of measurements, that should be of the form
# t + x, where t is the true time, and x is noise on top. We estimate t as the
# minimum over all samples. Assuming that the noise follows an exponential
# distribution, we can compute some additional statistics. NOTE: This assumption
# does not appear to be valid. The noise has a very particular shape (multiple
# lobes, and in the first lobe there are small spikes at clear intervals,
# possibly quantization noise). At a high level it looks more like a gamma
# distribution with k = 0.5. But we will assume exponential for now because it
# is easy to work with. NOTE 2: Actually, the noise distribution loosk a lot
# like lognormal after removing the zeros.
diffs = np.transpose(data) - mins
diffs = np.reshape(diffs, -1)
diffs = diffs[diffs > 0.0]
diffs = diffs[diffs < 4.0]
mean_noise = np.mean(diffs)

plt.hist(diffs, bins=np.linspace(0, 1, 300), normed=True)

log_diffs = np.log(diffs)
mu = np.mean(log_diffs)
variance = np.mean(np.square(log_diffs - mu))
stddev = np.sqrt(variance)
a = mean_noise / (2.0 * np.sqrt(2.0 * np.pi))

def pdf_logn(x):
    factor = x * stddev * np.sqrt(2.0 * np.pi)
    exponent = np.square(np.log(x) - mu) / (2.0 * variance)
    return np.exp(-exponent) / factor

def pdf_exp(x):
    return np.exp(-x / mean_noise) / mean_noise

def pdf_mb(x):
    factor = np.sqrt(2.0 / np.pi) / (a ** 3)
    exponent = np.square(x) / (2.0 * a * a)
    return np.exp(-exponent) * np.square(x) * factor

xs = np.linspace(0, 1, 600)
plt.plot(xs, pdf_logn(xs), lw=2)
plt.plot(xs, pdf_exp(xs), lw=2)
plt.plot(xs, pdf_mb(xs), lw=2)

plt.show()

# TODO: Take one of the bounds to 0?
# TODO: These new bounds look far too tight. What's up?
conf_bounds = np.array([0.975, 0.025])
qs = -np.log(1.0 - conf_bounds) / num_samples
noise_offset_bounds = qs * mean_noise

# Recall that we assume times to be of the form t + x where x is noise. We now
# have a 95% confidence interval for x, and by taking the mean over all timings
# for a block, we get an estimate of t + x. Combined we get a 95% confidence
# interval for t. It's a bit of a hack, and there should be a better statistic
# to report, but for now this will do.
t_bounds = t_mean - noise_offset_bounds

print(f'Mean time per sample:    {t_mean:6.3f} ns')
print(f'95% confidence interval: {t_bounds[0]:6.3f} ns .. {t_bounds[1]:6.3f} ns')
