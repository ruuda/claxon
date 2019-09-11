// Claxon -- A FLAC decoding library in Rust
// Copyright 2014 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

extern crate claxon;
extern crate hound;
extern crate walkdir;

use std::fs;
use std::io;
use std::iter;
use std::path::Path;

fn run_metaflac_streaminfo<P: AsRef<Path>>(fname: P) -> String {
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

fn run_metaflac_vorbis_comment<P: AsRef<Path>>(fname: P) -> String {
    use std::process::Command;

    // Run metaflac on the specified file and print all Vorbis comment data.
    let output = Command::new("metaflac")
        .arg("--block-type=VORBIS_COMMENT")
        .arg("--list")
        .arg(fname.as_ref().to_str().expect("unsupported filename"))
        .output()
        .expect("failed to run metaflac");
    String::from_utf8(output.stdout).expect("metaflac wrote invalid UTF-8")
}

fn decode_file<P: AsRef<Path>>(fname: P) -> Vec<u8> {
    use std::process::Command;

    // Run the the reference flac decoder on the file.
    let output = Command::new("flac")
        .arg("--decode")
        .arg("--silent")
        .arg("--stdout")
        .arg(fname.as_ref().to_str().expect("unsupported filename"))
        .output()
        .expect("failed to run flac");

    assert!(output.status.success());
    output.stdout
}

fn print_hex(seq: &[u8]) -> String {
    let vec: Vec<String> = seq.iter().map(|x| format!("{:0>2x}", *x)).collect();
    vec.concat()
}

fn read_streaminfo<P: AsRef<Path>>(fname: P) -> String {
    let reader = claxon::FlacReader::open(fname).unwrap();
    let streaminfo = reader.streaminfo();

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

fn compare_metaflac_streaminfo<P: AsRef<Path>>(fname: P) {
    let metaflac = run_metaflac_streaminfo(&fname);
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

fn compare_metaflac_vorbis_comment<P: AsRef<Path>>(fname: P) {
    let metaflac = run_metaflac_vorbis_comment(&fname);
    let reader = claxon::FlacReader::open(fname).unwrap();

    let mut mf_lines = metaflac.lines();

    // Search for the vendor string in the metaflac output.
    while let Some(line) = mf_lines.next() {
        let prefix = "  vendor string: ";
        if line.starts_with(prefix) {
            let mf_vendor_string = &line[prefix.len()..];

            // If the vendor string starts with a null byte, metaflac will not
            // print it -- my guess is because metaflac is written in C and uses
            // C-style string manipulation. In that case we skip it.
            match reader.vendor() {
                Some(x) if x.starts_with('\0') => {
                    assert_eq!("", mf_vendor_string);
                    break
                }
                _ => {}
            }

            assert_eq!(reader.vendor(), Some(mf_vendor_string));
            break
        }
    }

    let mut tags = reader.tags();

    // Loop through all of the comments.
    while let Some(line) = mf_lines.next() {
        let prefix = "    comment[";
        if line.starts_with(prefix) {
            let mf_line = &line[prefix.len()..];
            let prefix_sep_index = mf_line.find(' ').unwrap();
            let mf_pair = &mf_line[prefix_sep_index + 1..];

            let sep_index = mf_pair.find('=').unwrap();
            let mf_name = &mf_pair[..sep_index];
            let mf_value = &mf_pair[sep_index + 1..];

            let (name, value_lines) = tags.next().unwrap();
            let mut value_lines_iter = value_lines.lines();
            let value = value_lines_iter.next().unwrap_or("");

            assert_eq!(name, mf_name);
            assert_eq!(value, mf_value);

            // If there are newlines, then we also need to read those as
            // separate lines from the metaflac untput. This does assume that
            // the newline count that Claxon read is correct, and because of the
            // behavior of the `.lines()` iterator this does not accurately
            // verify carriage returns, but we could not anyway, because
            // metaflac does not escape them.
            while let Some(actual_line) = value_lines_iter.next() {
                assert_eq!(actual_line, mf_lines.next().unwrap());
            }
        }
    }
}

fn compare_decoded_stream<P: AsRef<Path>>(fname: P) {
    let wav = decode_file(&fname);
    let cursor = io::Cursor::new(wav);

    // Compare the reference decoded by the 'flac' program (stored as wav, which
    // we read with Hound) to how Claxon decodes it, sample by sample.
    let mut ref_wav_reader = hound::WavReader::new(cursor).unwrap();

    let mut flac_reader = claxon::FlacReader::open(fname).unwrap();

    // The streaminfo test will ensure that things like bit depth and
    // sample rate match, only the actual samples are compared here.
    let mut ref_samples = ref_wav_reader.samples::<i32>();

    let samples = flac_reader.streaminfo().samples.unwrap();
    let n_channels = flac_reader.streaminfo().channels;
    let mut blocks = flac_reader.blocks();
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

// Hard-coded tests for the test samples added py populate.sh.
// This allows us to run these tests in parallel and follow progress without
// enabling --nocapture.

#[test]
fn verify_streaminfo_p0() {
    compare_metaflac_streaminfo("testsamples/p0.flac");
}

#[test]
fn verify_streaminfo_p1() {
    compare_metaflac_streaminfo("testsamples/p1.flac");
}

#[test]
fn verify_streaminfo_p2() {
    compare_metaflac_streaminfo("testsamples/p2.flac");
}

#[test]
fn verify_streaminfo_p3() {
    compare_metaflac_streaminfo("testsamples/p3.flac");
}

#[test]
fn verify_streaminfo_p4() {
    compare_metaflac_streaminfo("testsamples/p4.flac");
}

#[test]
fn verify_streaminfo_pop() {
    compare_metaflac_streaminfo("testsamples/pop.flac");
}

#[test]
fn verify_streaminfo_short() {
    compare_metaflac_streaminfo("testsamples/short.flac");
}

#[test]
fn verify_streaminfo_wasted_bits() {
    compare_metaflac_streaminfo("testsamples/wasted_bits.flac");
}

#[test]
fn verify_streaminfo_non_subset() {
    compare_metaflac_streaminfo("testsamples/non_subset.flac");
}

#[test]
fn verify_vorbis_comment_p0() {
    compare_metaflac_vorbis_comment("testsamples/p0.flac");
}

#[test]
fn verify_vorbis_comment_p1() {
    compare_metaflac_vorbis_comment("testsamples/p1.flac");
}

#[test]
fn verify_vorbis_comment_p2() {
    compare_metaflac_vorbis_comment("testsamples/p2.flac");
}

#[test]
fn verify_vorbis_comment_p3() {
    compare_metaflac_vorbis_comment("testsamples/p3.flac");
}

#[test]
fn verify_vorbis_comment_p4() {
    compare_metaflac_vorbis_comment("testsamples/p4.flac");
}

#[test]
fn test_flac_reader_get_tag_is_case_insensitive() {
    let flac_reader = claxon::FlacReader::open("testsamples/p4.flac").unwrap();

    // This file contains the following metadata:
    // METADATA block #2
    //   type: 4 (VORBIS_COMMENT)
    //   is last: false
    //   length: 241
    //   vendor string: reference libFLAC 1.1.0 20030126
    //   comments: 5
    //     comment[0]: REPLAYGAIN_TRACK_PEAK=0.69879150
    //     comment[1]: REPLAYGAIN_TRACK_GAIN=-4.00 dB
    //     comment[2]: REPLAYGAIN_ALBUM_PEAK=0.69879150
    //     comment[3]: REPLAYGAIN_ALBUM_GAIN=-3.68 dB
    //     comment[4]: Comment=Encoded by FLAC v1.1.1a with FLAC Frontend v1.7.1

    let mut replaygain_upper = flac_reader.get_tag("REPLAYGAIN_TRACK_GAIN");
    assert_eq!(replaygain_upper.next(), Some("-4.00 dB"));
    assert_eq!(replaygain_upper.next(), None);

    // The lookup should be case-insensitive.
    let mut replaygain_lower = flac_reader.get_tag("replaygain_track_gain");
    assert_eq!(replaygain_lower.next(), Some("-4.00 dB"));
    assert_eq!(replaygain_lower.next(), None);

    // Non-existing tags should not be found.
    let mut foobar = flac_reader.get_tag("foobar");
    assert_eq!(foobar.next(), None);
}

#[test]
fn test_flac_reader_get_tag_returns_all_matches() {
    let flac_reader = claxon::FlacReader::open("testsamples/repeated_vorbis_comment.flac").unwrap();

    // This file contains two FOO tags, `FOO=bar` and `FOO=baz`.

    let mut foo = flac_reader.get_tag("FOO");
    assert_eq!(foo.next(), Some("bar"));
    assert_eq!(foo.next(), Some("baz"));
    assert_eq!(foo.next(), None);
}

#[test]
fn test_flac_reader_tags_skips_empty_vorbis_comments() {
    // This file has been prepared to contain one empty Vorbis comment; a string
    // of length 0, that does not contain the required `=` character. This is
    // invalid, but it occurs in the wild nonetheless. We should skip over such
    // Vorbis comments, and we should read the rest just fine.
    //
    // Note that we don't include this file in the metaflac tests, because
    // metaflac does print the empty comment, and we skip it. Behaving like
    // metaflac would require representing the empty comment, but then we have
    // to deal with the edge case everywhere, so we drop it instead.
    let flac_reader = claxon::FlacReader::open("testsamples/empty_vorbis_comment.flac").unwrap();

    // The file was adapted from `repeated_vorbis_comment.flac`, so it contains
    // the same `FOO=bar` tag, but the `FOO=baz` has been replaced with a 4-byte
    // length prefix to make the final tag `X=Y`.

    let mut tags = flac_reader.tags();
    assert_eq!(tags.next(), Some(("FOO", "bar")));
    assert_eq!(tags.next(), Some(("X", "Y")));
    assert_eq!(tags.next(), None);
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
fn verify_decoded_stream_wasted_bits() {
    // This sample has subframes where the number of wasted bits is not 0.
    compare_decoded_stream("testsamples/wasted_bits.flac");
}

#[test]
fn verify_decoded_stream_non_subset() {
    // This sample does not conform to "subset" flac. It has a subframe with LPC
    // order > 12. The file is a single frame extracted from a larger real-world
    // sample.
    compare_decoded_stream("testsamples/non_subset.flac");
}

#[test]
fn verify_limits_on_vendor_string() {
    // This file claims to have a vendor string which would not fit in the
    // block.
    let file = fs::File::open("testsamples/large_vendor_string.flac").unwrap();
    match claxon::FlacReader::new(file) {
        Ok(..) => panic!("This file should fail to load"),
        Err(err) => {
            assert_eq!(err, claxon::Error::FormatError("vendor string too long"))
        }
    }
}

#[test]
fn verify_limits_on_vorbis_comment_block() {
    // This file claims to have a very large Vorbis comment block, which could
    // make the decoder go OOM.
    match claxon::FlacReader::open("testsamples/large_vorbis_comment_block.flac") {
        Ok(..) => panic!("This file should fail to load"),
        Err(claxon::Error::Unsupported(..)) => { }
        Err(..) => panic!("Expected 'Unsupported' error."),
    }
}

#[test]
fn metadata_only_still_reads_vorbis_comment_block() {
    let opts = claxon::FlacReaderOptions {
        metadata_only: true,
        read_vorbis_comment: true,
    };
    let reader = claxon::FlacReader::open_ext("testsamples/short.flac", opts).unwrap();
    assert_eq!(reader.vendor(), Some("reference libFLAC 1.3.2 20170101"));
}

#[test]
fn no_read_vorbis_comment_block_does_not_contain_vendor_string() {
    let opts = claxon::FlacReaderOptions {
        metadata_only: true,
        read_vorbis_comment: false,
    };
    let reader = claxon::FlacReader::open_ext("testsamples/short.flac", opts).unwrap();
    assert_eq!(reader.vendor(), None);
}

#[test]
#[should_panic]
fn samples_panics_when_metadata_only_is_set() {
    let opts = claxon::FlacReaderOptions {
        metadata_only: true,
        read_vorbis_comment: true,
    };
    let mut reader = claxon::FlacReader::open_ext("testsamples/short.flac", opts).unwrap();
    let _samples = reader.samples();
}

#[test]
#[should_panic]
fn blocks_panics_when_metadata_only_is_set() {
    let opts = claxon::FlacReaderOptions {
        metadata_only: true,
        read_vorbis_comment: true,
    };
    let mut reader = claxon::FlacReader::open_ext("testsamples/short.flac", opts).unwrap();
    let _blocks = reader.blocks();
}

#[test]
fn verify_extra_samples() {
    use std::ffi::OsStr;

    if !Path::new("testsamples/extra").exists() {
        return
    }

    let wd = walkdir::WalkDir::new("testsamples/extra")
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok());

    // Recursively enumerate all the flac files in the testsamples/extra
    // directory, and compare the streaminfo and stream itself for those.
    for entry in wd {
        let path = entry.path();
        if path.is_file() && path.extension() == Some(OsStr::new("flac")) {
            print!("    comparing {} ...", path.to_str()
                                               .expect("unsupported filename"));
            compare_metaflac_streaminfo(&path);
            compare_metaflac_vorbis_comment(&path);
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

            let mut decodes = Vec::new();

            // Decode the file once into buffer pre-filled with "13", and keep
            // the buffers. All of the pre-filled samples should be overwritten.
            if let Ok(mut reader) = claxon::FlacReader::open(&path) {
                let mut buffer = iter::repeat(13_i32).take(1024 * 16).collect();
                while let Ok(Some(block)) = reader.blocks().read_next_or_eof(buffer) {
                    decodes.push(block.into_buffer());
                    buffer = iter::repeat(13_i32).take(1024 * 16).collect();
                }
            }

            // Decode the file again, into a buffer pre-filled with "17". There
            // should not be any difference between the decoded buffers and the
            // previously decoded ones. If there was, then part of the buffer
            // was not overwritten properly.
            if let Ok(mut reader) = claxon::FlacReader::open(&path) {
                let mut buffer = iter::repeat(17_i32).take(1024 * 16).collect();
                while let Ok(Some(block)) = reader.blocks().read_next_or_eof(buffer) {
                    let prev_decode = decodes.remove(0);
                    assert_eq!(prev_decode, block.into_buffer());
                    buffer = iter::repeat(17_i32).take(1024 * 16).collect();
                }
            }

            println!(" ok");
        }
    }
}
