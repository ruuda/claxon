use std::io::{IoResult, IoError, IoErrorKind, Reader};
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

    // The length field is 24 bits, or 3 bytes.
    let length = try!(input.read_be_uint_n(3)) as u32;
    
    let header = MetadataBlockHeader {
        is_last: is_last,
        block_type: block_type,
        length: length
    };
    Ok(header)
}

fn read_streaminfo_block(input: &mut Reader) -> Result<StreamInfo, Error> {
    let min_block_size = try!(input.read_be_u16());
    let max_block_size = try!(input.read_be_u16());
    // The frame size fields are 24 bits, or 3 bytes.
    let min_frame_size = try!(input.read_be_uint_n(3)) as u32;
    let max_frame_size = try!(input.read_be_uint_n(3)) as u32;

    // Sample data is packed as 20 bits sample rate, 3 bits #channels - 1,
    // 5 bits #bits per sample - 1, 36 bits #samples in stream.
    let sample_rate_msb = try!(input.read_be_u16());
    let sample_rate_lsb = try!(input.read_byte());
    // Stitch together the value from the first 16 bits,
    // and then 4 bits of the next byte.
    let sample_rate = sample_rate_msb as u32 << 4 | sample_rate_lsb as u32 >> 4;
    // Next three bits are the number of channels - 1; mask out and add 1.
    let n_channels = ((sample_rate_lsb >> 1) & 0x7) + 1;
    // The final bit is the most significant of bits per sample - 1
    let bps_msb = sample_rate_lsb & 1;
    let bps_lsb = try!(input.read_byte());
    // Stitch together these values, add 1 because # - 1 is stored.
    let bits_per_sample = (bps_msb << 5 | (bps_lsb >> 4)) + 1;
    // Number of samples in 36 bits, we have 4, 32 to go.
    let n_samples_lsb = try!(input.read_be_u32());
    let n_samples = (bps_lsb & 0xf) as u64 << 32 | n_samples_lsb as u64;

    let md5_signature = try!(input.read_exact(16));
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
