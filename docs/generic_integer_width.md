Generic integer width
=====================

Up to version 0.2.1, Claxon sported a `Sample` trait that allowed for generic
decoding into either `i16` or `i32` integers. If the bit depth of the source
file is 16 bit or less (which is extremely common, as CD audio is 16 bit),
decoding into an `i16` can save a bit of memory, and hopefully peformance.

However, measurements indicated that the performance win was virtually
non-existent (at least for v0.2.1, which was not optimised for speed anyway).
The memory savings are modest too because the decoder is streaming. The amount
of memory used is constant (if the block size is constant, which it usually is).
The amount of memory saved would be in the order of a few KiB.

To reduce the complexity, this feature was removed. Samples are now always
decoded into an `i32`, even if they would fit in 16 bits.

Evidence
--------

The benchmarks below were done on a Skylake i7-6700HQ clocked at 2.60 GHz, for
five different songs. These songs are all 16-bit 44100 Hz stereo, re-encoded
with Flac 1.3.1 at the highest compression level (`-8`). Reported times are the
average time per sample (stereo samples counted individually) in a block. For
these times the 0.1-quantile, 0.5-quantile (median), 0.9-quantile, and mean (μ)
are reported. The last column contains the duration for 16 bit decoding, divided
by the duration for 32 bit decoding. Reported numbers are the average and
standard deviation of five runs.

**Daft Punk -- One More Time**

|     | `i16` (ns) | `i32` (ns) | fraction      |
|-----|-----------:|-----------:|--------------:|
| p10 | 80.4 ± 1.6 | 82.0 ± 1.9 | 0.980 ± 0.029 |
| p50 | 88.3 ± 0.9 | 90.0 ± 2.0 | 0.981 ± 0.024 |
| p90 | 95.7 ± 3.9 | 99.5 ± 4.4 | 0.963 ± 0.058 |
| μ   | 88.3 ± 1.5 | 90.4 ± 2.1 | 0.977 ± 0.028 |

**Deadmau5 -- I Remember**

|     | `i16` (ns) | `i32` (ns) | fraction      |
|-----|-----------:|-----------:|---------------|
| p10 | 74.3 ± 1.3 | 74.3 ± 0.1 | 1.000 ± 0.018 |
| p50 | 85.4 ± 1.4 | 85.6 ± 0.3 | 0.998 ± 0.016 |
| p90 | 95.9 ± 3.1 | 95.3 ± 0.5 | 1.007 ± 0.033 |
| μ   | 85.3 ± 1.7 | 85.3 ± 0.3 | 1.001 ± 0.020 |

**Massive Attack -- Unfinished Sympathy**

|     | `i16` (ns) | `i32` (ns) | fraction      |
|-----|-----------:|-----------:|--------------:|
| p10 | 75.2 ± 0.1 | 75.9 ± 0.1 | 0.990 ± 0.002 |
| p50 | 82.7 ± 0.1 | 83.6 ± 0.3 | 0.990 ± 0.004 |
| p90 | 88.7 ± 0.1 | 89.7 ± 0.6 | 0.989 ± 0.006 |
| μ   | 82.5 ± 0.2 | 83.3 ± 0.3 | 0.990 ± 0.004 |

**Muse -- Starlight**

|     | `i16` (ns) | `i32` (ns) | fraction      |
|-----|-----------:|-----------:|--------------:|
| p10 | 78.2 ± 0.5 | 78.9 ± 0.4 | 0.991 ± 0.008 |
| p50 | 85.6 ± 1.2 | 86.0 ± 1.3 | 0.995 ± 0.021 |
| p90 | 95.2 ± 2.5 | 95.6 ± 4.7 | 0.995 ± 0.056 |
| μ   | 86.7 ± 1.4 | 86.8 ± 2.0 | 0.999 ± 0.028 |

**U2 -- Sunday Bloody Sunday**

|     | `i16` (ns) | `i32` (ns) | fraction      |
|-----|-----------:|-----------:|--------------:|
| p10 | 87.3 ± 0.4 | 87.2 ± 0.0 | 1.001 ± 0.004 |
| p50 | 93.7 ± 0.1 | 93.7 ± 0.0 | 1.000 ± 0.001 |
| p90 | 96.2 ± 0.1 | 96.2 ± 0.0 | 1.000 ± 0.001 |
| μ   | 92.4 ± 0.2 | 92.3 ± 0.0 | 1.001 ± 0.002 |

The effect of the different integer widths differ per file, but they are
universally small.
