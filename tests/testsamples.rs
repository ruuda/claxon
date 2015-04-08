// Claxon -- A FLAC decoding library in Rust
// Copyright (C) 2014-2015 Ruud van Asseldonk
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

#![feature(path_ext)]

extern crate claxon;
extern crate hound;

use std::fs;
use std::io;
use std::path;

fn run_metaflac(fname: &path::Path) -> String {
    use std::process::Command;

    // Run metaflac on the specified file and print all streaminfo data.
    let output = Command::new("metaflac")
                         .arg("--show-min-blocksize")
                         .arg("--show-max-blocksize")
                         .arg("--show-min-framesize")
                         .arg("--show-max-framesize")
                         .arg("--show-sample-rate")
                         .arg("--show-channels")
                         .arg("--show-bps")
                         .arg("--show-total-samples")
                         .arg("--show-md5sum")
                         .arg(fname.to_str().expect("unsupported filename"))
                         .output().ok().expect("failed to run metaflac");
    String::from_utf8(output.stdout).ok().expect("metaflac wrote invalid UTF-8")
}

fn decode_file(fname: &path::Path) {
    use std::process::Command;

    // Run the the reference flac decoder on the file.
    let success = Command::new("flac")
                          .arg("--decode")
                          .arg(fname.to_str().expect("unsupported filename"))
                          .status().ok().expect("failed to run flac")
                          .success();
    assert!(success);
}

fn print_hex(seq: &[u8]) -> String {
    let vec: Vec<String> = seq.iter().map(|x| format!("{:0>2x}", *x)).collect();
    vec.concat()
}

fn read_streaminfo(fname: &path::Path) -> String {
    use claxon::FlacStream;

    // Use a buffered reader, this speeds up the test by 120%.
    let file = fs::File::open(fname).unwrap();
    let mut reader = io::BufReader::new(file);
    let stream = FlacStream::new(&mut reader).unwrap();
    let streaminfo = stream.streaminfo();

    // Format the streaminfo in the same way that metaflac prints it.
    format!("{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n",
            streaminfo.min_block_size,
            streaminfo.max_block_size,
            streaminfo.min_frame_size.unwrap_or(0),
            streaminfo.max_frame_size.unwrap_or(0),
            streaminfo.sample_rate,
            streaminfo.n_channels,
            streaminfo.bits_per_sample,
            streaminfo.n_samples.unwrap_or(0),
            print_hex(&streaminfo.md5sum)) // TODO implement LowerHex for &[u8] and submit a PR.
}

fn compare_metaflac(fname: &path::Path) {
    let metaflac = run_metaflac(fname);
    let streaminfo = read_streaminfo(fname);
    let mut mf_lines = metaflac.lines_any();
    let mut si_lines = streaminfo.lines_any();
    while let (Some(mf), Some(si)) = (mf_lines.next(), si_lines.next()) {
        if mf != si {
            println!("metaflac\n--------\n{}", metaflac);
            println!("streaminfo\n----------\n{}", streaminfo);
            panic!("metaflac disagrees on parsed streaminfo");
        }
    };
}

fn compare_decoded_stream(fname: &path::Path) {
    use std::fs::PathExt;

    let ref_fname = fname.with_extension("wav");
    if !ref_fname.exists() {
        // TODO: actually, we might only want to run the test for files that
        // do exist already. It is fine to run streaminfo or decode-only tests
        // on thousands of files, but decoding thousands of files to wav before
        // running the tests will take a lot of space. For now, while the
        // number of samples is closer to 10 than to 100, this is fine.
        decode_file(fname);
    }

    // If the reference file does exist after decoding, we can compare it to
    // how Claxon decodes it, sample by sample.
    if ref_fname.exists() {
        let ref_file = fs::File::open(ref_fname).unwrap();
        let ref_reader = io::BufReader::new(ref_file);
        let mut ref_stream = hound::WavReader::new(ref_reader).unwrap();

        let try_file = fs::File::open(fname).unwrap();
        let mut try_reader = io::BufReader::new(try_file);
        // TODO: Claxon should take R by value, not by mut reference.
        let mut try_stream = claxon::FlacStream::new(&mut try_reader).unwrap();

        // The streaminfo test will ensure that things like bit depth and
        // sample rate match, only the actual samples are compared here.

        let mut ref_samples = ref_stream.samples::<i16>();

        let n_samples = try_stream.streaminfo().n_samples.unwrap();
        let n_channels = try_stream.streaminfo().n_channels;
        // TODO: use i32 to be generic (when Hound supports this).
        let mut blocks = try_stream.blocks::<i16>();
        let mut sample = 0u64;
        let mut b = 0u64;

        while sample < n_samples {
            let block = blocks.read_next().unwrap();
            let mut channels: Vec<_> = (0 .. n_channels)
                                       .map(|i| block.channel(i).iter().cloned())
                                       .collect();
            for i in (0 .. block.len()) {
                for ch in (0 .. n_channels as usize) {
                    let ref_sample = ref_samples.next().map(|r| r.ok().unwrap());
                    let try_sample = channels[ch].next();
                    if ref_sample != try_sample {
                        println!("disagreement at sample {} of block {} in channel {}: reference is {} but decoded is {}",
                                 i, b, ch, ref_sample.unwrap(), try_sample.unwrap());
                        panic!("decoding differs from reference decoder");
                    }
                }
            }
            sample = sample + block.len() as u64;
            b = b + 1;
        }
    }
}

fn for_each_test_sample<F: Fn(&path::Path)>(f: F) {
    use std::ffi::OsStr;
    use std::fs::PathExt;

    // Enumerate all the flac files in the testsamples directory, and execute
    // the function `f` for it.
    let dir = fs::read_dir("testsamples")
                 .ok().expect("failed to enumerate flac files");
    for path in dir {
        let path = path.ok().expect("failed to obtain path info").path();
        if path.is_file() && path.extension() == Some(OsStr::new("flac")) {
            print!("    comparing {} ...", path.to_str()
                                               .expect("unsupported filename"));
            f(&path);
            println!(" ok");
        }
    }
}

#[test]
fn verify_streaminfo() {
    for_each_test_sample(compare_metaflac);
}

#[test]
fn verify_decoded_stream() {
    for_each_test_sample(compare_decoded_stream);
}
