// Claxon -- A FLAC decoding library in Rust
// Copyright 2014 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

//! The `metadata` module deals with metadata at the beginning of a FLAC stream.

use error::{Error, Result, fmt_err};
use input::ReadBytes;
use std::str;
use std::slice;

#[derive(Clone, Copy)]
struct MetadataBlockHeader {
    is_last: bool,
    block_type: u8,
    length: u32,
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
    /// maximum block duration in seconds, divide by the sample rate. To avoid
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

/// A seek point in the seek table.
#[derive(Clone, Copy)]
pub struct SeekPoint {
    /// Sample number of the first sample in the target frame, or 2<sup>64</sup> - 1 for a placeholder.
    pub sample: u64,
    /// Offset in bytes from the first byte of the first frame header to the first byte of the
    /// target frame's header.
    pub offset: u64,
    /// Number of samples in the target frame.
    pub samples: u16,
}

/// A seek table to aid seeking in the stream.
pub struct SeekTable {
    /// The seek points, sorted in ascending order by sample number.
    #[allow(dead_code)] // TODO: Implement seeking.
    seekpoints: Vec<SeekPoint>,
}

/// Vorbis comments, also known as FLAC tags (e.g. artist, title, etc.).
pub struct VorbisComment {
    /// The “vendor string”, chosen by the encoder vendor.
    ///
    /// This string usually contains the name and version of the program that
    /// encoded the FLAC stream, such as `reference libFLAC 1.3.2 20170101`
    /// or `Lavf57.25.100`.
    pub vendor: String,

    /// Name-value pairs of Vorbis comments, such as `ARTIST=Queen`.
    ///
    /// This struct stores a raw low-level representation of tags. Use
    /// `FlacReader::tags()` for a friendlier iterator. The tuple consists of
    /// the string in `"NAME=value"` format, and the index of the `'='` into
    /// that string.
    ///
    /// The name is supposed to be interpreted case-insensitively, and is
    /// guaranteed to consist of ASCII characters. Claxon does not normalize
    /// the casing of the name. Use `metadata::GetTag` to do a case-insensitive
    /// lookup.
    ///
    /// Names need not be unique. For instance, multiple `ARTIST` comments might
    /// be present on a collaboration track.
    ///
    /// See <https://www.xiph.org/vorbis/doc/v-comment.html> for more details.
    pub comments: Vec<(String, usize)>,
}

/// A metadata about the flac stream.
pub enum MetadataBlock {
    /// A stream info block.
    StreamInfo(StreamInfo),
    /// A padding block (with no meaningful data).
    Padding {
        /// The number of padding bytes.
        length: u32,
    },
    /// An application block with application-specific data.
    Application {
        /// The registered application ID.
        id: u32,
        /// The contents of the application block.
        data: Vec<u8>,
    },
    /// A seek table block.
    SeekTable(SeekTable),
    /// A Vorbis comment block, also known as FLAC tags.
    VorbisComment(VorbisComment),
    /// A CUE sheet block.
    CueSheet, // TODO
    /// A picture block.
    Picture, // TODO
    /// A block with a reserved block type, not supported by this library.
    Reserved,
}

/// Iterates over Vorbis comments (FLAC tags) in a FLAC stream.
///
/// See `FlacReader::tags()` for more details.
pub struct Tags<'a> {
    /// The underlying iterator.
    iter: slice::Iter<'a, (String, usize)>,
}

impl<'a> Tags<'a> {
    /// Returns a new `Tags` iterator.
    #[inline]
    pub fn new(comments: &'a [(String, usize)]) -> Tags<'a> {
        Tags {
            iter: comments.iter(),
        }
    }
}

impl<'a> Iterator for Tags<'a> {
    type Item = (&'a str, &'a str);

    #[inline]
    fn next(&mut self) -> Option<(&'a str, &'a str)> {
        return self.iter.next().map(|&(ref comment, sep_idx)| {
            (&comment[..sep_idx], &comment[sep_idx+1..])
        })
    }
    
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<'a> ExactSizeIterator for Tags<'a> {}

/// Iterates over Vorbis comments looking for a specific one; returns its values as `&str`.
///
/// See `FlacReader::get_tag()` for more details.
pub struct GetTag<'a> {
    /// The Vorbis comments to search through.
    vorbis_comments: &'a [(String, usize)],
    /// The tag to look for.
    needle: &'a str,
    /// The index of the (name, value) pair that should be inspected next.
    index: usize,
}

impl<'a> GetTag<'a> {
    /// Returns a new `GetTag` iterator.
    #[inline]
    pub fn new(vorbis_comments: &'a [(String, usize)], needle: &'a str) -> GetTag<'a> {
        GetTag {
            vorbis_comments: vorbis_comments,
            needle: needle,
            index: 0,
        }
    }
}

impl<'a> Iterator for GetTag<'a> {
    type Item = &'a str;

    #[inline]
    fn next(&mut self) -> Option<&'a str> {
        // This import is actually required on Rust 1.13.
        #[allow(unused_imports)]
        use std::ascii::AsciiExt;

        while self.index < self.vorbis_comments.len() {
            let (ref comment, sep_idx) = self.vorbis_comments[self.index];
            self.index += 1;

            if comment[..sep_idx].eq_ignore_ascii_case(self.needle) {
                return Some(&comment[sep_idx + 1..])
            }
        }

        return None
    }
}

#[inline]
fn read_metadata_block_header<R: ReadBytes>(input: &mut R) -> Result<MetadataBlockHeader> {
    let byte = try!(input.read_u8());

    // The first bit specifies whether this is the last block, the next 7 bits
    // specify the type of the metadata block to follow.
    let is_last = (byte >> 7) == 1;
    let block_type = byte & 0b0111_1111;

    // The length field is 24 bits, or 3 bytes.
    let length = try!(input.read_be_u24());

    let header = MetadataBlockHeader {
        is_last: is_last,
        block_type: block_type,
        length: length,
    };
    Ok(header)
}

/// Read a single metadata block header and body from the input.
///
/// When reading a regular flac stream, there is no need to use this function
/// directly; constructing a `FlacReader` will read the header and its metadata
/// blocks.
///
/// When a flac stream is embedded in a container format, this function can be
/// used to decode a single metadata block. For instance, the Ogg format embeds
/// metadata blocks including their header verbatim in packets. This function
/// can be used to decode that raw data.
#[inline]
pub fn read_metadata_block_with_header<R: ReadBytes>(input: &mut R)
                                                     -> Result<MetadataBlock> {
  let header = try!(read_metadata_block_header(input));
  read_metadata_block(input, header.block_type, header.length)
}

/// Read a single metadata block of the given type and length from the input.
///
/// When reading a regular flac stream, there is no need to use this function
/// directly; constructing a `FlacReader` will read the header and its metadata
/// blocks.
///
/// When a flac stream is embedded in a container format, this function can be
/// used to decode a single metadata block. For instance, the MP4 format sports
/// a “FLAC Specific Box” which contains the block type and the raw data. This
/// function can be used to decode that raw data.
#[inline]
pub fn read_metadata_block<R: ReadBytes>(input: &mut R,
                                         block_type: u8,
                                         length: u32)
                                         -> Result<MetadataBlock> {
    match block_type {
        0 => {
            // The streaminfo block has a fixed size of 34 bytes.
            if length == 34 {
                let streaminfo = try!(read_streaminfo_block(input));
                Ok(MetadataBlock::StreamInfo(streaminfo))
            } else {
                fmt_err("invalid streaminfo metadata block length")
            }
        }
        1 => {
            try!(read_padding_block(input, length));
            Ok(MetadataBlock::Padding { length: length })
        }
        2 => {
            let (id, data) = try!(read_application_block(input, length));
            Ok(MetadataBlock::Application {
                id: id,
                data: data,
            })
        }
        3 => {
            // TODO: implement seektable reading. For now, pretend it is padding.
            try!(input.skip(length));
            Ok(MetadataBlock::Padding { length: length })
        }
        4 => {
            let vorbis_comment = try!(read_vorbis_comment_block(input, length));
            Ok(MetadataBlock::VorbisComment(vorbis_comment))
        }
        5 => {
            // TODO: implement CUE sheet reading. For now, pretend it is padding.
            try!(input.skip(length));
            Ok(MetadataBlock::Padding { length: length })
        }
        6 => {
            // TODO: implement picture reading. For now, pretend it is padding.
            try!(input.skip(length));
            Ok(MetadataBlock::Padding { length: length })
        }
        127 => {
            // This code is invalid to avoid confusion with a frame sync code.
            fmt_err("invalid metadata block type")
        }
        _ => {
            // Any other block type is 'reserved' at the moment of writing. The
            // reference implementation reads it as an 'unknown' block. That is
            // one way of handling it, but maybe there should be some kind of
            // 'strict' mode (configurable at compile time?) so that this can
            // be an error if desired.
            try!(input.skip(length));
            Ok(MetadataBlock::Reserved)
        }
    }
}

fn read_streaminfo_block<R: ReadBytes>(input: &mut R) -> Result<StreamInfo> {
    let min_block_size = try!(input.read_be_u16());
    let max_block_size = try!(input.read_be_u16());

    // The frame size fields are 24 bits, or 3 bytes.
    let min_frame_size = try!(input.read_be_u24());
    let max_frame_size = try!(input.read_be_u24());

    // Next up are 20 bits that determine the sample rate.
    let sample_rate_msb = try!(input.read_be_u16());
    let sample_rate_lsb = try!(input.read_u8());

    // Stitch together the value from the first 16 bits,
    // and then the 4 most significant bits of the next byte.
    let sample_rate = (sample_rate_msb as u32) << 4 | (sample_rate_lsb as u32) >> 4;

    // Next three bits are the number of channels - 1. Mask them out and add 1.
    let n_channels_bps = sample_rate_lsb;
    let n_channels = ((n_channels_bps >> 1) & 0b0000_0111) + 1;

    // The final bit is the most significant of bits per sample - 1. Bits per
    // sample - 1 is 5 bits in total.
    let bps_msb = n_channels_bps & 1;
    let bps_lsb_n_samples = try!(input.read_u8());

    // Stitch together these values, add 1 because # - 1 is stored.
    let bits_per_sample = (bps_msb << 4 | (bps_lsb_n_samples >> 4)) + 1;

    // Number of samples in 36 bits, we have 4 already, 32 to go.
    let n_samples_msb = bps_lsb_n_samples & 0b0000_1111;
    let n_samples_lsb = try!(input.read_be_u32());
    let n_samples = (n_samples_msb as u64) << 32 | n_samples_lsb as u64;

    // Next are 128 bits (16 bytes) of MD5 signature.
    let mut md5sum = [0u8; 16];
    try!(input.read_into(&mut md5sum));

    // Lower bounds can never be larger than upper bounds. Note that 0 indicates
    // unknown for the frame size. Also, the block size must be at least 16.
    if min_block_size > max_block_size {
        return fmt_err("inconsistent bounds, min block size > max block size");
    }
    if min_block_size < 16 {
        return fmt_err("invalid block size, must be at least 16");
    }
    if min_frame_size > max_frame_size && max_frame_size != 0 {
        return fmt_err("inconsistent bounds, min frame size > max frame size");
    }

    // A sample rate of 0 is invalid, and the maximum sample rate is limited by
    // the structure of the frame headers to 655350 Hz.
    if sample_rate == 0 || sample_rate > 655350 {
        return fmt_err("invalid sample rate");
    }

    let stream_info = StreamInfo {
        min_block_size: min_block_size,
        max_block_size: max_block_size,
        min_frame_size: if min_frame_size == 0 {
            None
        } else {
            Some(min_frame_size)
        },
        max_frame_size: if max_frame_size == 0 {
            None
        } else {
            Some(max_frame_size)
        },
        sample_rate: sample_rate,
        channels: n_channels as u32,
        bits_per_sample: bits_per_sample as u32,
        samples: if n_samples == 0 {
            None
        } else {
            Some(n_samples)
        },
        md5sum: md5sum,
    };
    Ok(stream_info)
}

fn read_vorbis_comment_block<R: ReadBytes>(input: &mut R, length: u32) -> Result<VorbisComment> {
    if length < 8 {
        // We expect at a minimum a 32-bit vendor string length, and a 32-bit
        // comment count.
        return fmt_err("Vorbis comment block is too short")
    }

    // Fail if the length of the Vorbis comment block is larger than 1 MiB. This
    // block is full of length-prefixed strings for which we allocate memory up
    // front. If there were no limit on these, a maliciously crafted file could
    // cause OOM by claiming to contain large strings. But at least the strings
    // cannot be longer than the size of the Vorbis comment block, and by
    // limiting the size of that block, we can mitigate such DoS attacks.
    //
    // The typical size of a the Vorbis comment block is 1 KiB; on a corpus of
    // real-world flac files, the 0.05 and 0.95 quantiles were 792 and 1257
    // bytes respectively, with even the 0.99 quantile below 2 KiB. The only
    // reason for having a large Vorbis comment block is when cover art is
    // incorrectly embedded there, but the Vorbis comment block is not the right
    // place for that anyway.
    if length > 10 * 1024 * 1024 {
        let msg = "Vorbis comment blocks larger than 10 MiB are not supported";
        return Err(Error::Unsupported(msg))
    }

    // The Vorbis comment block starts with a length-prefixed "vendor string".
    // It cannot be larger than the block length - 8, because there are the
    // 32-bit vendor string length, and comment count.
    let vendor_len = try!(input.read_le_u32());
    if vendor_len > length - 8 { return fmt_err("vendor string too long") }
    let mut vendor_bytes = Vec::with_capacity(vendor_len as usize);

    // We can safely set the lenght of the vector here; the uninitialized memory
    // is not exposed. If `read_into` succeeds, it will have overwritten all
    // bytes. If not, an error is returned and the memory is never exposed.
    unsafe { vendor_bytes.set_len(vendor_len as usize); }
    try!(input.read_into(&mut vendor_bytes));
    let vendor = try!(String::from_utf8(vendor_bytes));

    // Next up is the number of comments. Because every comment is at least 4
    // bytes to indicate its length, there cannot be more comments than the
    // length of the block divided by 4. This is only an upper bound to ensure
    // that we don't allocate a big vector, to protect against DoS attacks.
    let mut comments_len = try!(input.read_le_u32());
    if comments_len >= length / 4 {
        return fmt_err("too many entries for Vorbis comment block")
    }
    let mut comments = Vec::with_capacity(comments_len as usize);

    let mut bytes_left = length - 8 - vendor_len;

    // For every comment, there is a length-prefixed string of the form
    // "NAME=value".
    while bytes_left >= 4 && comments.len() < comments_len as usize {
        let comment_len = try!(input.read_le_u32());
        bytes_left -= 4;

        if comment_len > bytes_left {
            return fmt_err("Vorbis comment too long for Vorbis comment block")
        }

        // Some older versions of libflac allowed writing zero-length Vorbis
        // comments. ALthough such files are invalid, they do occur in the wild,
        // so we skip over the empty comment.
        if comment_len == 0 {
            // Does not overflow because `comments_len > comments.len() >= 0`.
            comments_len -= 1;
            continue;
        }

        // For the same reason as above, setting the length is safe here.
        let mut comment_bytes = Vec::with_capacity(comment_len as usize);
        unsafe { comment_bytes.set_len(comment_len as usize); }
        try!(input.read_into(&mut comment_bytes));

        bytes_left -= comment_len;

        if let Some(sep_index) = comment_bytes.iter().position(|&x| x == b'=') {
            {
                let name_bytes = &comment_bytes[..sep_index];

                // According to the Vorbis spec, the field name may consist of ascii
                // bytes 0x20 through 0x7d, 0x3d (`=`) excluded. Verifying this has
                // the advantage that if the check passes, the result is valid
                // UTF-8, so the conversion to string will not fail.
                if name_bytes.iter().any(|&x| x < 0x20 || x > 0x7d) {
                    return fmt_err("Vorbis comment field name contains invalid byte")
                }
            }

            let comment = try!(String::from_utf8(comment_bytes));
            comments.push((comment, sep_index));
        } else {
            return fmt_err("Vorbis comment does not contain '='")
        }
    }

    if bytes_left != 0 {
        return fmt_err("Vorbis comment block has excess data")
    }

    if comments.len() != comments_len as usize {
        return fmt_err("Vorbis comment block contains wrong number of entries")
    }

    let vorbis_comment = VorbisComment {
        vendor: vendor,
        comments: comments,
    };

    Ok(vorbis_comment)
}

fn read_padding_block<R: ReadBytes>(input: &mut R, length: u32) -> Result<()> {
    // The specification dictates that all bits of the padding block must be 0.
    // However, the reference implementation does not issue an error when this
    // is not the case, and frankly, when you are going to skip over these
    // bytes and do nothing with them whatsoever, why waste all those CPU
    // cycles checking that the padding is valid?
    Ok(try!(input.skip(length)))
}

fn read_application_block<R: ReadBytes>(input: &mut R, length: u32) -> Result<(u32, Vec<u8>)> {
    if length < 4 {
        return fmt_err("application block length must be at least 4 bytes")
    }

    // Reject large application blocks to avoid memory-based denial-
    // of-service attacks. See also the more elaborate motivation in
    // `read_vorbis_comment_block()`.
    if length > 10 * 1024 * 1024 {
        let msg = "application blocks larger than 10 MiB are not supported";
        return Err(Error::Unsupported(msg))
    }

    let id = try!(input.read_be_u32());

    // Four bytes of the block have been used for the ID, the rest is payload.
    // Create a vector of uninitialized memory, and read the block into it. The
    // uninitialized memory is never exposed: read_into will either fill the
    // buffer completely, or return an err, in which case the memory is not
    // exposed.
    let mut data = Vec::with_capacity(length as usize - 4);
    unsafe { data.set_len(length as usize - 4); }
    try!(input.read_into(&mut data));

    Ok((id, data))
}

/// Reads metadata blocks from a stream and exposes them as an iterator.
///
/// It is assumed that the next byte that the reader will read, is the first
/// byte of a metadata block header. This means that the iterator will yield at
/// least a single value. If the iterator ever yields an error, then no more
/// data will be read thereafter, and the next value will be `None`.
pub struct MetadataBlockReader<R: ReadBytes> {
    input: R,
    done: bool,
}

/// Either a `MetadataBlock` or an `Error`.
pub type MetadataBlockResult = Result<MetadataBlock>;

impl<R: ReadBytes> MetadataBlockReader<R> {
    /// Creates a metadata block reader that will yield at least one element.
    pub fn new(input: R) -> MetadataBlockReader<R> {
        MetadataBlockReader {
            input: input,
            done: false,
        }
    }

    #[inline]
    fn read_next(&mut self) -> MetadataBlockResult {
        let header = try!(read_metadata_block_header(&mut self.input));
        let block = try!(read_metadata_block(&mut self.input, header.block_type, header.length));
        self.done = header.is_last;
        Ok(block)
    }
}

impl<R: ReadBytes> Iterator for MetadataBlockReader<R> {
    type Item = MetadataBlockResult;

    #[inline]
    fn next(&mut self) -> Option<MetadataBlockResult> {
        if self.done {
            None
        } else {
            let block = self.read_next();

            // After a failure, no more attempts to read will be made,
            // because we don't know where we are in the stream.
            if !block.is_ok() {
                self.done = true;
            }

            Some(block)
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        // When done, there will be no more blocks,
        // when not done, there will be at least one more.
        if self.done { (0, Some(0)) } else { (1, None) }
    }
}
