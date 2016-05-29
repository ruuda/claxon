// Claxon -- A FLAC decoding library in Rust
// Copyright 2014 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

//! The `metadata` module deals with metadata at the beginning of a FLAC stream.

use std::io;
use std::iter;
use error::{Result, fmt_err};
use input::ReadExt;

#[derive(Clone, Copy)]
struct MetadataBlockHeader {
    is_last: bool,
    block_type: u8,
    length: u32,
}

/// The streaminfo metadata block, with important information about the stream.
#[derive(Clone, Copy)]
pub struct StreamInfo {
    /// The minimum block size (in samples) used in the stream.
    pub min_block_size: u16,
    /// The maximum block size (in samples) used in the stream.
    pub max_block_size: u16,
    /// The minimum frame size (in bytes) used in the stream.
    pub min_frame_size: Option<u32>,
    /// The maximum frame size (in bytes) used in the stream.
    pub max_frame_size: Option<u32>,
    /// The sample rate in Hz.
    pub sample_rate: u32,
    /// The number of channels.
    pub channels: u8,
    /// The number of bits per sample.
    pub bits_per_sample: u8,
    /// The total number of inter-channel samples in the stream.
    pub samples: Option<u64>,
    /// MD5 signature of the unencoded audio data.
    pub md5sum: [u8; 16],
}

/// A seek point in the seek table.
#[derive(Clone, Copy)]
pub struct SeekPoint {
    /// Sample number of the first sample in the target frame, or 2^64 - 1 for a placeholder.
    pub sample: u64,
    /// Offset in bytes from the first byte of the first frame header to the first byte of the
    /// target frame's header.
    pub offset: u64,
    /// Number of samples in the target frame.
    pub samples: u16,
}

/// A seek table to aid seeking in the stream.
pub struct SeekTable {
    /// The seek points, sorted in ascending order by sample number.
    #[allow(dead_code)] // TODO: Implement seeking.
    seekpoints: Vec<SeekPoint>,
}

/// A metadata about the flac stream.
pub enum MetadataBlock {
    /// A stream info block.
    StreamInfo(StreamInfo),
    /// A padding block (with no meaningful data).
    Padding {
        /// The number of padding bytes.
        length: u32,
    },
    /// An application block with application-specific data.
    Application {
        /// The registered application ID.
        id: u32,
        /// The contents of the application block.
        data: Vec<u8>,
    },
    /// A seek table block.
    SeekTable(SeekTable),
    /// A Vorbis comment block, also known as FLAC tags.
    VorbisComment, // TODO
    /// A CUE sheet block.
    CueSheet, // TODO
    /// A picture block.
    Picture, // TODO
    /// A block with a reserved block type, not supported by this library.
    Reserved,
}

fn read_metadata_block_header<R: io::Read>(input: &mut R) -> Result<MetadataBlockHeader> {
    let byte = try!(input.read_u8());

    // The first bit specifies whether this is the last block, the next 7 bits
    // specify the type of the metadata block to follow.
    let is_last = (byte >> 7) == 1;
    let block_type = byte & 0b0111_1111;

    // The length field is 24 bits, or 3 bytes.
    let length = try!(input.read_be_u24());

    let header = MetadataBlockHeader {
        is_last: is_last,
        block_type: block_type,
        length: length,
    };
    Ok(header)
}

fn read_metadata_block<R: io::Read>(input: &mut R,
                                    block_type: u8,
                                    length: u32)
                                    -> Result<MetadataBlock> {
    match block_type {
        0 => {
            // The streaminfo block has a fixed size of 34 bytes.
            if length == 34 {
                let streaminfo = try!(read_streaminfo_block(input));
                Ok(MetadataBlock::StreamInfo(streaminfo))
            } else {
                fmt_err("invalid streaminfo metadata block length")
            }
        }
        1 => {
            try!(read_padding_block(input, length));
            Ok(MetadataBlock::Padding { length: length })
        }
        2 => {
            let (id, data) = try!(read_application_block(input, length));
            Ok(MetadataBlock::Application {
                id: id,
                data: data,
            })
        }
        3 => {
            // TODO: implement seektable reading. For now, pretend it is padding.
            try!(skip_block(input, length));
            Ok(MetadataBlock::Padding { length: length })
        }
        4 => {
            // TODO: implement Vorbis comment reading. For now, pretend it is padding.
            try!(skip_block(input, length));
            Ok(MetadataBlock::Padding { length: length })
        }
        5 => {
            // TODO: implement CUE sheet reading. For now, pretend it is padding.
            try!(skip_block(input, length));
            Ok(MetadataBlock::Padding { length: length })
        }
        6 => {
            // TODO: implement picture reading. For now, pretend it is padding.
            try!(skip_block(input, length));
            Ok(MetadataBlock::Padding { length: length })
        }
        127 => {
            // This code is invalid to avoid confusion with a frame sync code.
            fmt_err("invalid metadata block type")
        }
        _ => {
            // Any other block type is 'reserved' at the moment of writing. The
            // reference implementation reads it as an 'unknown' block. That is
            // one way of handling it, but maybe there should be some kind of
            // 'strict' mode (configurable at compile time?) so that this can
            // be an error if desired.
            try!(skip_block(input, length));
            Ok(MetadataBlock::Reserved)
        }
    }
}

fn read_streaminfo_block<R: io::Read>(input: &mut R) -> Result<StreamInfo> {
    let min_block_size = try!(input.read_be_u16());
    let max_block_size = try!(input.read_be_u16());

    // The frame size fields are 24 bits, or 3 bytes.
    let min_frame_size = try!(input.read_be_u24());
    let max_frame_size = try!(input.read_be_u24());

    // Next up are 20 bits that determine the sample rate.
    let sample_rate_msb = try!(input.read_be_u16());
    let sample_rate_lsb = try!(input.read_u8());

    // Stitch together the value from the first 16 bits,
    // and then the 4 most significant bits of the next byte.
    let sample_rate = (sample_rate_msb as u32) << 4 | (sample_rate_lsb as u32) >> 4;

    // Next three bits are the number of channels - 1. Mask them out and add 1.
    let n_channels_bps = sample_rate_lsb;
    let n_channels = ((n_channels_bps >> 1) & 0b0000_0111) + 1;

    // The final bit is the most significant of bits per sample - 1. Bits per
    // sample - 1 is 5 bits in total.
    let bps_msb = n_channels_bps & 1;
    let bps_lsb_n_samples = try!(input.read_u8());

    // Stitch together these values, add 1 because # - 1 is stored.
    let bits_per_sample = (bps_msb << 4 | (bps_lsb_n_samples >> 4)) + 1;

    // Number of samples in 36 bits, we have 4 already, 32 to go.
    let n_samples_msb = bps_lsb_n_samples & 0b0000_1111;
    let n_samples_lsb = try!(input.read_be_u32());
    let n_samples = (n_samples_msb as u64) << 32 | n_samples_lsb as u64;

    // Next are 128 bits (16 bytes) of MD5 signature.
    let mut md5sum = [0u8; 16];
    try!(input.read_into(&mut md5sum));

    // Lower bounds can never be larger than upper bounds. Note that 0 indicates
    // unknown for the frame size. Also, the block size must be at least 16.
    if min_block_size > max_block_size {
        return fmt_err("inconsistent bounds, min block size > max block size");
    }
    if min_block_size < 16 {
        return fmt_err("invalid block size, must be at least 16");
    }
    if min_frame_size > max_frame_size && max_frame_size != 0 {
        return fmt_err("inconsistent bounds, min frame size > max frame size");
    }

    // A sample rate of 0 is invalid, and the maximum sample rate is limited by
    // the structure of the frame headers to 655350 Hz.
    if sample_rate == 0 || sample_rate > 655350 {
        return fmt_err("invalid sample rate");
    }

    let stream_info = StreamInfo {
        min_block_size: min_block_size,
        max_block_size: max_block_size,
        min_frame_size: if min_frame_size == 0 {
            None
        } else {
            Some(min_frame_size)
        },
        max_frame_size: if max_frame_size == 0 {
            None
        } else {
            Some(max_frame_size)
        },
        sample_rate: sample_rate,
        channels: n_channels,
        bits_per_sample: bits_per_sample,
        samples: if n_samples == 0 {
            None
        } else {
            Some(n_samples)
        },
        md5sum: md5sum,
    };
    Ok(stream_info)
}

fn read_padding_block<R: io::Read>(input: &mut R, length: u32) -> Result<()> {
    // The specification dictates that all bits of the padding block must be 0.
    // However, the reference implementation does not issue an error when this
    // is not the case, and frankly, when you are going to skip over these
    // bytes and do nothing with them whatsoever, why waste all those CPU
    // cycles checking that the padding is valid?
    skip_block(input, length)
}

fn skip_block<R: io::Read>(input: &mut R, length: u32) -> Result<()> {
    for _ in 0..length {
        try!(input.read_u8());
    }

    Ok(())
}

fn read_application_block<R: io::Read>(input: &mut R, length: u32) -> Result<(u32, Vec<u8>)> {
    let id = try!(input.read_be_u32());

    // Four bytes of the block have been used for the ID, the rest is payload.
    let mut data: Vec<u8> = iter::repeat(0).take(length as usize - 4).collect();
    try!(input.read_into(&mut data));

    Ok((id, data))
}

/// Reads metadata blocks from a stream and exposes them as an iterator.
///
/// It is assumed that the next byte that the reader will read, is the first
/// byte of a metadata block header. This means that the iterator will yield at
/// least a single value. If the iterator ever yields an error, then no more
/// data will be read thereafter, and the next value will be `None`.
pub struct MetadataBlockReader<R: io::Read> {
    input: R,
    done: bool,
}

/// Either a `MetadataBlock` or an `Error`.
pub type MetadataBlockResult = Result<MetadataBlock>;

impl<R: io::Read> MetadataBlockReader<R> {
    /// Creates a metadata block reader that will yield at least one element.
    pub fn new(input: R) -> MetadataBlockReader<R> {
        MetadataBlockReader {
            input: input,
            done: false,
        }
    }

    fn read_next(&mut self) -> MetadataBlockResult {
        let header = try!(read_metadata_block_header(&mut self.input));
        let block = try!(read_metadata_block(&mut self.input, header.block_type, header.length));
        self.done = header.is_last;
        Ok(block)
    }
}

impl<R: io::Read> Iterator for MetadataBlockReader<R> {
    type Item = MetadataBlockResult;

    fn next(&mut self) -> Option<MetadataBlockResult> {
        if self.done {
            None
        } else {
            let block = self.read_next();

            // After a failure, no more attempts to read will be made,
            // because we don't know where we are in the stream.
            if !block.is_ok() {
                self.done = true;
            }

            Some(block)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        // When done, there will be no more blocks,
        // when not done, there will be at least one more.
        if self.done {
            (0, Some(0))
        } else {
            (1, None)
        }
    }
}
