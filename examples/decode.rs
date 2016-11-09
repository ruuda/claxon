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
use std::path;

fn main() {
    let arg = env::args().nth(1).expect("no file given");
    let fname = path::Path::new(&arg);
    let mut reader = FlacReader::open(fname).expect("failed to open FLAC stream");

    let spec = WavSpec {
        // TODO: u8 for channels, is that weird? Would u32 be better?
        channels: reader.streaminfo().channels as u16,
        sample_rate: reader.streaminfo().sample_rate,
        // TODO: again, would u32 be better, even if the range is smaller?
        bits_per_sample: reader.streaminfo().bits_per_sample as u16,
        sample_format: hound::SampleFormat::Int,
    };
    let fname_wav = fname.with_extension("wav");
    let mut wav_writer = WavWriter::create(fname_wav, spec).expect("failed to create wav file");
    {
        // TODO: Write fallback for other sample widths and channel numbers.
        assert!(reader.streaminfo().bits_per_sample == 16);
        assert!(reader.streaminfo().channels == 2);

        // TODO: block_size could be confusing. Call it duration instead?
        let max_bs_len = reader.streaminfo().max_block_size as u32 * reader.streaminfo().channels as u32;
        let mut sample_writer = wav_writer.get_i16_writer(max_bs_len);

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
            for s in block.stereo_samples() {
                sample_writer.write_sample(s);
            }

            sample_writer.flush().expect("failed to write samples to wav file");
        }
    }

    wav_writer.finalize().expect("failed to finalize wav file");
}
