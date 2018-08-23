// Claxon -- A FLAC decoding library in Rust
// Copyright 2014 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

//! The `frame` module deals with the frames that make up a FLAC stream.

use std::i32;

use crc::{Crc8Reader, Crc16Reader};
use error::{Error, Result, fmt_err};
use input::{Bitstream, ReadBytes};
use subframe;

#[derive(Clone, Copy)]
enum BlockingStrategy {
    Fixed,
    Variable,
}

#[derive(Clone, Copy)]
enum BlockTime {
    FrameNumber(u32),
    SampleNumber(u64),
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
    MidSideStereo,
}

#[derive(Clone, Copy)]
struct FrameHeader {
    pub block_time: BlockTime,
    pub block_size: u16,
    pub sample_rate: Option<u32>,
    pub channel_assignment: ChannelAssignment,
    pub bits_per_sample: Option<u32>,
}

impl FrameHeader {
    pub fn channels(&self) -> u8 {
        match self.channel_assignment {
            ChannelAssignment::Independent(n) => n,
            ChannelAssignment::LeftSideStereo => 2,
            ChannelAssignment::RightSideStereo => 2,
            ChannelAssignment::MidSideStereo => 2,
        }
    }
}

/// Reads a variable-length integer encoded as what is called "UTF-8" coding
/// in the specification. (It is not real UTF-8.) This function can read
/// integers encoded in this way up to 36-bit integers.
fn read_var_length_int<R: ReadBytes>(input: &mut R) -> Result<u64> {
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
    for i in (0..read_additional as i16).rev() {
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
    use std::io;
    use error::Error;
    use input::BufferedReader;

    let mut reader = BufferedReader::new(
        io::Cursor::new(vec![0x24, 0xc2, 0xa2, 0xe2, 0x82, 0xac, 0xf0, 0x90, 0x8d,
                            0x88, 0xc2, 0x00, 0x80]));

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

fn read_frame_header_or_eof<R: ReadBytes>(input: &mut R) -> Result<Option<FrameHeader>> {
    // The frame header includes a CRC-8 at the end. It can be computed
    // automatically while reading, by wrapping the input reader in a reader
    // that computes the CRC.
    let mut crc_input = Crc8Reader::new(input);

    // First are 14 bits frame sync code, a reserved bit, and blocking stategy.
    // If instead of the two bytes we find the end of the stream, return
    // `Nothing`, indicating EOF.
    let sync_res_block = match try!(crc_input.read_be_u16_or_eof()) {
        None => return Ok(None),
        Some(x) => x,
    };

    // The first 14 bits must be 11111111111110.
    let sync_code = sync_res_block & 0b1111_1111_1111_1100;
    if sync_code != 0b1111_1111_1111_1000 {
        return fmt_err("frame sync code missing");
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
        n => block_size = 256 * (1 << (n - 8) as usize),
    }

    // For the sample rate there is a number of pre-defined bit patterns as
    // well. Again, some mean 'get from end of header instead'.
    let mut sample_rate = None;
    let mut read_8bit_sr = false;
    let mut read_16bit_sr = false;
    let mut read_16bit_sr_ten = false;

    match bs_sr & 0b0000_1111 {
        0b0000 => sample_rate = None, // 0000 means 'get from streaminfo block'.
        0b0001 => sample_rate = Some(88_200),
        0b0010 => sample_rate = Some(176_400),
        0b0011 => sample_rate = Some(192_000),
        0b0100 => sample_rate = Some(8_000),
        0b0101 => sample_rate = Some(16_000),
        0b0110 => sample_rate = Some(22_050),
        0b0111 => sample_rate = Some(24_000),
        0b1000 => sample_rate = Some(32_000),
        0b1001 => sample_rate = Some(44_100),
        0b1010 => sample_rate = Some(48_000),
        0b1011 => sample_rate = Some(96_000),
        0b1100 => read_8bit_sr = true, // Read Hz from end of header.
        0b1101 => read_16bit_sr = true, // Read Hz from end of header.
        0b1110 => read_16bit_sr_ten = true, // Read tens of Hz from end of header.
        // 1111 is invalid to prevent sync-fooling.
        // Other values are impossible at this point.
        _ => return fmt_err("invalid frame header"),
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
        _ => return fmt_err("invalid frame header, encountered reserved value"),
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
        _ => return fmt_err("invalid frame header, encountered reserved value"),
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
        }
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
        if bs == 0xffff {
            return fmt_err("invalid block size, exceeds 65535");
        }
        block_size = bs + 1;
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

    // Do not verify checksum during fuzzing,
    // otherwise malformed input from fuzzer won't reach the actually interesting code
    if ! cfg!(fuzzing) {
        if computed_crc != presumed_crc {
            return fmt_err("frame header CRC mismatch");
        }
    }

    let frame_header = FrameHeader {
        block_time: block_time,
        block_size: block_size,
        sample_rate: sample_rate,
        channel_assignment: channel_assignment,
        bits_per_sample: bits_per_sample,
    };
    Ok(Some(frame_header))
}

/// Converts a buffer with left samples and a side channel in-place to left ++ right.
fn decode_left_side(buffer: &mut [i32]) {
    let block_size = buffer.len() / 2;
    let (mids, sides) = buffer.split_at_mut(block_size);
    for (fst, snd) in mids.iter_mut().zip(sides) {
        let left = *fst;
        let side = *snd;

        // Left is correct already, only the right channel needs to be decoded.
        // side = left - right => right = left - side. A valid FLAC file will
        // never overflow here. If we do have an overflow then we decode
        // garbage, but at least Rust does not panic in debug mode due to
        // overflow.
        let right = left.wrapping_sub(side);
        *snd = right;
    }
}

#[test]
fn verify_decode_left_side() {
    let mut buffer = vec![2, 5, 83, 113, 127, -63, -45, -15, 7, 38, 142, 238, 0, -152, -52, -18];
    let result = vec![2, 5, 83, 113, 127, -63, -45, -15, -5, -33, -59, -125, 127, 89, 7, 3];
    decode_left_side(&mut buffer);
    assert_eq!(buffer, result);
}

/// Converts a buffer with right samples and a side channel in-place to left ++ right.
fn decode_right_side(buffer: &mut [i32]) {
    let block_size = buffer.len() / 2;
    let (mids, sides) = buffer.split_at_mut(block_size);
    for (fst, snd) in mids.iter_mut().zip(sides) {
        let side = *fst;
        let right = *snd;

        // Right is correct already, only the left channel needs to be decoded.
        // side = left - right => left = side + right. A valid FLAC file will
        // never overflow here. If we do have an overflow then we decode
        // garbage, but at least Rust does not panic in debug mode due to
        // overflow.
        let left = side.wrapping_add(right);
        *fst = left;
    }
}

#[test]
fn verify_decode_right_side() {
    let mut buffer = vec![7, 38, 142, 238, 0, -152, -52, -18, -5, -33, -59, -125, 127, 89, 7, 3];
    let result = vec![2, 5, 83, 113, 127, -63, -45, -15, -5, -33, -59, -125, 127, 89, 7, 3];
    decode_right_side(&mut buffer);
    assert_eq!(buffer, result);
}

/// Converts a buffer with mid samples and a side channel in-place to left ++ right.
fn decode_mid_side(buffer: &mut [i32]) {
    let block_size = buffer.len() / 2;
    let (mids, sides) = buffer.split_at_mut(block_size);
    for (fst, snd) in mids.iter_mut().zip(sides) {
        let mid = *fst;
        let side = *snd;

        // Double mid first, and then correct for truncated rounding that
        // will have occured if side is odd. Note that samples are never
        // expected to exceed 25 bits, so the wrapping multiplication does not
        // actually wrap for valid files.
        let mid = mid.wrapping_mul(2) | (side & 1);
        let left = mid.wrapping_add(side) / 2;
        let right = mid.wrapping_sub(side) / 2;

        *fst = left;
        *snd = right;
    }
}

#[test]
fn verify_decode_mid_side() {
    let mut buffer = vec!(-2, -14,  12,   -6, 127,   13, -19,  -6,
                           7,  38, 142,  238,   0, -152, -52, -18);
    let result =      vec!(2,   5,  83,  113, 127,  -63, -45, -15,
                          -5, -33, -59, -125, 127,   89,   7,   3);
    decode_mid_side(&mut buffer);
    assert_eq!(buffer, result);
}

/// A block of raw audio samples.
pub struct Block {
    /// The sample number of the first sample in the this block.
    first_sample_number: u64,
    /// The number of samples in the block.
    block_size: u32,
    /// The number of channels in the block.
    channels: u32,
    /// The decoded samples, the channels stored consecutively.
    buffer: Vec<i32>,
}

impl Block {
    fn new(time: u64, bs: u32, buffer: Vec<i32>) -> Block {
        Block {
            first_sample_number: time,
            block_size: bs,
            channels: buffer.len() as u32 / bs,
            buffer: buffer,
        }
    }

    /// Returns a block with 0 channels and 0 samples.
    pub fn empty() -> Block {
        Block {
            first_sample_number: 0,
            block_size: 0,
            channels: 0,
            buffer: Vec::with_capacity(0),
        }
    }

    /// Returns the inter-channel sample number of the first sample in the block.
    ///
    /// The time is independent of the number of channels. To get the start time
    /// of the block in seconds, divide this number by the sample rate in the
    /// streaminfo.
    pub fn time(&self) -> u64 {
        self.first_sample_number
    }

    /// Returns the total number of samples in this block.
    ///
    /// Samples in different channels are counted as distinct samples.
    #[inline(always)]
    pub fn len(&self) -> u32 {
        // Note: this cannot overflow, because the block size fits in 16 bits,
        // and the number of channels is at most 8.
        self.block_size * self.channels
    }

    /// Returns the number of inter-channel samples in the block.
    ///
    /// The duration is independent of the number of channels. The returned
    /// value is also referred to as the *block size*. To get the duration of
    /// the block in seconds, divide this number by the sample rate in the
    /// streaminfo.
    #[inline(always)]
    pub fn duration(&self) -> u32 {
        self.block_size
    }

    /// Returns the number of channels in the block.
    // TODO: Should a frame know this? #channels must be constant throughout the stream anyway ...
    // TODO: Rename to `num_channels` for clarity.
    #[inline(always)]
    pub fn channels(&self) -> u32 {
        self.channels
    }

    /// Returns the (zero-based) `ch`-th channel as a slice.
    ///
    /// # Panics
    ///
    /// Panics if `ch >= channels()`.
    #[inline(always)]
    pub fn channel(&self, ch: u32) -> &[i32] {
        let bsz = self.block_size as usize;
        let ch_usz = ch as usize;
        &self.buffer[ch_usz * bsz..(ch_usz + 1) * bsz]
    }

    /// Returns a sample in this block.
    ///
    /// The value returned is for the zero-based `ch`-th channel of the
    /// inter-channel sample with index `sample` in this block (so this is not
    /// the global sample number).
    ///
    /// # Panics
    ///
    /// Panics if `ch >= channels()` or if `sample >= len()` for the last
    /// channel.
    #[inline(always)]
    pub fn sample(&self, ch: u32, sample: u32) -> i32 {
        let bsz = self.block_size as usize;
        return self.buffer[ch as usize * bsz + sample as usize];
    }

    /// Returns the underlying buffer that stores the samples in this block.
    ///
    /// This allows the buffer to be reused to decode the next frame. The
    /// capacity of the buffer may be bigger than `len()` times `channels()`.
    pub fn into_buffer(self) -> Vec<i32> {
        return self.buffer;
    }

    /// Returns an iterator that produces left and right channel samples.
    ///
    /// This iterator can be more efficient than requesting a sample directly,
    /// because it avoids a bounds check.
    ///
    /// # Panics
    ///
    /// Panics if the number of channels in the block is not 2.
    #[inline]
    pub fn stereo_samples<'a>(&'a self) -> StereoSamples<'a> {
        if self.channels != 2 {
            panic!("stereo_samples() must only be called for blocks with two channels.");
        }

        assert!(self.buffer.len() >= self.block_size as usize * 2);

        StereoSamples {
            buffer: &self.buffer,
            block_duration: self.block_size,
            current_sample: 0,
        }
    }
}

#[test]
fn verify_block_sample() {
    let block = Block {
        first_sample_number: 0,
        block_size: 5,
        channels: 3,
        buffer: vec![2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47],
    };

    assert_eq!(block.sample(0, 2), 5);
    assert_eq!(block.sample(1, 3), 23);
    assert_eq!(block.sample(2, 4), 47);
}

/// An iterator over the stereo sample pairs in a block.
///
/// This iterator is produced by `Block::stereo_samples()`.
pub struct StereoSamples<'a> {
    buffer: &'a [i32],
    block_duration: u32,
    current_sample: u32,
}

impl<'a> Iterator for StereoSamples<'a> {
    type Item = (i32, i32);

    #[inline(always)]
    fn next(&mut self) -> Option<(i32, i32)> {
        if self.current_sample == self.block_duration {
            None
        } else {
            let ch_offset = self.block_duration as usize;
            let idx = self.current_sample as usize;

            // Indexing without bounds check is safe here, because the current
            // sample is less than the block duration, and the buffer size is at
            // least twice the block duration. (There is an assertion for that
            // too when the iterator is constructed.)
            let samples = unsafe {
                let left = *self.buffer.get_unchecked(idx);
                let right = *self.buffer.get_unchecked(idx + ch_offset);
                (left, right)
            };

            self.current_sample += 1;

            Some(samples)
        }
    }
}

#[test]
fn verify_block_stereo_samples_iterator() {
    let block = Block {
        first_sample_number: 0,
        block_size: 3,
        channels: 2,
        buffer: vec![2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47],
    };

    let mut iter = block.stereo_samples();

    assert_eq!(iter.next(), Some((2, 7)));
    assert_eq!(iter.next(), Some((3, 11)));
    assert_eq!(iter.next(), Some((5, 13)));
    assert_eq!(iter.next(), None);
}

/// Reads frames from a stream and exposes decoded blocks as an iterator.
///
/// TODO: for now, it is assumes that the reader starts at a frame header;
/// no searching for a sync code is performed at the moment.
pub struct FrameReader<R: ReadBytes> {
    input: R,
}

/// Either a `Block` or an `Error`.
// TODO: The option should not be part of FrameResult.
pub type FrameResult = Result<Option<Block>>;

/// A macro to expand the length of a buffer, or replace the buffer altogether,
/// so it can hold at least `new_len` elements. The contents of the buffer can
/// be anything, it is assumed they will be overwritten anyway.
fn ensure_buffer_len(mut buffer: Vec<i32>, new_len: usize) -> Vec<i32> {
    if buffer.len() < new_len {
        // Previous data will be overwritten, so instead of resizing the
        // vector if it is too small, we might as well allocate a new one.
        if buffer.capacity() < new_len {
            buffer = Vec::with_capacity(new_len);
        }

        // We are going to fill the buffer anyway, so there is no point in
        // initializing it with default values. This does mean that there could
        // be garbage in the buffer, but that is not exposed, as the buffer is
        // only exposed if a frame has been decoded successfully, and hence the
        // entire buffer has been overwritten.
        unsafe { buffer.set_len(new_len); }
    } else {
        buffer.truncate(new_len);
    }
    buffer
}

impl<R: ReadBytes> FrameReader<R> {
    /// Creates a new frame reader that will yield at least one element.
    pub fn new(input: R) -> FrameReader<R> {
        FrameReader {
            input: input,
        }
    }

    /// Decodes the next frame or returns an error if the data was invalid.
    ///
    /// The buffer is moved into the returned block, so that the same buffer may
    /// be reused to decode multiple blocks, avoiding a heap allocation every
    /// time. It can be retrieved again with `block.into_buffer()`. If the
    /// buffer is not large enough to hold all samples, a larger buffer is
    /// allocated automatically.
    ///
    /// TODO: I should really be consistent with 'read' and 'decode'.
    pub fn read_next_or_eof(&mut self, mut buffer: Vec<i32>) -> FrameResult {
        // The frame includes a CRC-16 at the end. It can be computed
        // automatically while reading, by wrapping the input reader in a reader
        // that computes the CRC. If the stream ended before the the frame
        // header (so not in the middle of the frame header), return `None`,
        // indicating EOF.
        let mut crc_input = Crc16Reader::new(&mut self.input);
        let header = match try!(read_frame_header_or_eof(&mut crc_input)) {
            None => return Ok(None),
            Some(h) => h,
        };

        // We must allocate enough space for all channels in the block to be
        // decoded.
        let total_samples = header.channels() as usize * header.block_size as usize;
        buffer = ensure_buffer_len(buffer, total_samples);

        let bps = match header.bits_per_sample {
            Some(x) => x,
            // TODO: if the bps is missing from the header, we must get it from
            // the streaminfo block.
            None => return Err(Error::Unsupported("header without bits per sample info")),
        };

        // The number of bits per sample must not exceed 32, for we decode into
        // an i32. TODO: Turn this into an error instead of panic? Or is it
        // enforced elsewhere?
        debug_assert!(bps as usize <= 32);

        // In the next part of the stream, nothing is byte-aligned any more,
        // we need a bitstream. Then we can decode subframes from the bitstream.
        {
            let mut bitstream = Bitstream::new(&mut crc_input);
            let bs = header.block_size as usize;

            match header.channel_assignment {
                ChannelAssignment::Independent(n_ch) => {
                    for ch in 0..n_ch as usize {
                        try!(subframe::decode(&mut bitstream,
                                              bps,
                                              &mut buffer[ch * bs..(ch + 1) * bs]));
                    }
                }
                ChannelAssignment::LeftSideStereo => {
                    // The side channel has one extra bit per sample.
                    try!(subframe::decode(&mut bitstream, bps, &mut buffer[..bs]));
                    try!(subframe::decode(&mut bitstream,
                                          bps + 1,
                                          &mut buffer[bs..bs * 2]));

                    // Then decode the side channel into the right channel.
                    decode_left_side(&mut buffer[..bs * 2]);
                }
                ChannelAssignment::RightSideStereo => {
                    // The side channel has one extra bit per sample.
                    try!(subframe::decode(&mut bitstream, bps + 1, &mut buffer[..bs]));
                    try!(subframe::decode(&mut bitstream, bps, &mut buffer[bs..bs * 2]));

                    // Then decode the side channel into the left channel.
                    decode_right_side(&mut buffer[..bs * 2]);
                }
                ChannelAssignment::MidSideStereo => {
                    // Decode mid as the first channel, then side with one
                    // extra bitp per sample.
                    try!(subframe::decode(&mut bitstream, bps, &mut buffer[..bs]));
                    try!(subframe::decode(&mut bitstream,
                                          bps + 1,
                                          &mut buffer[bs..bs * 2]));

                    // Then decode mid-side channel into left-right.
                    decode_mid_side(&mut buffer[..bs * 2]);
                }
            }

            // When the bitstream goes out of scope, we can use the `input`
            // reader again, which will be byte-aligned. The specification
            // dictates that padding should consist of zero bits, but we do not
            // enforce this here.
            // TODO: It could be enforced by having a read_to_byte_aligned
            // method on the bit reader; it'd be a simple comparison.
        }

        // The frame footer is a 16-bit CRC.
        let computed_crc = crc_input.crc();
        let presumed_crc = try!(crc_input.read_be_u16());

        // Do not verify checksum during fuzzing,
        // otherwise malformed input from fuzzer won't reach the actually interesting code
        if ! cfg!(fuzzing) {
            if computed_crc != presumed_crc {
                return fmt_err("frame CRC mismatch");
            }
        }

        // TODO: constant block size should be verified if a frame number is
        // encountered.
        let time = match header.block_time {
            BlockTime::FrameNumber(fnr) => header.block_size as u64 * fnr as u64,
            BlockTime::SampleNumber(snr) => snr,
        };

        let block = Block::new(time, header.block_size as u32, buffer);

        Ok(Some(block))
    }

    /// Destroy the frame reader, returning the wrapped reader.
    pub fn into_inner(self) -> R {
        self.input
    }
}

// TODO: implement Iterator<Item = FrameResult> for FrameReader, with an
// accurate size hint.
