// Claxon -- A FLAC decoding library in Rust
// Copyright 2014 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

//! Claxon, a FLAC decoding library.
//!
//! Examples
//! ========
//!
//! The following example computes the root mean square (RMS) of a FLAC file.
//!
//! ```
//! # use claxon;
//! let mut reader = claxon::FlacReader::open("testsamples/pop.flac").unwrap();
//! let mut sqr_sum = 0.0;
//! let mut count = 0;
//! for sample in reader.samples() {
//!     let s = sample.unwrap() as f64;
//!     sqr_sum += s * s;
//!     count += 1;
//! }
//! println!("RMS is {}", (sqr_sum / count as f64).sqrt());
//! ```
//!
//! A simple way to decode a file to wav with Claxon and
//! [Hound](https://github.com/ruuda/hound):
//!
//! ```
//! # extern crate hound;
//! # extern crate claxon;
//! # use std::path::Path;
//! # fn decode_file(fname: &Path) {
//! let mut reader = claxon::FlacReader::open(fname).expect("failed to open FLAC stream");
//!
//! let spec = hound::WavSpec {
//!     channels: reader.streaminfo().channels as u16,
//!     sample_rate: reader.streaminfo().sample_rate,
//!     bits_per_sample: reader.streaminfo().bits_per_sample as u16,
//!     sample_format: hound::SampleFormat::Int,
//! };
//!
//! let fname_wav = fname.with_extension("wav");
//! let opt_wav_writer = hound::WavWriter::create(fname_wav, spec);
//! let mut wav_writer = opt_wav_writer.expect("failed to create wav file");
//!
//! for opt_sample in reader.samples() {
//!     let sample = opt_sample.expect("failed to decode FLAC stream");
//!     wav_writer.write_sample(sample).expect("failed to write wav file");
//! }
//! # }
//! ```
//!
//! Retrieving the artist metadata:
//!
//! ```
//! # use claxon;
//! let vc = claxon::open_vorbis_comment("testsamples/pop.flac").unwrap();
//! for artist in vc.get_tag("ARTIST") {
//!     println!("{}", artist);
//! }
//! ```
//!
//! For more examples, see the [examples](https://github.com/ruuda/claxon/tree/master/examples)
//! directory in the crate.

#![warn(missing_docs)]

use std::fs;
use std::io;
use std::mem;
use std::path;
use error::fmt_err;
use frame::FrameReader;
use input::ReadBytes;
use metadata::{StreamInfo, OptionalVorbisComment};

mod crc;
mod error;
pub mod frame;
pub mod input;
pub mod subframe;
pub mod metadata;

pub use error::{Error, Result};
pub use frame::Block;

/// A FLAC decoder that can decode the stream from the underlying reader.
///
/// TODO: Add an example.
pub struct FlacReader<R: io::Read> {
    streaminfo: StreamInfo,
    input: R,
}

/// An iterator that yields samples read from a `FlacReader`.
pub struct FlacSamples<R: io::Read> {
    frame_reader: FrameReader<R>,
    block: Block,
    sample: u32,
    channel: u32,

    /// If reading ever failed, this flag is set, so that the iterator knows not
    /// to return any new values.
    has_failed: bool,
}

/// An iterator that yields samples read from a `FlacReader`.
pub struct FlacIntoSamples<R: io::Read> {
    inner: FlacSamples<R>,
}

/// Read the FLAC stream header.
///
/// This function can be used to quickly check if the file could be a flac file
/// by reading 4 bytes of the header. If an `Ok` is returned, the file is likely
/// a flac file. If an `Err` is returned, the file is definitely not a flac
/// file.
pub fn read_flac_header<R: io::Read>(input: &mut R) -> Result<()> {
    // A FLAC stream starts with a 32-bit header 'fLaC' (big endian).
    const FLAC_HEADER: u32 = 0x66_4c_61_43;

    // Some files start with ID3 tag data. The reference decoder supports this
    // for convenience. Claxon does not, but we can at least generate a helpful
    // error message if a file starts like this.
    const ID3_HEADER: u32 = 0x49_44_33_00;

    let header = input.read_be_u32()?;
    if header != FLAC_HEADER {
        if (header & 0xff_ff_ff_00) == ID3_HEADER {
            fmt_err("stream starts with ID3 header rather than FLAC header")
        } else {
            fmt_err("invalid stream header")
        }
    } else {
        Ok(())
    }
}

/// Read a FLAC file until the start of the audio data.
///
/// This reads the header, the STREAMINFO block, and skips over any other
/// metadata blocks. After this, the reader is positioned at the start of the
/// first frame, for use with `FlacReader::new_frame_aligned`.
pub fn read_until_audio<R: io::Read>(input: &mut R) -> Result<StreamInfo> {
    // TODO: Sort this skip out.
    use input::ReadBytes;

    read_flac_header(input)?;

    // Every FLAC file must contain a STREAMINFO block, and it must be first.
    let mut block = metadata::read_block_header(input)?;
    let streaminfo = match block.block_type {
        metadata::BlockType::StreamInfo => metadata::read_streaminfo_block(input)?,
        _ => return fmt_err("invalid metadata block, expected STREAMINFO first"),
    };

    // Skip over any other blocks.
    while !block.is_last {
        block = metadata::read_block_header(input)?;
        input.skip(block.length)?;
    }

    Ok(streaminfo)
}

/// Read the tag metadata (Vorbis comment block) from a FLAC stream.
///
/// This reads the FLAC header, skips over any uninteresting metadata blocks,
/// and then reads the VORBIS_COMMENT block, if present. If there are any other
/// metadata blocks after the VORBIS_COMMENT block, they are left unconsumed.
pub fn read_vorbis_comment<R: io::Read>(input: &mut R) -> Result<OptionalVorbisComment> {
    // TODO: Sort this skip out.
    use input::ReadBytes;

    read_flac_header(input)?;

    loop {
        let block = metadata::read_block_header(input)?;
        match block.block_type {
            metadata::BlockType::VorbisComment => {
                let vc = metadata::read_vorbis_comment_block(input, block.length)?;
                return Ok(OptionalVorbisComment(Some(vc)));
            }
            _ => input.skip(block.length)?,
        }

        if block.is_last {
            return Ok(OptionalVorbisComment(None));
        }
    }
}

/// Read the tag metadata (Vorbis comment block) from a FLAC file.
///
/// This is a convenience that opens the file and calls
/// [`read_vorbis_comment`](fn.read_vorbis_comment.html).
pub fn open_vorbis_comment<P: AsRef<path::Path>>(filename: P) -> Result<OptionalVorbisComment> {
    let file = fs::File::open(filename)?;
    let mut reader = io::BufReader::new(file);
    read_vorbis_comment(&mut reader)
}

impl<R: io::Read> FlacReader<R> {
    /// Create a reader that reads the FLAC format.
    ///
    /// The header is read immediately. Audio frames will be read on demand.
    ///
    /// Claxon rejects files that claim to contain excessively large metadata
    /// blocks, to protect against denial of service attacks where a
    /// small damaged or malicous file could cause gigabytes of memory
    /// to be allocated. `Error::Unsupported` is returned in that case.
    pub fn new(mut input: R) -> Result<FlacReader<R>> {
        let streaminfo = read_until_audio(&mut input)?;
        Ok(FlacReader::new_frame_aligned(input, streaminfo))
    }

    /// Create a reader, assuming the input reader is positioned at a frame header.
    ///
    /// This constructor takes a reader that is positioned at the start of the
    /// first frame (the start of the audio data). This is useful when the
    /// preceding metadata was read manually. After consuming the final
    /// metadata block (the block that has `is_last: true` in its header), the
    /// underlying reader will be positioned at the start of the first frame
    /// header. Constructing a `FlacReader` at this point requires the
    /// [`StreamInfo`][streaminfo] metadata block.
    ///
    /// [streaminfo]: metadata/struct.StreamInfo.html
    pub fn new_frame_aligned(input: R, streaminfo: StreamInfo) -> FlacReader<R> {
        FlacReader {
            streaminfo: streaminfo,
            input: input,
        }
    }

    /// Returns the streaminfo metadata.
    ///
    /// This contains information like the sample rate and number of channels.
    pub fn streaminfo(&self) -> StreamInfo {
        self.streaminfo
    }

    /// Returns an iterator that decodes a single frame on every iteration.
    /// TODO: It is not an iterator.
    ///
    /// This is a low-level primitive that gives you control over when decoding
    /// happens. The representation of the decoded audio is somewhat specific to
    /// the FLAC format. For a higher-level interface, see `samples()`.
    pub fn blocks<'r>(&'r mut self) -> FrameReader<&'r mut R> {
        FrameReader::new(&mut self.input)
    }

    /// Returns an iterator over all samples.
    ///
    /// The channel data is is interleaved. The iterator is streaming. That is,
    /// if you call this method once, read a few samples, and call this method
    /// again, the second iterator will not start again from the beginning of
    /// the file. It will continue somewhere after where the first iterator
    /// stopped, and it might skip some samples. (This is because FLAC divides
    /// a stream into blocks, which have to be decoded entirely. If you drop the
    /// iterator, you lose the unread samples in that block.)
    ///
    /// This is a user-friendly interface that trades performance for ease of
    /// use. If performance is an issue, consider using `blocks()` instead.
    ///
    /// This is a high-level interface to the decoder. The cost of retrieving
    /// the next sample can vary significantly, as sometimes a new block has to
    /// be decoded. Additionally, there is a cost to every iteration returning a
    /// `Result`. When a block has been decoded, iterating the samples in that
    /// block can never fail, but a match on every sample is required
    /// nonetheless. For more control over when decoding happens, and less error
    /// handling overhead, use `blocks()`.
    pub fn samples<'r>(&'r mut self) -> FlacSamples<&'r mut R> {
        FlacSamples {
            frame_reader: frame::FrameReader::new(&mut self.input),
            block: Block::empty(),
            sample: 0,
            channel: 0,
            has_failed: false,
        }
    }

    /// Same as `samples`, but takes ownership of the `FlacReader`.
    ///
    /// See `samples()` for more info.
    pub fn into_samples(self) -> FlacIntoSamples<R> {
        FlacIntoSamples {
            inner: FlacSamples {
                frame_reader: frame::FrameReader::new(self.input),
                block: Block::empty(),
                sample: 0,
                channel: 0,
                has_failed: false,
            },
        }
    }

    /// Destroys the FLAC reader and returns the underlying reader.
    pub fn into_inner(self) -> R {
        self.input
    }
}

impl FlacReader<fs::File> {
    /// Attempts to create a reader that reads from the specified file.
    ///
    /// This is a convenience constructor that opens a `File`, wraps it in a
    /// `BufReader`, and constructs a `FlacReader` from it.
    pub fn open<P: AsRef<path::Path>>(filename: P) -> Result<FlacReader<io::BufReader<fs::File>>> {
        let file = fs::File::open(filename)?;
        FlacReader::new(io::BufReader::new(file))
    }
}

impl<R: io::Read> Iterator for FlacSamples<R> {
    type Item = Result<i32>;

    fn next(&mut self) -> Option<Result<i32>> {
        // If the previous read failed, end iteration.
        if self.has_failed {
            return None;
        }

        // Iterate the samples channel interleaved, so first increment the
        // channel.
        self.channel += 1;

        // If that was the last channel, increment the sample number.
        if self.channel >= self.block.channels() {
            self.channel = 0;
            self.sample += 1;

            // If that was the last sample in the block, decode the next block.
            if self.sample >= self.block.duration() {
                self.sample = 0;

                // Replace the current block with an empty one so that we may
                // reuse the current buffer to decode again.
                let current_block = mem::replace(&mut self.block, Block::empty());

                match self.frame_reader.read_next_or_eof(
                    current_block.into_buffer(),
                ) {
                    Ok(Some(next_block)) => {
                        self.block = next_block;
                    }
                    Ok(None) => {
                        // The stream ended with EOF.
                        // TODO: If a number of samples was specified in the
                        // streaminfo metadata block, verify that we did not
                        // read more or less samples.
                        return None;
                    }
                    Err(error) => {
                        self.has_failed = true;
                        return Some(Err(error));
                    }
                }
            }
        }

        Some(Ok(self.block.sample(self.channel, self.sample)))
    }
}

impl<R: io::Read> Iterator for FlacIntoSamples<R> {
    type Item = Result<i32>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}
