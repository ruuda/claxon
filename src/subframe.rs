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

//! The `subframe` module deals with subframes that make up a frame of the FLAC stream.

use error::{Error, FlacResult};
use input::Bitstream;
use sample;

#[derive(Clone, Copy, Debug)]
enum SubframeType {
    Constant,
    Verbatim,
    Fixed(u8),
    Lpc(u8)
}

#[derive(Clone, Copy)]
struct SubframeHeader {
    sf_type: SubframeType,
    wasted_bits_per_sample: u8
}

fn read_subframe_header(input: &mut Bitstream) -> FlacResult<SubframeHeader> {
    // The first bit must be a 0 padding bit.
    if 0 != try!(input.read_leq_u8(1)) {
        return Err(Error::InvalidSubframeHeader);
    }

    // Next is a 6-bit subframe type.
    let sf_type = match try!(input.read_leq_u8(6)) {
        0 => SubframeType::Constant,
        1 => SubframeType::Verbatim,

        // Bit patterns 00001x, 0001xx and 01xxxx are reserved, this library
        // would not know how to handle them, so this is an error.
        n if (n & 0b111_110 == 0b000_010)
          || (n & 0b111_100 == 0b000_100)
          || (n & 0b110_000 == 0b010_000) => {
            return Err(Error::InvalidSubframeHeader);
        }

        n if n & 0b111_000 == 0b001_000 => {
            let order = n & 0b000_111;

            // A fixed frame has order up to 4, other bit patterns are reserved.
            if order > 4 { return Err(Error::InvalidSubframeHeader); }

            SubframeType::Fixed(order)
        }

        // The only possibility left is bit pattern 1xxxxx, an LPC subframe.
        n => {
            // The xxxxx bits are the order minus one.
            println!("subframe type is LPC, bits: {:b}", n); // TODO: remove this.
            let order_mo = n & 0b011_111;
            SubframeType::Lpc(order_mo + 1)
        }
    };

    // Next bits indicates whether there are wasted bits per sample.
    let wastes_bits = 1 == try!(input.read_leq_u8(1));

    // If so, k - 1 zero bits follow, where k is the number of wasted bits.
    let wasted_bits = if !wastes_bits {
        0
    } else {
        let mut wbits = 1;
        while 1 != try!(input.read_leq_u8(1)) {
            wbits += 1;
        }
        wbits
    };

    println!("subframe has {} wasted bits per sample", wasted_bits); // TODO: remove this.

    let subframe_header = SubframeHeader {
        sf_type: sf_type,
        wasted_bits_per_sample: wasted_bits
    };
    Ok(subframe_header)
}

/// Given a signed two's complement integer in the `bits` least significant
/// bits of `val`, extends the sign bit to a valid 16-bit signed integer.
fn extend_sign_u16(val: u16, bits: u8) -> i16 {
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
pub fn extend_sign_u32(val: u32, bits: u8) -> i32 {
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
fn rice_to_signed<Sample: sample::WideSample>(val: Sample) -> Sample {
    // This uses bitwise arithmetic, because a literal cannot have type `Sample`,
    // I believe this is the most concise way to express the decoding.
    let half = val >> 1;
    if val & Sample::one() == Sample::one() {
        -half - Sample::one()
    } else {
        half
    }
}

#[test]
fn verify_rice_to_signed() {
    assert_eq!(rice_to_signed::<i16>(0), 0);
    assert_eq!(rice_to_signed::<i16>(1), -1);
    assert_eq!(rice_to_signed::<i16>(2), 1);
    assert_eq!(rice_to_signed::<i16>(3), -2);
    assert_eq!(rice_to_signed::<i16>(4), 2);

    assert_eq!(rice_to_signed::<i32>(3), -2);
    assert_eq!(rice_to_signed::<i32>(4), 2);

    assert_eq!(rice_to_signed::<i64>(3), -2);
    assert_eq!(rice_to_signed::<i64>(4), 2);
}

/// Decodes a subframe into the provided block-size buffer.
///
/// It is assumed that the length of the buffer is the block size.
pub fn decode<Sample: sample::Sample>
             (input: &mut Bitstream,
              bps: u8,
              buffer: &mut [Sample::Wide])
              -> FlacResult<()> {
    // The sample type should be wide enough to accomodate for all bits of the
    // stream, but this can be verified at a higher level than here. Still, it
    // is a good idea to make the assumption explicit. Due to prediction delta,
    // it must be at least one bit wider than the desired bits per sample.
    debug_assert!(Sample::Wide::width() > bps);

    // First up is the subframe header.
    let header = try!(read_subframe_header(input));

    // Then decode the subframe, properly per type.
    println!("encountered subframe of type {:?}",
             header.sf_type); // TODO: Remove this.
    match header.sf_type {
        SubframeType::Constant => try!(decode_constant(input, bps, buffer)),
        SubframeType::Verbatim => try!(decode_verbatim(input, bps, buffer)),
        SubframeType::Fixed(ord) => try!(decode_fixed(input, bps, ord, buffer)),
        SubframeType::Lpc(ord) => try!(decode_lpc(input, bps, ord, buffer))
    }

    // Finally, everything must be shifted by 'wasted bits per sample' to
    // the left. Note: it might be better performance-wise to do this on
    // the fly while decoding. That could be done if this is a bottleneck.
    if header.wasted_bits_per_sample > 0 {
        for s in buffer.iter_mut() {
            *s = *s << header.wasted_bits_per_sample as usize;
        }
    }

    println!("subframe decoded"); // TODO: Remove this.
    Ok(())
}

fn decode_residual<Sample: sample::Sample>
                  (input: &mut Bitstream,
                   bps: u8,
                   block_size: u16,
                   buffer: &mut [Sample::Wide])
                   -> FlacResult<()> {
    // Residual starts with two bits of coding method.
    let method = try!(input.read_leq_u8(2));
    println!("  residual coding method: {:b}", method); // TODO: Remove this.
    match method {
        0b00 => decode_partitioned_rice(input, bps, block_size, buffer),
        0b01 => decode_partitioned_rice2(input, bps, block_size, buffer),
        _ => Err(Error::InvalidResidual) // 10 and 11 are reserved.
    }
}

fn decode_partitioned_rice<Sample: sample::Sample>
                          (input: &mut Bitstream,
                           bps: u8,
                           block_size: u16,
                           buffer: &mut [Sample::Wide])
                           -> FlacResult<()> {
    println!("  decoding partitioned Rice, bs = {}, buffer.len = {}",
            block_size, buffer.len()); // TODO: remove this.

    // The block size, and therefore the buffer, cannot exceed 2^16 - 1.
    debug_assert!(buffer.len() <= 0xffff);

    // First are 4 bits partition order.
    let order = try!(input.read_leq_u8(4));

    // There are 2^order partitions. Note: the specification states a 4-bit
    // partition order, so the order is at most 31, so there could be 2^31
    // partitions, but the block size is a 16-bit number, so there are at
    // most 2^16 - 1 samples in the block. No values have been marked as
    // invalid by the specification though.
    let n_partitions = 1u32 << order as usize;
    let n_samples = block_size >> order as usize;
    let n_warm_up = block_size - buffer.len() as u16;

    println!("  order: {}, partitions: {}, samples: {}",
             order, n_partitions, n_samples); // TODO: Remove this.

    // The partition size must be at least as big as the number of warm-up
    // samples, otherwise the size of the first partition is negative.
    if n_warm_up > n_samples { return Err(Error::InvalidResidual); }

    let mut start = 0;
    for i in 0 .. n_partitions {
        let partition_size = n_samples - if i == 0 { n_warm_up } else { 0 };
        try!(decode_rice_partition(input, bps, &mut buffer[start ..
                                   start + partition_size as usize]));
        start = start + partition_size as usize;
    }

    Ok(())
}

fn decode_rice_partition<Sample: sample::Sample>
                        (input: &mut Bitstream,
                         bps: u8,
                         buffer: &mut [Sample::Wide])
                         -> FlacResult<()> {
    use std::mem;

    // The Rice partition starts with 4 bits Rice parameter.
    let rice_param = try!(input.read_leq_u8(4));

    // 1111 is an escape code that indicates unencoded binary.
    if rice_param == 0b1111 {
        // For unencoded binary, there are five bits indicating bits-per-sample.
        let rice_bps = try!(input.read_leq_u8(5));

        // There cannot be more bits per sample than the sample type.
        if bps < rice_bps {
            return Err(Error::InvalidBitsPerSample);
        }

        panic!("unencoded binary is not yet implemented"); // TODO
    } else {
        let max_sample = Sample::Wide::max();
        let max_q = max_sample >> rice_param as usize;

        // TODO: It is possible for the rice_param to be larger than the
        // sample width, which would be invalid. Check for that.
        // So instead of using `Sample::Wide::max`, the important thing is
        // that the decoded value cannot require more bits than bps + 1,
        // because the prediction delta adds at most one bit.

        for sample in buffer.iter_mut() {
            // First part of the sample is the quotient, unary encoded.
            // This means that there are q zeroes, and then a one. There
            // should not be more than max_q consecutive zeroes.
            let mut q = Sample::Wide::zero();
            while try!(input.read_leq_u8(1)) == 0 {
                if q == max_q {
                    println!("WARNING:
                             max_sample = {:?},
                             max_q = {:?},
                             q = {:?},
                             rice_param = {:?},
                             sample width = {:?}",
                             max_sample,
                             max_q,
                             q,
                             rice_param,
                             mem::size_of::<Sample>() * 8);
                    //return Err(Error::InvalidRiceCode);

                    // TODO: The reason that this crashes here, might be that
                    // the residual does not fit within the sample type, but
                    // after prediction it does. This means that we must use a
                    // wider type internally. That would be a nice idea anyway,
                    // because we can get rid of the side buffer in FrameReader
                    // and be fully generic, but still use i32 to decode i16,
                    // so we don't have to use i64 everywhere.
                }
                q = q + Sample::Wide::one();
            }

            // What follows is the remainder in `rice_param` bits. Because
            // rice_param is at most 14, this fits in an u16. TODO: for
            // the RICE2 partition it will not fit.
            let r_u16 = try!(input.read_leq_u16(rice_param));
            let r = Sample::Wide::from_u16(r_u16);

            *sample = rice_to_signed((q << rice_param as usize) | r);
        }
    }

    Ok(())
}

fn decode_partitioned_rice2<Sample: sample::Sample>
                           (input: &mut Bitstream,
                            bps: u8,
                            block_size: u16,
                            buffer: &mut [Sample::Wide])
                            -> FlacResult<()> {
    panic!("partitioned_rice2 is not yet implemented"); // TODO
}

fn decode_constant<Sample: sample::Sample>
                  (input: &mut Bitstream,
                   bps: u8,
                   buffer: &mut [Sample::Wide])
                   -> FlacResult<()> {
    // A constant block has <bits per sample> bits: the value of all samples.
    // The nofail variant is safe, because it has been verified before that the
    // `Sample` type is wide enough for the bits per sample. FLAC does not
    // support samples wider than 32 bits, so `read_leq_u32` suffices.
    // TODO: Actually, no. FLAC supports 32-bit samples, so the mid/side delta
    // would require 33 bits. But that is not subset FLAC, and the reference
    // decoder does not support it either.
    let sample_u32 = try!(input.read_leq_u32(bps));
    let sample = Sample::from_i32_nofail(extend_sign_u32(sample_u32, bps));

    for s in buffer.iter_mut() {
        *s = sample;
    }

    Ok(())
}

fn decode_verbatim<Sample: sample::Sample>
                  (input: &mut Bitstream,
                   bps: u8,
                   buffer: &mut [Sample::Wide])
                   -> FlacResult<()> {
    // This function must not be called for a sample wider than the sample type.
    // This has been verified at an earlier stage, but it is good to state the
    // assumption explicitly.
    debug_assert!(Sample::Wide::width() >= bps);

    // A verbatim block stores samples without encoding whatsoever.
    for s in buffer.iter_mut() {
        // The nofail version is safe, because it has been verified before that
        // the `Sample` type is wide enough for the bits per sample. FLAC does
        // not support samples wider than 32 bits, so `read_leq_u32` suffices.
        // TODO: Actually, no. FLAC supports 32-bit samples, so the mid/side
        // delta would require 33 bits. But that is not subset FLAC, and the
        // reference decoder does not support it either.
        let sample_u32 = try!(input.read_leq_u32(bps));
        *s = Sample::from_i32_nofail(extend_sign_u32(sample_u32, bps));
    }

    Ok(())
}

fn predict_fixed<Sample: sample::Sample>
                (order: u8, buffer: &mut [Sample::Wide])
                 -> FlacResult<()> {
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
    // `Sample::Wide` does not overflow when the starting sample fitted in
    // <bps + 1> bits, this does not guarantee that the value will fit in
    // <bps + 1> bits after prediction, which should be the case for valid FLAC
    // streams.

    let coefficients: &[i8] = match order {
        0 => &o0,
        1 => &o1,
        2 => &o2,
        3 => &o3,
        4 => &o4,
        _ => unreachable!()
    };

    let window_size = order as usize + 1;

    // TODO: abstract away this iterating over a window into a function?
    for i in 0 .. buffer.len() - order as usize {
        // Manually do the windowing, because .windows() returns immutable slices.
        let window = &mut buffer[i .. i + window_size];

        // The #coefficcients elements of the window store already decoded
        // samples, the last element of the window is the delta. Therefore,
        // predict based on the first #coefficients samples.
        let prediction = coefficients.iter()
                                     .map(|&c| Sample::Wide::from_i8(c))
                                     .zip(window.iter())
                                     .map(|(c, &s)| c * s)
                                     .sum();

        // The delta is stored, so the sample is the prediction + delta.
        window[coefficients.len()] += prediction;

        // TODO: Verify that the value fits in bps here? It will have to be
        // verified somewhere either way. Probably before decoding left/side,
        // to ensure that those do not overflow.
    }

    Ok(())
}

#[test]
fn verify_predict_fixed() {
    // The following data is from an actual FLAC stream and has been verified
    // against the reference decoder.
    let mut buffer = [-729, -722, -667, -19, -16,  17, -23, -7,
                        16,  -16,   -5,   3,  -8, -13, -15, -1];
    assert!(predict_fixed::<i16>(3, &mut buffer).is_ok());
    assert_eq!(&buffer, &[-729, -722, -667, -583, -486, -359, -225, -91,
                            59,  209,  354,  497,  630,  740,  812, 845]);

    // The following data causes overflow when not handled with care.
    let mut buffer = [21877, 27482, -6513];
    assert!(predict_fixed::<i16>(2, &mut buffer).is_ok());
    assert_eq!(&buffer, &[21877, 27482, 26574]);
}

fn decode_fixed<Sample: sample::Sample>
               (input: &mut Bitstream,
                bps: u8,
                order: u8,
                buffer: &mut [Sample::Wide])
                -> FlacResult<()> {
    println!("begin decoding fixed subframe"); // TODO: Remove this.
    // There are order * bits per sample unencoded warm-up sample bits.
    try!(decode_verbatim(input, bps, &mut buffer[.. order as usize]));

    println!("the warm-up samples are {:?}", buffer[0 .. order as usize].iter()
             .collect::<Vec<_>>()); // TODO: Remove this.

    // Next up is the residual. We decode into the buffer directly, the
    // predictor contributions will be added in a second pass. The first
    // `order` samples have been decoded already, so continue after that.
    try!(decode_residual(input, bps, buffer.len() as u16,
                         &mut buffer[order as usize ..]));

    try!(predict_fixed(order, buffer));

    Ok(())
}

fn predict_lpc<Sample: sample::Sample>
              (coefficients: &[i16],
               qlp_shift: i16,
               buffer: &mut [Sample::Wide])
               -> FlacResult<()> {

    use sample::WideSample;

    // The linear prediction is essentially an inner product of the known
    // samples with the coefficients, followed by the shift. The
    // coefficients are 16-bit at most, and there are at most 32 (2^5)
    // coefficients, so multiplying and summing fits in an i64 for sample
    // widths up to 43 bits.
    // TODO: But it is never verified that the samples do actually fit in 43
    // bits. Is that required, or is it guaranteed in some other way?

    let window_size = coefficients.len() + 1;
    debug_assert!(buffer.len() >= window_size);

    println!("  predicting using LPC predictor"); // TODO: Remove this.

    for i in 0 .. buffer.len() - coefficients.len() {
        // Manually do the windowing, because .windows() returns immutable slices.
        let window = &mut buffer[i .. i + window_size];

        // The #coefficcients elements of the window store already decoded
        // samples, the last element of the window is the delta. Therefore,
        // predict based on the first #coefficients samples.
        let prediction = coefficients.iter().zip(window.iter())
                                     .map(|(&c, &s)| c as i64 * s.to_i64())
                                     .sum::<i64>() >> qlp_shift;

        // The result should fit in the `Sample::Wide` type again, even with
        // one bit unused.
        let prediction = Sample::Wide::from_i64_spare_bit(prediction)
                                      .ok_or(Error::InvalidLpcSample);

        // The delta is stored, so the sample is the prediction + delta.
        let delta = window[coefficients.len()];
        window[coefficients.len()] = try!(prediction) + delta;
    }

    Ok(())
}

#[test]
fn verify_predict_lpc() {
    // The following data is from an actual FLAC stream and has been verified
    // against the reference decoder.
    let coefficients = [-75, 166,  121, -269, -75, -399, 1042];
    let mut buffer = [-796, -547, -285,  -32, 199,  443,  670, -2,
                       -23,   14,    6,    3,  -4,   12,   -2, 10];
    assert!(predict_lpc::<i16>(&coefficients, 9, &mut buffer).is_ok());
    assert_eq!(&buffer, &[-796, -547, -285,  -32,  199,  443,  670,  875,
                          1046, 1208, 1343, 1454, 1541, 1616, 1663, 1701]);

    // The following data causes an overflow when not handled with care.
    let coefficients = [119, -255, 555, -836, 879, -1199, 1757];
    let mut buffer = [-21363, -21951, -22649, -24364, -27297, -26870, -30017, 3157];
    assert!(predict_lpc::<i16>(&coefficients, 10, &mut buffer).is_ok());
    assert_eq!(&buffer, &[-21363, -21951, -22649, -24364, -27297, -26870, -30017, -29718]);
}

fn decode_lpc<Sample: sample::Sample>
             (input: &mut Bitstream,
              bps: u8,
              order: u8,
              buffer: &mut [Sample::Wide])
              -> FlacResult<()> {
    println!("begin decoding of LPC subframe"); // TODO: Remove this.
    // There are order * bits per sample unencoded warm-up sample bits.
    try!(decode_verbatim(input, bps, &mut buffer[.. order as usize]));

    println!("the warm-up samples are {:?}", buffer[0 .. order as usize].iter()
             .collect::<Vec<_>>()); // TODO: Remove this.

    // Next are four bits quantised linear predictor coefficient precision - 1.
    let qlp_precision = try!(input.read_leq_u8(4)) + 1;

    // The bit pattern 1111 is invalid.
    if qlp_precision - 1 == 0b1111 {
        return Err(Error::InvalidSubframe);
    }

    // Next are five bits quantized linear predictor coefficient shift,
    // in signed two's complement. Read 5 bits and then extend the sign bit.
    let qlp_shift_unsig = try!(input.read_leq_u16(5));
    let qlp_shift = extend_sign_u16(qlp_shift_unsig, 5);

    println!("  lpc: qlp_precision: {}, qlp_shift: {}, order: {}",
             qlp_precision, qlp_shift, order); // TODO: Remove this.

    // Finally, the coefficients themselves.
    // TODO: get rid of the allocation by pre-allocating a vector in the decoder.
    let mut coefficients = Vec::new();
    for _ in 0 .. order {
        // We can safely read into an u16, qlp_precision is at most 15.
        let coef_unsig = try!(input.read_leq_u16(qlp_precision));
        let coef = extend_sign_u16(coef_unsig, qlp_precision);
        coefficients.push(coef);
        println!("  > coef: {}", coef); // TODO: Remove this.
    }

    // Coefficients are used in reverse order for prediction.
    coefficients.reverse();

    // Next up is the residual. We decode it into the buffer directly, the
    // predictor contributions will be added in a second pass. The first
    // `order` samples have been decoded already, so continue after that.
    try!(decode_residual(input, bps, buffer.len() as u16,
                         &mut buffer[order as usize ..]));

    println!("  > first residual: {:?}, last residual: {:?}",
             buffer[order as usize],
             buffer[buffer.len() - 1]); // TODO: Remove this.

    try!(predict_lpc(&coefficients, qlp_shift, buffer));

    Ok(())
}
