// Claxon -- A FLAC decoding library in Rust
// Copyright 2017 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

// This file contains a minimal example of using Claxon with mp4parse and Hound
// to decode a flac stream inside an MP4 (.mp4 or .m4a) container to a wav file.

extern crate claxon;
extern crate hound;
extern crate mp4parse;

use std::env;
use std::fs::File;
use std::io::Seek;
use std::io;
use std::path::Path;

use claxon::metadata::read_metadata_block;
use claxon::metadata::StreamInfo;
use hound::{WavSpec, WavWriter};
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
        if track.codec_type != CodecType::FLAC {
            continue
        }

        let streaminfo = get_streaminfo(track).expect("missing streaminfo");

        // Build a wav writer to write the decoded track to a wav file.
        let spec = WavSpec {
            channels: streaminfo.channels as u16,
            sample_rate: streaminfo.sample_rate,
            bits_per_sample: streaminfo.bits_per_sample as u16,
            sample_format: hound::SampleFormat::Int,
        };

        let fname_wav = fname.with_extension("wav");
        let opt_wav_writer = WavWriter::create(fname_wav, spec);
        let mut wav_writer = opt_wav_writer.expect("failed to create wav file");

        // The data in an MP4 file is split into "chunks", for which we can get
        // the file offset where the chunk starts. Every chunk contains one or
        // more "samples", which is one FLAC frame. These frames are all stored
        // adjacently in the chunk, so we can read them with a fingle frame
        // reader.

        // The "stco" in mp4parse's Track stands for "Chunk Offset Box", "stsc"
        // stands for "Sample to Chunk Box", which actually tells per chunk how
        // many samples it contains, not per sample in which chunk it is.
        let chunk_offsets = &track.stco.as_ref().expect("missing chunk offset box").offsets;
        let chunk_samples = &track.stsc.as_ref().expect("missing sample to chunk box").samples;

        let mut samples_iter = chunk_samples.iter();
        let mut next_samples = samples_iter.next();
        let mut samples_per_chunk = 0;

        // Iterate over all chunks in this track. We need all chunks, and every
        // chunk is present in the chunk offset box.
        for (i, offset) in chunk_offsets.iter().enumerate() {
            bufread.seek(io::SeekFrom::Start(*offset)).expect("failed to seek to chunk");

            // For some chunks, the "Sample to Chunk Box" stores details about
            // how many "samples" (FLAC frames) there are per chunk. When there
            // is no such data for a chunk, the samples per chunk is the same as
            // for the previous chunk.
            next_samples = next_samples.and_then(|ns| {
                // The first_chunk field is a 1-based index, not 0-based.
                if ns.first_chunk == 1 + i as u32 {
                    samples_per_chunk = ns.samples_per_chunk;
                    samples_iter.next()
                } else {
                    Some(ns)
                }
            });

            bufread = decode_frames(bufread, &streaminfo, samples_per_chunk, &mut wav_writer);
        }

        // Stop iterating over tracks; if there are more FLAC tracks, we would
        // overwrite the previously written output. (This could be avoided by
        // picking a unique file name for every track, or by asking the user
        // which track to decode.)
        break
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

/// Decode a number of FLAC frames. Takes an input `io::Read` and returns it.
fn decode_frames<R, W>(input: R,
                       streaminfo: &StreamInfo,
                       num_frames: u32,
                       wav_writer: &mut WavWriter<W>)
                       -> R
where R: io::Read, W: io::Write + io::Seek {
    let mut frame_reader = claxon::frame::FrameReader::new(input);
    let mut buffer = Vec::with_capacity(streaminfo.max_block_size as usize *
                                        streaminfo.channels as usize);

    for _ in 0..num_frames {
        // TODO There should be a read_next method too that does not tolerate
        // EOF.
        let result = frame_reader.read_next_or_eof(buffer);
        let block = result.expect("failed to decode frame").expect("unexpected EOF");

        // TODO: Here we assume that we are decoding a stereo stream, which
        // is wrong, but very convenient, as there is no interleaved sample
        // iterator for `Block`. One should be added.
        for (sl, sr) in block.stereo_samples() {
            wav_writer.write_sample(sl).expect("failed to write wav file");
            wav_writer.write_sample(sr).expect("failed to write wav file");
        }

        buffer = block.into_buffer();
    }

    // Strip off the frame reader to get back the original reader.
    frame_reader.into_inner()
}

fn main() {
    for fname in env::args().skip(1) {
        decode_file(&Path::new(&fname));
    }
}
