#!/usr/bin/env python3

"""
Prints a summary of sample times recorded by benchmark.sh.

Usage:
    tools/summarize.py measurements/basename_sha.dat
"""

import numpy as np
import sys

data = np.genfromtxt(sys.argv[1])

#data = data[:,0:80]
for k in range(1, len(data[0]), 10):
    ds = data[:, 0:k]
    mins = np.min(ds, axis=1)
    diffs = np.transpose(ds) - mins
    mean_noise = np.mean(diffs)
    print(f'{k:3}: {mean_noise:.3f}')

# Take the minimum time of all of the recorded times that we have for a block.
# This is the time undisturbed by noise.
mins = np.min(data, axis=1)

# Remote a few outliers from the measurements. These outliers are legitimite
# measurements (after taking the min), some samples really are that fast to
# decode. We're just not interested in them. This happens mostly for the few
# blocks of silence at the beginning or end of a song; these are trivial to
# decode at less than 1 ns per sample. These samples drive up the variance, so
# we exclude them.
outlier_threshold = np.mean(mins) * 0.75
assert np.quantile(mins, 0.002) > outlier_threshold, 'Unexpected distribution'
ok_samples = mins[mins > outlier_threshold]

diffs = np.transpose(data) - mins
mean_noise = np.mean(diffs)

qs_noise = -np.log(1.0 - np.array([0.05, 0.50, 0.95])) * mean_noise
print(f'Noise quantiles if noise were exponential: {qs_noise}')
print(f'Noise quantiles actual:                    {np.quantile(diffs, (0.05, 0.50, 0.95))}')

error = -np.log(1.0 - (1.0 / len(data[0]))) * mean_noise
print(f'Error on mean: {error:0.3f}')


print(f'removed noise: {np.mean(diffs):6.3f} ns or {np.quantile(diffs, 0.05)}')
print(f'Time per sample: {np.mean(mins):6.3f} pm {np.std(mins):6.3f}')
print(f'Time per sample: {np.mean(ok_samples):6.3f} pm {np.std(ok_samples):6.3f}')
