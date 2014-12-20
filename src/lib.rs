use std::io::{IoResult, IoError, IoErrorKind, Reader};
use std::io::fs::File;

struct Frame;

// TODO: there should be a way to signal IO error.
// This will be handled by the error reform RFCs, I think.
// For now, use IO error to indicate any failure.
pub type Error = IoError;

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

// TODO: this should be private, but it must be public for the test for now.
pub fn read_stream_header(input: &mut Reader) -> Result<(), Error> {
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

// TODO: should this be private or not?
#[deriving(Show)]
pub struct StreamInfo {
    pub min_block_size: u16,
    pub max_block_size: u16,
    pub min_frame_size: Option<u32>,
    pub max_frame_size: Option<u32>,
    pub sample_rate: u32,
    pub n_channels: u8,
    pub bits_per_sample: u8,
    pub n_samples: Option<u64>,
    pub md5sum: [u8, ..16]
}

// TODO: should this be private?
pub struct Seekpoint {
    pub sample: u64,
    pub offset: u64,
    pub n_samples: u16
}

// TODO: should this be private?
pub struct SeekTable {
    seekpoints: Vec<Seekpoint>
}

pub enum MetadataBlock {
    StreamInfoBlock(StreamInfo),
    PaddingBlock(u32),
    ApplicationBlock(u32, Vec<u8>),
    SeekTableBlock(SeekTable),
    VorbisCommentBlock, // TODO
    CuesheetBlock, // TODO
    PictureBlock // TODO
}

// TODO: this should be private.
pub struct MetadataBlockHeader {
    is_last: bool,
    block_type: u8,
    length: u32
}

// TODO: this should be private, but it must be public for the test for now.
pub fn read_metadata_block_header(input: &mut Reader) -> Result<MetadataBlockHeader, Error> {
    let byte = try!(input.read_u8());

    // The first bit specifies whether this is the last block, the next 7 bits
    // specify the type of the metadata block to follow.
    let is_last = (byte & 1) == 1;
    let block_type = byte >> 1;

    if block_type == 127 { return Err(mk_err()); } // TODO: "invalid, to avoid confusion with a frame sync code"
    if block_type > 6 { return Err(mk_err()); } // TODO: "reserved"

    // The length field is 24 bits, or 3 bytes.
    let length = try!(input.read_be_uint_n(3)) as u32;
    
    let header = MetadataBlockHeader {
        is_last: is_last,
        block_type: block_type,
        length: length
    };
    Ok(header)
}

// TODO: this should be private, but it must be public for the test for now.
pub fn read_streaminfo_block(input: &mut Reader) -> Result<StreamInfo, Error> {
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
    let sample_rate = sample_rate_msb as u32 << 4 | sample_rate_lsb as u32 >> 4;

    // Next three bits are the number of channels - 1. Mask them out and add 1.
    let n_channels_bps = sample_rate_lsb;
    let n_channels = ((n_channels_bps >> 1) & 0x7) + 1;

    // The final bit is the most significant of bits per sample - 1. Bits per
    // sample - 1 is 5 bits in total.
    let bps_msb = n_channels_bps & 1;
    let bps_lsb_n_samples = try!(input.read_byte());

    // Stitch together these values, add 1 because # - 1 is stored.
    let bits_per_sample = (bps_msb << 4 | (bps_lsb_n_samples >> 4)) + 1;

    // Number of samples in 36 bits, we have 4 already, 32 to go.
    let n_samples_msb = bps_lsb_n_samples & 0xf;
    let n_samples_lsb = try!(input.read_be_u32());
    let n_samples = n_samples_msb as u64 << 32 | n_samples_lsb as u64;

    let mut md5sum = [0u8, ..16];
    try!(input.read_at_least(16, &mut md5sum));

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

impl FlacStream {
    pub fn new(input: &mut Reader) -> Result<FlacStream, Error> {
        try!(read_stream_header(input));

        let hdr = try!(read_metadata_block_header(input));
        println!("type: {}, length: {}, last: {}",
                 hdr.block_type, hdr.length, hdr.is_last);

        let streaminfo = try!(read_streaminfo_block(input));
        println!("streaminfo: {}", streaminfo);
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
