// Claxon -- A FLAC decoding library in Rust
// Copyright 2018 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

//! The `metadata` module deals with metadata at the beginning of a FLAC stream.

use std::io;

use error::{Error, Result, fmt_err};
use input::{BufferedReader, ReadBytes};


/// A metadata about the FLAC stream.
pub enum MetadataBlock<'a, R: 'a + io::Read> {
    /// The stream info block.
    StreamInfo(StreamInfo),

    /// A padding block, of a given number of zero bytes.
    Padding(u32),

    /// An block with application-specific data.
    Application(ApplicationBlock<'a, R>),

    /// A seek table block.
    SeekTable(LazySeekTable<'a, R>),

    /// A Vorbis comment block, also known as FLAC tags.
    VorbisComment(LazyVorbisComment<'a, R>),

    /// A CUE sheet block.
    CueSheet(LazyCueSheet<'a, R>),

    /// A picture block.
    Picture(LazyPicture<'a, R>),
}

/// The streaminfo metadata block, with important information about the stream.
#[derive(Clone, Copy, Debug)]
pub struct StreamInfo {
    // TODO: "size" would better be called "duration" for clarity.
    /// The minimum block size (in inter-channel samples) used in the stream.
    ///
    /// This number is independent of the number of channels. To get the minimum
    /// block duration in seconds, divide this by the sample rate.
    pub min_block_size: u16,

    /// The maximum block size (in inter-channel samples) used in the stream.
    ///
    /// This number is independent of the number of channels. To get the
    /// maximum block duratin in seconds, divide by the sample rate. To avoid
    /// allocations during decoding, a buffer of this size times the number of
    /// channels can be allocated up front and passed into
    /// `FrameReader::read_next_or_eof()`.
    pub max_block_size: u16,

    /// The minimum frame size (in bytes) used in the stream.
    pub min_frame_size: Option<u32>,

    /// The maximum frame size (in bytes) used in the stream.
    pub max_frame_size: Option<u32>,

    /// The sample rate in Hz.
    pub sample_rate: u32,

    /// The number of channels.
    pub channels: u32,

    /// The number of bits per sample.
    pub bits_per_sample: u32,

    /// The total number of inter-channel samples in the stream.
    // TODO: rename to `duration` for clarity?
    pub samples: Option<u64>,

    /// MD5 signature of the unencoded audio data.
    pub md5sum: [u8; 16],
}

pub struct ApplicationBlock<'a, R: 'a + io::Read> {
    /// The application id, registered with Xiph.org.
    ///
    /// [The list of registered ids can be found on Xiph.org][ids].
    /// 
    /// [ids]: https://xiph.org/flac/id.html
    pub id: u32,
    reader: &'a mut BufferedReader<R>,
    len: u32,
}

macro_rules! lazy_block_impl {
    ($struct: ident) => {
        impl <'a, R: 'a + io::Read> $struct<'a, R> {
            /// Skip over this metadata block without parsing anything.
            pub fn discard(mut self) -> io::Result<()> {
                try!(self.reader.skip(self.len));
                self.len = 0;
                Ok(())
            }
        }

        impl<'a, R: 'a + io::Read> Drop for $struct<'a, R> {
            fn drop(&mut self) {
                if self.len != 0 {
                    panic!("{} was dropped, call .discard() or .get() instead.", stringify!($struct))
                }
            }
        }
    }
}

/// An unparsed seek table.
///
/// This struct must be consumed in one of two ways:
///
///  * Call `discard()` to skip over the block.
///  * Call `get()` to read and parse the seek table. (Not implemented yet.)
///
/// **Dropping this struct without calling either will panic.**
#[must_use = "Discard using discard() or consume with get()."]
pub struct LazySeekTable<'a, R: 'a + io::Read> {
    reader: &'a mut BufferedReader<R>,
    len: u32,
}

lazy_block_impl!(LazySeekTable);

/// An unparsed Vorbis comment (also called FLAC tags) block.
///
/// This struct must be consumed in one of two ways:
///
///  * Call `discard()` to skip over the block.
///  * Call `get()` to read and parse the Vorbis comments.
///
/// **Dropping this struct without calling either will panic.**
#[must_use = "Discard using discard() or consume with get()."]
pub struct LazyVorbisComment<'a, R: 'a + io::Read> {
    reader: &'a mut BufferedReader<R>,
    len: u32,
}

lazy_block_impl!(LazyVorbisComment);

/// An unparsed CUE sheet block.
///
/// This struct must be consumed in one of two ways:
///
///  * Call `discard()` to skip over the block.
///  * Call `get()` to read and parse the CUE sheet. (Not implemented yet.)
///
/// **Dropping this struct without calling either will panic.**
#[must_use = "Discard using discard() or consume with get()."]
pub struct LazyCueSheet<'a, R: 'a + io::Read> {
    reader: &'a mut BufferedReader<R>,
    len: u32,
}

lazy_block_impl!(LazyCueSheet);

/// An unparsed picture block.
///
/// This struct must be consumed in one of two ways:
///
///  * Call `discard()` to skip over the block.
///  * Call `get()` to read and parse the picture metadata, and to expose the
///    inner picture data.
///
/// **Dropping this struct without calling either will panic.**
#[must_use = "Discard using discard() or consume with get()."]
pub struct LazyPicture<'a, R: 'a + io::Read> {
    reader: &'a mut BufferedReader<R>,
    len: u32,
}

lazy_block_impl!(LazyPicture);
