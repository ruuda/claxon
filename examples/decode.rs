// Claxon -- A FLAC decoding library in Rust
// Copyright 2015 van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

extern crate claxon;
extern crate hound;

use claxon::{Block, FlacReader};
use hound::{WavSpec, WavWriter};
use std::env;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path;

fn main() {
    let arg = env::args().nth(1).expect("no file given");
    let fname = path::Path::new(&arg);
    let mut file = File::open(fname).expect("failed to open FLAC file");

    // Load the entire file into memory at once. This allows using a cursor
    // afterwards to read the data, which is cheaper than mixing IO with the
    // actual FLAC reading due to inlining.
    let mut bytes = Vec::new();
    bytes.reserve(file.metadata().unwrap().len() as usize);
    file.read_to_end(&mut bytes).expect("failed to read FLAC data into memory");

    let data = Cursor::new(bytes);
    let mut reader = FlacReader::new(data).expect("failed to open FLAC stream");

    let spec = WavSpec {
        // TODO: u8 for channels, is that weird? Would u32 be better?
        channels: reader.streaminfo().channels as u16,
        sample_rate: reader.streaminfo().sample_rate,
        // TODO: again, would u32 be better, even if the range is smaller?
        bits_per_sample: reader.streaminfo().bits_per_sample as u16,
        sample_format: hound::SampleFormat::Int,
    };
    let fname_wav = fname.with_extension("wav");
    let mut output = WavWriter::create(fname_wav, spec).expect("failed to create wav file");

    let mut frame_reader = reader.blocks();
    let mut block = Block::empty();
    loop {
        // Read a single frame. Recycle the buffer from the previous frame to
        // avoid allocations as much as possible.
        match frame_reader.read_next_or_eof(block.into_buffer()) {
            Ok(Some(next_block)) => block = next_block,
            Ok(None) => break, // EOF.
            Err(error) => panic!("{}", error),
        }

        // Write the samples in the block to the wav file, channels interleaved.
        for s in 0..block.duration() {
            for ch in 0..block.channels() {
                output.write_sample(block.sample(ch, s))
                      .expect("failed to write to wav file");
            }
        }
    }

    output.finalize().expect("failed to finalize wav file");
}
