// Snow -- A FLAC decoding library in Rust
// Copyright (C) 2014  Ruud van Asseldonk
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

use std::num::{NumCast, UnsignedInt};
use bitstream::Bitstream;
use error::{FlacError, FlacResult};

#[deriving(Copy, Show)] // TODO: this should not implement Show.
enum SubframeType {
    Constant,
    Verbatim,
    Fixed(u8),
    Lpc(u8)
}

#[deriving(Copy)]
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
    let sign_bit = val >> (bits as uint - 1);

    // Extend the sign bit into the remaining bits.
    let sign_extension = range(bits as uint, 16)
                         .fold(0, |s, i| s | (sign_bit << i));

    // Note: overflow in the cast is intended.
    (val | sign_extension) as i16
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
        debug_assert!(bits_per_sample as uint <= size_of::<Sample>() * 8);

        SubframeDecoder { bits_per_sample: bits_per_sample, input: input }
    }

    /// Decodes the subframe into the provided block-size buffer.
    ///
    /// It is assumed that the length of the buffer is the block size.
    pub fn decode(&mut self, buffer: &mut [Sample]) -> FlacResult<()> {
        // First up is the subframe header.
        let header = try!(read_subframe_header(self.input));

        // Then decode the subframe, properly per type.
        println!("encountered subframe of type {}", header.sf_type);
        match header.sf_type {
            SubframeType::Constant => try!(self.decode_constant(buffer)),
            SubframeType::Verbatim => try!(self.decode_verbatim(buffer)),
            SubframeType::Lpc(ord) => try!(self.decode_lpc(ord, buffer)),
            _ => { } // TODO: implement other decoders
        }

        // Finally, everything must be shifted by 'wasted bits per sample' to
        // the left.
        if header.wasted_bits_per_sample > 0 {
            for s in buffer.iter_mut() {
                *s = *s << header.wasted_bits_per_sample as uint;
            }
        }

        Ok(())
    }

    fn decode_constant(&mut self, buffer: &mut [Sample]) -> FlacResult<()> {
        // A constant block has <bits per sample> bits: the value of all
        // samples. The unwrap is safe, because it has been verified before
        // that the `Sample` type is wide enough for the bits per sample.
        let sample_u32 = try!(self.input.read_leq_u32(self.bits_per_sample));
        let sample = NumCast::from(sample_u32).unwrap();

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
            *s = NumCast::from(sample_u32).unwrap();
        }

        Ok(())
    }

    fn decode_lpc(&mut self, order: u8, buffer: &mut [Sample])
                  -> FlacResult<()> {
        // There are order * bits per sample unencoded warm-up samples.
        for i in range(0, order as uint) {
            // The unwrap is safe, because it has been verified before that the
            // `Sample` type is wide enough for the bits per sample.
            let sample_u32 = try!(self.input.read_leq_u32(self.bits_per_sample));
            buffer[i] = NumCast::from(sample_u32).unwrap();
        }

        // Next are four bits quantised linear predictor coefficient precision.
        let qlp_precision = try!(self.input.read_leq_u8(4));

        // The bit pattern 1111 is invalid.
        if qlp_precision == 0b1111 {
            return Err(FlacError::InvalidSubframe);
        }

        // Next are five bits quantized linear predictor coefficient shift,
        // in signed two's complement. Read 5 bits and then extend the sign bit.
        let qlp_shift_unsig = try!(self.input.read_leq_u16(5));
        let qlp_shift = extend_sign(qlp_shift_unsig, 5) as uint;

        // Finally, the coefficients themselves.
        // TODO: get rid of the allocation by pre-allocating a vector in the decoder.
        let mut coefficients = Vec::new();
        for _ in range(0, order) {
            let coef_unsig = try!(self.input.read_leq_u16(qlp_precision));
            let coef = extend_sign(coef_unsig, qlp_precision);
            coefficients.push(coef);
        }

        // TODO: decode residual.
        Ok(())
    }
}
