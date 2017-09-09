# Background

Many media players crash on corrupted input (not FLAC in particular). This is
bad, the decoder should signal an error on invalid input, it should not crash.
I suspect that this is partly due to the fact that most decoders are written in
C. I thought I'd try and write a decoder in a safe language: Rust. Video codecs
can be quite complex, and nowadays CPU decoding is not all that common any more.
Therefore, I decided to first try and write a decoder for an audio codec that I
love and use on a daily basis: FLAC.
