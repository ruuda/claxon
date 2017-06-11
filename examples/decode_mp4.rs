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
extern crate mp4parse;

use std::env;
use std::path::Path;
use std::fs::File;
use std::io;
use std::io::Seek;

use claxon::metadata::read_metadata_block;
use claxon::metadata::StreamInfo;
use mp4parse::CodecType;

fn decode_file(fname: &Path) {
    // Create a file to read the mp4 structure from. We will later need to seek
    // in this file to get to the parts that contain FLAC data.
    let file = File::open(fname).expect("failed to open mp4 file");
    let mut bufread = io::BufReader::new(file);

    // Parse the mp4 metadata.
    let mut context = mp4parse::MediaContext::new();
    mp4parse::read_mp4(&mut bufread, &mut context).expect("failed to decode mp4");

    // An MP4 file contains one or more tracks. A track can contain a single
    // FLAC stream. Iterate over those.
    for track in &context.tracks {
        if track.codec_type == CodecType::FLAC {
            let streaminfo = get_streaminfo(track).expect("missing streaminfo");

            // Build a wav writer to write the decoded track to a wav file.
            let spec = hound::WavSpec {
                channels: streaminfo.channels as u16,
                sample_rate: streaminfo.sample_rate,
                bits_per_sample: streaminfo.bits_per_sample as u16,
                sample_format: hound::SampleFormat::Int,
            };

            let fname_wav = fname.with_extension("wav");
            let opt_wav_writer = hound::WavWriter::create(fname_wav, spec);
            let mut wav_writer = opt_wav_writer.expect("failed to create wav file");

            // The data in an MP4 file is split into "chunks", for which we can
            // get the file offset where the chunk starts. Every chunk contains
            // one or more "samples", which is one FLAC frame. These frames are
            // all stored adjacently in the chunk, so we can read them with a
            // fingle frame reader.

            // The "stco" in mp4parse's Track stands for "Chunk Offset Box",
            // "stsc" stands for "Sample to Chunk Box", which actually tells per
            // chunk how many samples it contains, not per sample in which chunk
            // it is.
            let chunk_offsets = &track.stco.as_ref().expect("missing chunk offset box").offsets;
            let chunk_samples = &track.stsc.as_ref().expect("missing sample to chunk box").samples;

            for (ck_index, (co, ref cs)) in chunk_offsets.iter().zip(chunk_samples).enumerate() {
                // The first_chunk value appears to be redundant and equal to
                // the current chunk index, but 1-based rather than 0-based.
                assert_eq!(ck_index as u32 + 1, cs.first_chunk);

                bufread.seek(io::SeekFrom::Start(*co)).expect("failed to seek to chunk");
                
                // The simplest way to read frames now, is unfortunately to
                // double buffer. This might be avoided by not wrapping the
                // original file in an `io::BufReader`, but in a Claxon
                // `BufferedReader`. It would have to implement `io::Read` then.
                let buffered_reader = claxon::input::BufferedReader::new(bufread);
                let mut frame_reader = claxon::frame::FrameReader::new(buffered_reader);
                let mut buffer = Vec::with_capacity(streaminfo.max_block_size as usize);

                for _ in 0..cs.samples_per_chunk {
                    // TODO There should be a read_next method too that does not
                    // tolerate EOF.
                    let result = frame_reader.read_next_or_eof(buffer);
                    let block = result.expect("failed to decode frame").expect("unexpected EOF");

                    // TODO: Here we assume that we are decoding a stereo
                    // stream, which is wrong, but very convenient, as there is
                    // no interleaved sample iterator for `Block`. One should be
                    // added.
                    for (sl, sr) in block.stereo_samples() {
                        wav_writer.write_sample(sl).expect("failed to write wav file");
                        wav_writer.write_sample(sr).expect("failed to write wav file");
                    }

                    buffer = block.into_buffer();
                }

                // Strip off the frame reader and buffered reader to get back
                // the original reader, so we can seek to the right point for
                // the next chunk.
                bufread = frame_reader.into_inner().into_inner();
            }

            // Stop iterating over tracks; if there are more FLAC tracks, we
            // would overwrite the previously written output. (This could be
            // avoided by picking a unique file name for every track, or by
            // asking the user which track to decode.)
            break
        }
    }
}

/// Decode the metadata blocks until the streaminfo block is found.
fn get_streaminfo(track: &mp4parse::Track) -> Option<StreamInfo> {
    use mp4parse::{AudioCodecSpecific, SampleEntry};
    use claxon::metadata::MetadataBlock;

    let audio_entry = match &track.data {
        &Some(SampleEntry::Audio(ref ae)) => ae,
        _ => panic!("expected to find audio entry in FLAC track"),
    };

    let flac_box = match &audio_entry.codec_specific {
        &AudioCodecSpecific::FLACSpecificBox(ref fb) => fb,
        _ => return None,
    };

    for raw_block in &flac_box.blocks {
        let len = raw_block.data.len() as u32;
        let mut cursor = io::Cursor::new(&raw_block.data);
        let result = read_metadata_block(&mut cursor, raw_block.block_type, len);
        match result.expect("failed to decode metadata block") {
            MetadataBlock::StreamInfo(si) => return Some(si),
            _ => {}
        }
    }

    None
}

fn main() {
    for fname in env::args().skip(1) {
        decode_file(&Path::new(&fname));
    }
}
