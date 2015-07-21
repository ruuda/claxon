// Claxon -- A FLAC decoding library in Rust
// Copyright (C) 2014-2015 Ruud van Asseldonk
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License, version 3,
// as published by the Free Software Foundation.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

//! The `error` module defines the error and result types.

use std::error;
use std::fmt;
use std::io;
use std::result;

/// An error that prevents succesful decoding of the FLAC stream.
#[derive(Debug)]
pub enum Error {
    /// Not a decoding error, but a problem with the underlying IO.
    IoError(io::Error),

    /// An ill-formed FLAC stream was encountered.
    FormatError(&'static str),

    /// The audio stream has more bits per sample than the provided sample
    /// buffer to decode into.
    TooWide,

    /// A currently unsupported feature of the FLAC format was encountered.
    Unsupported(&'static str),

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
    /// An overflow occurred when decoding the side channel.
    InvalidSideSample,
    /// An overflow occurred when doing fixed prediction.
    InvalidFixedSample,
    /// An overflow occurred when doing LPC prediction.
    InvalidLpcSample,

    /// The audio stream has more bits per sample than the provided sample
    /// buffer to decode into.
    SampleTooWide
}

impl PartialEq for Error {
    fn eq(&self, other: &Error) -> bool {
        use error::Error::{FormatError, TooWide, Unsupported};
        match (self, other) {
            (&FormatError(r1), &FormatError(r2)) => r1 == r2,
            (&TooWide, &TooWide) => true,
            (&Unsupported(f1), &Unsupported(f2)) => f1 == f2,
            // TODO: this is error-prone. The _ case is required for all
            // non-equal combinations, but it will prevent the compiler from
            // emitting a warning once a new enum variant is added. There must
            // be a better way, right?
            _ => false
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter)
           -> result::Result<(), fmt::Error> {
        match *self {
            Error::IoError(ref err) => err.fmt(formatter),
            Error::FormatError(reason) => {
                try!(formatter.write_str("Ill-formed FLAC stream: "));
                formatter.write_str(reason)
            },
            Error::TooWide => {
                formatter.write_str("The audio stream has more bits per sample than the provided sample buffer to decode into.")
            },
            Error::Unsupported(feature) => {
                try!(formatter.write_str("A currently unsupported feature of the FLAC format was encountered: "));
                formatter.write_str(feature)
            },
            // TODO: Remove this when possible.
            _ => formatter.write_str("deprecated error variant")
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::IoError(ref err) => err.description(),
            Error::FormatError(reason) => reason,
            Error::TooWide => "the sample has more bits than the destination type",
            Error::Unsupported(_) => "unsupported feature",
            // TODO: Remove this when possible.
            _ => "deprecated error variant"
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        match *self {
            Error::IoError(ref err) => Some(err),
            Error::FormatError(_) => None,
            Error::TooWide => None,
            Error::Unsupported(_) => None,
            // TODO: Remove this when possible.
            _ => None
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

// TODO: Remove the `Flac` prefix.
/// Either `T` on success, or an `Error` on failure.
pub type FlacResult<T> = Result<T, Error>;
