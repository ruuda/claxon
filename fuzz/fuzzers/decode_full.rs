// Claxon -- A FLAC decoding library in Rust
// Copyright 2017 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

#![no_main]

extern crate libfuzzer_sys;
extern crate claxon;

use std::io::Cursor;

#[export_name="rust_fuzzer_test_input"]
pub extern fn go(data: &[u8]) {
    let cursor = Cursor::new(data);
    let mut reader = match claxon::FlacReader::new(cursor) {
        Ok(r) => r,
        Err(..) => return,
    };

    for sample in reader.samples() {
        match sample {
            Ok(..) => { }
            Err(..) => return,
        }
    }
}
