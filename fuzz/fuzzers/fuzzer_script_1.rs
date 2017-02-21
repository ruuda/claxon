// Claxon -- A FLAC decoding library in Rust
// Copyright 2017 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

#![no_main]

extern crate fuzzer_sys;
extern crate claxon;

use std::slice;
use std::io::Cursor;

#[export_name="LLVMFuzzerTestOneInput"]
pub extern fn go(data: *const u8, size: isize) -> i32 {
    let data_slice = unsafe { slice::from_raw_parts(data, size as usize) };
    let cursor = Cursor::new(data_slice);
    let mut reader = match claxon::FlacReader::new(cursor) {
        Ok(r) => r,
        Err(..) => return 0,
    };
    let mut csum = 0;
    for sample in reader.samples() {
        match sample {
            Ok(s) => csum ^= s,
            Err(..) => return 0,
        }
    }

    // TODO: Do I need to consume a variable somewhere, or call test::black_box?

    0
}
