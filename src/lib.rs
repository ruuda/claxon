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
//! let reader = claxon::FlacReader::open("testsamples/pop.flac").unwrap();
//! for artist in reader.get_tag("ARTIST") {
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
use metadata::{MetadataBlock, MetadataBlockReader, Picture, PictureKind, StreamInfo, VorbisComment};
use metadata2::{SeekTable};

mod crc;
mod error;
pub mod frame;
pub mod input;
pub mod metadata;
pub mod subframe;
pub mod metadata2;

pub use error::{Error, Result};
pub use frame::Block;
pub use metadata2::MetadataReader;

/// A FLAC decoder that can decode the stream from the underlying reader.
///
/// TODO: Add an example.
pub struct FlacReader<R: io::Read> {
    streaminfo: StreamInfo,
    vorbis_comment: Option<VorbisComment>,
    pictures: Vec<Picture>,
    input: R,
}

/// Determines how to read picture metadata.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ReadPicture {
    /// Do not read picture metadata at all.
    Skip,

    /// Record the offset of the first front cover picture in the stream.
    ///
    /// Claxon will skip over the image itself to avoid allocating memory, but
    /// record the offset and length in the stream. The picture can be extracted
    /// at a later time by seeking to the offset.
    ///
    /// If multiple front cover pictures are present, this will only
    /// record the first one. Pictures that have a different kind than
    /// `PictureKind::FrontCover` are skipped altogether.
    CoverAsOffset,

    /// Record the offset of all pictures in the stream.
    ///
    /// Unlike `CoverAsOffset`, all picture blocks are recorded.
    AllAsOffset,

    /// Read the first front cover picture into a `Vec`.
    ///
    /// If multiple front cover pictures are present, this will only
    /// read the first one. Pictures that have a different kind than
    /// `PictureKind::FrontCover` are skipped altogether.
    CoverAsVec,

    /// Read all pictures into `Vec`s.
    ///
    /// Unlike `CoverAsVec`, all picture blocks are read.
    AllAsVec,
}

/// Controls what metadata `FlacReader` reads when constructed.
///
/// The FLAC format contains a number of metadata blocks before the start of
/// audio data. Reading these is wasteful if the data is never used. The
/// `FlacReaderOptions` indicate which blocks to look for. As soon as all
/// desired blocks have been read, `FlacReader::new_ext()` returns without
/// reading remaining metadata blocks.
///
/// A few use cases:
///
/// * To read only the streaminfo, as quickly as possible, set `metadata_only`
///   to true, `read_vorbis_comment` to false, and `read_picture` to `Skip`.
///   The resulting reader cannot be used to read audio data.
/// * To read only the streaminfo and tags, set `metadata_only` and
///   `read_vorbis_comment` both to true, but `read_picture` to `Skip`. The
///   resulting reader cannot be used to read audio data.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FlacReaderOptions {
    /// When true, return a reader as soon as all desired metadata has been read.
    ///
    /// If this is set, the `FlacReader` will not be able to read audio samples.
    /// When reading audio is not desired anyway, enabling `metadata_only` can
    /// save a lot of expensive reads.
    ///
    /// Defaults to false.
    pub metadata_only: bool,

    /// When true, read metadata blocks at least until a Vorbis comment block is found.
    ///
    /// When false, the `FlacReader` will be constructed without reading a
    /// Vorbis comment block, even if the stream contains one. Consequently,
    /// `FlacReader::tags()` and other tag-related methods will not return tag
    /// data.
    ///
    /// Defaults to true.
    pub read_vorbis_comment: bool,

    /// When not `Skip`, read metadata blocks at least until a picture block is found.
    ///
    /// When `Skip`, the `FlacReader` will be constructed without reading a
    /// picture block, even if the stream contains one. When `CoverAsOffset` or
    /// `CoverAsVec`, the `FlacReader` will be constructed with at most one
    /// picture, the front cover, even if the stream contained more.
    ///
    /// Defaults to `ReadPicture::AllAsVec`.
    pub read_picture: ReadPicture,
}

impl Default for FlacReaderOptions {
    fn default() -> FlacReaderOptions {
        FlacReaderOptions {
            metadata_only: false,
            read_vorbis_comment: true,
            read_picture: ReadPicture::AllAsVec,
        }
    }
}

impl FlacReaderOptions {
    /// Return whether any metadata blocks need to be read.
    fn has_desired_blocks(&self) -> bool {
        // If we do not want only metadata, we want everything. Hence there are
        // desired blocks left.
        if !self.metadata_only {
            return true
        }

        let pictures_left = match self.read_picture {
            ReadPicture::Skip => false,
            _ => true,
        };

        self.read_vorbis_comment || pictures_left
    }
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
pub struct FlacIntoSamples<R: ReadBytes> {
    // This works because `ReadBytes` is implemented for both `&mut R` and `R`.
    inner: FlacSamples<R>,
}

/// Read the FLAC stream header.
///
/// This function can be used to quickly check if the file could be a flac file
/// by reading 4 bytes of the header. If an `Ok` is returned, the file is likely
/// a flac file. If an `Err` is returned, the file is definitely not a flac
/// file.
pub fn read_flac_header<R: io::Read>(mut input: R) -> Result<()> {
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
    /// Create a reader that reads the FLAC format.
    ///
    /// The header and metadata blocks are read immediately. Audio frames
    /// will be read on demand.
    ///
    /// Claxon rejects files that claim to contain excessively large metadata
    /// blocks, to protect against denial of service attacks where a
    /// small damaged or malicous file could cause gigabytes of memory
    /// to be allocated. `Error::Unsupported` is returned in that case.
    pub fn new(reader: R) -> Result<FlacReader<R>> {
        FlacReader::new_ext(reader, FlacReaderOptions::default())
    }

    /// Create a reader, assuming the input reader is positioned at a frame header.
    ///
    /// This constructor takes a reader that is positioned at the start of the
    /// first frame (the start of the audio data). This is useful when the
    /// preceding metadata was read manually using a
    /// [`MetadataReader`][metadatareader]. After
    /// [`MetadataReader::next()`][metadatareader-next] returns `None`, the
    /// underlying reader will be positioned at the start of the first frame
    /// header. Constructing a `FlacReader` at this point requires the
    /// [`StreamInfo`][streaminfo] metadata block, and optionally a
    /// [`SeekTable`][seektable] to aid seeking. Seeking is not implemented
    /// yet.
    ///
    /// [metadatareader]:      metadata2/struct.MetadataReader.html
    /// [metadatareader-next]: metadata2/struct.MetadataReader.html#method.next
    /// [streaminfo]:          metadata2/struct.StreamInfo.html
    /// [seektable]:           metadata2/struct.SeekTable.html
    // TODO: Patch url when renaming metadata2.
    pub fn new_frame_aligned(input: R, streaminfo: StreamInfo, seektable: Option<SeekTable>) -> FlacReader<R> {
        // Ignore the seek table for now. When we implement seeking in the
        // future, it will be stored in the FlacReader and used for seeking. We
        // already take it as a constructor argument now, to avoid breaking
        // changes in the future.
        let _ = seektable;
        FlacReader {
            streaminfo: streaminfo,
            vorbis_comment: None,
            pictures: Vec::new(),
            input: input,
        }
    }

    /// Create a reader that reads the FLAC format, with reader options.
    ///
    /// The header and metadata blocks are read immediately, but only as much as
    /// specified in the options. See `FlacReaderOptions` for more details.
    ///
    /// Claxon rejects files that claim to contain excessively large metadata
    /// blocks, to protect against denial of service attacks where a
    /// small damaged or malicous file could cause gigabytes of memory
    /// to be allocated. `Error::Unsupported` is returned in that case.
    pub fn new_ext(mut input: R, options: FlacReaderOptions) -> Result<FlacReader<R>> {
        let mut opts_current = options;

        // A flac stream first of all starts with a stream header.
        try!(read_flac_header(&mut input));

        let mut pictures = Vec::new();

        // Start a new scope, because the input reader must be available again
        // for the frame reader next.
        let (streaminfo, vorbis_comment) = {
            // Next are one or more metadata blocks. The flac specification
            // dictates that the streaminfo block is the first block. The metadata
            // block reader will yield at least one element, so the unwrap is safe.
            let mut metadata_iter = MetadataBlockReader::new(&mut input);
            metadata_iter.read_picture_as_vec = match options.read_picture {
                ReadPicture::AllAsVec => true,
                ReadPicture::CoverAsVec => true,
                _ => false,
            };
            let streaminfo_block = try!(metadata_iter.next().unwrap());
            let streaminfo = match streaminfo_block {
                MetadataBlock::StreamInfo(info) => info,
                _ => return fmt_err("streaminfo block missing"),
            };

            let mut vorbis_comment = None;

            // There might be more metadata blocks, read and store them.
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

                        // We have one, no new one is desired.
                        opts_current.read_vorbis_comment = false;
                    }
                    MetadataBlock::StreamInfo(..) => {
                        return fmt_err("encountered second streaminfo block")
                    }
                    MetadataBlock::Picture(p) => {
                        match opts_current.read_picture {
                            ReadPicture::CoverAsVec | ReadPicture::CoverAsOffset => {
                                // If this was the cover that we were looking
                                // for, there is no need to read further images.
                                if p.kind == PictureKind::FrontCover {
                                    pictures.push(p);
                                    opts_current.read_picture = ReadPicture::Skip;
                                }
                            }
                            ReadPicture::AllAsVec | ReadPicture::AllAsOffset => {
                                pictures.push(p);
                            }
                            ReadPicture::Skip => {}
                        }
                    }
                    // Other blocks are currently not handled.
                    _block => {}
                }

                // Early-out reading metadata once all desired blocks have been
                // collected.
                if !opts_current.has_desired_blocks() {
                    break
                }
            }

            // TODO: Rather than discarding afterwards, never parse it in the
            // first place; treat it like padding in the MetadataBlockReader.
            if !options.read_vorbis_comment {
                vorbis_comment = None;
            }

            (streaminfo, vorbis_comment)
        };

        // The flac reader will contain the reader that will read frames.
        let flac_reader = FlacReader {
            streaminfo: streaminfo,
            vorbis_comment: vorbis_comment,
            pictures: pictures,
            input: input,
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
    /// encoded the FLAC stream, such as `reference libFLAC 1.3.2 20170101`
    /// or `Lavf57.25.100`.
    pub fn vendor(&self) -> Option<&str> {
        self.vorbis_comment.as_ref().map(|vc| &vc.vendor[..])
    }

    /// Returns name-value pairs of Vorbis comments, such as `("ARTIST", "Queen")`.
    ///
    /// The name is supposed to be interpreted case-insensitively, and is
    /// guaranteed to consist of ASCII characters. Claxon does not normalize
    /// the casing of the name. Use `get_tag()` to do a case-insensitive lookup.
    ///
    /// Names need not be unique. For instance, multiple `ARTIST` comments might
    /// be present on a collaboration track.
    ///
    /// See <https://www.xiph.org/vorbis/doc/v-comment.html> for more details.
    pub fn tags<'a>(&'a self) -> metadata::Tags<'a> {
        match self.vorbis_comment.as_ref() {
            Some(vc) => metadata::Tags::new(&vc.comments[..]),
            None => metadata::Tags::new(&[]),
        }
    }

    /// Look up a Vorbis comment such as `ARTIST` in a case-insensitive way.
    ///
    /// Returns an iterator,  because tags may occur more than once. There could
    /// be multiple `ARTIST` tags on a collaboration track, for instance.
    ///
    /// Note that tag names are ASCII and never contain `'='`; trying to look up
    /// a non-ASCII tag will return no results. Furthermore, the Vorbis comment
    /// spec dictates that tag names should be handled case-insensitively, so
    /// this method performs a case-insensitive lookup.
    ///
    /// See also `tags()` for access to the raw tags.
    /// See <https://www.xiph.org/vorbis/doc/v-comment.html> for more details.
    pub fn get_tag<'a>(&'a self, tag_name: &'a str) -> metadata::GetTag<'a> {
        match self.vorbis_comment.as_ref() {
            Some(vc) => metadata::GetTag::new(&vc.comments[..], tag_name),
            None => metadata::GetTag::new(&[], tag_name),
        }
    }

    /// Returns the pictures embedded in the stream.
    pub fn pictures(&self) -> &[Picture] {
        &self.pictures[..]
    }

    /// Take ownership of the pictures in the stream, destroying the reader.
    pub fn into_pictures(self) -> Vec<Picture> {
        self.pictures
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
            }
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
        let file = try!(fs::File::open(filename));
        FlacReader::new(io::BufReader::new(file))
    }

    /// Attemps to create a reader that reads from the specified file.
    ///
    /// This is a convenience constructor that opens a `File`, wraps it in a
    /// `BufReader`, and constructs a `FlacReader` from it.
    pub fn open_ext<P: AsRef<path::Path>>(filename: P,
                                          options: FlacReaderOptions)
                                          -> Result<FlacReader<io::BufReader<fs::File>>> {
        let file = try!(fs::File::open(filename));
        FlacReader::new_ext(io::BufReader::new(file), options)
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

impl<R: ReadBytes> Iterator for FlacIntoSamples<R> {
    type Item = Result<i32>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}
