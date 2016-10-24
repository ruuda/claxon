// Claxon -- A FLAC decoding library in Rust
// Copyright 2016 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

#![feature(test)]

extern crate claxon;
extern crate test;

use std::env;
use std::path::Path;
use std::fs::File;
use std::io::{Cursor, Read};
use claxon::FlacReader;

/// Reads a file into memory entirely.
fn read_file<P: AsRef<Path>>(path: P) -> Vec<u8> {
    let mut file = File::open(path).unwrap();
    let mut data = Vec::new();
    file.read_to_end(&mut data).unwrap();
    data
}

/// Decode a file into 16-bit integers.
///
/// This consumes the decoded samples into a black box.
fn decode_file_i16(data: &[u8]) {
    let cursor = Cursor::new(data);
    let mut reader = FlacReader::new(cursor).unwrap();

    let bps = reader.streaminfo().bits_per_sample as u64;
    assert!(bps < 8 * 16);

    for sample in reader.samples::<i16>() {
        test::black_box(sample.unwrap());
    }
}

fn main() {
    let bits = env::args().nth(1).expect("no bit depth given");
    let fname = env::args().nth(2).expect("no file given");

    let data = read_file(fname);
    if bits == "16" {
        // TODO: Do several passes and report timing information.
        decode_file_i16(&data);
    } else if bits == "32" {
        // TODO
    } else {
        panic!("expected bit depth of 16 or 32");
    }
}
