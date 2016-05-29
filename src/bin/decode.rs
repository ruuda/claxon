// Claxon -- A FLAC decoding library in Rust
// Copyright 2015 van Asseldonk
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License, version 3,
// as published by the Free Software Foundation.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

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
    };
    let fname_wav = fname.with_extension("wav");
    let mut output = WavWriter::create(fname_wav, spec).expect("failed to create wav file");

    for maybe_sample in reader.samples::<i32>() {
        let sample = maybe_sample.expect("failed to read sample");
        output.write_sample(sample).expect("failed to write sample");
    }

    output.finalize().expect("failed to finalize wav file");
}
