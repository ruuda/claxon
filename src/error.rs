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

//! The `error` module defines the error and result types.

use std::error::FromError;
use std::old_io::IoError;

/// An error that prevents succesful decoding of the FLAC stream.
#[derive(PartialEq, Eq, Debug)]
pub enum Error {
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

    /// The subframe header contains an invalid or reserved bit pattern.
    InvalidSubframeHeader,
    /// The subframe contains an invalid or reserved bit pattern.
    InvalidSubframe,

    /// The residual contains an invalid or reserved bit pattern.
    InvalidResidual,
    /// The number of bits per sample in an unencoded binary Rice partition
    /// is larger than the bits per sample of the stream.
    InvalidBitsPerSample,
    /// A bit pattern is not a valid Rice code in the context.
    InvalidRiceCode,

    /// The audio stream has more bits per sample than the provided sample
    /// buffer to decode into.
    SampleTooWide
}

// TODO: implement the Error trait for claxon::error::Error.

impl FromError<IoError> for Error {
    fn from_error(err: IoError) -> Error {
        Error::IoError(err)
    }
}

/// Either `T` on success, or an `Error` on failure.
pub type FlacResult<T> = Result<T, Error>;
