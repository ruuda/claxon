use std::io::{IoError, IoErrorKind, Reader};
use std::io::fs::File;

struct MetadataBlock;
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

fn read_header(input: &mut Reader) -> Result<(), Error> {
    // A FLAC stream starts with a 32-bit header 'fLaC' (big endian).
    const HEADER: u32 = 0x66_4c_61_43;
    let header = try!(input.read_be_u32());
    if header != HEADER { return Err(mk_err()); } // TODO: provide more error info.
    Ok(())
}

#[test]
fn test_read_header() {
    let mut input = get_foo_file();
    read_header(&mut input).unwrap();
}

impl FlacStream {
    pub fn new(input: &mut Reader) -> Result<FlacStream, Error> {
        let _ = try!(read_header(input));
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
