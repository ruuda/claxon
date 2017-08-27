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
//! For more examples, see the [examples](https://github.com/ruuda/claxon/tree/master/examples)
//! directory in the crate.

#![warn(missing_docs)]

use std::fs;
use std::io;
use std::mem;
use std::path;
use error::fmt_err;
use frame::FrameReader;
use input::{BufferedReader, ReadBytes};
use metadata::{MetadataBlock, MetadataBlockReader, StreamInfo, VorbisComment};

mod crc;
mod error;
pub mod frame;
pub mod input;
pub mod metadata;
pub mod subframe;

pub use error::{Error, Result};
pub use frame::Block;

/// A FLAC decoder that can decode the stream from the underlying reader.
///
/// TODO: Add an example.
pub struct FlacReader<R: io::Read> {
    streaminfo: StreamInfo,
    vorbis_comment: Option<VorbisComment>,
    #[allow(dead_code)] // TODO: Expose metadata nicely.
    metadata_blocks: Vec<MetadataBlock>,
    input: BufferedReader<R>,
}

/// An iterator that yields samples read from a `FlacReader`.
pub struct FlacSamples<R: ReadBytes> {
    frame_reader: FrameReader<R>,
    block: Block,
    sample: u32,
    channel: u32,

    /// If reading ever failed, this flag is set, so that the iterator knows not
    /// to return any new values.
    has_failed: bool,
}

// TODO: Add a `FlacIntoSamples`.

fn read_stream_header<R: ReadBytes>(input: &mut R) -> Result<()> {
    // A FLAC stream starts with a 32-bit header 'fLaC' (big endian).
    const FLAC_HEADER: u32 = 0x66_4c_61_43;

    // Some files start with ID3 tag data. The reference decoder supports this
    // for convenience. Claxon does not, but we can at least generate a helpful
    // error message if a file starts like this.
    const ID3_HEADER: u32 = 0x49_44_33_00;

    let header = try!(input.read_be_u32());
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

impl<R: io::Read> FlacReader<R> {
    /// Attempts to create a reader that reads the FLAC format.
    ///
    /// The header and metadata blocks are read immediately. Audio frames will
    /// be read on demand.
    pub fn new(reader: R) -> Result<FlacReader<R>> {
        let mut buf_reader = BufferedReader::new(reader);

        // A flac stream first of all starts with a stream header.
        try!(read_stream_header(&mut buf_reader));

        // Start a new scope, because the input reader must be available again
        // for the frame reader next.
        let (streaminfo, vorbis_comment, metadata_blocks) = {
            // Next are one or more metadata blocks. The flac specification
            // dictates that the streaminfo block is the first block. The metadata
            // block reader will yield at least one element, so the unwrap is safe.
            let mut metadata_iter = MetadataBlockReader::new(&mut buf_reader);
            let streaminfo_block = try!(metadata_iter.next().unwrap());
            let streaminfo = match streaminfo_block {
                MetadataBlock::StreamInfo(info) => info,
                _ => return fmt_err("streaminfo block missing"),
            };

            let mut vorbis_comment = None;

            // There might be more metadata blocks, read and store them.
            let mut metadata_blocks = Vec::new();
            for block_result in metadata_iter {
                match try!(block_result) {
                    MetadataBlock::VorbisComment(vc) => {
                        // The Vorbis comment block need not be present, but
                        // when it is, it must be unique.
                        if vorbis_comment.is_some() {
                            return fmt_err("encountered second Vorbis comment block")
                        } else {
                            vorbis_comment = Some(vc);
                        }
                    }
                    MetadataBlock::StreamInfo(..) => {
                        return fmt_err("encountered second streaminfo block")
                    }
                    // Other blocks are currently not handled.
                    block => metadata_blocks.push(block),
                }
            }

            (streaminfo, vorbis_comment, metadata_blocks)
        };

        // The flac reader will contain the reader that will read frames.
        let flac_reader = FlacReader {
            streaminfo: streaminfo,
            vorbis_comment: vorbis_comment,
            metadata_blocks: metadata_blocks,
            input: buf_reader,
        };

        Ok(flac_reader)
    }

    /// Returns the streaminfo metadata.
    ///
    /// This contains information like the sample rate and number of channels.
    pub fn streaminfo(&self) -> StreamInfo {
        self.streaminfo
    }

    /// Returns the vendor string of the Vorbis comment block, if present.
    ///
    /// This string usually contains the name and version of the program that
    /// produced the FLAC stream, such as `reference libFLAC 1.3.2 20170101`
    /// or `Lavf57.25.100`.
    pub fn vendor(&self) -> Option<&str> {
        self.vorbis_comment.as_ref().map(|vc| &vc.vendor[..])
    }

    /// Returns an iterator that decodes a single frame on every iteration.
    /// TODO: It is not an iterator.
    ///
    /// This is a low-level primitive that gives you control over when decoding
    /// happens. The representation of the decoded audio is somewhat specific to
    /// the FLAC format. For a higher-level interface, see `samples()`.
    pub fn blocks<'r>(&'r mut self) -> FrameReader<&'r mut BufferedReader<R>> {
        FrameReader::new(&mut self.input)
    }

    /// Returns an iterator over all samples.
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
    ///
    /// The channel data is is interleaved. The iterator is streaming. That is,
    /// if you call this method once, read a few samples, and call this method
    /// again, the second iterator will not start again from the beginning of
    /// the file. It will continue somewhere after where the first iterator
    /// stopped, and it might skip some samples. (This is because FLAC divides
    /// a stream into blocks, which have to be decoded entirely. If you drop the
    /// iterator, you lose the unread samples in that block.)
    pub fn samples<'r>(&'r mut self) -> FlacSamples<&'r mut BufferedReader<R>> {
        FlacSamples {
            frame_reader: frame::FrameReader::new(&mut self.input),
            block: Block::empty(),
            sample: 0,
            channel: 0,
            has_failed: false,
        }
    }

    /// Destroys the FLAC reader and returns the underlying reader.
    ///
    /// Because the reader employs buffering internally, anything in the buffer
    /// will be lost.
    pub fn into_inner(self) -> R {
        self.input.into_inner()
    }
}

impl FlacReader<fs::File> {
    /// Attempts to create a reader that reads from the specified file.
    ///
    /// This is a convenience constructor that opens a `File`, and constructs a
    /// `FlacReader` from it. There is no need to wrap the file in a
    /// `BufReader`, as the `FlacReader` employs buffering already.
    pub fn open<P: AsRef<path::Path>>(filename: P) -> Result<FlacReader<fs::File>> {
        let file = try!(fs::File::open(filename));
        FlacReader::new(file)
    }
}

impl<R: ReadBytes> Iterator for FlacSamples<R> {
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

                match self.frame_reader.read_next_or_eof(current_block.into_buffer()) {
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
