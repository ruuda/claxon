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

#![allow(dead_code)] // TODO: Remove for v0.1
#![allow(unstable)]

use std::num::UnsignedInt;
use error::{FlacError, FlacResult};
use frame::{FrameReader};
use metadata::{MetadataBlock, MetadataBlockReader, StreamInfo};

mod bitstream;
mod crc;
pub mod error;
pub mod frame;
pub mod subframe;
pub mod metadata;

pub struct FlacStream<'r> {
    streaminfo: StreamInfo,
    metadata_blocks: Vec<MetadataBlock>,
    input: &'r mut (Reader + 'r)
}

fn read_stream_header(input: &mut Reader) -> FlacResult<()> {
    // A FLAC stream starts with a 32-bit header 'fLaC' (big endian).
    const HEADER: u32 = 0x66_4c_61_43;
    let header = try!(input.read_be_u32());
    if header != HEADER { return Err(FlacError::InvalidStreamHeader); }
    Ok(())
}

impl<'r> FlacStream<'r> {
    /// Constructs a flac stream from the given input.
    ///
    /// This will read all metadata and stop at the first audio frame.
    pub fn new<R>(input: &mut R) -> FlacResult<FlacStream> where R: Reader {
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
                _ => return Err(FlacError::MissingStreamInfoBlock)
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
    pub fn blocks<Sample>(&'r mut self) -> FrameReader<'r, Sample>
        where Sample: UnsignedInt {
        FrameReader::new(&mut self.input)
    }
}
