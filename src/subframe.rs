// Claxon -- A FLAC decoding library in Rust
// Copyright (C) 2014-2015 Ruud van Asseldonk
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use std::num;
use std::num::{Int, UnsignedInt};
use bitstream::Bitstream;
use error::{FlacError, FlacResult};

#[derive(Copy, Show)] // TODO: this should not implement Show.
enum SubframeType {
    Constant,
    Verbatim,
    Fixed(u8),
    Lpc(u8)
}

#[derive(Copy)]
struct SubframeHeader {
    sf_type: SubframeType,
    wasted_bits_per_sample: u8
}

fn read_subframe_header(input: &mut Bitstream) -> FlacResult<SubframeHeader> {
    // The first bit must be a 0 padding bit.
    if 0 != try!(input.read_leq_u8(1)) {
        return Err(FlacError::InvalidSubframeHeader);
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
            return Err(FlacError::InvalidSubframeHeader);
        }

        n if n & 0b111_000 == 0b001_000 => {
            let order = n & 0b000_111;

            // A fixed frame has order up to 4, other bit patterns are reserved.
            if order > 4 { return Err(FlacError::InvalidSubframeHeader); }

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
            wbits = wbits + 1;
        }
        wbits
    };

    let subframe_header = SubframeHeader {
        sf_type: sf_type,
        wasted_bits_per_sample: wasted_bits
    };
    Ok(subframe_header)
}

/// Given a signed two's complement integer in the `bits` least significant
/// bits of `val`, extends the sign bit to a valid 16-bit signed integer.
fn extend_sign(val: u16, bits: u8) -> i16 {
    let sign_bit = val >> (bits as usize - 1);

    // Extend the sign bit into the remaining bits.
    let sign_extension = range(bits as usize, 16)
                         .fold(0, |s, i| s | (sign_bit << i));

    // Note: overflow in the cast is intended.
    (val | sign_extension) as i16
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
/// number. The return type still has unsigned type, because arithmetic relies
/// on overflow.
fn rice_to_signed<Sample>(val: Sample) -> Sample where Sample: UnsignedInt {
    // This uses bitwise arithmetic, because a literal cannot have type `Sample`,
    // I believe this is the most concise way to express the decoding.
    let sample_max: Sample = Int::max_value();
    if val & Int::one() == Int::one() {
        sample_max - (val >> 1)
    } else {
        val >> 1
    }
}

#[test]
fn verify_rice_to_signed() {
    assert_eq!(rice_to_signed(0u8), 0u8);
    assert_eq!(rice_to_signed(1u8), 0xffu8 - 0);
    assert_eq!(rice_to_signed(2u8), 1u8);
    assert_eq!(rice_to_signed(3u8), 0xffu8 - 1);
    assert_eq!(rice_to_signed(4u8), 2u8);

    assert_eq!(rice_to_signed(3u16), 0xffffu16 - 1);
    assert_eq!(rice_to_signed(4u16), 2u16);

    assert_eq!(rice_to_signed(3u32), 0xffffffffu32 - 1);
    assert_eq!(rice_to_signed(4u32), 2u32);
}

// TODO: Remove this function.
fn show_sample<Sample: UnsignedInt>(x: Sample) -> i16 {
    let x_u16: u16 = num::cast(x).unwrap();
    x_u16 as i16
}

pub struct SubframeDecoder<'r, Sample> {
    bits_per_sample: u8,
    input: &'r mut Bitstream<'r>
}

impl<'r, Sample> SubframeDecoder<'r, Sample> where Sample: UnsignedInt {
    /// Creates a new subframe decoder capable of decoding several subframes.
    ///
    /// The size of the `Sample` type must be wide enough to accomodate for
    /// `bits_per_sample` bits per sample.
    pub fn new(bits_per_sample: u8, input: &'r mut Bitstream<'r>)
               -> SubframeDecoder<'r, Sample> {
        // The sample type should be wide enough to accomodate for all bits of
        // the stream, but this can be verified at a higher level than here.
        // Still, it is a good idea to make the assumption explicit.
        use std::mem::size_of;
        debug_assert!(bits_per_sample as usize <= size_of::<Sample>() * 8);

        SubframeDecoder { bits_per_sample: bits_per_sample, input: input }
    }

    /// Decodes the subframe into the provided block-size buffer.
    ///
    /// It is assumed that the length of the buffer is the block size.
    pub fn decode(&mut self, buffer: &mut [Sample]) -> FlacResult<()> {
        // First up is the subframe header.
        let header = try!(read_subframe_header(self.input));

        // Then decode the subframe, properly per type.
        println!("encountered subframe of type {:?}",
                 header.sf_type); // TODO: Remove this.
        match header.sf_type {
            SubframeType::Constant => try!(self.decode_constant(buffer)),
            SubframeType::Verbatim => try!(self.decode_verbatim(buffer)),
            SubframeType::Fixed(ord) => try!(self.decode_fixed(ord, buffer)),
            SubframeType::Lpc(ord) => try!(self.decode_lpc(ord, buffer))
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

    fn decode_residual(&mut self, block_size: u16,
                       buffer: &mut [Sample]) -> FlacResult<()> {
        // Residual starts with two bits of coding method.
        let method = try!(self.input.read_leq_u8(2));
        println!("  residual coding method: {:b}", method); // TODO: Remove this.
        match method {
            0b00 => self.decode_partitioned_rice(block_size, buffer),
            0b01 => self.decode_partitioned_rice2(block_size, buffer),
            _ => Err(FlacError::InvalidResidual) // 10 and 11 are reserved.
        }
    }

    fn decode_partitioned_rice(&mut self, block_size: u16,
                               buffer: &mut [Sample]) -> FlacResult<()> {
        println!("  decoding partitioned Rice, bs = {}, buffer.len = {}",
                block_size, buffer.len()); // TODO: remove this.

        // The block size, and therefore the buffer, cannot exceed 2^16 - 1.
        debug_assert!(buffer.len() <= 0xffff);

        // First are 4 bits partition order.
        let order = try!(self.input.read_leq_u8(4));

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
        if n_warm_up > n_samples { return Err(FlacError::InvalidResidual); }

        let mut start = 0us;
        for i in range(0, n_partitions) {
            let partition_size = n_samples - if i == 0 { n_warm_up } else { 0 };
            println!("  > decoding partition {}, from {} to {}",
                     i, start, start + partition_size as usize); // TODO: Remove this.
            try!(self.decode_rice_partition(buffer.slice_mut(start,
                                            start + partition_size as usize)));
            start = start + partition_size as usize;
        }

        Ok(())
    }

    fn decode_rice_partition(&mut self,
                             buffer: &mut [Sample]) -> FlacResult<()> {
        // The Rice partition starts with 4 bits Rice parameter.
        let rice_param = try!(self.input.read_leq_u8(4));

        println!("  >   rice param: {}", rice_param); // TODO: Remove this.

        // 1111 is an escape code that indicates unencoded binary.
        if rice_param == 0b1111 {
            // For unencoded binary, there are five bits indicating bits-per-sample.
            let bps = try!(self.input.read_leq_u8(5));

            // There cannot be more bits per sample than the sample type.
            if self.bits_per_sample < bps {
                return Err(FlacError::InvalidBitsPerSample);
            }

            panic!("unencoded binary is not yet implemented"); // TODO
        } else {
            let max_sample: Sample = Int::max_value();
            let max_q = max_sample >> rice_param as usize;

            // TODO: It is possible for the rice_param to be larger than the
            // sample width, which would be invalid. Check for that.

            for sample in buffer.iter_mut() {
                // First part of the sample is the quotient, unary encoded.
                // This means that there are q zeroes, and then a one. There
                // should not be more than max_q consecutive zeroes.
                let mut q: Sample = Int::zero();
                while try!(self.input.read_leq_u8(1)) == 0 {
                    if q == max_q { return Err(FlacError::InvalidRiceCode); }
                    q = q + Int::one();
                }

                // What follows is the remainder in `rice_param` bits. Because
                // rice_param is at most 14, this fits in an u16. TODO: for
                // the RICE2 partition it will not fit.
                let r_u16 = try!(self.input.read_leq_u16(rice_param));
                let r: Sample = num::cast(r_u16).unwrap();

                *sample = rice_to_signed((q << rice_param as usize) | r);
            }
        }

        Ok(())
    }

    fn decode_partitioned_rice2(&mut self, block_size: u16,
                                buffer: &mut [Sample]) -> FlacResult<()> {
        println!("  decoding partitioned Rice 2"); // TODO: Remove this.
        panic!("partitioned_rice2 is not yet implemented"); // TODO
        Ok(())
    }

    fn decode_constant(&mut self, buffer: &mut [Sample]) -> FlacResult<()> {
        // A constant block has <bits per sample> bits: the value of all
        // samples. The unwrap is safe, because it has been verified before
        // that the `Sample` type is wide enough for the bits per sample.
        let sample_u32 = try!(self.input.read_leq_u32(self.bits_per_sample));
        let sample = num::cast(sample_u32).unwrap();

        for s in buffer.iter_mut() {
            *s = sample;
        }

        Ok(())
    }

    fn decode_verbatim(&mut self, buffer: &mut [Sample]) -> FlacResult<()> {
        // A verbatim block stores samples without encoding whatsoever.
        for s in buffer.iter_mut() {
            // The unwrap is safe, because it has been verified before that the
            // `Sample` type is wide enough for the bits per sample.
            let sample_u32 = try!(self.input.read_leq_u32(self.bits_per_sample));
            *s = num::cast(sample_u32).unwrap();
        }

        Ok(())
    }

    fn decode_fixed(&mut self, order: u8, buffer: &mut [Sample])
                    -> FlacResult<()> {
        println!("begin decoding fixed subframe"); // TODO: Remove this.
        // There are order * bits per sample unencoded warm-up sample bits.
        try!(self.decode_verbatim(buffer.slice_to_mut(order as usize)));

        println!("the warm-up samples are {:?}", buffer[0 .. order as usize].iter()
                 .map(|x| show_sample(*x))
                 .collect::<Vec<i16>>()); // TODO: Remove this.

        // Next up is the residual. We decode into the buffer directly, the
        // predictor contributions will be added in a second pass. The first
        // `order` samples have been decoded already, so continue after that.
        try!(self.decode_residual(buffer.len() as u16,
                                  buffer.slice_from_mut(order as usize)));

        // TODO: do prediction.

        Ok(())
    }

    fn decode_lpc(&mut self, order: u8, buffer: &mut [Sample])
                  -> FlacResult<()> {
        println!("begin decoding of LPC subframe"); // TODO: Remove this.
        // There are order * bits per sample unencoded warm-up sample bits.
        try!(self.decode_verbatim(buffer.slice_to_mut(order as usize)));

        println!("the warm-up samples are {:?}", buffer[0 .. order as usize].iter()
                 .map(|x| show_sample(*x))
                 .collect::<Vec<i16>>()); // TODO: Remove this.

        // Next are four bits quantised linear predictor coefficient precision - 1.
        let qlp_precision = try!(self.input.read_leq_u8(4)) + 1;

        // The bit pattern 1111 is invalid.
        if qlp_precision - 1 == 0b1111 {
            return Err(FlacError::InvalidSubframe);
        }

        // Next are five bits quantized linear predictor coefficient shift,
        // in signed two's complement. Read 5 bits and then extend the sign bit.
        let qlp_shift_unsig = try!(self.input.read_leq_u16(5));
        let qlp_shift = extend_sign(qlp_shift_unsig, 5);

        println!("  lpc: qlp_precision: {}, qlp_shift: {}, order: {}",
                 qlp_precision, qlp_shift, order); // TODO: Remove this.

        // Finally, the coefficients themselves.
        // TODO: get rid of the allocation by pre-allocating a vector in the decoder.
        let mut coefficients = Vec::new();
        for _ in range(0, order) {
            // We can safely read into an u16, qlp_precision is at most 15.
            let coef_unsig = try!(self.input.read_leq_u16(qlp_precision));
            let coef = extend_sign(coef_unsig, qlp_precision);
            coefficients.push(coef);
            println!("  > coef: {}", coef); // TODO: Remove this.
        }

        // Next up is the residual. We decode it into the buffer directly, the
        // predictor contributions will be added in a second pass. The first
        // `order` samples have been decoded already, so continue after that.
        try!(self.decode_residual(buffer.len() as u16,
                                  buffer.slice_from_mut(order as usize)));

        println!("  > first residual: {}, last residual: {}",
                 show_sample(buffer[order as usize]),
                 show_sample(buffer[buffer.len() - 1])); // TODO: Remove this.

        // TODO: do prediction.

        Ok(())
    }
}
