Changelog
=========

0.2.0
-----

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

This is the initial release. It is far from complete, but something has to be
released at some point. Claxon was able to correctly decode 1.25 GiB of test
samples, so I think this is a good point for a release.

This release features correct decoding of most real-world FLAC files. It does
not fully verify that the files are correct (CRC-16 and the MD5 checksum are
not verified). It does not provide support for unencoded binary, for I never
encountered a FLAC file that utilised it. The API is very rough and will change
significantly. Finally, performance was not an explicit goal of this release.
Future work will be done towards improving it.
