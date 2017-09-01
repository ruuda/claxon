// Claxon -- A FLAC decoding library in Rust
// Copyright 2017 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

// This file contains a minimal example of using Claxon and Hound to decode a
// flac file. This can be done more efficiently, but it is also more verbose.
// See the `decode` example for that.

extern crate claxon;

use std::env;

fn main() {
    for fname in env::args().skip(1) {
        let reader = claxon::FlacReader::open(&fname).expect("failed to open FLAC stream");

        // We can use `tags()` to iterate over all tags. When looking for a
        // specific tag, `get_tag()` may be useful instead.
        for (name, value) in reader.tags() {
            // Print comments in a format similar to what
            // `metaflac --block-type=VORBIS_COMMENT --list` would print.
            println!("{}: {}={}", fname, name, value);
        }
    }
}
