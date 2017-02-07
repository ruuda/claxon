You can put (or symlink) FLAC files in the "extra" directory, and the tests will
be run on all files in that directory. This allows the decoder to be tested on
thousands of files, which is at least an indication that it is correct. The
benchmark script will also use these files for benchmarking.

For convenience, but mainly for automated testing, the script populate.sh will
download five audio files from archive.org (about 126 MiB total). No copyright
or trademark infringement is intended in downloading these works. The files
comprise several sample rates, bit depths, and channel counts. Some of them
contain metadata. Let’s hope that these files are a representative sample of
real-world FLAC files.

The populated test files are hard-coded in the tests so they can run in
parallel, and provide better feedback.
