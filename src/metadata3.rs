// Claxon -- A FLAC decoding library in Rust
// Copyright 2021 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

//! The `metadata` module deals with metadata at the beginning of a FLAC stream.

use std::io;

use error::{Error, Result, fmt_err};
use input::ReadBytes;

/// The different kinds of metadata block defined by the FLAC format.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum BlockType {
    /// A STREAMINFO block, with technical details about the stream.
    StreamInfo = 0,
    /// A PADDING block, filled with zeros.
    Padding = 1,
    /// An APPLICATION block that holds application-defined data.
    Application = 2,
    /// A SEEKTABLE block, with data for supporting faster seeks.
    SeekTable = 3,
    /// A VORBIS_COMMENT block, with metadata tags.
    VorbisComment = 4,
    /// A CUESHEET block.
    CueSheet = 5,
    /// A PICTURE block, with cover art or other image metadata.
    Picture = 6,
}

/// A metadata block header.
#[derive(Clone, Copy)]
pub struct BlockHeader {
    /// Whether this is the last metadata block before the audio data.
    pub is_last: bool,
    /// The type of metadata block.
    pub block_type: BlockType,
    /// Length of the metadata block in bytes, excluding this header.
    pub length: u32,
}

/// Read a metadata block header.
///
/// How the contents of the block shoult be interpreted depends on the type of
/// block, and there are dedicated functions to read each block type. It is
/// always possible to skip over the block by skipping `header.length` bytes
/// after reading the header.
#[inline]
pub fn read_block_header<R: io::Read>(input: &mut R) -> Result<BlockHeader> {
    let byte = input.read_u8()?;

    // The first bit specifies whether this is the last block, the next 7 bits
    // specify the type of the metadata block to follow.
    let is_last = (byte >> 7) == 1;
    let block_type_u8 = byte & 0b0111_1111;

    // The length field is 24 bits, or 3 bytes.
    let length = input.read_be_u24()?;

    let block_type = match block_type_u8 {
        0 => BlockType::StreamInfo,
        1 => BlockType::Padding,
        2 => BlockType::Application,
        3 => BlockType::SeekTable,
        4 => BlockType::VorbisComment,
        5 => BlockType::CueSheet,
        6 => BlockType::Picture,
        127 => {
            // This code is invalid to avoid confusion with a frame sync code.
            return fmt_err("invalid metadata block type");
        }
        _ => {
            // Any other block type is 'reserved' at the moment of writing.
            return fmt_err("invalid metadata block, encountered reserved block type");
        }
    };

    // The STREAMINFO block contains no variable-size parts.
    if block_type == BlockType::StreamInfo && length != 34 {
        return fmt_err("invalid streaminfo metadata block length");
    }

    let header = BlockHeader {
        is_last,
        block_type,
        length,
    };
    Ok(header)
}

/// The streaminfo metadata block, with technical information about the stream.
#[derive(Clone, Copy, Debug)]
pub struct StreamInfo {
    // TODO: "size" would better be called "duration" for clarity.
    /// The minimum block size (in inter-channel samples) used in the stream.
    ///
    /// This number is independent of the number of channels. To get the minimum
    /// block duration in seconds, divide this by the sample rate.
    pub min_block_size: u16,

    /// The maximum block size (in inter-channel samples) used in the stream.
    ///
    /// This number is independent of the number of channels. To get the
    /// maximum block duratin in seconds, divide by the sample rate. To avoid
    /// allocations during decoding, a buffer of this size times the number of
    /// channels can be allocated up front and passed into
    /// `FrameReader::read_next_or_eof()`.
    pub max_block_size: u16,

    /// The minimum frame size (in bytes) used in the stream.
    pub min_frame_size: Option<u32>,

    /// The maximum frame size (in bytes) used in the stream.
    pub max_frame_size: Option<u32>,

    /// The sample rate in Hz.
    pub sample_rate: u32,

    /// The number of channels.
    pub channels: u32,

    /// The number of bits per sample.
    pub bits_per_sample: u32,

    /// The total number of inter-channel samples in the stream.
    // TODO: rename to `duration` for clarity?
    pub samples: Option<u64>,

    /// MD5 signature of the unencoded audio data.
    pub md5sum: [u8; 16],
}

/// Read a STREAMINFO block.
pub fn read_streaminfo_block<R: io::Read>(input: &mut R) -> Result<StreamInfo> {
    let min_block_size = input.read_be_u16()?;
    let max_block_size = input.read_be_u16()?;

    // The frame size fields are 24 bits, or 3 bytes.
    let min_frame_size = input.read_be_u24()?;
    let max_frame_size = input.read_be_u24()?;

    // Next up are 20 bits that determine the sample rate.
    let sample_rate_msb = input.read_be_u16()?;
    let sample_rate_lsb = input.read_u8()?;

    // Stitch together the value from the first 16 bits,
    // and then the 4 most significant bits of the next byte.
    let sample_rate = (sample_rate_msb as u32) << 4 | (sample_rate_lsb as u32) >> 4;

    // Next three bits are the number of channels - 1. Mask them out and add 1.
    let n_channels_bps = sample_rate_lsb;
    let n_channels = ((n_channels_bps >> 1) & 0b0000_0111) + 1;

    // The final bit is the most significant of bits per sample - 1. Bits per
    // sample - 1 is 5 bits in total.
    let bps_msb = n_channels_bps & 1;
    let bps_lsb_n_samples = input.read_u8()?;

    // Stitch together these values, add 1 because # - 1 is stored.
    let bits_per_sample = (bps_msb << 4 | (bps_lsb_n_samples >> 4)) + 1;

    // Number of samples in 36 bits, we have 4 already, 32 to go.
    let n_samples_msb = bps_lsb_n_samples & 0b0000_1111;
    let n_samples_lsb = input.read_be_u32()?;
    let n_samples = (n_samples_msb as u64) << 32 | n_samples_lsb as u64;

    // Next are 128 bits (16 bytes) of MD5 signature.
    let mut md5sum = [0u8; 16];
    input.read_exact(&mut md5sum)?;

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
        channels: n_channels as u32,
        bits_per_sample: bits_per_sample as u32,
        samples: if n_samples == 0 {
            None
        } else {
            Some(n_samples)
        },
        md5sum: md5sum,
    };
    Ok(stream_info)
}

/// Application id used in an APPLICATION block.
///
/// Registered application ids are listed at https://www.xiph.org/flac/id.html.
pub struct ApplicationId(pub u32);

/// Read the application id from an APPLICATION block.
///
/// The first 4 bytes of an application block contain its id, the remaining
/// `header.length - 4` bytes contain application-specific data. This function
/// only consumes the first 4 bytes, not the application-specific data.
#[inline]
pub fn read_application_block<R: io::Read>(input: &mut R) -> Result<ApplicationId> {
    Ok(ApplicationId(input.read_be_u32()?))
}
