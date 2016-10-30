// Claxon -- A FLAC decoding library in Rust
// Copyright 2014 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

//! The `subframe` module deals with subframes that make up a frame of the FLAC stream.

use std::i64;
use std::io;
use error::{Error, Result, fmt_err};
use input::Bitstream;

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

fn read_subframe_header<R: io::Read>(input: &mut Bitstream<R>) -> Result<SubframeHeader> {
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

    let subframe_header = SubframeHeader {
        sf_type: sf_type,
        wasted_bits_per_sample: wasted_bits,
    };
    Ok(subframe_header)
}

/// Given a signed two's complement integer in the `bits` least significant
/// bits of `val`, extends the sign bit to a valid 16-bit signed integer.
fn extend_sign_u16(val: u16, bits: u32) -> i16 {
    // For 32-bit integers, shifting by 32 bits causes different behaviour in
    // release and debug builds. While `(1_i16 << 16) == 0` both in debug and
    // release mode on my machine, I do not want to rely on it.
    if bits >= 16 {
        val as i16
    } else if val < (1 << (bits - 1)) {
        val as i16
    } else {
        (val as i16).wrapping_sub(1 << bits)
    }
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

// TODO: Extract this into a separate module.
/// Given a signed two's complement integer in the `bits` least significant
/// bits of `val`, extends the sign bit to a valid 32-bit signed integer.
pub fn extend_sign_u32(val: u32, bits: u32) -> i32 {
    // Shifting a 32-bit integer by more than 31 bits will panic, so we must
    // treat that case separately.
    if bits >= 32 {
        val as i32
    } else if val < (1 << (bits - 1)) {
        val as i32
    } else {
        (val as i32).wrapping_sub(1 << bits)
    }
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
fn rice_to_signed(val: i64) -> i64 {
    // I believe this is the most concise way to express the decoding.
    let half = val / 2;
    if val & 1 == 1 {
        -half - 1
    } else {
        half
    }
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
pub fn decode<R: io::Read>(input: &mut Bitstream<R>,
                           bps: u32,
                           buffer: &mut [i64])
                           -> Result<()> {
    // The sample type i64 should be wide enough to accomodate for all bits of
    // the stream, but this can be verified at a higher level than here. Still,
    // it is a good idea to make the assumption explicit. Due to prediction
    // delta it must be at least one bit wider than the desired bits per sample.
    debug_assert!(64 > bps);

    let header = try!(read_subframe_header(input));

    match header.sf_type {
        SubframeType::Constant => try!(decode_constant(input, bps, buffer)),
        SubframeType::Verbatim => try!(decode_verbatim(input, bps, buffer)),
        SubframeType::Fixed(ord) => try!(decode_fixed(input, bps, ord as u32, buffer)),
        SubframeType::Lpc(ord) => try!(decode_lpc(input, bps, ord as u32, buffer)),
    }

    // Finally, everything must be shifted by 'wasted bits per sample' to
    // the left. Note: it might be better performance-wise to do this on
    // the fly while decoding. That could be done if this is a bottleneck.
    if header.wasted_bits_per_sample > 0 {
        for s in buffer {
            *s = *s << header.wasted_bits_per_sample as usize;
        }
    }

    Ok(())
}

#[derive(Copy, Clone)]
enum RicePartitionType {
    Rice,
    Rice2,
}

fn decode_residual<R: io::Read>(input: &mut Bitstream<R>,
                                block_size: u16,
                                buffer: &mut [i64])
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
    let n_samples = block_size >> order;
    let n_warm_up = block_size - buffer.len() as u16;

    // The partition size must be at least as big as the number of warm-up
    // samples, otherwise the size of the first partition is negative.
    if n_warm_up > n_samples {
        return fmt_err("invalid residual");
    }

    // Finally decode the partitions themselves.
    match partition_type {
        RicePartitionType::Rice => {
            let mut start = 0;
            let mut len = n_samples - n_warm_up;
            for _ in 0..n_partitions {
                let slice = &mut buffer[start..start + len as usize];
                try!(decode_rice_partition(input, slice));
                start = start + len as usize;
                len = n_samples;
            }
        }
        RicePartitionType::Rice2 => {
            let mut start = 0;
            let mut len = n_samples - n_warm_up;
            for _ in 0..n_partitions {
                let slice = &mut buffer[start..start + len as usize];
                try!(decode_rice2_partition(input, slice));
                start = start + len as usize;
                len = n_samples;
            }
        }
    }

    Ok(())
}

// Performance note: all Rice partitions in real-world FLAC files are Rice
// partitions, not Rice2 partitions. Therefore it makes sense to inline this
// function into decode_residual.
#[inline(always)]
fn decode_rice_partition<R: io::Read>(input: &mut Bitstream<R>,
                                      buffer: &mut [i64])
                                      -> Result<()> {
    // A Rice partition (not Rice2), starts with a 4-bit Rice parameter.
    let rice_param = try!(input.read_leq_u8(4)) as u32;

    // All ones is an escape code that indicates unencoded binary.
    if rice_param == 0b1111 {
        // TODO: Return "unsupported" result instead.
        panic!("unencoded binary is not yet implemented");
    }

    if rice_param == 10 {
        return decode_rice_partition_param_10(input, buffer);
    }

    // TODO: Add monomorphized versions for other widths too.

    for sample in buffer.iter_mut() {
        // First part of the sample is the quotient, unary encoded.
        // This means that there are q zeros, and then a one.
        //
        // The reference decoder supports sample widths up to 24 bits, so with
        // the additional bytes for difference in channels and for prediction, a
        // sample fits in 26 bits. The Rice parameter could be as little as 1,
        // so the quotient can potentially be very large. However, in practice
        // it is not. For one test file (with 16 bit samples), the distribution
        // was as follows: q = 0: 45%, q = 1: 29%, q = 2: 15%, q = 3: 6%, q = 4:
        // 3%, q = 5: 1%, ... Values of q as large as 75 still occur though.
        let q = try!(input.read_unary()) as i64;

        // Next is the remainder, in rice_param bits. Because at this
        // point rice_param is at most 14, we can safely read into a u16.
        let r = try!(input.read_leq_u16(rice_param)) as i64;
        *sample = rice_to_signed((q << rice_param) | r);
    }

    Ok(())
}

// Some data about the freqency of various Rice parameters for a Rice partition,
// measured from real-world FLAC files:
//
// param  percentage
// -----  ----------
//     0   0.035
//     1   0.002
//     2   0.001
//     3   0.526
//     4   3.535
//     5   2.634
//     6   4.386
//     7  15.045
//     8  18.813
//     9  17.789
//    10  20.893
//    11  12.812
//    12   3.424
//    13   0.103
//    14   0.005

/// Decodes the samples in a Rice partition with parameter 10.
#[inline(always)]
fn decode_rice_partition_param_10<R: io::Read>(input: &mut Bitstream<R>,
                                               buffer: &mut [i64])
                                               -> Result<()> {
    for sample in buffer.iter_mut() {
        let q = try!(input.read_unary()) as i64;
        let r = try!(input.read_u10()) as i64;
        *sample = rice_to_signed((q << 10) | r);
    }
    Ok(())
}

// Performance note: a Rice2 partition is extremely uncommon, I haven’t seen a
// single one in any real-world FLAC file. So do not inline it, in order not to
// pollute the caller with dead code.
#[inline(never)]
fn decode_rice2_partition<R: io::Read>(input: &mut Bitstream<R>,
                                       buffer: &mut [i64])
                                       -> Result<()> {
    // A Rice2 partition, starts with a 5-bit Rice parameter.
    let rice_param = try!(input.read_leq_u8(5)) as u32;

    // All ones is an escape code that indicates unencoded binary.
    if rice_param == 0b11111 {
        // TODO: Return "unsupported" result instead.
        panic!("unencoded binary is not yet implemented");
    }

    for sample in buffer.iter_mut() {
        // First part of the sample is the quotient, unary encoded.
        let q = try!(input.read_unary()) as i64;

        // Next is the remainder, in rice_param bits. Because at this
        // point rice_param is at most 30, we can safely read into a u32.
        let r = try!(input.read_leq_u32(rice_param)) as i64;
        *sample = rice_to_signed((q << rice_param) | r);
    }

    Ok(())
}

fn decode_constant<R: io::Read>(input: &mut Bitstream<R>,
                                bps: u32,
                                buffer: &mut [i64])
                                -> Result<()> {
    let sample_u32 = try!(input.read_leq_u32(bps));
    let sample = extend_sign_u32(sample_u32, bps) as i64;

    for s in buffer {
        *s = sample;
    }

    Ok(())
}

fn decode_verbatim<R: io::Read>(input: &mut Bitstream<R>,
                                bps: u32,
                                buffer: &mut [i64])
                                -> Result<()> {

    // This function must not be called for a sample wider than the sample type.
    // This has been verified at an earlier stage, but it is good to state the
    // assumption explicitly. FLAC supports up to 32-bit samples, so the
    // mid/side delta would require 33 bits per sample. But that is not subset
    // FLAC, and the reference decoder does not support it either.
    // TODO: When this is assumed, it is possible to decode into the buffer
    // immediately, there is no need for i64 samples.
    debug_assert!(bps <= 32);

    // A verbatim block stores samples without encoding whatsoever.
    for s in buffer {
        let sample_u32 = try!(input.read_leq_u32(bps));
        *s = extend_sign_u32(sample_u32, bps) as i64;
    }

    Ok(())
}

fn predict_fixed(order: u32, buffer: &mut [i64]) -> Result<()> {

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
    // of those values again adds at most 3 bits, so a sample type that is 6
    // bits wider than bps should suffice. Note that although this means that
    // i64 does not overflow when the starting sample fitted in <bps + 1>
    // bits, this does not guarantee that the value will fit in <bps + 1> bits
    // after prediction, which should be the case for valid FLAC streams.

    let coefficients: &[i8] = match order {
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
        // predict based on the first #coefficients samples.
        let prediction = coefficients.iter()
                                     .map(|&c| c as i64)
                                     .zip(window.iter())
                                     .map(|(c, &s)| c * s)
                                     .sum::<i64>();

        // The delta is stored, so the sample is the prediction + delta.
        let delta = window[coefficients.len()];
        window[coefficients.len()] = prediction + delta;

        // TODO: Verify that the value fits in bps here? It will have to be
        // verified somewhere either way. Probably before decoding left/side,
        // to ensure that those do not overflow.
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

fn decode_fixed<R: io::Read>(input: &mut Bitstream<R>,
                             bps: u32,
                             order: u32,
                             buffer: &mut [i64])
                             -> Result<()> {
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

fn predict_lpc(coefficients: &[i16],
               qlp_shift: i16,
               buffer: &mut [i64])
               -> Result<()> {

    // The linear prediction is essentially an inner product of the known
    // samples with the coefficients, followed by the shift. The
    // coefficients are 16-bit at most, and there are at most 32 (2^5)
    // coefficients, so multiplying and summing fits in an i64 for sample
    // widths up to 43 bits.
    // TODO: But it is never verified that the samples do actually fit in 43
    // bits. Is that required, or is it guaranteed in some other way?

    let window_size = coefficients.len() + 1;
    debug_assert!(buffer.len() >= window_size);

    for i in 0..buffer.len() - coefficients.len() {
        // Manually do the windowing, because .windows() returns immutable slices.
        let window = &mut buffer[i..i + window_size];

        // The #coefficients elements of the window store already decoded
        // samples, the last element of the window is the delta. Therefore,
        // predict based on the first #coefficients samples.
        let prediction = coefficients.iter()
                                     .zip(window.iter())
                                     .map(|(&c, &s)| c as i64 * s)
                                     .sum::<i64>() >> qlp_shift;

        // The result should fit in an i64 again, even with one bit unused. This
        // ensures that adding the delta does not overflow, if the delta is also
        // within the correct range.
        // TODO: Do we have to do this check every time? Or can we just assume
        // that the values are within range, because valid FLAC files will never
        // violate these checks.
        if (prediction < (i64::MIN >> 1)) || (prediction > (i64::MAX >> 1)) {
            return Err(Error::FormatError("invalid LPC sample"));
        }

        // The delta is stored, so the sample is the prediction + delta.
        let delta = window[coefficients.len()];
        window[coefficients.len()] = prediction + delta;
    }

    Ok(())
}

#[test]
fn verify_predict_lpc() {
    // The following data is from an actual FLAC stream and has been verified
    // against the reference decoder. The data is from a 16-bit stream.
    let coefficients = [-75, 166,  121, -269, -75, -399, 1042];
    let mut buffer = [-796, -547, -285,  -32, 199,  443,  670, -2,
                       -23,   14,    6,    3,  -4,   12,   -2, 10];
    assert!(predict_lpc(&coefficients, 9, &mut buffer).is_ok());
    assert_eq!(&buffer, &[-796, -547, -285,  -32,  199,  443,  670,  875,
                          1046, 1208, 1343, 1454, 1541, 1616, 1663, 1701]);

    // The following data causes an overflow when not handled with care.
    let coefficients = [119, -255, 555, -836, 879, -1199, 1757];
    let mut buffer = [-21363, -21951, -22649, -24364, -27297, -26870, -30017, 3157];
    assert!(predict_lpc(&coefficients, 10, &mut buffer).is_ok());
    assert_eq!(&buffer, &[-21363, -21951, -22649, -24364, -27297, -26870, -30017, -29718]);
}

fn decode_lpc<R: io::Read>(input: &mut Bitstream<R>,
                           bps: u32,
                           order: u32,
                           buffer: &mut [i64])
                           -> Result<()> {
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

    // Finally, the coefficients themselves.
    // TODO: get rid of the allocation by pre-allocating a vector in the decoder.
    let mut coefficients = Vec::new();
    for _ in 0..order {
        // We can safely read into an u16, qlp_precision is at most 15.
        let coef_unsig = try!(input.read_leq_u16(qlp_precision));
        let coef = extend_sign_u16(coef_unsig, qlp_precision);
        coefficients.push(coef);
    }

    // Coefficients are used in reverse order for prediction.
    coefficients.reverse();

    // Next up is the residual. We decode it into the buffer directly, the
    // predictor contributions will be added in a second pass. The first
    // `order` samples have been decoded already, so continue after that.
    try!(decode_residual(input,
                         buffer.len() as u16,
                         &mut buffer[order as usize..]));

    try!(predict_lpc(&coefficients, qlp_shift, buffer));

    Ok(())
}
