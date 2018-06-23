// Claxon -- A FLAC decoding library in Rust
// Copyright 2017 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

// This file contains a minimal example of using Claxon to read metadata tags
// (also called Vorbis comments) from a flac file. It behaves similarly to
// `metaflac --block-type=VORBIS_COMMENT --list <file>`.

extern crate claxon;

use std::env;

fn main() {
    for fname in env::args().skip(1) {
        let mut reader = claxon::MetadataReader::open(&fname).expect("failed to open FLAC stream");
        let tags = reader.next_vorbis_comment().expect("failed to read metadata");

        // We can iterate directly over all tags. When looking for a specific
        // tag, `OptionalVorbisComment::get_tag()` may be useful instead.
        for (name, value) in &tags {
            // Print comments in a format similar to what
            // `metaflac --block-type=VORBIS_COMMENT --list` would print.
            println!("{}: {}={}", fname, name, value);
        }
    }
}
