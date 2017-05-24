// Claxon -- A FLAC decoding library in Rust
// Copyright 2014 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

extern crate claxon;
extern crate hound;

use std::fs;
use std::io;
use std::path::Path;

fn run_metaflac<P: AsRef<Path>>(fname: P) -> String {
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
                     .arg(fname.as_ref().to_str().expect("unsupported filename"))
                     .output()
                     .expect("failed to run metaflac");
    String::from_utf8(output.stdout).expect("metaflac wrote invalid UTF-8")
}

fn decode_file<P: AsRef<Path>>(fname: P) {
    use std::process::Command;

    // Run the the reference flac decoder on the file.
    let success = Command::new("flac")
                      .arg("--decode")
                      .arg("--silent")
                      .arg(fname.as_ref().to_str().expect("unsupported filename"))
                      .status()
                      .expect("failed to run flac")
                      .success();
    assert!(success);
}

fn print_hex(seq: &[u8]) -> String {
    let vec: Vec<String> = seq.iter().map(|x| format!("{:0>2x}", *x)).collect();
    vec.concat()
}

fn read_streaminfo<P: AsRef<Path>>(fname: P) -> String {
    // Use a buffered reader, this speeds up the test by 120%.
    let file = fs::File::open(fname).unwrap();
    let reader = io::BufReader::new(file);
    let stream = claxon::FlacReader::new(reader).unwrap();
    let streaminfo = stream.streaminfo();

    // Format the streaminfo in the same way that metaflac prints it.
    format!("{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n",
            streaminfo.min_block_size,
            streaminfo.max_block_size,
            streaminfo.min_frame_size.unwrap_or(0),
            streaminfo.max_frame_size.unwrap_or(0),
            streaminfo.sample_rate,
            streaminfo.channels,
            streaminfo.bits_per_sample,
            streaminfo.samples.unwrap_or(0),
            print_hex(&streaminfo.md5sum)) // TODO implement LowerHex for &[u8] and submit a PR.
}

fn compare_metaflac<P: AsRef<Path>>(fname: P) {
    let metaflac = run_metaflac(&fname);
    let streaminfo = read_streaminfo(&fname);
    let mut mf_lines = metaflac.lines();
    let mut si_lines = streaminfo.lines();
    while let (Some(mf), Some(si)) = (mf_lines.next(), si_lines.next()) {
        if mf != si {
            println!("metaflac\n--------\n{}", metaflac);
            println!("streaminfo\n----------\n{}", streaminfo);
            panic!("metaflac disagrees on parsed streaminfo");
        }
    }
}

fn compare_decoded_stream<P: AsRef<Path>>(fname: P) {
    let ref_fname = fname.as_ref().with_extension("wav");
    if !ref_fname.exists() {
        // TODO: actually, we might only want to run the test for files that
        // do exist already. It is fine to run streaminfo or decode-only tests
        // on thousands of files, but decoding thousands of files to wav before
        // running the tests will take a lot of space. For now, while the
        // number of samples is closer to 10 than to 100, this is fine.
        decode_file(&fname);
    }

    // If the reference file does exist after decoding, we can compare it to
    // how Claxon decodes it, sample by sample.
    if ref_fname.exists() {
        let mut ref_wav_reader = hound::WavReader::open(ref_fname).unwrap();

        let try_file = fs::File::open(fname).unwrap();
        let try_buf_reader = io::BufReader::new(try_file);
        let mut try_flac_reader = claxon::FlacReader::new(try_buf_reader).unwrap();

        // The streaminfo test will ensure that things like bit depth and
        // sample rate match, only the actual samples are compared here.

        let mut ref_samples = ref_wav_reader.samples::<i32>();

        let samples = try_flac_reader.streaminfo().samples.unwrap();
        let n_channels = try_flac_reader.streaminfo().channels;
        let mut blocks = try_flac_reader.blocks();
        let mut sample = 0u64;
        let mut b = 0u64;
        let mut buffer = Vec::new();

        while sample < samples {
            let block = blocks.read_next_or_eof(buffer).unwrap().unwrap();
            {
                let mut channels: Vec<_> = (0..n_channels)
                                               .map(|i| block.channel(i).iter().cloned())
                                               .collect();
                for i in 0..block.duration() {
                    for ch in 0..n_channels as usize {
                        let ref_sample = ref_samples.next().map(|r| r.ok().unwrap());
                        let try_sample = channels[ch].next();
                        if ref_sample != try_sample {
                            println!("disagreement at sample {} of block {} in channel {}: \
                                      reference is {} but decoded is {}",
                                     i,
                                     b,
                                     ch,
                                     ref_sample.unwrap(),
                                     try_sample.unwrap());
                            panic!("decoding differs from reference decoder");
                        }
                    }
                }
                sample = sample + block.duration() as u64;
            }
            b = b + 1;
            buffer = block.into_buffer();
        }
    }
}

// Hard-coded tests for the test samples added py populate.sh.
// This allows us to run these tests in parallel and follow progress without
// enabling --nocapture.

#[test]
fn verify_streaminfo_p0() {
    compare_metaflac("testsamples/p0.flac");
}

#[test]
fn verify_streaminfo_p1() {
    compare_metaflac("testsamples/p1.flac");
}

#[test]
fn verify_streaminfo_p2() {
    compare_metaflac("testsamples/p2.flac");
}

#[test]
fn verify_streaminfo_p3() {
    compare_metaflac("testsamples/p3.flac");
}

#[test]
fn verify_streaminfo_p4() {
    compare_metaflac("testsamples/p4.flac");
}

#[test]
fn verify_streaminfo_pop() {
    compare_metaflac("testsamples/pop.flac");
}

#[test]
fn verify_streaminfo_short() {
    compare_metaflac("testsamples/short.flac");
}

#[test]
fn verify_decoded_stream_p0() {
    compare_decoded_stream("testsamples/p0.flac");
}

#[test]
fn verify_decoded_stream_p1() {
    compare_decoded_stream("testsamples/p1.flac");
}

#[test]
fn verify_decoded_stream_p2() {
    compare_decoded_stream("testsamples/p2.flac");
}

#[test]
fn verify_decoded_stream_p3() {
    compare_decoded_stream("testsamples/p3.flac");
}

#[test]
fn verify_decoded_stream_p4() {
    compare_decoded_stream("testsamples/p4.flac");
}

#[test]
fn verify_decoded_stream_pop() {
    compare_decoded_stream("testsamples/pop.flac");
}

#[test]
fn verify_decoded_stream_short() {
    // The short sample has only 4 samples, even less than pop.flac.
    compare_decoded_stream("testsamples/short.flac");
}

#[test]
fn verify_extra_samples() {
    use std::ffi::OsStr;

    if !Path::new("testsamples/extra").exists() {
        return
    }

    // Enumerate all the flac files in the testsamples/extra directory, and
    // compare the streaminfo and stream itself for those.
    let dir = fs::read_dir("testsamples/extra")
                 .ok().expect("failed to enumerate flac files");
    for path in dir {
        let path = path.ok().expect("failed to obtain path info").path();
        if path.is_file() && path.extension() == Some(OsStr::new("flac")) {
            print!("    comparing {} ...", path.to_str()
                                               .expect("unsupported filename"));
            compare_metaflac(&path);
            compare_decoded_stream(&path);
            println!(" ok");
        }
    }
}

#[test]
fn regression_test_fuzz_samples() {
    use std::ffi::OsStr;

    // Enumerate all the flac files in the testsamples/fuzz directory,
    // and ensure that they can be decoded without panic.
    let dir = fs::read_dir("testsamples/fuzz")
                 .ok().expect("failed to enumerate flac files");
    for path in dir {
        let path = path.ok().expect("failed to obtain path info").path();
        if path.is_file() && path.extension() == Some(OsStr::new("flac")) {
            print!("    regression testing {} ...", path.to_str()
                   .expect("unsupported filename"));

            if let Ok(mut reader) = claxon::FlacReader::open(&path) {
                let mut buffer = Vec::new();
                while let Ok(Some(block)) = reader.blocks().read_next_or_eof(buffer) {
                    buffer = block.into_buffer();
                }
            }
            println!(" ok");
        }
    }
}
