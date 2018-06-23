// Claxon -- A FLAC decoding library in Rust
// Copyright 2018 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

//! The `metadata` module deals with metadata at the beginning of a FLAC stream.

use std::io;

use error::{Error, Result, fmt_err};
use input::{EmbeddedReader, ReadBytes};

/// A metadata about the FLAC stream.
pub enum MetadataBlock<'a, R: 'a + io::Read> {
    /// The stream info block.
    StreamInfo(StreamInfo),

    /// A padding block, of a given number of `0x00` bytes.
    Padding(u32),

    /// A block with application-specific data.
    Application(ApplicationBlock<'a, R>),

    /// A lazy seek table block.
    ///
    /// Yields the [`SeekTable`](struct.SeekTable.html) from `get()` (not implemented yet).
    // TODO: get() docs when implemented.
    SeekTable(LazySeekTable<'a, R>),

    /// A lazy Vorbis comment block, also known as FLAC tags.
    ///
    /// Yields the [`VorbisComment`](struct.VorbisComment.html) from
    /// [`get()`](struct.LazyVorbisComment.html#method.get).
    VorbisComment(LazyVorbisComment<'a, R>),

    /// A lazy CUE sheet block.
    ///
    /// Yields the `CueSheet` from `get()` (not implemented yet).
    CueSheet(LazyCueSheet<'a, R>),

    /// A lazy picture block.
    ///
    /// Yields the [`Picture`](struct.Picture.html) from
    /// [`get()`](struct.LazyPicture.html#method.get).
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

/// A metadata block that holds application-specific data.
pub struct ApplicationBlock<'a, R: 'a + io::Read> {
    /// The application id, registered with Xiph.org.
    ///
    /// [The list of registered ids can be found on Xiph.org][ids].
    /// 
    /// [ids]: https://xiph.org/flac/id.html
    pub id: u32,

    /// A reader that exposes the embedded application-specific data.
    ///
    /// The reader is constrained to the application data, and will return EOF
    /// when that data ends. The reader can safely be dropped even if it was not
    /// consumed until the end.
    pub reader: EmbeddedReader<'a, R>,
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

/// The picture kind: front cover, leaflet, etc.
///
/// The FLAC format uses the picture kinds from the ID3v2 API frame.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum PictureKind {
    /// Front cover.
    FrontCover = 3,
    /// Back cover.
    BackCover = 4,
    /// Leaflet page.
    LeafletPage = 5,
    /// Media (e.g. label side of CD).
    Media = 6,
    /// Lead artist, lead performer, or soloist.
    LeadArtist = 7,
    /// Artist or performer.
    Artist = 8,
    /// Conductor.
    Conductor = 9,
    /// Band or orchestra.
    Band = 10,
    /// Composer.
    Composer = 11,
    /// Lyricist or text writer.
    Lyricist = 12,
    /// Recording location.
    RecordingLocation = 13,
    /// Picture taken during recording.
    DuringRecording = 14,
    /// Picture taken during performance.
    DuringPerformance = 15,
    /// A screen capture of a movie or video.
    VideoScreenCapture = 16,
    /// Bright colored fish, presumably a xiph.org joke.
    BrightColoredFish = 17,
    /// Illustration.
    Illustration = 18,
    /// Band or artist logotype.
    BandLogotype = 19,
    /// Publisher or studio logotype.
    PublisherLogotype = 20,
    /// 32x32 pixels png file icon.
    FileIcon32x32 = 1,
    /// Other file icon.
    FileIconOther = 2,
    /// Other.
    Other = 0,
}

/// Metadata about an embedded picture.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PictureMetadata {
    /// The picture kind: front cover, leaflet, etc.
    pub kind: PictureKind,

    /// MIME type of the picture.
    ///
    /// The type can also be `-->`, in which case the data should be interpreted
    /// as a URL rather than the actual picture data.
    pub mime_type: String,

    /// A description of the picture. Often empty in practice.
    pub description: String,

    /// The width of the picture in pixels.
    pub width: u32,

    /// The height of the picture in pixels.
    pub height: u32,
}

/// A block that embeds a picture (e.g. cover art).
pub struct Picture<'a, R: 'a + io::Read> {
    /// Metadata about the picture, such as its kind and dimensions.
    pub metadata: PictureMetadata,

    /// A reader that exposes the embedded picture data.
    ///
    /// The reader is constrained to the picture data, and will return EOF
    /// when the picture data ends. The reader can safely be dropped even if
    /// it was not consumed until the end.
    pub reader: EmbeddedReader<'a, R>
}

macro_rules! lazy_block_impl {
    ($typename: ident) => {
        impl <'a, R: 'a + io::Read> $typename<'a, R> {
            /// Skip over this metadata block without parsing anything.
            pub fn discard(mut self) -> io::Result<()> {
                let len = self.len;
                self.len = 0;
                self.input.skip(len)
            }
        }

        impl<'a, R: 'a + io::Read> Drop for $typename<'a, R> {
            fn drop(&mut self) {
                if self.len != 0 {
                    panic!("{} was dropped, call .discard() or .get() instead.", stringify!($typename))
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
    input: &'a mut R,
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
    input: &'a mut R,
    len: u32,
}

lazy_block_impl!(LazyVorbisComment);

impl <'a, R: 'a + io::Read> LazyVorbisComment<'a, R> {
    /// Read and parse the Vorbis comment block.
    pub fn get(mut self) -> Result<VorbisComment> {
        // Set len to zero before reading to indicate that we consumed the data,
        // dropping does not panic, also if reading the vorbis comment block
        // returns an error.
        let len = self.len;
        self.len = 0;
        read_vorbis_comment_block(self.input, len)
    }
}

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
    input: &'a mut R,
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
    input: &'a mut R,
    len: u32,
}

lazy_block_impl!(LazyPicture);

#[inline]
fn read_metadata_block_header<R: io::Read>(input: &mut R) -> Result<MetadataBlockHeader> {
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
pub fn read_metadata_block_with_header<'a, R>(input: &'a mut R) -> Result<MetadataBlock<'a, R>>
where R: 'a + io::Read {
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
pub fn read_metadata_block<'a, R>(input: &'a mut R,
                                  block_type: u8,
                                  length: u32)
                                  -> Result<MetadataBlock<'a, R>>
where R: 'a + io::Read {
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
            try!(input.skip(length));
            Ok(MetadataBlock::Padding(length))
        }
        2 => {
            let application_block = try!(read_application_block(input, length));
            Ok(MetadataBlock::Application(application_block))
        }
        3 => {
            let lazy_seek_table = LazySeekTable {
                input: input,
                len: length
            };
            Ok(MetadataBlock::SeekTable(lazy_seek_table))
        }
        4 => {
            let lazy_vorbis_comment = LazyVorbisComment {
                input: input,
                len: length,
            };
            Ok(MetadataBlock::VorbisComment(lazy_vorbis_comment))
        }
        5 => {
            let lazy_cue_sheet = LazyCueSheet {
                input: input,
                len: length,
            };
            Ok(MetadataBlock::CueSheet(lazy_cue_sheet))
        }
        6 => {
            let lazy_picture = LazyPicture {
                input: input,
                len: length,
            };
            Ok(MetadataBlock::Picture(lazy_picture))
        }
        127 => {
            // This code is invalid to avoid confusion with a frame sync code.
            fmt_err("invalid metadata block type")
        }
        _ => {
            // Any other block type is 'reserved' at the moment of writing.
            // TODO: Add test to ensure that after a reserved block, the next
            // block can be read properly.
            try!(input.skip(length));
            fmt_err("invalid metadata block, encountered reserved block type")
        }
    }
}

fn read_streaminfo_block<R: io::Read>(input: &mut R) -> Result<StreamInfo> {
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
    try!(input.read_exact(&mut md5sum));

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

fn read_vorbis_comment_block<R: io::Read>(input: &mut R, length: u32) -> Result<VorbisComment> {
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

    // We can safely set the length of the vector here; the uninitialized memory
    // is not exposed. If `read_exact` succeeds, it will have overwritten all
    // bytes. If not, an error is returned and the memory is never exposed.
    unsafe { vendor_bytes.set_len(vendor_len as usize); }
    try!(input.read_exact(&mut vendor_bytes));
    let vendor = try!(String::from_utf8(vendor_bytes));

    // Next up is the number of comments. Because every comment is at least 4
    // bytes to indicate its length, there cannot be more comments than the
    // length of the block divided by 4. This is only an upper bound to ensure
    // that we don't allocate a big vector, to protect against DoS attacks.
    let comments_len = try!(input.read_le_u32());
    if comments_len >= length / 4 {
        return fmt_err("too many entries for Vorbis comment block")
    }
    let mut comments = Vec::with_capacity(comments_len as usize);

    let mut bytes_left = length - 8 - vendor_len;

    // For every comment, there is a length-prefixed string of the form
    // "NAME=value".
    while bytes_left >= 4 {
        let comment_len = try!(input.read_le_u32());
        bytes_left -= 4;

        if comment_len > bytes_left {
            return fmt_err("Vorbis comment too long for Vorbis comment block")
        }

        // For the same reason as above, setting the length is safe here.
        let mut comment_bytes = Vec::with_capacity(comment_len as usize);
        unsafe { comment_bytes.set_len(comment_len as usize); }
        try!(input.read_exact(&mut comment_bytes));

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

    if comments.len() != comments_len as usize {
        return fmt_err("Vorbis comment block contains wrong number of entries")
    }

    let vorbis_comment = VorbisComment {
        vendor: vendor,
        comments: comments,
    };

    Ok(vorbis_comment)
}

fn read_padding_block<R: io::Read>(input: &mut R, length: u32) -> Result<()> {
    // The specification dictates that all bits of the padding block must be 0.
    // However, the reference implementation does not issue an error when this
    // is not the case, and frankly, when you are going to skip over these
    // bytes and do nothing with them whatsoever, why waste all those CPU
    // cycles checking that the padding is valid?
    Ok(try!(input.skip(length)))
}

fn read_application_block<'a, R: 'a + io::Read>(input: &'a mut R, length: u32) -> Result<ApplicationBlock<'a, R>> {
    if length < 4 {
        return fmt_err("application block length must be at least 4 bytes")
    }

    // Reject large application blocks to avoid memory-based denial-
    // of-service attacks. See also the more elaborate motivation in
    // `read_vorbis_comment_block()`.
    // TODO: Now that we expose an EmbeddedReader, this is no longer a concern
    // for Claxon. However, it might still be a concern for users who have not
    // considered the issue of an incorrect length field in the header. Should
    // we enfore this in the API somehow?
    if length > 10 * 1024 * 1024 {
        let msg = "application blocks larger than 10 MiB are not supported";
        return Err(Error::Unsupported(msg))
    }

    let id = try!(input.read_be_u32());

    let application = ApplicationBlock {
        id: id,
        reader: EmbeddedReader {
            input: input,
            cursor: 0,
            // The application id took 4 bytes, the remainder is data.
            len: length - 4,
        },
    };

    Ok(application)
}

fn read_picture_block<R: io::Read>(input: &mut R, length: u32) -> Result<Picture<R>> {
    if length < 32 {
        // We expect at a minimum 8 all of the 32-bit fields.
        return fmt_err("picture block is too short")
    }

    let picture_type = try!(input.read_be_u32());

    let kind = match picture_type {
        0 => PictureKind::Other,
        1 => PictureKind::FileIcon32x32,
        2 => PictureKind::FileIconOther,
        3 => PictureKind::FrontCover,
        4 => PictureKind::BackCover,
        5 => PictureKind::LeafletPage,
        6 => PictureKind::Media,
        7 => PictureKind::LeadArtist,
        8 => PictureKind::Artist,
        9 => PictureKind::Conductor,
        10 => PictureKind::Band,
        11 => PictureKind::Composer,
        12 => PictureKind::Lyricist,
        13 => PictureKind::RecordingLocation,
        14 => PictureKind::DuringRecording,
        15 => PictureKind::DuringPerformance,
        16 => PictureKind::VideoScreenCapture,
        17 => PictureKind::BrightColoredFish,
        18 => PictureKind::Illustration,
        19 => PictureKind::BandLogotype,
        20 => PictureKind::PublisherLogotype,
        // Picture types up to 20 are valid, others are reserved.
        _ => return fmt_err("invalid picture type"),
    };

    let mime_len = try!(input.read_be_u32());

    // The mime type string must fit within the picture block. Also put a limit
    // on the length, to ensure we don't allocate large strings, in order to
    // prevent denial of service attacks.
    if mime_len > length - 32 { return fmt_err("picture MIME type string too long") }
    if mime_len > 256 {
        let msg = "picture MIME types larger than 256 bytes are not supported";
        return Err(Error::Unsupported(msg))
    }
    let mut mime_bytes = Vec::with_capacity(mime_len as usize);

    // We can safely set the length of the vector here; the uninitialized memory
    // is not exposed. If `read_exact` succeeds, it will have overwritten all
    // bytes. If not, an error is returned and the memory is never exposed.
    unsafe { mime_bytes.set_len(mime_len as usize); }
    try!(input.read_exact(&mut mime_bytes));

    // According to the spec, the MIME type string must consist of printable
    // ASCII characters in the range 0x20-0x7e; validate that. This also means
    // that we don't have to check for valid UTF-8 to turn it into a string.
    if mime_bytes.iter().any(|&b| b < 0x20 || b > 0x7e) {
        return fmt_err("picture mime type string contains invalid characters")
    }
    let mime_type = unsafe { String::from_utf8_unchecked(mime_bytes) };

    let description_len = try!(input.read_be_u32());

    // The description must fit within the picture block. Also put a limit
    // on the length, to ensure we don't allocate large strings, in order to
    // prevent denial of service attacks.
    if description_len > length - 32 { return fmt_err("picture description too long") }
    if description_len > 256 {
        let msg = "picture descriptions larger than 256 bytes are not supported";
        return Err(Error::Unsupported(msg))
    }
    let mut description_bytes = Vec::with_capacity(description_len as usize);

    // We can safely set the length of the vector here; the uninitialized memory
    // is not exposed. If `read_exact` succeeds, it will have overwritten all
    // bytes. If not, an error is returned and the memory is never exposed.
    unsafe { description_bytes.set_len(description_len as usize); }
    try!(input.read_exact(&mut description_bytes));
    let description = try!(String::from_utf8(description_bytes));

    // Next are a few fields with pixel metadata. It seems a bit weird to me
    // that FLAC stores the bits per pixel, and especially the number of indexed
    // colors. Perhaps the idea was to allow choosing which picture to decode,
    // but peeking the image data itself would be better anyway. I have no use
    // case for these fields, so they are not exposed, to keep the API cleaner,
    // and to save a few bytes of memory.
    let width = try!(input.read_be_u32());
    let height = try!(input.read_be_u32());
    let _bits_per_pixel = try!(input.read_be_u32());
    let _num_indexed_colors = try!(input.read_be_u32());

    let data_len = try!(input.read_be_u32());

    // The length field is redundant, because we already have the size of the
    // block. The picture should fill up the remainder of the block.
    if data_len > length - 32 - mime_len - description_len {
        return fmt_err("picture data does not fit the picture block")
    }

    // The largers picture in my personal collection is 13_318_155 bytes; the
    // 95th percentile is 3_672_912 bytes. Having larger cover art embedded kind
    // of defeats the purpose, as the cover art would be larger than the audio
    // data for the typical track. Hence a 100 MiB limit should be reasonable.
    if data_len > 100 * 1024 * 1024 {
        let msg = "pictures larger than 100 MiB are not supported";
        return Err(Error::Unsupported(msg))
    }

    // TODO: Expose reader.

    let picture = Picture {
        metadata: PictureMetadata {
            kind: kind,
            mime_type: mime_type,
            description: description,
            width: width,
            height: height,
        },
        reader: unimplemented!(),
    };

    Ok(picture)
}

#[derive(Clone, Copy)]
struct MetadataBlockHeader {
    is_last: bool,
    block_type: u8,
    length: u32,
}

/// An iterator over metadata blocks in the stream.
///
/// The metadata reader reads the FLAC stream header and subsequent medatadata
/// blocks, up to the start of the audio data. It can be used as an iterator
/// of [`MetadataBlock`][metadatablock] items (wrapped in
/// [`MetadataResult`][metadataresult]), although `MetadataReader` does not
/// implement [`std::Iterator`][std-iter] for technical reasons.*
///
/// A valid FLAC stream contains at least one metadata block: the
/// [`StreamInfo`][streaminfo] block. This is always the first block in the
/// stream.
///
/// Some metadata blocks, such as the [`VorbisComment`][vorbiscomment] block,
/// require heap allocations for convenient use. `MetadataReader` does not parse
/// these blocks immediately, it returns them as *lazy metadata blocks* instead.
/// Calling `get()` on the lazy block will read and parse it; calling
/// `discard()` will skip over the block without allocating anything on the
/// heap.
///
/// <small>* For [`std::Iterator`][std-iter], the `Item` type is fixed for
/// the iterator, and values returned from `Iterator::next()` need to live at
/// least as long as the iterator itself. For `MetadataReader` this is not
/// possible due to the lazy blocks that borrow the underlying reader.</small>
///
/// [streaminfo]:     struct.StreamInfo.html
/// [metadataresult]: type.MetadataResult.html
/// [metadatablock]:  enum.MetadataBlock.html
/// [vorbiscomment]:  struct.VorbisComment.html
/// [std-iter]:       https://doc.rust-lang.org/std/iter/trait.Iterator.html
pub struct MetadataReader<R: io::Read> {
    input: R,
    done: bool,
}

/// Either a `MetadataBlock` or an `Error`.
pub type MetadataResult<'a, R> = Result<MetadataBlock<'a, R>>;

impl<R: io::Read> MetadataReader<R> {
    /// Create a metadata reader from a reader positioned at the beginning of a FLAC stream.
    ///
    /// This function reads the FLAC stream header and positions the
    /// `MetadataReader` at the first block. For a valid FLAC stream, the first
    /// call to [`next()`][next] will yield a [`StreamInfo`][streaminfo] block.
    ///
    /// Use [`new_aligned()`][new-aligned] if the input reader is already
    /// positioned at a metadata block header, and not at the start of the
    /// FLAC stream. This is the case if you manually read the header with
    /// [`read_flac_header()`][read-flac-header], for example.
    ///
    /// [next]:             #method.next
    /// [streaminfo]:       struct.StreamInfo.html
    /// [new-aligned]:      #method.new_aligned
    /// [read-flac-header]: ../fn.read_flac_header.html
    pub fn new(mut input: R) -> Result<MetadataReader<R>> {
        try!(::read_flac_header(&mut input));
        let reader = MetadataReader {
            input: input,
            done: false,
        };
        Ok(reader)
    }

    /// Create a metadata reader, assuming the input reader is positioned at a metadata block header.
    ///
    /// It is assumed that the next byte that the reader will read, is the first
    /// byte of a metadata block header. This means that the iterator will yield
    /// at least a single value.
    pub fn new_aligned(input: R) -> MetadataReader<R> {
        MetadataReader {
            input: input,
            done: false,
        }
    }

    #[inline]
    fn read_next(&mut self) -> MetadataResult<R> {
        let header = try!(read_metadata_block_header(&mut self.input));
        let block = try!(read_metadata_block(&mut self.input, header.block_type, header.length));
        self.done = header.is_last;
        Ok(block)
    }

    /// Read the next metadata block.
    ///
    /// This method corresponds to [`Iterator::next()`][iter-next].
    ///
    /// [iter-next]: https://doc.rust-lang.org/std/iter/trait.Iterator.html#tymethod.next
    #[inline]
    pub fn next(&mut self) -> Option<MetadataResult<R>> {
        if self.done {
            None
        } else {
            Some(self.read_next())
        }
    }

    /// Return bounds on the remaining number of metadata blocks.
    ///
    /// The returned bounds are either `(0, Some(0))` when the last metadata
    /// block has been read, or `(1, None)` when there is at least one metadata
    /// block remaining.
    ///
    /// This method corresponds to [`Iterator::size_hint()`][iter-size-hint].
    ///
    /// [iter-size-hint]: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.size_hint
    #[inline]
    pub fn size_hint(&self) -> (usize, Option<usize>) {
        // When done, there will be no more blocks,
        // when not done, there will be at least one more.
        if self.done { (0, Some(0)) } else { (1, None) }
    }
}
