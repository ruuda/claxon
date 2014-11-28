use std::io::{IoError, IoErrorKind, Reader};

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

const HEADER: u32 = 0x66_4c_61_43; // The header 'fLaC' (big endian).

impl FlacStream {
    pub fn new(input: &mut Reader) -> Result<FlacStream, Error> {
        // A FLAC stream starts with a 32-bit header.
        let header = try!(input.read_be_u32());
        if header != HEADER { return Err(mk_err()); }

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
fn it_works() {
}
