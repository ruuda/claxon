// Claxon -- A FLAC decoding library in Rust
// Copyright 2017 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

// This file contains a minimal example of using Claxon and Hound to decode a
// flac file. This can be done more efficiently, but it is also more verbose.
// See the `decode` example for that.

extern crate claxon;
extern crate hound;

use std::env;
use std::path::Path;

fn decode_file(fname: &Path) {
    let mut reader = claxon::FlacReader::open(fname).expect("failed to open FLAC stream");

    let spec = hound::WavSpec {
        channels: reader.streaminfo().channels as u16,
        sample_rate: reader.streaminfo().sample_rate,
        bits_per_sample: reader.streaminfo().bits_per_sample as u16,
        sample_format: hound::SampleFormat::Int,
    };

    let fname_wav = fname.with_extension("wav");
    let opt_wav_writer = hound::WavWriter::create(fname_wav, spec);
    let mut wav_writer = opt_wav_writer.expect("failed to create wav file");

    for opt_sample in reader.samples() {
        let sample = opt_sample.expect("failed to decode FLAC stream");
        wav_writer.write_sample(sample).expect("failed to write wav file");
    }
}

fn main() {
    for fname in env::args().skip(1) {
        decode_file(&Path::new(&fname));
    }
}
