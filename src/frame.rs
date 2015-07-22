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

//! The `frame` module deals with the frames that make up a FLAC stream.

use std::io;
use std::iter::repeat;
use crc::Crc8Reader;
use error::{Error, FlacResult, fmt_err};
use input::{Bitstream, ReadExt};
use sample;
use subframe;

#[derive(Clone, Copy)]
enum BlockingStrategy {
    Fixed,
    Variable
}

#[derive(Clone, Copy)]
enum BlockTime {
    FrameNumber(u32),
    SampleNumber(u64)
}

#[derive(Clone, Copy, Debug)]
enum ChannelAssignment {
    /// The `n: u8` channels are coded as-is.
    Independent(u8),
    /// Channel 0 is the left channel, channel 1 is the side channel.
    LeftSideStereo,
    /// Channel 0 is the side channel, channel 1 is the right channel.
    RightSideStereo,
    /// Channel 0 is the mid channel, channel 1 is the side channel.
    MidSideStereo
}

#[derive(Clone, Copy)]
struct FrameHeader {
    pub block_time: BlockTime,
    pub block_size: u16,
    pub sample_rate: Option<u32>,
    pub channel_assignment: ChannelAssignment,
    pub bits_per_sample: Option<u8>
}

impl FrameHeader {
    pub fn channels(&self) -> u8 {
        match self.channel_assignment {
            ChannelAssignment::Independent(n) => n,
            ChannelAssignment::LeftSideStereo => 2,
            ChannelAssignment::RightSideStereo => 2,
            ChannelAssignment::MidSideStereo => 2
        }
    }
}

/// Reads a variable-length integer encoded as what is called "UTF-8" coding
/// in the specification. (It is not real UTF-8.) This function can read
/// integers encoded in this way up to 36-bit integers.
fn read_var_length_int<R: io::Read>(input: &mut R) -> FlacResult<u64> {
    // The number of consecutive 1s followed by a 0 is the number of additional
    // bytes to read.
    let first = try!(input.read_u8());
    let mut read_additional = 0u8;
    let mut mask_data = 0b0111_1111u8;
    let mut mask_mark = 0b1000_0000u8;

    // Determine the number of leading 1s.
    while first & mask_mark != 0 {
        read_additional = read_additional + 1;
        mask_data = mask_data >> 1;
        mask_mark = mask_mark >> 1;
    }

    // A single leading 1 is a follow-up byte and thus invalid.
    if read_additional > 0 {
        if read_additional == 1 {
            return fmt_err("invalid variable-length integer");
        } else {
            // The number of 1s (if > 1) is the total number of bytes, not the
            // number of additional bytes.
            read_additional = read_additional - 1;
        }
    }

    // Each additional byte will yield 6 extra bits, so shift the most
    // significant bits into the correct position.
    let mut result = ((first & mask_data) as u64) << (6 * read_additional);
    for i in (0 .. read_additional as i16).rev() {
        let byte = try!(input.read_u8());

        // The two most significant bits _must_ be 10.
        if byte & 0b1100_0000 != 0b1000_0000 {
            return fmt_err("invalid variable-length integer");
        }

        result = result | (((byte & 0b0011_1111) as u64) << (6 * i as usize));
    }

    Ok(result)
}

#[test]
fn verify_read_var_length_int() {

    let mut reader = io::Cursor::new(vec!(0x24, 0xc2, 0xa2, 0xe2, 0x82, 0xac,
                                          0xf0, 0x90, 0x8d, 0x88, 0xc2, 0x00,
                                          0x80));
    assert_eq!(read_var_length_int(&mut reader).unwrap(), 0x24);
    assert_eq!(read_var_length_int(&mut reader).unwrap(), 0xa2);
    assert_eq!(read_var_length_int(&mut reader).unwrap(), 0x20ac);
    assert_eq!(read_var_length_int(&mut reader).unwrap(), 0x010348);
    // Two-byte integer with invalid continuation byte should fail.
    assert_eq!(read_var_length_int(&mut reader).err().unwrap(),
               Error::FormatError("invalid variable-length integer"));
    // Continuation byte can never be the first byte.
    assert_eq!(read_var_length_int(&mut reader).err().unwrap(),
               Error::FormatError("invalid variable-length integer"));
}

fn read_frame_header(input: &mut io::Read) -> FlacResult<FrameHeader> {
    // The frame header includes a CRC-8 at the end. It can be computed
    // automatically while reading, by wrapping the input reader in a reader
    // that computes the CRC.
    let mut crc_input = Crc8Reader::new(input);

    // First are 14 bits frame sync code, a reserved bit, and blocking stategy.
    let sync_res_block = try!(crc_input.read_be_u16());

    // The first 14 bits must be 11111111111110.
    let sync_code = sync_res_block & 0b1111_1111_1111_1100;
    if sync_code != 0b1111_1111_1111_1000 {
        return fmt_err("frame sync code missing")
    }

    // The next bit has a mandatory value of 0 at the moment of writing. The
    // spec says "0: mandatory value, 1: reserved for future use". As it is
    // unlikely that the FLAC format will every change, we treat features in
    // the spec that are not implemented as `Error::Unsupported`, and this is
    // a format error.
    if sync_res_block & 0b0000_0000_0000_0010 != 0 {
        return fmt_err("invalid frame header, encountered reserved value");
    }

    // The final bit determines the blocking strategy.
    let blocking_strategy = if sync_res_block & 0b0000_0000_0000_0001 == 0 {
        BlockingStrategy::Fixed
    } else {
        BlockingStrategy::Variable
    };

    // Next are 4 bits block size and 4 bits sample rate.
    let bs_sr = try!(crc_input.read_u8());
    let mut block_size = 0u16;
    let mut read_8bit_bs = false;
    let mut read_16bit_bs = false;

    // There are some pre-defined bit patterns. Some mean 'get from end of
    // header instead'.
    match bs_sr >> 4 {
        // The value 0000 is reserved.
        0b0000 => return fmt_err("invalid frame header, encountered reserved value"),
        0b0001 => block_size = 192,
        n if 0b0010 <= n && n <= 0b0101 => block_size = 576 * (1 << (n - 2) as usize),
        0b0110 => read_8bit_bs = true,
        0b0111 => read_16bit_bs = true,
        n => block_size = 256 * (1 << (n - 8) as usize)
    }

    // For the sample rate there is a number of pre-defined bit patterns as
    // well. Again, some mean 'get from end of header instead'.
    let mut sample_rate = None;
    let mut read_8bit_sr = false;
    let mut read_16bit_sr = false;
    let mut read_16bit_sr_ten = false;

    match bs_sr & 0b0000_1111 {
        0b0000 => sample_rate = None, // 0000 means 'get from streaminfo block'.
        0b0001 => sample_rate = Some( 88_200),
        0b0010 => sample_rate = Some(176_400),
        0b0011 => sample_rate = Some(192_000),
        0b0100 => sample_rate = Some(  8_000),
        0b0101 => sample_rate = Some( 16_000),
        0b0110 => sample_rate = Some( 22_050),
        0b0111 => sample_rate = Some( 24_000),
        0b1000 => sample_rate = Some( 32_000),
        0b1001 => sample_rate = Some( 44_100),
        0b1010 => sample_rate = Some( 48_000),
        0b1011 => sample_rate = Some( 96_000),
        0b1100 => read_8bit_sr = true, // Read Hz from end of header.
        0b1101 => read_16bit_sr = true, // Read Hz from end of header.
        0b1110 => read_16bit_sr_ten = true, // Read tens of Hz from end of header.
        // 1111 is invalid to prevent sync-fooling.
        // Other values are impossible at this point.
        _ => return fmt_err("invalid frame header")
    }

    // Next are 4 bits channel assignment, 3 bits sample size, and 1 reserved bit.
    let chan_bps_res = try!(crc_input.read_u8());

    // The most significant 4 bits determine channel assignment.
    let channel_assignment = match chan_bps_res >> 4 {
        // Values 0 through 7 indicate n + 1 channels without mixing.
        n if n < 8 => ChannelAssignment::Independent(n + 1),
        0b1000 => ChannelAssignment::LeftSideStereo,
        0b1001 => ChannelAssignment::RightSideStereo,
        0b1010 => ChannelAssignment::MidSideStereo,
        // Values 1011 through 1111 are reserved and thus invalid.
        _ => return fmt_err("invalid frame header, encountered reserved value")
    };

    // The next three bits indicate bits per sample.
    let bits_per_sample = match (chan_bps_res & 0b0000_1110) >> 1 {
        0b000 => None, // 000 means 'get from streaminfo block'.
        0b001 => Some(8),
        0b010 => Some(12),
        0b100 => Some(16),
        0b101 => Some(20),
        0b110 => Some(24),
        // Values 011 and 111 are reserved. Other values are impossible.
        _ => return fmt_err("invalid frame header, encountered reserved value")
    };

    // The final bit has a mandatory value of 0, it is a reserved bit.
    if chan_bps_res & 0b0000_0001 != 0 {
        return fmt_err("invalid frame header, encountered reserved value");
    }

    let block_time = match blocking_strategy {
        BlockingStrategy::Variable => {
            // The sample number is encoded in 8-56 bits, at most a 36-bit int.
            let sample = try!(read_var_length_int(&mut crc_input));
            BlockTime::SampleNumber(sample)
        },
        BlockingStrategy::Fixed => {
            // The frame number is encoded in 8-48 bits, at most a 31-bit int.
            let frame = try!(read_var_length_int(&mut crc_input));
            // A frame number larger than 31 bits is therefore invalid.
            if frame > 0x7fffffff {
                return fmt_err("invalid frame header, frame number too large");
            }
            BlockTime::FrameNumber(frame as u32)
        }
    };

    if read_8bit_bs {
        // 8 bit block size - 1 is stored.
        let bs = try!(crc_input.read_u8());
        block_size = bs as u16 + 1;
    }
    if read_16bit_bs {
        // 16-bit block size - 1 is stored. Note that the max block size that
        // can be indicated in the streaminfo block is a 16-bit number, so a
        // value of 0xffff would be invalid because it exceeds the max block
        // size, though this is not mentioned explicitly in the specification.
        let bs = try!(crc_input.read_be_u16());
        if bs == 0xffff { return fmt_err("invalid block size, exceeds 65535"); }
        block_size = bs + 1;
    }

    if block_size < 16 {
        return fmt_err("invalid block size, must be at least 16");
    }

    if read_8bit_sr {
        let sr = try!(crc_input.read_u8());
        sample_rate = Some(sr as u32);
    }
    if read_16bit_sr {
        let sr = try!(crc_input.read_be_u16());
        sample_rate = Some(sr as u32);
    }
    if read_16bit_sr_ten {
        let sr_ten = try!(crc_input.read_be_u16());
        sample_rate = Some(sr_ten as u32 * 10);
    }

    // Next is an 8-bit CRC that is computed over the entire header so far.
    let computed_crc = crc_input.crc();
    let presumed_crc = try!(crc_input.read_u8());

    if computed_crc != presumed_crc {
        return fmt_err("frame header CRC mismatch");
    }

    let frame_header = FrameHeader {
       block_time: block_time,
       block_size: block_size,
       sample_rate: sample_rate,
       channel_assignment: channel_assignment,
       bits_per_sample: bits_per_sample
    };
    Ok(frame_header)
}

/// Converts a buffer with left samples and a side channel in-place to left ++ right.
// TODO: This could take iterators, and decode and narrow in-place. Better yet,
// this should produce an iterator that decodes.
fn decode_left_side<Sample: sample::WideSample>
                   (buffer: &mut [Sample])
                    -> FlacResult<()> {

    let block_size = buffer.len() / 2;
    for i in 0 .. block_size {
        let left = buffer[i];
        let side = buffer[i + block_size];

        // Left is correct already, only the right channel needs to be decoded.
        // side = left - right => right = left - side.
        let right = left - side;
        buffer[block_size + i] = right;
    }

    Ok(())
}

#[test]
fn verify_decode_left_side() {
    let mut buffer = vec!(2i16,    5,   83, 113, 127, -63, -45, -15,
                             7,  38, 142,  238,   0, -152, -52, -18);
    let result =     vec!(2i16,   5,  83,  113, 127,  -63, -45, -15,
                            -5, -33, -59, -125, 127,   89,   7,   3);
    decode_left_side(&mut buffer).ok().unwrap();
    assert_eq!(buffer, result);
}

/// Converts a buffer with right samples and a side channel in-place to left ++ right.
fn decode_right_side<Sample: sample::WideSample>
                    (buffer: &mut [Sample])
                     -> FlacResult<()> {

    let block_size = buffer.len() / 2;
    for i in 0 .. block_size {
        let side = buffer[i];
        let right = buffer[block_size + i];

        // Right is correct already, only the left channel needs to be decoded.
        // side = left - right => left = side + right.
        let left = side + right;
        buffer[i] = left;
    }

    Ok(())
}

#[test]
fn verify_decode_right_side() {
    let mut buffer = vec!(7i16,  38, 142,  238,   0, -152, -52, -18,
                            -5, -33, -59, -125, 127,   89,   7,  3);
    let result =     vec!(2i16,   5,  83,  113, 127,  -63, -45, -15,
                            -5, -33, -59, -125, 127,   89,   7,   3);
    decode_right_side(&mut buffer).ok().expect("decoding is wrong");
    assert_eq!(buffer, result);
}

/// Converts a buffer with mid samples and a side channel in-place to left ++ right.
fn decode_mid_side<Sample: sample::WideSample>
                  (buffer: &mut [Sample])
                   -> FlacResult<()> {

    let block_size = buffer.len() / 2;
    for i in 0 .. block_size {
        let mid = buffer[i];
        let side = buffer[i + block_size];

        // The code below uses shifts insead of multiplication/division by two,
        // because Rust does not infer a literal `2` to be of type `Sample`.

        // Double mid first, and then correct for truncated rounding that
        // will have occured if side is odd.
        let mid = (mid << 1) | (side & Sample::one());
        let left = (mid + side) >> 1;
        let right = (mid - side) >> 1;

        buffer[i] = left;
        buffer[block_size + i] = right;
    }

    Ok(())
}

#[test]
fn verify_decode_mid_side() {
    let mut buffer = vec!(-2i16, -14,  12,   -6, 127,   13, -19,  -6,
                              7,  38, 142,  238,   0, -152, -52, -18);
    let result =      vec!(2i16,   5,  83,  113, 127,  -63, -45, -15,
                             -5, -33, -59, -125, 127,   89,   7,   3);
    decode_mid_side(&mut buffer).ok().expect("decoding is wrong");
    assert_eq!(buffer, result);
}

/// A block of raw audio samples.
pub struct Block<'b, Sample> where Sample: 'b {
    /// The sample number of the first sample in the this block.
    first_sample_number: u64,
    /// The number of samples in the block.
    block_size: u16,
    /// The number of channels in the block.
    n_channels: u8,
    /// The decoded samples, the channels stored consecutively.
    samples: &'b [Sample]
}

impl <'b, Sample: sample::Sample> Block<'b, Sample> {
    fn new(time: u64, bs: u16, buffer: &'b [Sample]) -> Block<'b, Sample> {
        Block {
            first_sample_number: time,
            block_size: bs,
            n_channels: (buffer.len() / bs as usize) as u8,
            samples: buffer
        }
    }

    /// Returns the sample number of the first sample in the block.
    pub fn time(&self) -> u64 {
        self.first_sample_number
    }

    /// Returns the number of inter-channel samples in the block.
    pub fn len(&self) -> u16 {
        self.block_size
    }

    /// Returns the number of channels in the block.
    // TODO: should a frame know this? #channels must be constant throughout the stream anyway ...
    pub fn channels(&self) -> u8 {
        self.n_channels
    }

    /// Returns the (zero-based) `ch`-th channel as a slice.
    ///
    /// # Panics
    /// Panics if `ch` is larger than `channels()`.
    pub fn channel(&'b self, ch: u8) -> &'b [Sample] {
        &self.samples[ch as usize * self.block_size as usize ..
                     (ch as usize + 1) * self.block_size as usize]
    }
}

/// Reads frames from a stream and exposes decoded blocks as an iterator.
///
/// TODO: for now, it is assumes that the reader starts at a frame header;
/// no searching for a sync code is performed at the moment.
pub struct FrameReader<'r, Sample: sample::Sample> {
    input: &'r mut (io::Read + 'r),
    buffer: Vec<Sample>,
    wide_buffer: Vec<Sample::Wide>
}

/// Either a `Block` or an `Error`.
pub type FrameResult<'b, Sample> = FlacResult<Block<'b, Sample>>;

impl<'r, Sample: sample::Sample> FrameReader<'r, Sample> {

    /// Creates a new frame reader that will yield at least one element.
    pub fn new(input: &'r mut io::Read) -> FrameReader<'r, Sample> {
        // TODO: a hit for the vector size can be provided.
        FrameReader {
            input: input,
            buffer: Vec::new(),
            wide_buffer: Vec::new()
        }
    }
 
    fn ensure_buffer_len(&mut self, new_len: usize) {
        if self.buffer.len() < new_len {
            // Previous data will be overwritten, so instead of resizing the
            // vector if it is too small, we might as well allocate a new one.
            if self.buffer.capacity() < new_len {
                self.buffer = Vec::with_capacity(new_len);
            }
            let len = self.buffer.len();
            self.buffer.extend(repeat(Sample::zero()).take(new_len - len));
        }
    }

    fn ensure_wide_buffer_len(&mut self, new_len: usize) {
        use std::num::Zero;

        if self.wide_buffer.len() < new_len {
            // Previous data will be overwritten, so instead of resizing the
            // vector if it is too small, we might as well allocate a new one.
            if self.wide_buffer.capacity() < new_len {
                self.wide_buffer = Vec::with_capacity(new_len);
            }
            let len = self.wide_buffer.len();
            self.wide_buffer.extend(repeat(Sample::Wide::zero())
                                   .take(new_len - len));
        }
    }

    /// Tries to decode the next frame.
    ///
    /// TODO: I should really be consistent with 'read' and 'decode'.
    pub fn read_next<'s>(&'s mut self) -> FrameResult<'s, Sample> {
        use std::mem::size_of;

        let header = try!(read_frame_header(self.input));

        // We must allocate enough space for all channels in the block to be
        // decoded.
        let total_samples = header.channels() as usize * header.block_size as usize;
        self.ensure_buffer_len(total_samples);
        self.ensure_wide_buffer_len(total_samples);

        // TODO: if the bps is missing from the header, we must get it from
        // the streaminfo block.
        let bps = header.bits_per_sample.unwrap();

        // The sample size must be wide enough to accomodate for the bits per sample.
        // TODO: Turn this into an error instead of panic? Or is it enforced elsewhere?
        debug_assert!(bps as usize <= size_of::<Sample>() * 8);

        // In the next part of the stream, nothing is byte-aligned any more,
        // we need a bitstream. Then we can decode subframes from the bitstream.
        {
            let mut bitstream = Bitstream::new(self.input);
            let bs = header.block_size as usize;

            match header.channel_assignment {
                ChannelAssignment::Independent(n_ch) => {
                    for ch in 0 .. n_ch as usize {
                        try!(subframe::decode(&mut bitstream, bps,
                                              &mut self.wide_buffer[ch * bs ..
                                                                   (ch + 1) * bs]));
                    }
                }
                ChannelAssignment::LeftSideStereo => {
                    // The side channel has one extra bit per sample.
                    try!(subframe::decode(&mut bitstream, bps,
                                          &mut self.wide_buffer[.. bs]));
                    try!(subframe::decode(&mut bitstream, bps + 1,
                                          &mut self.wide_buffer[bs .. bs * 2]));

                    // Then decode the side channel into the right channel.
                    try!(decode_left_side(&mut self.wide_buffer[.. bs * 2]));
                },
                ChannelAssignment::RightSideStereo => {
                    // The side channel has one extra bit per sample.
                    try!(subframe::decode(&mut bitstream, bps + 1,
                                          &mut self.wide_buffer[.. bs]));
                    try!(subframe::decode(&mut bitstream, bps,
                                          &mut self.wide_buffer[bs .. bs * 2]));

                    // Then decode the side channel into the left channel.
                    try!(decode_right_side(&mut self.wide_buffer[.. bs * 2]));
                },
                ChannelAssignment::MidSideStereo => {
                    // Decode mid as the first channel, then side with one
                    // extra bitp per sample.
                    try!(subframe::decode(&mut bitstream, bps,
                                          &mut self.wide_buffer[.. bs]));
                    try!(subframe::decode(&mut bitstream, bps + 1,
                                          &mut self.wide_buffer[bs .. bs * 2]));

                    // Then decode mid-side channel into left-right.
                    try!(decode_mid_side(&mut self.wide_buffer[.. bs * 2]));
                }
            }

            // When the bitstream goes out of scope, we can use the `input`
            // reader again, which will be byte-aligned. The specification
            // dictates that padding should consist of zero bits, but we do not
            // enforce this here.
            // TODO: It could be enforced by having a read_to_byte_aligned
            // method on the bit reader; it'd be a simple comparison.
        }

        {
            // Narrow down the wide samples to the requested sample type.
            // TODO: this should not verify that it fits the sample type, but that
            // it fits the requested bps.
            let n = header.block_size as usize * header.channels() as usize;
            let wide_iter = self.wide_buffer[.. n].iter();
            let dest_iter = self.buffer[.. n].iter_mut();
            for (&src, dest) in wide_iter.zip(dest_iter) {
                let narrow = Sample::from_wide(src).ok_or(Error::TooWide);
                *dest = try!(narrow);
            }
        }

        // The frame footer is a 16-bit CRC.
        // TODO: Get CRC of frame read so far.
        let _frame_crc = try!(self.input.read_be_u16());
        // TODO: Compare CRCs.

        // TODO: constant block size should be verified if a frame number is
        // encountered.
        let time = match header.block_time {
            BlockTime::FrameNumber(fnr) => header.block_size as u64 * fnr as u64,
            BlockTime::SampleNumber(snr) => snr
        };

        let block = Block::new(time, header.block_size,
                               &self.buffer[.. total_samples]);

        Ok(block)
    }
}

// TODO: implement Iterator<Item = FrameResult> for FrameReader, with an
// accurate size hint.
