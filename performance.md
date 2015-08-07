Performance
===========

Now that Claxon can decode all the real-word FLAC files that I tried, it is time
to start thinking about performance. Currently, Claxon is about a factor 2 to 10
slower than the reference implementation (meaning the time to decode is two to
ten times as long). Running `perf stat` on my machine reveals the following:

 - RAM access is not a problem; the number of page faults does not differ
   significantly.

 - Claxon takes almost ten times as much cycles to decode the same file.

 - Claxon executes about 27 times as much branches as the reference
   implementation. Branch mispredictions are 0.66% for Claxon and 2.26% for the
   reference implementation.

 - The reference implementation executes 1.92 instructions per cycle, Claxon
   executes 1.88 instructions per cycle.

This suggests that there is a lot of room for improvement by reducing the number
of instructions that Claxon needs for decoding. In particular, the number of
branches should be reduced.
