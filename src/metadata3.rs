// Claxon -- A FLAC decoding library in Rust
// Copyright 2021 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

//! The `metadata` module deals with metadata at the beginning of a FLAC stream.

use std::io;
use std::slice;
use std::iter::FusedIterator;

use error::{Error, Result, fmt_err};
use input::ReadBytes;

/// The different kinds of metadata block defined by the FLAC format.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum BlockType {
    /// A STREAMINFO block, with technical details about the stream.
    ///
    /// Use [`read_streaminfo_block`](fn.read_streaminfo_block.html) to read.
    StreamInfo = 0,

    /// A PADDING block, filled with zeros.
    ///
    /// To read, skip over `header.length` bytes.
    Padding = 1,

    /// An APPLICATION block that holds application-defined data.
    ///
    /// Use [`read_application_block`](fn.read_application_block.html) to read
    /// the application id.
    Application = 2,

    /// A SEEKTABLE block, with data for supporting faster seeks.
    ///
    /// There is currently no support for reading the seek table.
    SeekTable = 3,

    /// A VORBIS_COMMENT block, with metadata tags.
    ///
    /// Use [`read_vorbis_comment_block`](fn.read_vorbis_comment_block.html) to read.
    VorbisComment = 4,

    /// A CUESHEET block.
    ///
    /// There is currently no support for reading the CUE sheet.
    CueSheet = 5,

    /// A PICTURE block, with cover art or other image metadata.
    Picture = 6,
}

/// A metadata block header.
#[derive(Clone, Copy)]
pub struct BlockHeader {
    /// Whether this is the last metadata block before the audio data.
    pub is_last: bool,
    /// The type of metadata block.
    pub block_type: BlockType,
    /// Length of the metadata block in bytes, excluding this header.
    pub length: u32,
}

/// Read a metadata block header.
///
/// How the contents of the block shoult be interpreted depends on the type of
/// block, and there are dedicated functions to read each block type. It is
/// always possible to skip over the block by skipping `header.length` bytes
/// after reading the header.
#[inline]
pub fn read_block_header<R: io::Read>(input: &mut R) -> Result<BlockHeader> {
    let byte = input.read_u8()?;

    // The first bit specifies whether this is the last block, the next 7 bits
    // specify the type of the metadata block to follow.
    let is_last = (byte >> 7) == 1;
    let block_type_u8 = byte & 0b0111_1111;

    // The length field is 24 bits, or 3 bytes.
    let length = input.read_be_u24()?;

    let block_type = match block_type_u8 {
        0 => BlockType::StreamInfo,
        1 => BlockType::Padding,
        2 => BlockType::Application,
        3 => BlockType::SeekTable,
        4 => BlockType::VorbisComment,
        5 => BlockType::CueSheet,
        6 => BlockType::Picture,
        127 => {
            // This code is invalid to avoid confusion with a frame sync code.
            return fmt_err("invalid metadata block type");
        }
        _ => {
            // Any other block type is 'reserved' at the moment of writing.
            return fmt_err("invalid metadata block, encountered reserved block type");
        }
    };

    // The STREAMINFO block contains no variable-size parts.
    if block_type == BlockType::StreamInfo && length != 34 {
        return fmt_err("invalid streaminfo metadata block length");
    }

    let header = BlockHeader {
        is_last,
        block_type,
        length,
    };
    Ok(header)
}

/// The streaminfo metadata block, with technical information about the stream.
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

/// Read a STREAMINFO block.
pub fn read_streaminfo_block<R: io::Read>(input: &mut R) -> Result<StreamInfo> {
    let min_block_size = input.read_be_u16()?;
    let max_block_size = input.read_be_u16()?;

    // The frame size fields are 24 bits, or 3 bytes.
    let min_frame_size = input.read_be_u24()?;
    let max_frame_size = input.read_be_u24()?;

    // Next up are 20 bits that determine the sample rate.
    let sample_rate_msb = input.read_be_u16()?;
    let sample_rate_lsb = input.read_u8()?;

    // Stitch together the value from the first 16 bits,
    // and then the 4 most significant bits of the next byte.
    let sample_rate = (sample_rate_msb as u32) << 4 | (sample_rate_lsb as u32) >> 4;

    // Next three bits are the number of channels - 1. Mask them out and add 1.
    let n_channels_bps = sample_rate_lsb;
    let n_channels = ((n_channels_bps >> 1) & 0b0000_0111) + 1;

    // The final bit is the most significant of bits per sample - 1. Bits per
    // sample - 1 is 5 bits in total.
    let bps_msb = n_channels_bps & 1;
    let bps_lsb_n_samples = input.read_u8()?;

    // Stitch together these values, add 1 because # - 1 is stored.
    let bits_per_sample = (bps_msb << 4 | (bps_lsb_n_samples >> 4)) + 1;

    // Number of samples in 36 bits, we have 4 already, 32 to go.
    let n_samples_msb = bps_lsb_n_samples & 0b0000_1111;
    let n_samples_lsb = input.read_be_u32()?;
    let n_samples = (n_samples_msb as u64) << 32 | n_samples_lsb as u64;

    // Next are 128 bits (16 bytes) of MD5 signature.
    let mut md5sum = [0u8; 16];
    input.read_exact(&mut md5sum)?;

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

/// Application id used in an APPLICATION block.
///
/// Registered application ids are listed at <https://www.xiph.org/flac/id.html>.
pub struct ApplicationId(pub u32);

/// Read the application id from an APPLICATION block.
///
/// The first 4 bytes of an application block contain its id, the remaining
/// `header.length - 4` bytes contain application-specific data. This function
/// only consumes the first 4 bytes, not the application-specific data.
#[inline]
pub fn read_application_block<R: io::Read>(input: &mut R) -> Result<ApplicationId> {
    Ok(ApplicationId(input.read_be_u32()?))
}

/// A seek point in the seek table.
#[derive(Clone, Copy)]
pub struct SeekPoint {
    /// Sample number of the first sample in the target frame.
    ///
    /// Or 2<sup>64</sup> - 1 for a placeholder.
    pub sample: u64,

    /// Offset in bytes from the first byte of the first frame header to the
    /// first byte of the target frame’s header.
    pub offset: u64,

    /// Number of samples in the target frame.
    pub samples: u16,
}

/// A seek table to aid seeking in the stream.
pub struct SeekTable {
    /// The seek points, sorted in ascending order by sample number.
    pub seekpoints: Vec<SeekPoint>,
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
    /// This struct stores a raw low-level representation of tags. The tuple
    /// consists of the string in `"NAME=value"` format, and the index of the
    /// `'='` into that string.
    ///
    /// Use [`tags()`](#method.tags) for a friendlier iterator.
    ///
    /// The tag name is supposed to be interpreted case-insensitively, and is
    /// guaranteed to consist of ASCII characters. Claxon does not normalize
    /// the casing of the name. Use [`get_tag()`](#method.get_tag) to do a
    /// case-insensitive lookup.
    ///
    /// Names need not be unique. For instance, multiple `ARTIST` comments might
    /// be present on a collaboration track.
    ///
    /// See <https://www.xiph.org/vorbis/doc/v-comment.html> for more details.
    pub comments: Vec<(String, usize)>,
}

impl VorbisComment {
    /// Return name-value pairs of Vorbis comments, such as `("ARTIST", "Queen")`.
    ///
    /// The name is supposed to be interpreted case-insensitively, and is
    /// guaranteed to consist of ASCII characters. Claxon does not normalize
    /// the casing of the name. Use [`get_tag()`](#method.get_tag) to do a
    /// case-insensitive lookup.
    ///
    /// Names need not be unique. For instance, multiple `ARTIST` comments might
    /// be present on a collaboration track.
    ///
    /// See <https://www.xiph.org/vorbis/doc/v-comment.html> for more details
    /// about tag conventions.
    pub fn tags(&self) -> Tags {
        Tags::new(&self.comments)
    }

    /// Look up a Vorbis comment such as `ARTIST` in a case-insensitive way.
    ///
    /// Returns an iterator, because tags may occur more than once. There could
    /// be multiple `ARTIST` tags on a collaboration track, for instance.
    ///
    /// Note that tag names are ASCII and never contain `'='`; trying to look up
    /// a non-ASCII tag will return no results. Furthermore, the Vorbis comment
    /// spec dictates that tag names should be handled case-insensitively, so
    /// this method performs a case-insensitive lookup.
    ///
    /// See also [`tags()`](#method.tags) for access to all tags.
    /// See <https://www.xiph.org/vorbis/doc/v-comment.html> for more details
    /// about tag conventions.
    pub fn get_tag<'a>(&'a self, tag_name: &'a str) -> GetTag<'a> {
        GetTag::new(&self.comments, tag_name)
    }
}

impl<'a> IntoIterator for &'a VorbisComment {
    type Item = (&'a str, &'a str);
    type IntoIter = Tags<'a>;

    fn into_iter(self) -> Tags<'a> {
        self.tags()
    }
}

/// Iterates over Vorbis comments (FLAC tags) in a FLAC stream.
///
/// See [`VorbisComment::tags()`](struct.VorbisComment#method.tags) for more details.
pub struct Tags<'a> {
    /// The underlying iterator.
    iter: slice::Iter<'a, (String, usize)>,
}

impl<'a> Tags<'a> {
    /// Return a new `Tags` iterator.
    #[inline]
    fn new(comments: &'a [(String, usize)]) -> Tags<'a> {
        Tags { iter: comments.iter() }
    }
}

impl<'a> Iterator for Tags<'a> {
    type Item = (&'a str, &'a str);

    #[inline]
    fn next(&mut self) -> Option<(&'a str, &'a str)> {
        return self.iter.next().map(|&(ref comment, sep_idx)| {
            (&comment[..sep_idx], &comment[sep_idx + 1..])
        });
    }
}

impl<'a> ExactSizeIterator for Tags<'a> {
    #[inline]
    fn len(&self) -> usize {
        self.iter.len()
    }
}

impl<'a> FusedIterator for Tags<'a> {}

/// Iterates over Vorbis comments looking for a specific one; returns its values as `&str`.
///
/// See [`VorbisComment::get_tag()`](struct.VorbisComment#method.get_tag) for more details.
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
    fn new(vorbis_comments: &'a [(String, usize)], needle: &'a str) -> GetTag<'a> {
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
        while self.index < self.vorbis_comments.len() {
            let (ref comment, sep_idx) = self.vorbis_comments[self.index];
            self.index += 1;

            if comment[..sep_idx].eq_ignore_ascii_case(self.needle) {
                return Some(&comment[sep_idx + 1..]);
            }
        }

        return None;
    }
}

impl<'a> FusedIterator for GetTag<'a> {}

/// Read a VORBIS_COMMENT block.
///
/// Takes `header.length` as input.
///
/// To prevent malicious inputs from causing large allocations, this function
/// returns `Error::Unsupported` if the length is greater than 10 MiB. Use
/// [`read_vorbis_comment_block_unchecked`](fn.read_vorbis_comment_block_unchecked.html)
/// to sidestep this check.
pub fn read_vorbis_comment_block<R: io::Read>(input: &mut R, length: u32) -> Result<VorbisComment> {
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
        return Err(Error::Unsupported(msg));
    }

    // The unchecked variant is now safe; we performed the check above.
    read_vorbis_comment_block_unchecked(input, length)
}

/// Read a VORBIS_COMMENT block from a trusted source.
///
/// Takes `header.length` as input.
///
/// This function imposes no upper limit on the length of a block (other than
/// the limits of the FLAC format itself). Because the result contains
/// heap-allocated values that are preallocated to the right size, using this
/// function on untrusted inputs may trigger an out of memory condition.
///
/// Use [`read_vorbis_comment_block`](fn.read_vorbis_comment_block.html) to read
/// untrusted inputs.
pub fn read_vorbis_comment_block_unchecked<R: io::Read>(
    input: &mut R,
    length: u32,
) -> Result<VorbisComment> {
    if length < 8 {
        // We expect at a minimum a 32-bit vendor string length, and a 32-bit
        // comment count.
        return fmt_err("Vorbis comment block is too short");
    }

    // The Vorbis comment block starts with a length-prefixed "vendor string".
    // It cannot be larger than the block length - 8, because there are the
    // 32-bit vendor string length, and comment count.
    let vendor_len = input.read_le_u32()?;
    if vendor_len > length - 8 {
        return fmt_err("vendor string too long");
    }
    let mut vendor_bytes = Vec::with_capacity(vendor_len as usize);

    // We can safely set the length of the vector here; the uninitialized memory
    // is not exposed. If `read_exact` succeeds, it will have overwritten all
    // bytes. If not, an error is returned and the memory is never exposed.
    unsafe {
        vendor_bytes.set_len(vendor_len as usize);
    }
    input.read_exact(&mut vendor_bytes)?;
    let vendor = String::from_utf8(vendor_bytes)?;

    // Next up is the number of comments. Because every comment is at least 4
    // bytes to indicate its length, there cannot be more comments than the
    // length of the block divided by 4. This is only an upper bound to ensure
    // that we don't allocate a big vector, to protect against DoS attacks.
    let mut comments_len = input.read_le_u32()?;
    if comments_len >= length / 4 {
        return fmt_err("too many entries for Vorbis comment block");
    }
    let mut comments = Vec::with_capacity(comments_len as usize);

    let mut bytes_left = length - 8 - vendor_len;

    // For every comment, there is a length-prefixed string of the form
    // "NAME=value".
    while bytes_left >= 4 && comments.len() < comments_len as usize {
        let comment_len = input.read_le_u32()?;
        bytes_left -= 4;

        if comment_len > bytes_left {
            return fmt_err("Vorbis comment too long for Vorbis comment block");
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
        unsafe {
            comment_bytes.set_len(comment_len as usize);
        }
        input.read_exact(&mut comment_bytes)?;

        bytes_left -= comment_len;

        if let Some(sep_index) = comment_bytes.iter().position(|&x| x == b'=') {
            {
                let name_bytes = &comment_bytes[..sep_index];

                // According to the Vorbis spec, the field name may consist of ascii
                // bytes 0x20 through 0x7d, 0x3d (`=`) excluded. Verifying this has
                // the advantage that if the check passes, the result is valid
                // UTF-8, so the conversion to string will not fail.
                if name_bytes.iter().any(|&x| x < 0x20 || x > 0x7d) {
                    return fmt_err("Vorbis comment field name contains invalid byte");
                }
            }

            let comment = String::from_utf8(comment_bytes)?;
            comments.push((comment, sep_index));
        } else {
            return fmt_err("Vorbis comment does not contain '='");
        }
    }

    if bytes_left != 0 {
        return fmt_err("Vorbis comment block has excess data");
    }

    if comments.len() != comments_len as usize {
        return fmt_err("Vorbis comment block contains wrong number of entries");
    }

    let vorbis_comment = VorbisComment {
        vendor: vendor,
        comments: comments,
    };

    Ok(vorbis_comment)
}
