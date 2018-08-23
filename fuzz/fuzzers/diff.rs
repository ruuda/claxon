// Claxon -- A FLAC decoding library in Rust
// Copyright 2018 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

#![no_main]

extern crate libfuzzer_sys;
extern crate claxon;

use std::io;
use std::io::Seek;
use std::iter;

#[export_name="rust_fuzzer_test_input"]
pub extern fn go(data: &[u8]) {
    // We want two distinct marker bytes.
    if data.len() < 2 || data[0] == data[1] {
        return
    }

    let mut cursor = io::Cursor::new(&data[2..]);

    // Allocate two large buffers filled with a marker byte. We will decode one
    // block twice, once into each buffer. If both decodes are successful, then
    // the two outputs should be identical. If we don't overwrite parts of the
    // buffer, then we would see a difference in the marker byte. The buffer
    // allocated up front should be large enough that Claxon does not need to
    // allocate a new one.
    let buffer0: Vec<i32> = iter::repeat(data[0] as i32).take(1024 * 1024 * 4).collect();
    let buffer1: Vec<i32> = iter::repeat(data[1] as i32).take(1024 * 1024 * 4).collect();

    let result0 = {
        let mut reader = match claxon::FlacReader::new(&mut cursor) {
            Ok(r) => r,
            Err(..) => return,
        };

        match reader.blocks().read_next_or_eof(buffer0) {
            Ok(Some(block)) => Some(block.into_buffer()),
            _ => None,
        }
    };

    cursor.seek(io::SeekFrom::Start(0)).unwrap();

    let result1 = {
        let mut reader = match claxon::FlacReader::new(&mut cursor) {
            Ok(r) => r,
            Err(..) => panic!("First time decoded fine, second time should too."),
        };

        match reader.blocks().read_next_or_eof(buffer1) {
            Ok(Some(block)) => Some(block.into_buffer()),
            _ => None,
        }
    };

    assert_eq!(result0, result1);
}
