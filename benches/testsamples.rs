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

fn bench_decode_i16(path: &Path, bencher: &mut Bencher) {
    // Read the file into memory. We want to measure decode speed, not IO
    // overhead.
    let mut file = File::open(path).unwrap();
    let mut data = Vec::new();
    file.read_to_end(&mut data).unwrap();
    let cursor = Cursor::new(data);

    let mut reader = claxon::FlacReader::new(cursor).unwrap();

    let bps = reader.streaminfo().bits_per_sample as u64;
    assert!(bps < 8 * 16);

    let mut samples = reader.samples::<i16>();
    let mut bytes = 0u64;
    bencher.iter(|| {
        for _ in 0..1024 {
            let sample = samples.next().unwrap().unwrap();
            test::black_box(sample);
        }
        bytes += 1024 * bps / 8;
    });
    bencher.bytes = bytes;
}

fn bench_decode_i32(path: &Path, bencher: &mut Bencher) {
    // This function is identical to `bench_decode_i16`, except it decodes into
    // i32s, so it can also deal with 24-bit audio.
    let mut file = File::open(path).unwrap();
    let mut data = Vec::new();
    file.read_to_end(&mut data).unwrap();
    let cursor = Cursor::new(data);

    let mut reader = claxon::FlacReader::new(cursor).unwrap();

    let bps = reader.streaminfo().bits_per_sample as u64;
    assert!(bps < 8 * 32);

    let mut samples = reader.samples::<i32>();
    let mut bytes = 0u64;
    bencher.iter(|| {
        for _ in 0..1024 {
            let sample = samples.next().unwrap().unwrap();
            test::black_box(sample);
        }
        bytes += 1024 * bps / 8;
    });
    bencher.bytes = bytes;
}

#[bench]
fn bench_p0_mono_16bit(bencher: &mut Bencher) {
    bench_decode_i16(Path::new("testsamples/p0.flac"), bencher);
}

#[bench]
fn bench_p0_mono_16bit_as_i32(bencher: &mut Bencher) {
    bench_decode_i32(Path::new("testsamples/p0.flac"), bencher);
}

#[bench]
fn bench_p1_stereo_24bit(bencher: &mut Bencher) {
    bench_decode_i32(Path::new("testsamples/p1.flac"), bencher);
}

#[bench]
fn bench_p2_stereo_16bit(bencher: &mut Bencher) {
    bench_decode_i16(Path::new("testsamples/p2.flac"), bencher);
}

#[bench]
fn bench_p2_stereo_16bit_as_i32(bencher: &mut Bencher) {
    bench_decode_i32(Path::new("testsamples/p2.flac"), bencher);
}

#[bench]
fn bench_p3_stereo_16bit(bencher: &mut Bencher) {
    bench_decode_i16(Path::new("testsamples/p3.flac"), bencher);
}

#[bench]
fn bench_p3_stereo_16bit_as_i32(bencher: &mut Bencher) {
    bench_decode_i32(Path::new("testsamples/p3.flac"), bencher);
}

#[bench]
fn bench_p4_stereo_16bit(bencher: &mut Bencher) {
    bench_decode_i16(Path::new("testsamples/p4.flac"), bencher);
}

#[bench]
fn bench_p4_stereo_16bit_as_i32(bencher: &mut Bencher) {
    bench_decode_i32(Path::new("testsamples/p4.flac"), bencher);
}
