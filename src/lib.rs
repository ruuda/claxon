#![allow(dead_code)]

use error::{FlacError, FlacResult};
use metadata::{MetadataBlock, MetadataBlockReader, StreamInfo};

pub mod decode;
pub mod error;
pub mod metadata;

pub struct FlacStream {
    streaminfo: StreamInfo,
    metadata_blocks: Vec<MetadataBlock>
}

fn read_stream_header(input: &mut Reader) -> FlacResult<()> {
    // A FLAC stream starts with a 32-bit header 'fLaC' (big endian).
    const HEADER: u32 = 0x66_4c_61_43;
    let header = try!(input.read_be_u32());
    if header != HEADER { return Err(FlacError::InvalidStreamHeader); }
    Ok(())
}

impl FlacStream {
    /// Constructs a flac stream from the given input.
    ///
    /// This will read all metadata and stop at the first audio frame.
    pub fn new<R>(input: &mut R) -> FlacResult<FlacStream> where R: Reader {
        // A flac stream first of all starts with a stream header.
        try!(read_stream_header(input));

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

        // Read frames

        let flac_stream = FlacStream {
            streaminfo: streaminfo,
            metadata_blocks: metadata_blocks
        };

        Ok(flac_stream)
    }

    /// Returns the streaminfo metadata.
    pub fn streaminfo(&self) -> &StreamInfo {
        &self.streaminfo
    }
}

#[test]
fn test_open_stream() {
    use std::io::File;
    let mut input = File::open(&Path::new("foo.flac")).unwrap();
    let flac_stream = FlacStream::new(&mut input).unwrap();
}
