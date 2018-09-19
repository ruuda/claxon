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

    let mut sample_times_ns_per_run = Vec::new();

    // Decode the file 25 times. We will take the minium time per block for
    // every decoded block. Note: repeating 25 times is not a luxury. When we
    // take the minimum over num_runs runs, and repeat that 5 times, then the
    // difference between the minimum of the 5 reported block times (and the
    // reported values are already a minimum over num_runs runs) and the maximum
    // reported minimum is substantial, see the table below. Interestingly when
    // plotting this max-min difference for num_runs = 5, the histogram looks
    // bimodal, with a big peak around 0.2 ns, and then a smaller peak around
    // 0.9 ns. From num_runs = 10 on, the second peak goes away. As we increase
    // the number of runs then, the histogram of deltas starts to look like a
    // gamma distribution, with lower and lower scale parameter. Further still,
    // the distribution starts to look sharply peaked around 0.05 ns. Note:
    // rather than doing 25 runs here and 5 runs of this program, we can also do
    // 20 runs here and 7 rounds.
    // | num_runs | median(max - min) | mean(max - min) |
    // |        5 |          0.090 ns |        0.152 ns |
    // |       10 |          0.065 ns |        0.073 ns |
    // |       15 |          0.054 ns |        0.064 ns |
    // |       20 |          0.059 ns |        0.072 ns |
    // |       25 |          0.051 ns |        0.052 ns |
    let num_runs = 20;
    for _ in 0..num_runs {
        let mut sample_times_ns = Vec::with_capacity(10 * 1000);
        decode_file(&data, &mut sample_times_ns);
        sample_times_ns_per_run.push(sample_times_ns);
    }

    // For every block, print all of the samples times (in nanoseconds) on one
    // row. Then later on we take the minimum over the row to get the sample
    // time for the block. This should rule out incidental sources of noise,
    // such as interrupts, or a trashed cache. There can still be variation in
    // decode times among blocks, because some require more expensive operations
    // than others. But the same block decoded multiple times should have a
    // stable baseline, and anything on top of that is noise. This means that we
    // report an absolute best-case decode time, which may not be a typical
    // decode time. But it also means that what we measure is truly the decode
    // time, and not external influences.
    let num_blocks = sample_times_ns_per_run[0].len();
    for i in 0..num_blocks {
        for r in 0..num_runs {
            print!("{:.6}", sample_times_ns_per_run[r][i]);
            if r == num_runs - 1 {
                print!("\n");
            } else {
                print!("\t");
            }
        }
    }
}
