use std::error::FromError;
use std::io::IoError;

#[deriving(PartialEq, Eq, Show)]
pub enum FlacError {
    /// Not a decoding error, but a problem with the underlying IO.
    IoError(IoError),

    /// The stream header does not equal 'fLaC'.
    InvalidStreamHeader,

    /// Metadata block type 127 is invalid, to avoid confusion with a frame sync code.
    InvalidMetadataBlockType,
    /// The streaminfo block must have length 34.
    InvalidMetadataBlockLength,

    /// A lower bound was encountered that was bigger than an upper bound.
    InconsistentBounds,
    /// The minimum block size must be larger than 15, and the block size must
    /// not exceed 65535.
    InvalidBlockSize,
    /// The sample rate must be positive and no larger than 6553550 Hz.
    InvalidSampleRate,

    /// The streaminfo block must be the very first metadata block.
    MissingStreamInfoBlock,

    /// A frame must start with the frame sync code.
    MissingFrameSyncCode,
    /// The frame header contains an invalid value in one of the reserved bits,
    /// or it contains one of the bit patterns that is invalid to prevent
    /// confusion with a frame sync code, or a bit pattern that is reserved.
    InvalidFrameHeader,
    /// The expected UTF-8-ish encoded integer contains invalid bit sequences.
    InvalidVarLengthInt,
    /// The observed frame header CRC does not match the stored CRC.
    FrameHeaderCrcMismatch,
}

// TODO: implement the Error trait for FlacError.

impl FromError<IoError> for FlacError {
    fn from_error(err: IoError) -> FlacError {
        FlacError::IoError(err)
    }
}

pub type FlacResult<T> = Result<T, FlacError>;
