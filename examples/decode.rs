// Claxon -- A FLAC decoding library in Rust
// Copyright 2015 van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

extern crate claxon;
extern crate hound;

fn main() {
    use std::env;
    use std::path;
    use claxon::FlacReader;
    use hound::{WavSpec, WavWriter};

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
    let mut output = WavWriter::create(fname_wav, spec).expect("failed to create wav file");

    for maybe_sample in reader.samples() {
        let sample = maybe_sample.expect("failed to read sample");
        output.write_sample(sample).expect("failed to write sample");
    }

    output.finalize().expect("failed to finalize wav file");
}
