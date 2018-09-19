#!/usr/bin/env python3

"""
Prints a summary of sample times recorded by benchmark.sh.

Usage:
    tools/summarize.py measurements/basename_sha.dat
"""

import numpy as np
import sys

data = np.genfromtxt(sys.argv[1])

for num_chunks in range(1, 10):

    # Take the minimum time of all of the recorded times that we have for a block.
    # This is the time undisturbed by noise.
    mins = np.min(data, axis=1)

    chunk_mins = np.array([
        np.min(chunk, axis=1) for chunk in np.array_split(data, num_chunks, axis=1)
    ])

    print(f'{num_chunks} chunks')
    for q in (0.05, 0.50, 0.95):
        values = np.quantile(chunk_mins, q, axis=1)
        mean = np.mean(values)
        sd = np.std(values)
        print(f'  q{q:0.2f}: {mean:0.3f} pm {sd:0.03f}')

q05 = np.quantile(mins, 0.05)
q50 = np.quantile(mins, 0.50)
q95 = np.quantile(mins, 0.95)

print(f'Time per sample: {q50:.3f} ns ({q05:.3f} .. {q95:.3f})')
