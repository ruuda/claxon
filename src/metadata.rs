// Claxon -- A FLAC decoding library in Rust
// Copyright (C) 2014-2015 Ruud van Asseldonk
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

//! The `metadata` module deals with metadata at the beginning of a FLAC stream.

use error::{Error, FlacResult};

#[derive(Copy)]
struct MetadataBlockHeader {
    is_last: bool,
    block_type: u8,
    length: u32
}

/// The streaminfo metadata block, with important information about the stream.
#[derive(Copy)]
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
    pub n_channels: u8,
    /// The number of bits per sample.
    pub bits_per_sample: u8,
    /// The total number of inter-channel samples in the stream.
    pub n_samples: Option<u64>,
    /// MD5 signature of the unencoded audio data.
    pub md5sum: [u8; 16]
}

/// A seek point in the seek table.
#[derive(Copy)]
pub struct SeekPoint {
    /// Sample number of the first sample in the target frame, or 2^64 - 1 for a placeholder.
    pub sample: u64,
    /// Offset in bytes from the first byte of the first frame header to the first byte of the
    /// target frame's header.
    pub offset: u64,
    /// Number of samples in the target frame.
    pub n_samples: u16
}

/// A seek table to aid seeking in the stream.
pub struct SeekTable {
    /// The seek points, sorted in ascending order by sample number.
    seekpoints: Vec<SeekPoint>
}

/// A metadata about the flac stream.
pub enum MetadataBlock {
    /// A stream info block.
    StreamInfo(StreamInfo),
    /// A padding block (with no meaningful data).
    Padding {
        /// The number of padding bytes.
        length: u32
    },
    /// An application block with application-specific data.
    Application {
        /// The registered application ID.
        id: u32,
        /// The contents of the application block.
        data: Vec<u8>
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
    Reserved
}

fn read_metadata_block_header(input: &mut Reader)
                              -> FlacResult<MetadataBlockHeader> {
    let byte = try!(input.read_u8());

    // The first bit specifies whether this is the last block, the next 7 bits
    // specify the type of the metadata block to follow.
    let is_last = (byte >> 7) == 1;
    let block_type = byte & 0b0111_1111;

    // The length field is 24 bits, or 3 bytes.
    let length = try!(input.read_be_uint_n(3)) as u32;
    
    let header = MetadataBlockHeader {
        is_last: is_last,
        block_type: block_type,
        length: length
    };
    Ok(header)
}

fn read_metadata_block(input: &mut Reader, block_type: u8, length: u32)
                       -> FlacResult<MetadataBlock> {
    match block_type {
        0 => {
            // The streaminfo block has a fixed size of 34 bytes.
            if length == 34 {
                let streaminfo = try!(read_streaminfo_block(input));
                Ok(MetadataBlock::StreamInfo(streaminfo))
            } else {
                Err(Error::InvalidMetadataBlockLength)
            }
        },
        1 => {
            try!(read_padding_block(input, length));
            Ok(MetadataBlock::Padding { length: length })
        },
        2 => {
            let (id, data) = try!(read_application_block(input, length));
            Ok(MetadataBlock::Application { id: id, data: data })
        },
        3 => {
            // TODO: implement seektable reading. For now, pretend it is padding.
            try!(skip_block(input, length));
            Ok(MetadataBlock::Padding { length: length })
        },
        4 => {
            // TODO: implement Vorbis comment reading. For now, pretend it is padding.
            try!(skip_block(input, length));
            Ok(MetadataBlock::Padding { length: length })
        },
        5 => {
            // TODO: implement CUE sheet reading. For now, pretend it is padding.
            try!(skip_block(input, length));
            Ok(MetadataBlock::Padding { length: length })
        },
        6 => {
            // TODO: implement picture reading. For now, pretend it is padding.
            try!(skip_block(input, length));
            Ok(MetadataBlock::Padding { length: length })
        },
        127 => {
            // This code is invalid to avoid confusion with a frame sync code.
            Err(Error::InvalidMetadataBlockType)
        },
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

fn read_streaminfo_block(input: &mut Reader) -> FlacResult<StreamInfo> {
    let min_block_size = try!(input.read_be_u16());
    let max_block_size = try!(input.read_be_u16());

    // The frame size fields are 24 bits, or 3 bytes.
    let min_frame_size = try!(input.read_be_uint_n(3)) as u32;
    let max_frame_size = try!(input.read_be_uint_n(3)) as u32;

    // Next up are 20 bits that determine the sample rate.
    let sample_rate_msb = try!(input.read_be_u16());
    let sample_rate_lsb = try!(input.read_byte());

    // Stitch together the value from the first 16 bits,
    // and then the 4 most significant bits of the next byte.
    let sample_rate = (sample_rate_msb as u32) << 4 | (sample_rate_lsb as u32) >> 4;

    // Next three bits are the number of channels - 1. Mask them out and add 1.
    let n_channels_bps = sample_rate_lsb;
    let n_channels = ((n_channels_bps >> 1) & 0b0000_0111) + 1;

    // The final bit is the most significant of bits per sample - 1. Bits per
    // sample - 1 is 5 bits in total.
    let bps_msb = n_channels_bps & 1;
    let bps_lsb_n_samples = try!(input.read_byte());

    // Stitch together these values, add 1 because # - 1 is stored.
    let bits_per_sample = (bps_msb << 4 | (bps_lsb_n_samples >> 4)) + 1;

    // Number of samples in 36 bits, we have 4 already, 32 to go.
    let n_samples_msb = bps_lsb_n_samples & 0b0000_1111;
    let n_samples_lsb = try!(input.read_be_u32());
    let n_samples = (n_samples_msb as u64) << 32 | n_samples_lsb as u64;

    let mut md5sum = [0u8; 16];
    try!(input.read_at_least(16, &mut md5sum));

    // Lower bounds can never be larger than upper bounds. Note that 0 indicates
    // unknown for the frame size. Also, the block size must be at least 16.
    if min_block_size > max_block_size {
        return Err(Error::InconsistentBounds);
    }
    if min_block_size < 16 {
        return Err(Error::InvalidBlockSize);
    }
    if min_frame_size > max_frame_size && max_frame_size != 0 {
        return Err(Error::InconsistentBounds);
    }

    // A sample rate of 0 is invalid, and the maximum sample rate is limited by
    // the structure of the frame headers to 655350 Hz.
    if sample_rate == 0 || sample_rate > 655350 {
        return Err(Error::InvalidSampleRate);
    }

    let stream_info = StreamInfo {
        min_block_size: min_block_size,
        max_block_size: max_block_size,
        min_frame_size: if min_frame_size == 0 { None } else { Some(min_frame_size) },
        max_frame_size: if max_frame_size == 0 { None } else { Some(max_frame_size) },
        sample_rate: sample_rate,
        n_channels: n_channels,
        bits_per_sample: bits_per_sample,
        n_samples: if n_samples == 0 { None } else { Some(n_samples) },
        md5sum: md5sum
    };
    Ok(stream_info)
}

fn read_padding_block(input: &mut Reader, length: u32) -> FlacResult<()> {
    // The specification dictates that all bits of the padding block must be 0.
    // However, the reference implementation does not issue an error when this
    // is not the case, and frankly, when you are going to skip over these
    // bytes and do nothing with them whatsoever, why waste all those CPU
    // cycles checking that the padding is valid?
    skip_block(input, length)
}

fn skip_block(input: &mut Reader, length: u32) -> FlacResult<()> {
    for _ in 0 .. length {
        try!(input.read_byte());
    }

    Ok(())
}

fn read_application_block(input: &mut Reader, length: u32)
                          -> FlacResult<(u32, Vec<u8>)> {
    let id = try!(input.read_be_u32());

    // Four bytes of the block have been used for the ID, the rest is payload.
    let data = try!(input.read_exact((length - 4) as usize));

    Ok((id, data))
}

/// Reads metadata blocks from a stream and exposes them as an iterator.
///
/// It is assumed that the next byte that the reader will read, is the first
/// byte of a metadata block header. This means that the iterator will yield at
/// least a single value. If the iterator ever yields an error, then no more
/// data will be read thereafter, and the next value will be `None`.
pub struct MetadataBlockReader<'r, R> where R: 'r {
    input: &'r mut R,
    done: bool
}

/// Either a `MetadataBlock` or an `Error`.
pub type MetadataBlockResult = FlacResult<MetadataBlock>;

impl<'r, R> MetadataBlockReader<'r, R> where R: Reader + 'r {

    /// Creates a metadata block reader that will yield at least one element.
    pub fn new(input: &'r mut R) -> MetadataBlockReader<'r, R> {
        MetadataBlockReader { input: input, done: false }
    }

    fn read_next(&mut self) -> MetadataBlockResult {
        let header = try!(read_metadata_block_header(self.input));
        let block = try!(read_metadata_block(self.input, header.block_type,
                                                         header.length));
        self.done = header.is_last;
        Ok(block)
    }
}

impl<'r, R> Iterator
    for MetadataBlockReader<'r, R> where R: Reader + 'r {

    type Item = MetadataBlockResult;

    fn next(&mut self) -> Option<MetadataBlockResult> {
        if self.done {
            None
        } else {
            let block = self.read_next();

            // After a failure, no more attempts to read will be made,
            // because we don't know where we are in the stream.
            if !block.is_ok() { self.done = true; }

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
