// Claxon -- A FLAC decoding library in Rust
// Copyright 2017 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

// This file contains a minimal example of using Claxon with Ogg and Hound
// to decode a flac stream inside an ogg container to a wav file. It assumes
// that the ogg file actually contains flac data, this is not verified. See
// also https://xiph.org/flac/ogg_mapping.html for the format.

extern crate claxon;
extern crate hound;
extern crate ogg;

use std::env;
use std::fs::File;
use std::io;
use std::path::Path;

use claxon::input::ReadBytes;
use claxon::metadata::{StreamInfo, read_metadata_block_with_header};
use hound::{WavSpec, WavWriter};

fn decode_file(fname: &Path) {
    // Create a file to read from. The ogg crate will read "packets" from here.
    let file = File::open(fname).expect("failed to open ogg file");
    let bufread = io::BufReader::new(file);
    let mut preader = ogg::PacketReader::new(bufread);

    // According to the FLAC to Ogg mapping, the first packet contains the
    // streaminfo. It also contains a count of how many metadata packets follow.
    let first_packet = preader.read_packet_expected().expect("failed to read ogg");
    let (streaminfo, header_packets_left) = read_first_packet(&first_packet);

    // Skip over the packets that contain metadata. We do decode the metadata
    // just for demonstration purposes, but then we throw it away.
    for _ in 0..header_packets_left {
        let packet = preader.read_packet_expected().expect("failed to read ogg");
        let mut cursor = io::Cursor::new(&packet.data);
        let _metadata_block = read_metadata_block_with_header(&mut cursor).unwrap();
    }

    // Build a wav writer to write the decoded audio to a wav file.
    let spec = WavSpec {
        channels: streaminfo.channels as u16,
        sample_rate: streaminfo.sample_rate,
        bits_per_sample: streaminfo.bits_per_sample as u16,
        sample_format: hound::SampleFormat::Int,
    };

    let fname_wav = fname.with_extension("wav");
    let opt_wav_writer = WavWriter::create(fname_wav, spec);
    let mut wav_writer = opt_wav_writer.expect("failed to create wav file");

    // All the packets that follow contain one flac frame each. Decode those one
    // by one, recycling the buffer.
    let mut buffer = Vec::with_capacity(streaminfo.max_block_size as usize *
                                        streaminfo.channels as usize);
    while let Some(packet) = preader.read_packet().expect("failed to read ogg") {
        // Empty packets do occur, skip them. So far I have only observed the
        // final packet to be empty.
        if packet.data.len() == 0 { continue }
        buffer = decode_frame(&packet, buffer, &mut wav_writer);
    }
}

/// Decode the streaminfo block, and the number of metadata packets that still follow.
fn read_first_packet(packet: &ogg::Packet) -> (StreamInfo, u16) {
    use claxon::metadata::MetadataBlock;

    let mut cursor = io::Cursor::new(&packet.data);

    // The first 7 bytes contain magic values and version info. We don't
    // verify this. A real application should. Because we are using an
    // `io::Cursor`, the IO operation will not fail, so the unwrap is safe.
    cursor.skip(7).unwrap();

    // Next is a 16-bit big-endian number that specifies the number of header
    // packets that follow. Claxon exposes a `ReadBytes` trait that simplifies
    // reading this.
    let header_packets_left = cursor.read_be_u16().unwrap();

    // The 'fLaC' magic signature follows. We don't verify it.
    cursor.skip(4).unwrap();

    // Next is the streaminfo metadata block, which we return.
    match read_metadata_block_with_header(&mut cursor) {
      Ok(MetadataBlock::StreamInfo(si)) => (si, header_packets_left),
      Ok(..) => panic!("expected streaminfo, found other metadata block"),
      Err(err) => panic!("failed to read streaminfo: {:?}", err),
    }
}

/// Decode a single frame stored in an ogg packet.
///
/// Takes a buffer to decode into, and returns it again so it can be recycled.
fn decode_frame<W>(packet: &ogg::Packet,
                   buffer: Vec<i32>,
                   wav_writer: &mut WavWriter<W>)
                   -> Vec<i32>
where W: io::Seek + io::Write {
    // The Claxon `FrameReader` takes something that implements `ReadBytes`, it
    // needs a buffer to decode efficiently. The packet stores the bytes in a
    // vec, so wrapping it in an `io::Cursor` works; it implements `ReadBytes`.
    let cursor = io::Cursor::new(&packet.data);

    let mut frame_reader = claxon::frame::FrameReader::new(cursor);

    // TODO There should be a read_next method too that does not tolerate EOF.
    let result = frame_reader.read_next_or_eof(buffer);
    let block = result.expect("failed to decode frame").expect("unexpected EOF");

    // TODO: Here we assume that we are decoding a stereo stream, which is
    // wrong, but very convenient, as there is no interleaved sample iterator
    // for `Block`. One should be added.
    for (sl, sr) in block.stereo_samples() {
        wav_writer.write_sample(sl).expect("failed to write wav file");
        wav_writer.write_sample(sr).expect("failed to write wav file");
    }

    // Give the buffer back so it can be recycled.
    block.into_buffer()
}

fn main() {
    for fname in env::args().skip(1) {
        decode_file(&Path::new(&fname));
    }
}
