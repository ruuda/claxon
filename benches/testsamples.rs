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

fn bench_decode(path: &Path, bencher: &mut Bencher) {
    // Read the file into memory. We want to measure decode speed, not IO
    // overhead. (There is no stable form of mmap in Rust that I know of, so
    // we read manually.)
    let mut file = File::open(path).unwrap();
    let mut data = Vec::new();
    file.read_to_end(&mut data).unwrap();
    let cursor = Cursor::new(data);

    let mut reader = claxon::FlacReader::new(cursor).unwrap();

    let bps = reader.streaminfo().bits_per_sample as u64;
    let channels = reader.streaminfo().channels as u64;
    let bytes_per_sample = channels * (bps / 8);

    // Use the more space-efficient 16-bit integers if it is sufficient,
    // otherwise decode into 32-bit integers, which is always sufficient.
    // TODO: If the closure gets called more often than the number of blocks
    // in the file, the measurement is wrong. When `blocks` implements
    // `Iterator`, we can assume values and panic on `None`.
    match bps {
        n if n <= 16 => {
            let mut blocks = reader.blocks::<i16>();
            let mut bytes = 0u64;
            bencher.iter(|| {
                let block = blocks.read_next_or_eof(Vec::new()).unwrap().unwrap();
                test::black_box(block.channel(0));
                bytes += bytes_per_sample * block.len() as u64;
            });
            bencher.bytes = bytes;
        }
        _ => {
            let mut blocks = reader.blocks::<i32>();
            let mut bytes = 0u64;
            bencher.iter(|| {
                let block = blocks.read_next_or_eof(Vec::new()).unwrap().unwrap();
                test::black_box(block.channel(0));
                bytes += bytes_per_sample * block.len() as u64;
            });
            bencher.bytes = bytes;
        }
    }
}

#[bench]
fn bench_p0_mono_16bit(bencher: &mut Bencher) {
    bench_decode(Path::new("testsamples/p0.flac"), bencher);
}

#[bench]
fn bench_p1_stereo_24bit(bencher: &mut Bencher) {
    bench_decode(Path::new("testsamples/p1.flac"), bencher);
}

#[bench]
fn bench_p2_stereo_16bit(bencher: &mut Bencher) {
    bench_decode(Path::new("testsamples/p2.flac"), bencher);
}

#[bench]
fn bench_p3_stereo_16bit(bencher: &mut Bencher) {
    bench_decode(Path::new("testsamples/p3.flac"), bencher);
}

#[bench]
fn bench_p4_stereo_16bit(bencher: &mut Bencher) {
    bench_decode(Path::new("testsamples/p4.flac"), bencher);
}
