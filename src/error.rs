use std::error::FromError;
use std::io::IoError;

#[deriving(Show)]
pub enum FlacError {
    /// Not a decoding error, but a problem with the underlying IO.
    IoError(IoError),

    /// The stream header does not equal 'fLaC'.
    InvalidStreamHeader,

    /// Metadata block type 127 is invalid, to avoid confusion with a frame sync code.
    InvalidMetadataBlockType,
    /// This version of the library cannot handle this kind of metadata block,
    /// it was reserved at the time of writing.
    ReservedMetadataBlockType,

    /// A lower bound was encountered that was bigger than an upper bound.
    InconsistentBounds,
    /// The minimum block size must be larger than 15.
    InvalidBlockSize,
    /// The sample rate must be positive and no larger than 6553550 Hz.
    InvalidSampleRate,

    /// The streaminfo block must be the very first metadata block.
    MissingStreamInfoBlock,
}

// TODO: implement the Error trait for FlacError.

impl FromError<IoError> for FlacError {
    fn from_error(err: IoError) -> FlacError {
        FlacError::IoError(err)
    }
}
