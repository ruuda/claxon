// Claxon -- A FLAC decoding library in Rust
// Copyright 2016 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

extern crate claxon;
extern crate time;

use claxon::FlacReader;
use std::env;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use time::PreciseTime;

/// Reads a file into memory entirely.
fn read_file<P: AsRef<Path>>(path: P) -> Vec<u8> {
    let mut file = File::open(path).unwrap();
    let mut data = Vec::new();
    file.read_to_end(&mut data).unwrap();
    data
}

/// Decode a file into 32-bit integers.
///
/// For every block decoded, appends to the vector the average time spent per
/// sample for that block in nanoseconds.
///
/// Returns a pair of (total time / total samples, total bytes / total time) in
/// units (nanoseconds, bytes per second). Different channels account for
/// different samples; a stereo file of the same sample rate and duration as a
/// mono file will have twice as many samples. Bytes refers to the number of
/// input bytes.
fn decode_file(data: &[u8], sample_times_ns: &mut Vec<f64>) {
    let mut reader = FlacReader::new(Cursor::new(data)).unwrap();

    let bps = reader.streaminfo().bits_per_sample as u64;
    let num_channels = reader.streaminfo().channels;
    assert!(bps < 8 * 16);

    // Allocate a buffer once that is big enough to fit all the blocks (after
    // one another), so we never need to allocate during decoding.
    let max_block_len = reader.streaminfo().max_block_size as usize * num_channels as usize;
    let mut sample_buffer = Vec::with_capacity(max_block_len);

    let mut frame_reader = reader.blocks();
    let mut frame_epoch = PreciseTime::now();

    loop {
        match frame_reader.read_next_or_eof(sample_buffer) {
            Ok(Some(block)) => {
                // Update timing information.
                let now = PreciseTime::now();
                let duration_ns = frame_epoch.to(now).num_nanoseconds().unwrap();
                sample_times_ns.push(duration_ns as f64 / block.len() as f64);
                frame_epoch = now;

                // Recycle the buffer for the next frame. There should be a
                // black box to prevent optimizing this away, but it is only
                // available on unstable Rust, and it seems that nothing is
                // optimized away here anyway. The decode needs to happen anyway
                // to decide whether we hit the panic below.
                sample_buffer = block.into_buffer();
            },
            Ok(None) => return, // End of file.
            Err(_) => panic!("failed to decode")
        }
    }
}

fn main() {
    let fname = env::args().nth(1).expect("no file given");

    let data = read_file(fname);
    let mut sample_times_ns = Vec::with_capacity(10 * 1000);

    // Do 10 entire runs to collect more data.
    for _ in 0..10 {
        decode_file(&data, &mut sample_times_ns);
        for ns in &sample_times_ns[..] {
            println!("{:.5}", ns);
        }
        sample_times_ns.clear();
    }

}
