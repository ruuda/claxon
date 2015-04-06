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

//! Claxon, a FLAC decoding library.
//!
//! TODO: Add some examples here.

#![warn(missing_docs)]
#![allow(dead_code)] // TODO: Remove for v0.1
#![feature(core, zero_one)]

use std::io;
use std::cmp::Eq;
use std::ops::{Add, BitAnd, BitOr, Shl, Shr, Sub};
use std::num::{FromPrimitive, One, ToPrimitive, Zero};
use error::{Error, FlacResult};
use frame::{FrameReader};
use input::ReadExt;
use metadata::{MetadataBlock, MetadataBlockReader, StreamInfo};

mod crc;
mod input;
pub mod error;
pub mod frame;
pub mod subframe;
pub mod metadata;

/// An trait that allows for interegers to be generic in width.
pub trait Sample: Zero + One +
    Add<Output = Self> +
    Sub<Output = Self> +
    Shl<usize, Output = Self> +
    Shr<usize, Output = Self> +
    BitOr<Self, Output = Self> +
    BitAnd<Self, Output = Self> +
    Copy + Clone + Eq +
    FromPrimitive + ToPrimitive {

    /// Returns the maximal value that the type can contain.
    // TODO: is this actually required, can we do without in non-debug versions?
    fn max() -> Self;

    /// Returns the minimal value that the type can contain.
    // TODO: is this actually required, can we do without in non-debug versions?
    fn min() -> Self;

    /// Adds with wraparound on overflow.
    fn wrapping_add(self, other: Self) -> Self;

    /// Subtracts with wraparound on overflow.
    fn wrapping_sub(self, other: Self) -> Self;
}

impl Sample for i8 {
    fn max() -> i8 {
        use std::i8;
        i8::MAX
    }

    fn min() -> i8 {
        use std::i8;
        i8::MIN
    }

    fn wrapping_add(self, other: i8) -> i8 {
        self.wrapping_add(other)
    }

    fn wrapping_sub(self, other: i8) -> i8 {
        self.wrapping_sub(other)
    }
}

impl Sample for i16 {
    fn max() -> i16 {
        use std::i16;
        i16::MAX
    }

    fn min() -> i16 {
        use std::i16;
        i16::MIN
    }

    fn wrapping_add(self, other: i16) -> i16 {
        self.wrapping_add(other)
    }

    fn wrapping_sub(self, other: i16) -> i16 {
        self.wrapping_sub(other)
    }
}

impl Sample for i32 {
    fn max() -> i32 {
        use std::i32;
        i32::MAX
    }

    fn min() -> i32 {
        use std::i32;
        i32::MIN
    }

    fn wrapping_add(self, other: i32) -> i32 {
        self.wrapping_add(other)
    }

    fn wrapping_sub(self, other: i32) -> i32 {
        self.wrapping_sub(other)
    }
}

/// A FLAC decoder that can decode the stream from the underlying reader.
///
/// TODO: Is stream a good name? Should it be called reader/decoder?
/// TODO: Add an example.
pub struct FlacStream<'r> {
    streaminfo: StreamInfo,
    metadata_blocks: Vec<MetadataBlock>,
    input: &'r mut (io::Read + 'r)
}

fn read_stream_header<R: io::Read>(input: &mut R) -> FlacResult<()> {
    // A FLAC stream starts with a 32-bit header 'fLaC' (big endian).
    const HEADER: u32 = 0x66_4c_61_43;
    let header = try!(input.read_be_u32());
    if header != HEADER { return Err(Error::InvalidStreamHeader); }
    Ok(())
}

impl<'r> FlacStream<'r> {
    /// Constructs a flac stream from the given input.
    ///
    /// This will read all metadata and stop at the first audio frame.
    pub fn new<R>(input: &mut R) -> FlacResult<FlacStream> where R: io::Read {
        // A flac stream first of all starts with a stream header.
        try!(read_stream_header(input));

        // Start a new scope, because the input reader must be available again
        // for the frame reader next.
        let (streaminfo, metadata_blocks) = {
            // Next are one or more metadata blocks. The flac specification
            // dictates that the streaminfo block is the first block. The metadata
            // block reader will yield at least one element, so the unwrap is safe.
            let mut metadata_iter = MetadataBlockReader::new(input);
            let streaminfo_block = try!(metadata_iter.next().unwrap());
            let streaminfo = match streaminfo_block {
                MetadataBlock::StreamInfo(info) => info,
                _ => return Err(Error::MissingStreamInfoBlock)
            };

            // There might be more metadata blocks, read and store them.
            let mut metadata_blocks = Vec::new();
            for block_result in metadata_iter {
                match block_result {
                    Err(error) => return Err(error),
                    Ok(block) => metadata_blocks.push(block)
                }
            }

            (streaminfo, metadata_blocks)
        };

        // The flac stream will contain the reader that will read frames.
        let flac_stream = FlacStream {
            streaminfo: streaminfo,
            metadata_blocks: metadata_blocks,
            input: input
        };

        Ok(flac_stream)
    }

    /// Returns the streaminfo metadata.
    pub fn streaminfo(&self) -> &StreamInfo {
        &self.streaminfo
    }

    /// Returns an iterator that decodes a single frame on every iteration.
    pub fn blocks<S: Sample>(&'r mut self) -> FrameReader<'r, S> {
        FrameReader::new(&mut self.input)
    }
}
