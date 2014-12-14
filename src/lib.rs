use std::io::{IoError, IoErrorKind, Reader};
use std::io::fs::File;

struct Frame;

// TODO: there should be a way to signal IO error.
// This will be handled by the error reform RFCs, I think.
// For now, use IO error to indicate any failure.
type Error = IoError;

fn mk_err() -> Error {
    IoError {
        kind: IoErrorKind::OtherIoError,
        desc: "FLAC decoding error",
        detail: None
    }
}

struct FlacStream {
    metadata_blocks: Vec<MetadataBlock>
}

#[cfg(test)]
fn get_foo_file() -> File {
    File::open(&Path::new("foo.flac")).unwrap()
}

fn read_stream_header(input: &mut Reader) -> Result<(), Error> {
    // A FLAC stream starts with a 32-bit header 'fLaC' (big endian).
    const HEADER: u32 = 0x66_4c_61_43;
    let header = try!(input.read_be_u32());
    if header != HEADER { return Err(mk_err()); } // TODO: provide more error info.
    Ok(())
}

#[test]
fn test_read_stream_header() {
    let mut input = get_foo_file();
    read_stream_header(&mut input).unwrap();
}

struct StreamInfo {
    pub min_block_size: u16,
    pub max_block_size: u16,
    pub min_frame_size: Option<u32>,
    pub max_frame_size: Option<u32>,
    pub sample_rate: u32,
    pub n_channels: u8,
    pub bits_per_sample: u8,
    pub n_samples: Option<u64>,
    pub md5_signature: [u8, ..16]
}

struct Seekpoint {
    pub sample: u64,
    pub offset: u64,
    pub n_samples: u16
}

struct SeekTable {
    seekpoints: Vec<Seekpoint>
}

enum MetadataBlock {
    StreamInfoBlock(StreamInfo),
    PaddingBlock(u32),
    ApplicationBlock(u32, Vec<u8>),
    SeekTableBlock(SeekTable),
    VorbisCommentBlock, // TODO
    CuesheetBlock, // TODO
    PictureBlock // TODO
}

struct MetadataBlockHeader {
    is_last: bool,
    block_type: u8,
    length: u32
}

fn read_metadata_block_header(input: &mut Reader) -> Result<MetadataBlockHeader, Error> {
    let byte = try!(input.read_u8());

    // The first bit specifies whether this is the last block, the next 7 bits
    // specify the type of the metadata block to follow.
    let is_last = (byte & 1) == 1;
    let block_type = byte >> 1;

    if block_type == 127 { return Err(mk_err()); } // TODO: "invalid, to avoid confusion with a frame sync code"
    if block_type > 6 { return Err(mk_err()); } // TODO: "reserved"

    // TODO: extend reader to be able to read u24 or maybe arbitrary size ingers.
    let length_msb = try!(input.read_u8());
    let length_lsb = try!(input.read_be_u16());
    let length = length_msb as u32 << 16 | length_lsb as u32;
    
    let header = MetadataBlockHeader {
        is_last: is_last,
        block_type: block_type,
        length: length
    };
    Ok(header)
}

impl FlacStream {
    pub fn new(input: &mut Reader) -> Result<FlacStream, Error> {
        try!(read_stream_header(input));

        let hdr = try!(read_metadata_block_header(input));
        println!("type: {}, length: {}, last: {}",
                 hdr.block_type, hdr.length, hdr.is_last);
        // Read the STREAMINFO block
        // Read any metadata
        // Read frames

        let flac_stream = FlacStream {
            metadata_blocks: Vec::new()
        };

        Ok(flac_stream)
    }
}

#[test]
fn test_open_stream() {
    let mut input = get_foo_file();
    let flac_stream = FlacStream::new(&mut input).unwrap();
}
