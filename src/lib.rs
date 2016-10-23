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
//! The following example computes the root mean square (RMS) of an audio file
//! with at most 16 bits per sample.
//!
//! ```
//! use claxon;
//!
//! let mut reader = claxon::FlacReader::open("testsamples/pop.flac").unwrap();
//! let mut sqr_sum = 0.0;
//! let mut count = 0;
//! for sample in reader.samples::<i16>() {
//!     let s = sample.unwrap() as f64;
//!     sqr_sum += s * s;
//!     count += 1;
//! }
//! println!("RMS is {}", (sqr_sum / count as f64).sqrt());
//! ```
//!
//! TODO: more examples.

#![warn(missing_docs)]

use std::fs;
use std::io;
use std::mem;
use std::path;
use error::fmt_err;
use frame::FrameReader;
use input::ReadExt;
use metadata::{MetadataBlock, MetadataBlockReader, StreamInfo};

mod crc;
mod input;
mod error;
pub mod frame;
pub mod sample;
pub mod subframe;
pub mod metadata;

pub use error::{Error, Result};

/// A FLAC decoder that can decode the stream from the underlying reader.
///
/// TODO: Is stream a good name? Should it be called reader/decoder?
/// TODO: Add an example.
pub struct FlacReader<R: io::Read> {
    streaminfo: StreamInfo,
    #[allow(dead_code)] // TODO: Expose metadata nicely.
    metadata_blocks: Vec<MetadataBlock>,
    input: R,
}

/// An iterator that yields samples of type `S` read from a `FlacReader`.
///
/// The type `S` must have at least as many bits as the bits per sample of the
/// stream, otherwise every iteration will return an error.
pub struct FlacSamples<'fr, R: 'fr + io::Read, S: sample::Sample> {
    frame_reader: FrameReader<&'fr mut R, S>,
    block: frame::Block<S>,
    sample: u16,
    channel: u8,

    /// If reading ever failed, this flag is set, so that the iterator knows not
    /// to return any new values.
    has_failed: bool,
}

// TODO: Add a `FlacIntoSamples`.

fn read_stream_header<R: io::Read>(input: &mut R) -> Result<()> {
    // A FLAC stream starts with a 32-bit header 'fLaC' (big endian).
    const HEADER: u32 = 0x66_4c_61_43;
    let header = try!(input.read_be_u32());
    if header != HEADER {
        fmt_err("invalid stream header")
    } else {
        Ok(())
    }
}

impl<R: io::Read> FlacReader<R> {
    /// Attempts to create a reader that reads the FLAC format.
    ///
    /// The header and metadata blocks are read immediately. Audio frames will
    /// be read on demand.
    pub fn new(mut reader: R) -> Result<FlacReader<R>> {
        // A flac stream first of all starts with a stream header.
        try!(read_stream_header(&mut reader));

        // Start a new scope, because the input reader must be available again
        // for the frame reader next.
        let (streaminfo, metadata_blocks) = {
            // Next are one or more metadata blocks. The flac specification
            // dictates that the streaminfo block is the first block. The metadata
            // block reader will yield at least one element, so the unwrap is safe.
            let mut metadata_iter = MetadataBlockReader::new(&mut reader);
            let streaminfo_block = try!(metadata_iter.next().unwrap());
            let streaminfo = match streaminfo_block {
                MetadataBlock::StreamInfo(info) => info,
                _ => return fmt_err("streaminfo block missing"),
            };

            // There might be more metadata blocks, read and store them.
            let mut metadata_blocks = Vec::new();
            for block_result in metadata_iter {
                match block_result {
                    Err(error) => return Err(error),
                    Ok(block) => metadata_blocks.push(block),
                }
            }

            (streaminfo, metadata_blocks)
        };

        // The flac reader will contain the reader that will read frames.
        let flac_reader = FlacReader {
            streaminfo: streaminfo,
            metadata_blocks: metadata_blocks,
            input: reader,
        };

        Ok(flac_reader)
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
    pub fn blocks<'r, S: sample::Sample>(&'r mut self) -> FrameReader<&'r mut R, S> {
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
    /// The type `S` must have at least `streaminfo().bits_per_sample` bits,
    /// otherwise iteration will return an error. All bit depths up to 32 bits
    /// per sample can be decoded into an `i32`, but if you know beforehand that
    /// you will be reading a file with 16 bits per sample, you can save memory
    /// by decoding into an `i16`.
    ///
    /// This is a high-level interface to the decoder. The cost of retrieving
    /// the next sample can vary significantly, as sometimes a new block has to
    /// be decoded. For more control over when decoding happens, use `blocks()`.
    pub fn samples<'r, S: sample::Sample>(&'r mut self) -> FlacSamples<'r, R, S> {
        FlacSamples {
            frame_reader: frame::FrameReader::new(&mut self.input),
            block: frame::Block::empty(),
            sample: 0,
            channel: 0,
            has_failed: false,
        }
    }
}

impl FlacReader<io::BufReader<fs::File>> {
    /// Attempts to create a reader that reads from the specified file.
    ///
    /// This is a convenience constructor that opens a `File`, wraps it in a
    /// `BufReader` and then constructs a `FlacReader` from it.
    pub fn open<P: AsRef<path::Path>>(filename: P) -> Result<FlacReader<io::BufReader<fs::File>>> {
        let file = try!(fs::File::open(filename));
        let buf_reader = io::BufReader::new(file);
        FlacReader::new(buf_reader)
    }
}

impl<'fr, R: 'fr + io::Read, S: sample::Sample> Iterator for FlacSamples<'fr, R, S> {
    type Item = Result<S>;

    fn next(&mut self) -> Option<Result<S>> {
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
            if self.sample >= self.block.len() {
                self.sample = 0;

                // Replace the current block with an empty one so that we may
                // reuse the current buffer to decode again.
                let current_block = mem::replace(&mut self.block, frame::Block::empty());

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
