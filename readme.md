Claxon
======

A FLAC decoding library in Rust.

[![Build Status][ci-img]][ci]
[![Crates.io version][crate-img]][crate]

Many media players crash on corrupted input (not FLAC in particular). This is
bad, the decoder should signal an error on invalid input, it should not crash.
I suspect that this is partly due to the fact that most decoders are written in
C. I thought I'd try and write a decoder in a safe language: Rust. Video codecs
can be quite complex, and nowadays CPU decoding is not all that common any more.
Therefore, I decided to first try and write a decoder for an audio codec that I
love and use on a daily basis: FLAC.

License
-------
Claxon is licensed under the [Apache 2.0][apache2] license. It may be used in
free software as well as closed-source applications, both for commercial and
non-commercial use under the conditions given in the license. If you want to
use Claxon in your GPLv2-licensed software, you can add an [exception][except]
to your copyright notice.

[ci-img]:    https://travis-ci.org/ruuda/claxon.svg?branch=master
[ci]:        https://travis-ci.org/ruuda/claxon
[crate-img]: http://img.shields.io/crates/v/claxon.svg
[crate]:     https://crates.io/crates/claxon
[apache2]:   https://www.apache.org/licenses/LICENSE-2.0
[except]:    https://www.gnu.org/licenses/gpl-faq.html#GPLIncompatibleLibs
