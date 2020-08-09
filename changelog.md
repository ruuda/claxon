Changelog
=========

0.4.3
-----

Released 2020-08-09.

**Compatibility**:

- No breaking changes
- Compatible with Rust 1.13 through 1.46.

New features:

- `metadata::Tags` now implements `ExactSizeIterator`.
- Skip empty Vorbis comments that would previously cause a parse error.

Thanks to har0ke and Anthony Mikh for contributing to this release.

0.4.2 and 0.3.3
---------------

Released 2019-05-19.

**Compatibility**:

- No breaking changes.
- Compatible with Rust 1.13 through 1.34.

New features:

- Claxon can now decode non-subset files that use an LPC order > 12. Decoding
  such a file would previously return `Error::Unsupported`.

Internal changes:

- Defensively zero the decode buffer when growing it.

0.4.1 and 0.3.2
---------------

Released 2018-08-25.

This is a bugfix release that addresses a security issue that had been present
in Claxon since its initial release.

**Compatibility**:

- No breaking changes.
- Compatible with Rust 1.13 through 1.28.

Bugs fixed:

- A bug where uninitialized memory or previous decode buffer contents could be
  exposed when decoding a maliciously crafted file has been fixed.

Thanks Sergey "Shnatsel" Davidoff for identifying the bug.

0.4.0
-----

Released 2017-12-02.

**Breaking changes**:

- The unused `Error::TooWide` variant has been removed.
- Files with metadata blocks larger than 10 MiB are now rejected to protect
  against memory allocation denial of service attacks.

Release highlights:

- Support for reading metadata (Vorbis comments, also known as FLAC tags) has
  been added.
- Claxon can now avoid reading metadata blocks when reading them is not desired,
  see the new `FlacReader::new_ext()` constructor.
- Functionality for reading FLAC embedded in an ogg or mp4 container has been
  added, together with new examples. This functionality is in an early stage and
  will likely change in future versions.
- Ensures compatibility with Rust 1.13 through 1.22.

0.3.1
-----

Released 2017-06-06.

This is a bugfix release. Claxon has been fuzzed, and all issues discovered have
been fixed. Claxon has also been verified against the reference decoder on more
than 11000 real-world FLAC files.

**Breaking changes**:

- None.

Release highlights:

- Fuzzing targets for libFuzzer have been added. All crashes and hangs
  discovered have been fixed.
- `StreamInfo` now implements `fmt::Debug`.
- A specialized format error message was added for ID3-prefixed streams.
- Skipping over an “application” metadata block is now faster.
- The test suite will no longer write intermediate wav files.
- Ensures compatibility with Rust 1.13 through 1.17.

Bugs fixed:

- Files where the length of the last block is less than 16 samples are now
  decoded correctly.
- Subframes with wasted bits are now decoded correctly.
- Decoding of pathological ill-formed files will now report an error,
  rather than crashing.
- A panic due to index out of bounds has been fixed.
- Arithmetic overflow bugs have been fixed.

0.3.0
-----

Released 2017-02-07.

This release focuses on performance. Internally quite a lot has happened, and a
few changes were made to the API too, in order to support faster decoding, and
to make the API more consistent.

**Breaking changes**:

- Support for generic integer widths has been removed, along with the `Sample`
  trait. Samples are now always `i32`. This is both simpler and faster.
- Many functions now return a `u32`, even if the actual range of legal values
  would fit in a narrower integer.
- `Block::len()` now returns the total number of samples for all channels, not
  the number of inter-channel samples. To get the number of inter-channel
  samples, use the new `Block::duration()`.

Release highlights:

- The `decode` example is now 7.5 times faster. Performance is close to that of
  the reference implementation.
- Ensures compatibility with Rust 1.13 through 1.15.
- Documentation is now hosted on docs.rs. (Thanks, docs.rs authors!)
- Scripts for benchmarking two revisions, and for benchmarking against libflac,
  are now included.
- A `Block::stereo_samples()` iterator has been added for convenience and
  performance.

0.2.1
-----

Released 2016-08-27.

Release highlights:

- Ensures compatibility with Rust 1.13. Older versions are not supported, but
  once Rust 1.13 becomes stable, Claxon will become usable with stable Rust.
- The example was upgraded to Hound 2.0.0.

0.2.0
-----

Released 2016-05-29.

Release highlights:

- Claxon now verifies the CRC of the frames it decodes.
- Claxon now recognises the end of a stream, instead of returning an
  error when you try to decode past it.
- Testing with sample files was automated for continuous integration.
- A samples iterator was added as a simple convenient way to decode.
- The `Error` type was simplified and it now implements `error::Error`.
- The `UnexpectedEof` error kind is now used when appropriate.
- `FlacStream` was renamed to `FlacReader`.
- `claxon::FlacResult` was renamed to `claxon::Result`.
- A `FlacReader` can now be constructed directly from a filename.
- Readers now take the underlying reader by value.
- Streaminfo is now returned by value, not by reference.
- Claxon is now licensed under the Apache 2.0 license.

Overall, this release tries to to make the API easier to use, and it
improves consistency with Hound. There are a few breaking changes,
although they should not be hard to resolve. The `samples()` iterator is
not expected to change by a lot any more. More changes might be required
to make the lower-level block decoding API pleasant to use.

0.1.0
-----

Released 2015-06-06.

This is the initial release. It is far from complete, but something has to be
released at some point. Claxon was able to correctly decode 1.25 GiB of test
samples, so I think this is a good point for a release.

This release features correct decoding of most real-world FLAC files. It does
not fully verify that the files are correct (CRC-16 and the MD5 checksum are
not verified). It does not provide support for unencoded binary, for I never
encountered a FLAC file that utilised it. The API is very rough and will change
significantly. Finally, performance was not an explicit goal of this release.
Future work will be done towards improving it.
