// Claxon -- A FLAC decoding library in Rust
// Copyright 2014 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

//! The `subframe` module deals with subframes that make up a frame of the FLAC stream.

use std::cmp;
use std::num;
use error::{Error, Result, fmt_err};
use input::{Bitstream, ReadBytes};

#[derive(Clone, Copy, Debug)]
enum SubframeType {
    Constant,
    Verbatim,
    Fixed(u8),
    Lpc(u8),
}

#[derive(Clone, Copy)]
struct SubframeHeader {
    sf_type: SubframeType,
    wasted_bits_per_sample: u32,
}

fn read_subframe_header<R: ReadBytes>(input: &mut Bitstream<R>) -> Result<SubframeHeader> {
    // The first bit must be a 0 padding bit.
    if try!(input.read_bit()) {
        return fmt_err("invalid subframe header");
    }

    // Next is a 6-bit subframe type.
    let sf_type = match try!(input.read_leq_u8(6)) {
        0 => SubframeType::Constant,
        1 => SubframeType::Verbatim,

        // Bit patterns 00001x, 0001xx and 01xxxx are reserved, this library
        // would not know how to handle them, so this is an error. Values that
        // are reserved at the time of writing are a format error, the
        // `Unsupported` error type is for specified features that are not
        // implemented.
        n if (n & 0b111_110 == 0b000_010) || (n & 0b111_100 == 0b000_100) ||
             (n & 0b110_000 == 0b010_000) => {
            return fmt_err("invalid subframe header, encountered reserved value");
        }

        n if n & 0b111_000 == 0b001_000 => {
            let order = n & 0b000_111;

            // A fixed frame has order up to 4, other bit patterns are reserved.
            if order > 4 {
                return fmt_err("invalid subframe header, encountered reserved value");
            }

            SubframeType::Fixed(order)
        }

        // The only possibility left is bit pattern 1xxxxx, an LPC subframe.
        n => {
            // The xxxxx bits are the order minus one.
            let order_mo = n & 0b011_111;
            SubframeType::Lpc(order_mo + 1)
        }
    };

    // Next bits indicates whether there are wasted bits per sample.
    let wastes_bits = try!(input.read_bit());

    // If so, k - 1 zero bits follow, where k is the number of wasted bits.
    let wasted_bits = if !wastes_bits {
        0
    } else {
        1 + try!(input.read_unary())
    };

    // The spec puts no bounds on the number of wasted bits per sample, but more
    // than 31 does not make sense, as it would remove all data even for 32-bit
    // samples.
    if wasted_bits > 31 {
        return fmt_err("wasted bits per sample must not exceed 31");
    }

    let subframe_header = SubframeHeader {
        sf_type: sf_type,
        wasted_bits_per_sample: wasted_bits,
    };
    Ok(subframe_header)
}

/// Given a signed two's complement integer in the `bits` least significant
/// bits of `val`, extends the sign bit to a valid 16-bit signed integer.
#[inline(always)]
fn extend_sign_u16(val: u16, bits: u32) -> i16 {
    // First shift the value so the desired sign bit is the actual sign bit,
    // then convert to a signed integer, and then do an arithmetic shift back,
    // which will extend the sign bit.
    return ((val << (16 - bits)) as i16) >> (16 - bits);
}

#[test]
fn verify_extend_sign_u16() {
    assert_eq!(5, extend_sign_u16(5, 4));
    assert_eq!(0x3ffe, extend_sign_u16(0x3ffe, 15));
    assert_eq!(-5, extend_sign_u16(16 - 5, 4));
    assert_eq!(-3, extend_sign_u16(512 - 3, 9));
    assert_eq!(-1, extend_sign_u16(0xffff, 16));
    assert_eq!(-2, extend_sign_u16(0xfffe, 16));
    assert_eq!(-1, extend_sign_u16(0x7fff, 15));
}

/// Given a signed two's complement integer in the `bits` least significant
/// bits of `val`, extends the sign bit to a valid 32-bit signed integer.
#[inline(always)]
pub fn extend_sign_u32(val: u32, bits: u32) -> i32 {
    // First shift the value so the desired sign bit is the actual sign bit,
    // then convert to a signed integer, and then do an arithmetic shift back,
    // which will extend the sign bit.
    ((val << (32 - bits)) as i32) >> (32 - bits)
}

#[test]
fn verify_extend_sign_u32() {
    assert_eq!(5, extend_sign_u32(5, 4));
    assert_eq!(0x3ffffffe, extend_sign_u32(0x3ffffffe, 31));
    assert_eq!(-5, extend_sign_u32(16 - 5, 4));
    assert_eq!(-3, extend_sign_u32(512 - 3, 9));
    assert_eq!(-2, extend_sign_u32(0xfffe, 16));
    assert_eq!(-1, extend_sign_u32(0xffffffff_u32, 32));
    assert_eq!(-2, extend_sign_u32(0xfffffffe_u32, 32));
    assert_eq!(-1, extend_sign_u32(0x7fffffff, 31));

    // The data below are samples from a real FLAC stream.
    assert_eq!(-6392, extend_sign_u32(124680, 17));
    assert_eq!(-6605, extend_sign_u32(124467, 17));
    assert_eq!(-6850, extend_sign_u32(124222, 17));
    assert_eq!(-7061, extend_sign_u32(124011, 17));
}

/// Decodes a signed number from Rice coding to the two's complement.
///
/// The Rice coding used by FLAC operates on unsigned integers, but the
/// residual is signed. The mapping is done as follows:
///
///  0 -> 0
/// -1 -> 1
///  1 -> 2
/// -2 -> 3
///  2 -> 4
///  etc.
///
/// This function takes the unsigned value and converts it into a signed
/// number.
#[inline(always)]
fn rice_to_signed(val: u32) -> i32 {
    // The following bit-level hackery compiles to only four instructions on
    // x64. It is equivalent to the following code:
    //
    //   if val & 1 == 1 {
    //       -1 - (val / 2) as i32
    //   } else {
    //       (val / 2) as i32
    //   }
    //
    let half = (val >> 1) as i32;
    let extended_bit_0 = ((val << 31) as i32) >> 31;
    half ^ extended_bit_0
}

#[test]
fn verify_rice_to_signed() {
    assert_eq!(rice_to_signed(0), 0);
    assert_eq!(rice_to_signed(1), -1);
    assert_eq!(rice_to_signed(2), 1);
    assert_eq!(rice_to_signed(3), -2);
    assert_eq!(rice_to_signed(4), 2);
}

/// Decodes a subframe into the provided block-size buffer.
///
/// It is assumed that the length of the buffer is the block size.
pub fn decode<R: ReadBytes>(input: &mut Bitstream<R>,
                            bps: u32,
                            buffer: &mut [i32])
                            -> Result<()> {
    // The sample type i32 should be wide enough to accomodate for all bits of
    // the stream, but this can be verified at a higher level than here. Still,
    // it is a good idea to make the assumption explicit. FLAC supports up to
    // sample widths of 32 in theory, so with the delta between channels that
    // requires 33 bits, but the reference decoder supports only subset FLAC of
    // 24 bits per sample at most, so restricting ourselves to i32 is fine.
    debug_assert!(32 >= bps);

    let header = try!(read_subframe_header(input));

    if header.wasted_bits_per_sample >= bps {
        return fmt_err("subframe has no non-wasted bits");
    }

    // If there are wasted bits, the subframe stores samples with a lower bps
    // than the stream bps. We later shift all the samples left to correct this.
    let sf_bps = bps - header.wasted_bits_per_sample;

    match header.sf_type {
        SubframeType::Constant => try!(decode_constant(input, sf_bps, buffer)),
        SubframeType::Verbatim => try!(decode_verbatim(input, sf_bps, buffer)),
        SubframeType::Fixed(ord) => try!(decode_fixed(input, sf_bps, ord as u32, buffer)),
        SubframeType::Lpc(ord) => try!(decode_lpc(input, sf_bps, ord as u32, buffer)),
    }

    // Finally, everything must be shifted by 'wasted bits per sample' to
    // the left. Note: it might be better performance-wise to do this on
    // the fly while decoding. That could be done if this is a bottleneck.
    if header.wasted_bits_per_sample > 0 {
        debug_assert!(header.wasted_bits_per_sample <= 31,
                      "Cannot shift by more than the sample width.");
        for s in buffer {
            // For a valid FLAC file, this shift does not overflow. For an
            // invalid file it might, and then we decode garbage, but we don't
            // crash the program in debug mode due to shift overflow.
            *s = s.wrapping_shl(header.wasted_bits_per_sample);
        }
    }

    Ok(())
}

#[derive(Copy, Clone)]
enum RicePartitionType {
    Rice,
    Rice2,
}

fn decode_residual<R: ReadBytes>(input: &mut Bitstream<R>,
                                 block_size: u16,
                                 buffer: &mut [i32])
                                 -> Result<()> {
    // Residual starts with two bits of coding method.
    let partition_type = match try!(input.read_leq_u8(2)) {
        0b00 => RicePartitionType::Rice,
        0b01 => RicePartitionType::Rice2,
        // 10 and 11 are reserved.
        _ => return fmt_err("invalid residual, encountered reserved value"),
    };

    // Next are 4 bits partition order.
    let order = try!(input.read_leq_u8(4));

    // There are 2^order partitions. Note: the specification states a 4-bit
    // partition order, so the order is at most 31, so there could be 2^31
    // partitions, but the block size is a 16-bit number, so there are at
    // most 2^16 - 1 samples in the block. No values have been marked as
    // invalid by the specification though.
    let n_partitions = 1u32 << order;
    let n_samples_per_partition = block_size >> order;

    // The partitions together must fill the block. If the block size is not a
    // multiple of 2^order; if we shifted off some bits, then we would not fill
    // the entire block. Such a partition order is invalid for this block size.
    if block_size & (n_partitions - 1) as u16 != 0 {
        return fmt_err("invalid partition order")
    }

    // NOTE: the check above checks that block_size is a multiple of n_partitions
    // (this works because n_partitions is a power of 2). The check below is
    // equivalent but more expensive.
    debug_assert_eq!(n_partitions * n_samples_per_partition as u32, block_size as u32);

    let n_warm_up = block_size - buffer.len() as u16;

    // The partition size must be at least as big as the number of warm-up
    // samples, otherwise the size of the first partition is negative.
    if n_warm_up > n_samples_per_partition {
        return fmt_err("invalid residual");
    }

    // Finally decode the partitions themselves.
    match partition_type {
        RicePartitionType::Rice => {
            let mut start = 0;
            let mut len = n_samples_per_partition - n_warm_up;
            for _ in 0..n_partitions {
                let slice = &mut buffer[start..start + len as usize];
                try!(decode_rice_partition(input, slice));
                start = start + len as usize;
                len = n_samples_per_partition;
            }
        }
        RicePartitionType::Rice2 => {
            let mut start = 0;
            let mut len = n_samples_per_partition - n_warm_up;
            for _ in 0..n_partitions {
                let slice = &mut buffer[start..start + len as usize];
                try!(decode_rice2_partition(input, slice));
                start = start + len as usize;
                len = n_samples_per_partition;
            }
        }
    }

    Ok(())
}

// Performance note: all Rice partitions in real-world FLAC files are Rice
// partitions, not Rice2 partitions. Therefore it makes sense to inline this
// function into decode_residual.
#[inline(always)]
fn decode_rice_partition<R: ReadBytes>(input: &mut Bitstream<R>,
                                       buffer: &mut [i32])
                                       -> Result<()> {
    // A Rice partition (not Rice2), starts with a 4-bit Rice parameter.
    let rice_param = try!(input.read_leq_u8(4)) as u32;

    // All ones is an escape code that indicates unencoded binary.
    if rice_param == 0b1111 {
        return Err(Error::Unsupported("unencoded binary is not yet implemented"))
    }

    // About the decoding below: the first part of the sample is the quotient,
    // unary encoded. This means that there are q zeros, and then a one.
    //
    // The reference decoder supports sample widths up to 24 bits, so with
    // the additional bytes for difference in channels and for prediction, a
    // sample fits in 26 bits. The Rice parameter could be as little as 1,
    // so the quotient can potentially be very large. However, in practice
    // it is rarely greater than 5. Values as large as 75 still occur though.
    //
    // Next up is the remainder in rice_param bits. Depending on the number of
    // bits, at most two or three bytes need to be read, so the code below is
    // split into two cases to allow a more efficient reading function to be
    // used when possible. About 45% of the time, rice_param is less than 9
    // (measured from real-world FLAC files).

    if rice_param <= 8 {
        for sample in buffer.iter_mut() {
            let q = try!(input.read_unary());
            let r = try!(input.read_leq_u8(rice_param)) as u32;
            *sample = rice_to_signed((q << rice_param) | r);
        }
    } else {
        for sample in buffer.iter_mut() {
            let q = try!(input.read_unary());
            let r = try!(input.read_gt_u8_leq_u16(rice_param));
            *sample = rice_to_signed((q << rice_param) | r);
        }
    }

    Ok(())
}

// Performance note: a Rice2 partition is extremely uncommon, I haven’t seen a
// single one in any real-world FLAC file. So do not inline it, in order not to
// pollute the caller with dead code.
#[inline(never)]
#[cold]
fn decode_rice2_partition<R: ReadBytes>(input: &mut Bitstream<R>,
                                        buffer: &mut [i32])
                                        -> Result<()> {
    // A Rice2 partition, starts with a 5-bit Rice parameter.
    let rice_param = try!(input.read_leq_u8(5)) as u32;

    // All ones is an escape code that indicates unencoded binary.
    if rice_param == 0b11111 {
        return Err(Error::Unsupported("unencoded binary is not yet implemented"))
    }

    for sample in buffer.iter_mut() {
        // First part of the sample is the quotient, unary encoded.
        let q = try!(input.read_unary());

        // Next is the remainder, in rice_param bits. Because at this
        // point rice_param is at most 30, we can safely read into a u32.
        let r = try!(input.read_leq_u32(rice_param));
        *sample = rice_to_signed((q << rice_param) | r);
    }

    Ok(())
}

fn decode_constant<R: ReadBytes>(input: &mut Bitstream<R>,
                                 bps: u32,
                                 buffer: &mut [i32])
                                 -> Result<()> {
    let sample_u32 = try!(input.read_leq_u32(bps));
    let sample = extend_sign_u32(sample_u32, bps);

    for s in buffer {
        *s = sample;
    }

    Ok(())
}

#[cold]
fn decode_verbatim<R: ReadBytes>(input: &mut Bitstream<R>,
                                 bps: u32,
                                 buffer: &mut [i32])
                                 -> Result<()> {

    // This function must not be called for a sample wider than the sample type.
    // This has been verified at an earlier stage, but it is good to state the
    // assumption explicitly. FLAC supports up to 32-bit samples, so the
    // mid/side delta would require 33 bits per sample. But that is not subset
    // FLAC, and the reference decoder does not support it either.
    debug_assert!(bps <= 32);

    // A verbatim block stores samples without encoding whatsoever.
    for s in buffer {
        *s = extend_sign_u32(try!(input.read_leq_u32(bps)), bps);
    }

    Ok(())
}

fn predict_fixed(order: u32, buffer: &mut [i32]) -> Result<()> {
    // When this is called during decoding, the order as read from the subframe
    // header has already been verified, so it is safe to assume that
    // 0 <= order <= 4. Still, it is good to state that assumption explicitly.
    debug_assert!(order <= 4);

    // Coefficients for fitting an order n polynomial. You get these
    // coefficients by writing down n numbers, then their differences, then the
    // differences of the differences, etc. What results is Pascal's triangle
    // with alternating signs.
    let o0 = [];
    let o1 = [1];
    let o2 = [-1, 2];
    let o3 = [1, -3, 3];
    let o4 = [-1, 4, -6, 4];

    // Multiplying samples with at most 6 adds 3 bits. Then summing at most 5
    // of those values again adds at most 4 bits, so a sample type that is 7
    // bits wider than bps should suffice. Subset FLAC supports at most 24 bits
    // per sample, 25 for the channel delta, so using an i32 is safe here.

    let coefficients: &[i32] = match order {
        0 => &o0,
        1 => &o1,
        2 => &o2,
        3 => &o3,
        4 => &o4,
        _ => unreachable!(),
    };

    let window_size = order as usize + 1;

    // TODO: abstract away this iterating over a window into a function?
    for i in 0..buffer.len() - order as usize {
        // Manually do the windowing, because .windows() returns immutable slices.
        let window = &mut buffer[i..i + window_size];

        // The #coefficients elements of the window store already decoded
        // samples, the last element of the window is the delta. Therefore,
        // predict based on the first #coefficients samples. From the note
        // above we know that the multiplication will not overflow for 24-bit
        // samples, so the wrapping mul is safe. If it wraps, the file was
        // invalid, and we make no guarantees about the decoded result. But
        // we explicitly do not crash.
        let prediction = coefficients.iter()
                                     .zip(window.iter())
                                     .map(|(&c, &s)| num::Wrapping(c) * num::Wrapping(s))
                                     // Rust 1.13 does not support using `sum`
                                     // with `Wrapping`, so do a fold.
                                     .fold(num::Wrapping(0), |a, x| a + x).0;

        // The delta is stored, so the sample is the prediction + delta.
        let delta = window[coefficients.len()];
        window[coefficients.len()] = prediction.wrapping_add(delta);
    }

    Ok(())
}

#[test]
fn verify_predict_fixed() {
    // The following data is from an actual FLAC stream and has been verified
    // against the reference decoder. The data is from a 16-bit stream.
    let mut buffer = [-729, -722, -667, -19, -16,  17, -23, -7,
                        16,  -16,   -5,   3,  -8, -13, -15, -1];
    assert!(predict_fixed(3, &mut buffer).is_ok());
    assert_eq!(&buffer, &[-729, -722, -667, -583, -486, -359, -225, -91,
                            59,  209,  354,  497,  630,  740,  812, 845]);

    // The following data causes overflow of i32 when not handled with care.
    let mut buffer = [21877, 27482, -6513];
    assert!(predict_fixed(2, &mut buffer).is_ok());
    assert_eq!(&buffer, &[21877, 27482, 26574]);
}

fn decode_fixed<R: ReadBytes>(input: &mut Bitstream<R>,
                              bps: u32,
                              order: u32,
                              buffer: &mut [i32])
                              -> Result<()> {
    // The length of the buffer which is passed in, is the length of the block.
    // Thus, the number of warm-up samples must not exceed that length.
    if buffer.len() < order as usize {
        return fmt_err("invalid fixed subframe, order is larger than block size")
    }

    // There are order * bits per sample unencoded warm-up sample bits.
    try!(decode_verbatim(input, bps, &mut buffer[..order as usize]));

    // Next up is the residual. We decode into the buffer directly, the
    // predictor contributions will be added in a second pass. The first
    // `order` samples have been decoded already, so continue after that.
    try!(decode_residual(input,
                         buffer.len() as u16,
                         &mut buffer[order as usize..]));

    try!(predict_fixed(order, buffer));

    Ok(())
}

/// Apply LPC prediction for subframes with LPC order of at most 12.
///
/// This function takes advantage of the upper bound on the order. Virtually all
/// files that occur in the wild are subset-compliant files, which have an order
/// of at most 12, so it makes sense to optimize for this. A simpler (but
/// slower) fallback is implemented in `predict_lpc_high_order`.
fn predict_lpc_low_order(
    raw_coefficients: &[i16],
    qlp_shift: i16,
    buffer: &mut [i32],
) {
    debug_assert!(qlp_shift >= 0, "Right-shift by negative value is not allowed.");
    debug_assert!(qlp_shift < 64, "Cannot shift by more than integer width.");
    // The decoded residuals are 25 bits at most (assuming subset FLAC of at
    // most 24 bits per sample, but there is the delta encoding for channels).
    // The coefficients are 16 bits at most, so their product is 41 bits. In
    // practice the predictor order does not exceed 12, so adding 12 numbers of
    // 41 bits each requires at most 53 bits. Therefore, do all intermediate
    // computations as i64.

    // In the code below, a predictor order of 12 is assumed. This aids
    // optimization and vectorization by making some counts available at compile
    // time. If the actual order is less than 12, simply set the early
    // coefficients to 0.
    let order = raw_coefficients.len();
    let coefficients = {
        let mut buf = [0i64; 12];
        let mut i = 12 - order;
        for c in raw_coefficients {
            buf[i] = *c as i64;
            i = i + 1;
        }
        buf
    };

    // The linear prediction is essentially an inner product of the known
    // samples with the coefficients, followed by a shift. To be able to do an
    // inner product of 12 elements at a time, we must first have 12 samples.
    // If the predictor order is less, first predict the few samples after the
    // warm-up samples.
    let left = cmp::min(12, buffer.len()) - order;
    for i in 0..left {
        let prediction = raw_coefficients.iter()
                                         .zip(&buffer[i..order + i])
                                         .map(|(&c, &s)| c as i64 * s as i64)
                                         .sum::<i64>() >> qlp_shift;
        let delta = buffer[order + i] as i64;
        buffer[order + i] = (prediction + delta) as i32;
    }

    if buffer.len() <= 12 {
        return
    }

    // At this point, buffer[0..12] has been predicted. For the rest of the
    // buffer we can do inner products of 12 samples. This reduces the amount of
    // conditional code, and improves performance significantly.
    for i in 12..buffer.len() {
        let prediction = coefficients.iter()
                                     .zip(&buffer[i - 12..i])
                                     .map(|(&c, &s)| c * s as i64)
                                     .sum::<i64>() >> qlp_shift;
        let delta = buffer[i] as i64;
        buffer[i] = (prediction + delta) as i32;
    }
}

/// Apply LPC prediction for non-subset subframes, with LPC order > 12.
fn predict_lpc_high_order(
    coefficients: &[i16],
    qlp_shift: i16,
    buffer: &mut [i32],
) {
    // NOTE: See `predict_lpc_low_order` for more details. This function is a
    // copy that lifts the order restrictions (and specializations) at the cost
    // of performance. It is only used for subframes with a high LPC order,
    // which only occur in non-subset files. Such files are rare in the wild.

    let order = coefficients.len();

    debug_assert!(qlp_shift >= 0, "Right-shift by negative value is not allowed.");
    debug_assert!(qlp_shift < 64, "Cannot shift by more than integer width.");
    debug_assert!(order > 12, "Use the faster predict_lpc_low_order for LPC order <= 12.");
    debug_assert!(buffer.len() >= order, "Buffer must fit at least `order` warm-up samples.");

    // The linear prediction is essentially an inner product of the known
    // samples with the coefficients, followed by a shift. The first `order`
    // samples are stored as-is.
    for i in order..buffer.len() {
        let prediction = coefficients.iter()
                                     .zip(&buffer[i - order..i])
                                     .map(|(&c, &s)| c as i64 * s as i64)
                                     .sum::<i64>() >> qlp_shift;
        let delta = buffer[i] as i64;
        buffer[i] = (prediction + delta) as i32;
    }
}

#[test]
fn verify_predict_lpc() {
    // The following data is from an actual FLAC stream and has been verified
    // against the reference decoder. The data is from a 16-bit stream.
    let coefficients = [-75, 166,  121, -269, -75, -399, 1042];
    let mut buffer = [-796, -547, -285,  -32, 199,  443,  670, -2,
                       -23,   14,    6,    3,  -4,   12,   -2, 10];
    predict_lpc_low_order(&coefficients, 9, &mut buffer);
    assert_eq!(&buffer, &[-796, -547, -285,  -32,  199,  443,  670,  875,
                          1046, 1208, 1343, 1454, 1541, 1616, 1663, 1701]);

    // The following data causes an overflow when not handled with care.
    let coefficients = [119, -255, 555, -836, 879, -1199, 1757];
    let mut buffer = [-21363, -21951, -22649, -24364, -27297, -26870, -30017, 3157];
    predict_lpc_low_order(&coefficients, 10, &mut buffer);
    assert_eq!(&buffer, &[-21363, -21951, -22649, -24364, -27297, -26870, -30017, -29718]);

    // The following data from a real-world file has a high LPC order, is has
    // more than 12 coefficients. The excepted output has been verified against
    // the reference decoder.
    let coefficients = [
        709, -2589, 4600, -4612, 1350, 4220, -9743, 12671, -12129, 8586,
        -3775, -645, 3904, -5543, 4373, 182, -6873, 13265, -15417, 11550,
    ];
    let mut buffer = [
        213238, 210830, 234493, 209515, 235139, 201836, 208151, 186277, 157720, 148176,
        115037, 104836, 60794, 54523, 412, 17943, -6025, -3713, 8373, 11764, 30094,
    ];
    predict_lpc_high_order(&coefficients, 12, &mut buffer);
    assert_eq!(&buffer, &[
        213238, 210830, 234493, 209515, 235139, 201836, 208151, 186277, 157720, 148176,
        115037, 104836, 60794, 54523, 412, 17943, -6025, -3713, 8373, 11764, 33931,
    ]);
}

fn decode_lpc<R: ReadBytes>(input: &mut Bitstream<R>,
                            bps: u32,
                            order: u32,
                            buffer: &mut [i32])
                            -> Result<()> {
    // The order minus one fits in 5 bits, so the order is at most 32.
    debug_assert!(order <= 32);

    // On the frame decoding level it is ensured that the buffer is large
    // enough. If it can't even fit the warm-up samples, then there is a frame
    // smaller than its lpc order, which is invalid.
    if buffer.len() < order as usize {
        return fmt_err("invalid LPC subframe, lpc order is larger than block size")
    }

    // There are order * bits per sample unencoded warm-up sample bits.
    try!(decode_verbatim(input, bps, &mut buffer[..order as usize]));

    // Next are four bits quantised linear predictor coefficient precision - 1.
    let qlp_precision = try!(input.read_leq_u8(4)) as u32 + 1;

    // The bit pattern 1111 is invalid.
    if qlp_precision - 1 == 0b1111 {
        return fmt_err("invalid subframe, qlp precision value invalid");
    }

    // Next are five bits quantized linear predictor coefficient shift,
    // in signed two's complement. Read 5 bits and then extend the sign bit.
    let qlp_shift_unsig = try!(input.read_leq_u16(5));
    let qlp_shift = extend_sign_u16(qlp_shift_unsig, 5);

    // The spec does allow the qlp shift to be negative, but in practice this
    // does not happen. Fully supporting it would be a performance hit, as an
    // arithmetic shift by a negative amount is invalid, so this would incur a
    // branch. If a real-world file ever hits this case, then we should consider
    // making two LPC predictors, one for positive, and one for negative qlp.
    if qlp_shift < 0 {
        let msg = "a negative quantized linear predictor coefficient shift is \
                   not supported, please file a bug.";
        return Err(Error::Unsupported(msg))
    }

    // Finally, the coefficients themselves. The order is at most 32, so all
    // coefficients can be kept on the stack. Store them in reverse, because
    // that how they are used in prediction.
    let mut coefficients = [0; 32];
    for coef in coefficients[..order as usize].iter_mut().rev() {
        // We can safely read into a u16, qlp_precision is at most 15.
        let coef_unsig = try!(input.read_leq_u16(qlp_precision));
        *coef = extend_sign_u16(coef_unsig, qlp_precision);
    }

    // Next up is the residual. We decode it into the buffer directly, the
    // predictor contributions will be added in a second pass. The first
    // `order` samples have been decoded already, so continue after that.
    try!(decode_residual(input,
                         buffer.len() as u16,
                         &mut buffer[order as usize..]));

    // In "subset"-compliant files, the LPC order is at most 12. For LPC
    // prediction of such files we have a special fast path that takes advantage
    // of the low order. We can still decode non-subset file using a less
    // specialized implementation. Non-subset files are rare in the wild.
    if order <= 12 {
        predict_lpc_low_order(&coefficients[..order as usize], qlp_shift, buffer);
    } else {
        predict_lpc_high_order(&coefficients[..order as usize], qlp_shift, buffer);
    }

    Ok(())
}
