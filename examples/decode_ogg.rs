// Claxon -- A FLAC decoding library in Rust
// Copyright 2017 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

// This file contains a minimal example of using Claxon with Ogg and Hound
// to decode a flac stream inside an ogg container to a wav file. See also
// https://xiph.org/flac/ogg_mapping.html.

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
    let mut _wav_writer = opt_wav_writer.expect("failed to create wav file");

    // TODO: Actually read audio packets.
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

fn main() {
    for fname in env::args().skip(1) {
        decode_file(&Path::new(&fname));
    }
}
