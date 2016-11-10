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
fn decode_file(data: &[u8], sample_times_ns: &mut Vec<f64>) -> (f64, f64) {
    // To get the real number of data bytes (excluding metadata -- which can be
    // quite big due to album art), construct a new reader around a cursor, but
    // extract the cursor immediately afterwards. This will read the metadata
    // but not yet any audio data. The position of the cursor is then the amount
    // of metadata bytes.
    let data_bytes = data.len() - FlacReader::new(Cursor::new(data))
        .unwrap()
        .into_inner()
        .position() as usize;

    let cursor = Cursor::new(data);
    let mut reader = FlacReader::new(cursor).unwrap();

    let bps = reader.streaminfo().bits_per_sample as u64;
    let num_channels = reader.streaminfo().channels;
    let num_samples = reader.streaminfo().samples.unwrap() as i64 * num_channels as i64;
    assert!(bps < 8 * 16);

    // Allocate a buffer once that is big enough to fit all the blocks (after
    // one another), so we never need to allocate during decoding.
    let max_block_len = reader.streaminfo().max_block_size as usize * num_channels as usize;
    let mut sample_buffer = Vec::with_capacity(max_block_len);

    let mut frame_reader = reader.blocks();
    let mut frame_epoch = PreciseTime::now();
    let epoch = frame_epoch;

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
            Ok(None) => break, // End of file.
            Err(_) => panic!("failed to decode")
        }
    }

    let total_duration_ns = epoch.to(PreciseTime::now()).num_nanoseconds().unwrap();
    let ns_per_sample = total_duration_ns as f64 / num_samples as f64;
    let bytes_per_sec = data_bytes as f64 * 1000_000_000.0 / total_duration_ns as f64;
    (ns_per_sample, bytes_per_sec)
}

fn print_stats(sample_times_ns: &mut Vec<f64>, stats_pair: (f64, f64)) {
    let (ns_per_sample, bytes_per_sec) = stats_pair;
    sample_times_ns.sort_by(|x, y| x.partial_cmp(y).unwrap());

    let p10 = sample_times_ns[10 * sample_times_ns.len() / 100];
    let p50 = sample_times_ns[50 * sample_times_ns.len() / 100];
    let p90 = sample_times_ns[90 * sample_times_ns.len() / 100];

    println!("{:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2}",
             p10, p50, p90, ns_per_sample, bytes_per_sec / 1024.0 / 1024.0);
}

fn main() {
    let fname = env::args().nth(1).expect("no file given");

    let data = read_file(fname);
    let mut sample_times_ns = Vec::new();

    // Do a few runs to get more robust statistics.
    for _ in 0..5 {
        let stats_pair = decode_file(&data, &mut sample_times_ns);
        print_stats(&mut sample_times_ns, stats_pair);
        sample_times_ns.clear();
    }
}
