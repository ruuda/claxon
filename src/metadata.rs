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

use crate::FlacReaderOptions;

#[derive(Clone, Copy)]
struct MetadataBlockHeader {
    is_last: bool,
    block_type: u8,
    length: u32,
}

/// The streaminfo metadata block, with important information about the stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Copy, PartialEq, Eq)]
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

/// A picture embedded in a FLAC file.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Picture {
    /// The picture type. Generally, if present, FrontCover is displayed during playback.
    pub picture_type: PictureType,
    /// The description of the picture.
    pub description: Option<String>,
    /// The width of the picture in pixels.
    ///
    /// Applications MUST NOT use this field in decoding the picture or deciding how to display it,
    /// but applications MAY use it to decide whether or not to process a block (e.g., when
    /// selecting between different picture blocks) and MAY show it to the user. May be None if a
    /// picture has no concept for width (e.g., vector images) or the content of this field is
    /// unknown.
    pub width: Option<u32>,
    /// The height of the picture in pixels.
    ///
    /// Applications MUST NOT use this field in decoding the picture or deciding how to display it,
    /// but applications MAY use it to decide whether or not to process a block (e.g., when
    /// selecting between different picture blocks) and MAY show it to the user. May be None if a
    /// picture has no concept for height (e.g., vector images) or the content of this field is
    /// unknown.
    pub height: Option<u32>,
    /// The color depth of the picture in bits per pixel.
    ///
    /// Applications MUST NOT use this field in decoding the picture or deciding how to display it,
    /// but applications MAY use it to decide whether or not to process a block (e.g., when
    /// selecting between different picture blocks) and MAY show it to the user. May be None if a
    /// picture has no concept for color depth or the content of this field is unknown.
    pub color_depth: Option<u32>,
    /// For indexed-color pictures (e.g., GIF), the number of colors used
    ///
    /// Applications MUST NOT use this field in decoding the picture or deciding how to display it,
    /// but applications MAY use it to decide whether or not to process a block (e.g., when
    /// selecting between different picture blocks) and MAY show it to the user. May be None if a
    /// picture has no concept for color count (e.g, PNG) or the content of this field is unknown.
    pub color_count: Option<u32>,
    /// The picture contents.
    pub contents: PictureContents,
}

/// The contents of the picture.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PictureContents {
    /// The header contains a reference to the picture in the form of a URI as specified by
    /// RFC 3986. The following points apply:
    /// - The URI can be in either absolute or relative form. If a URI is in relative form, it is
    /// related to the URI of the FLAC content processed.
    /// - Applications MUST obtain explicit user approval to retrieve images via remote protocols
    /// and to retrieve local images that are not located in the same directory as the FLAC file
    /// being processed.
    /// - Applications supporting linked images MUST handle unavailability of URIs gracefully. They
    /// MAY report unavailability to the user.
    /// - Applications MAY reject processing URIs for any reason, particularly for security or
    /// privacy reasons.
    URI(String),
    /// The raw picture data is embedded directly in the metadata block.
    Data {
        /// The media type string as specified by RFC 2046.
        media_type: String,
        /// The raw data of the picture.
        data: Vec<u8>
    }
}

/// Picture type, derived from the ID3v2 standard. Generally, if present, FrontCover is displayed
/// during playback.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PictureType {
    /// Anything that does not fall under other categories
    Other,
    /// PNG file icon of 32x32 pixels.
    ///
    /// There may only be one picture of this type in a file.
    FileIcon32,
    /// General File Icon
    ///
    /// There may only be one picture of this type in a file.
    GeneralFileIcon,
    /// Front Cover
    ///
    /// Generally, if present, this is displayed during playback.
    FrontCover,
    /// Back Cover
    BackCover,
    /// Liner Notes Page
    LinerNotesPage,
    /// Media label (e.g., CD, Vinyl or Cassette label)
    MediaLabel,
    /// Lead artist, lead performer, or soloist
    LeadArtist,
    /// Artist or performer
    Artist,
    /// Conductor
    Conductor,
    /// Band or orchestra
    Band,
    /// Composer
    Composer,
    /// Lyricist or text writer
    Lyricist,
    /// Recording location
    RecordingLocation,
    /// During recording
    DuringRecording,
    /// During performance
    DuringPerformance,
    /// Movie or video screen capture
    Video,
    /// A bright colored fish.
    ///
    /// The origin and use of this value is unclear. This was copied to maintain compatibility with
    /// ID3v2. Applications are discouraged from offering this value to users when embedding a
    /// picture.
    Fish,
    /// Illustration
    Illustration,
    /// Band or artist logotype
    BandLogotype,
    /// Publisher or studio logotype
    PublisherLogotype,
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
    Picture(Picture),
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

    read_metadata_block_ext(input, block_type, length, &FlacReaderOptions::default())

}


/// Same as [read_metadata_block], but takes [FlacReaderOptions] as an argument to decide whether to
/// skip parsing some blocks.
#[inline]
pub fn read_metadata_block_ext<R: ReadBytes>(input: &mut R,
                                             block_type: u8,
                                             length: u32,
                                             options: &FlacReaderOptions)
                                             -> Result<MetadataBlock> {
    match block_type {
        0 => {
            // The streaminfo block has a fixed size of 34 bytes.
            if length == 34 {
                let streaminfo = read_streaminfo_block(input)?;
                Ok(MetadataBlock::StreamInfo(streaminfo))
            } else {
                fmt_err("invalid streaminfo metadata block length")
            }
        }
        1 => {
            read_padding_block(input, length)?;
            Ok(MetadataBlock::Padding { length: length })
        }
        2 => {
            let (id, data) = read_application_block(input, length)?;
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
            if options.read_vorbis_comment {
                let vorbis_comment = read_vorbis_comment_block(input, length)?;
                Ok(MetadataBlock::VorbisComment(vorbis_comment))
            } else {
                input.skip(length)?;
                Ok(MetadataBlock::Padding { length: length })
            }
        }
        5 => {
            // TODO: implement CUE sheet reading. For now, pretend it is padding.
            input.skip(length)?;
            Ok(MetadataBlock::Padding { length: length })
        }
        6 => {
            if options.read_picture {
                let picture = read_picture_block(input, length)?;
                Ok(MetadataBlock::Picture(picture))
            } else {
                input.skip(length)?;
                Ok(MetadataBlock::Padding { length: length })
            }
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
            input.skip(length)?;
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

    let vendor = input.read_utf8(vendor_len as usize)?;

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

        let comment_bytes = input.read_n(comment_len as usize)?;

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

fn read_picture_block<R: ReadBytes>(input: &mut R, length: u32) -> Result<Picture> {
    if length < 32 {
        return fmt_err("picture block length must be at least 32 bytes")
    }

    let picture_type = match input.read_be_u32()? {
        0  => PictureType::Other,
        1  => PictureType::FileIcon32,
        2  => PictureType::GeneralFileIcon,
        3  => PictureType::FrontCover,
        4  => PictureType::BackCover,
        5  => PictureType::LinerNotesPage,
        6  => PictureType::MediaLabel,
        7  => PictureType::LeadArtist,
        8  => PictureType::Artist,
        9  => PictureType::Conductor,
        10 => PictureType::Band,
        11 => PictureType::Composer,
        12 => PictureType::Lyricist,
        13 => PictureType::RecordingLocation,
        14 => PictureType::DuringRecording,
        15 => PictureType::DuringPerformance,
        16 => PictureType::Video,
        17 => PictureType::Fish,
        18 => PictureType::Illustration,
        19 => PictureType::BandLogotype,
        20 => PictureType::PublisherLogotype,
        _ => return fmt_err("invalid picture type")
    };

    let media_type_len = input.read_be_u32()?;
    let mut bytes_left = length - 8;
    if media_type_len > bytes_left - 24 {
        return fmt_err("media type string is longer than the block size allows")
    }
    bytes_left -= media_type_len;

    let media_type_bytes = input.read_n(media_type_len as usize)?;

    if media_type_bytes.iter().any(|b| !(0x20..=0x7E).contains(b)) {
        return fmt_err("media type string is not printable ASCII characters 0x20-0x7E")
    }
    // already checked that the bytes are printable standard ASCII, and all standard ASCII is valid
    // utf8
    let media_type = unsafe { String::from_utf8_unchecked(media_type_bytes) };

    let description_len = input.read_be_u32()?;
    bytes_left -= 4;

    if description_len > bytes_left - 20 {
        return fmt_err("description is longer than the block size allows")
    }
    bytes_left -= description_len;

    let description = input.read_utf8(description_len as usize)?;

    let width = input.read_be_u32()?;
    let height = input.read_be_u32()?;
    let color_depth = input.read_be_u32()?;
    let color_count = input.read_be_u32()?;
    let data_len = input.read_be_u32()?;
    bytes_left -= 20;
    if data_len != bytes_left {
        return fmt_err("block size does not match picture data size")
    }

    let data = input.read_n(data_len as usize)?;

    Ok(Picture {
        picture_type,
        description: if description_len > 0 { Some(description) } else { None },
        width:       if width != 0          { Some(width)       } else { None },
        height:      if height != 0         { Some(height)      } else { None },
        color_depth: if color_depth != 0    { Some(color_depth) } else { None },
        color_count: if color_count != 0    { Some(color_count) } else { None },
        contents: if media_type == "-->" {
            PictureContents::URI(String::from_utf8(data)?)
        } else {
            PictureContents::Data { media_type, data }
        }
    })
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
    let data = input.read_n(length as usize - 4)?;

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
    options: FlacReaderOptions
}

/// Either a `MetadataBlock` or an `Error`.
pub type MetadataBlockResult = Result<MetadataBlock>;

impl<R: ReadBytes> MetadataBlockReader<R> {
    /// Creates a metadata block reader that will yield at least one element.
    pub fn new(input: R) -> MetadataBlockReader<R> {
        MetadataBlockReader {
            input: input,
            done: false,
            options: FlacReaderOptions::default()
        }
    }

    /// Creates a metadata block reader that will yield at least one element, using the given
    /// options to read or discard specific elements.
    pub fn new_ext(input: R, options: FlacReaderOptions) -> MetadataBlockReader<R> {
        MetadataBlockReader {
            input,
            done: false,
            options
        }
    }

    #[inline]
    fn read_next(&mut self) -> MetadataBlockResult {
        let header = read_metadata_block_header(&mut self.input)?;
        let block = read_metadata_block_ext(&mut self.input, header.block_type, header.length, &self.options)?;
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
