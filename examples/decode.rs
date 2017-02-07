// Claxon -- A FLAC decoding library in Rust
// Copyright 2015 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

// This file implements a decoder, like the reference `flac -d`. It is fast, but
// being fast requires dealing with a few details of the FLAC format. There is
// also a simpler example, `decode_simple`, which is less verbose.

extern crate claxon;
extern crate hound;

use claxon::{Block, FlacReader};
use hound::{WavSpec, WavWriter};
use std::env;
use std::path::Path;

fn decode_file(fname: &Path) {
    let mut reader = FlacReader::open(fname).expect("failed to open FLAC stream");

    // TODO: Write fallback for other sample widths and channel numbers.
    assert!(reader.streaminfo().bits_per_sample == 16);
    assert!(reader.streaminfo().channels == 2);

    let spec = WavSpec {
        channels: reader.streaminfo().channels as u16,
        sample_rate: reader.streaminfo().sample_rate,
        bits_per_sample: reader.streaminfo().bits_per_sample as u16,
        sample_format: hound::SampleFormat::Int,
    };

    let fname_wav = fname.with_extension("wav");
    let mut wav_writer = WavWriter::create(fname_wav, spec).expect("failed to create wav file");

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

        let mut sample_writer = wav_writer.get_i16_writer(block.duration() * 2);

        // Write the samples in the block to the wav file, channels interleaved.
        for (left, right) in block.stereo_samples() {
            // The `stereo_samples()` iterator does not yield more samples
            // than the duration of the block, so we never write more
            // samples to the writer than requested, hence using the
            // unchecked functions is safe here.
            unsafe {
                sample_writer.write_sample_unchecked(left);
                sample_writer.write_sample_unchecked(right);
            }
        }

        sample_writer.flush().expect("failed to write samples to wav file");
    }

    wav_writer.finalize().expect("failed to finalize wav file");
}

fn main() {
    let mut no_args = true;

    for fname in env::args().skip(1) {
        no_args = false;

        print!("{}", fname);
        decode_file(&Path::new(&fname));
        println!(": done");
    }

    if no_args {
        println!("no files to decode");
    }
}
