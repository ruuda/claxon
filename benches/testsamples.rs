// Claxon -- A FLAC decoding library in Rust
// Copyright 2014 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

#![feature(test)]

extern crate claxon;
extern crate test;

use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use test::Bencher;

/// Replace the reader with one that starts again from the beginning.
fn refresh_reader(reader: claxon::FlacReader<Cursor<Vec<u8>>>) -> claxon::FlacReader<Cursor<Vec<u8>>> {
    let cursor = reader.into_inner();
    let vec = cursor.into_inner();
    let new_cursor = Cursor::new(vec);
    claxon::FlacReader::new(new_cursor).unwrap()
}

fn bench_decode<P: AsRef<Path>>(path: P, bencher: &mut Bencher) {
    // Read the file into memory. We want to measure decode speed, not IO
    // overhead.
    let mut file = File::open(path).unwrap();
    let mut data = Vec::new();
    file.read_to_end(&mut data).unwrap();
    let cursor = Cursor::new(data);

    let mut reader = claxon::FlacReader::new(cursor).unwrap();

    let bps = reader.streaminfo().bits_per_sample as u32;
    assert!(bps < 8 * 16);

    let mut bytes = 0u32;
    let mut buffer = Vec::new();
    let mut iterations = 0;
    bencher.iter(|| {
        let mut should_refresh = true;
        {
            let mut blocks = reader.blocks();
            let stolen_buffer = std::mem::replace(&mut buffer, Vec::new());
            let block = blocks.read_next_or_eof(stolen_buffer).expect("decode error");
            if let Some(b) = block {
                bytes += b.len() * (bps / 8);
                iterations += 1;
                buffer = test::black_box(b.into_buffer());
                should_refresh = false;
            }
        }
        if should_refresh {
            // We decoded until the end, but the bencher wants to measure more
            // still. Re-create a new FlacReader and start over then.
            reader = refresh_reader(reader);
        }
    });

    // The `bytes` field of the bencher indicates the number of bytes *per
    // iteration*, not the total number of bytes.
    bencher.bytes = (bytes as u64) / iterations;
}

#[bench]
fn bench_p0_mono_16bit(bencher: &mut Bencher) {
    bench_decode("testsamples/p0.flac", bencher);
}

#[bench]
fn bench_p1_stereo_24bit(bencher: &mut Bencher) {
    bench_decode("testsamples/p1.flac", bencher);
}

#[bench]
fn bench_p2_stereo_16bit(bencher: &mut Bencher) {
    bench_decode("testsamples/p2.flac", bencher);
}

#[bench]
fn bench_p3_stereo_16bit(bencher: &mut Bencher) {
    bench_decode("testsamples/p3.flac", bencher);
}

#[bench]
fn bench_p4_stereo_16bit(bencher: &mut Bencher) {
    bench_decode("testsamples/p4.flac", bencher);
}
