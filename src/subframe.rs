use std::mem::size_of;
use std::num::{NumCast, UnsignedInt};
use bitstream::Bitstream;
use error::{FlacError, FlacResult};

#[deriving(Copy)]
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

pub struct SubframeDecoder<'r> {
    block_size: u16,
    bits_per_sample: u8,
    header: SubframeHeader,
    input: &'r mut Bitstream<'r>
}

impl<'r> SubframeDecoder<'r> {
    pub fn new(block_size: u16,
               bits_per_sample: u8,
               input: &'r mut Bitstream<'r>)
               -> FlacResult<SubframeDecoder<'r>> {
        let header = try!(read_subframe_header(input));
        let decoder = SubframeDecoder {
            block_size: block_size,
            bits_per_sample: bits_per_sample,
            header: header,
            input: input
        };
        Ok(decoder)
    }

    /// Decodes the subframe into the provided buffer.
    ///
    /// The buffer must have _exactly_ the right size (the block size of the
    /// frame). The `Sample` type must be wide enough to accomodate for all
    /// bits per sample of the frame.
    pub fn decode<Sample>(&mut self, buffer: &mut [Sample]) -> FlacResult<()>
                          where Sample: UnsignedInt {
        // The sample type should be wide enough to accomodate for all bits of
        // the stream, but this can be verified at a higher level than here.
        // Still, it is a good idea to make the assumption explicit.
        //
        // TODO: the compiler refuses to compile the next line at the moment of
        // writing. Maybe in the future it will?
        // debug_assert!(self.bits_per_sample <= size_of<Sample>() * 8);
 
        // We assume that the buffer has the right size; obviously it cannot be
        // to small if we are to decode the entire subframe. Requiring it to
        // have the exact block size also ensures that there cannot be any
        // confusion about what happens to excess samples: there are none.
        debug_assert_eq!(self.block_size as uint, buffer.len());

        match self.header.sf_type {
            SubframeType::Constant => self.decode_constant(buffer),
            SubframeType::Verbatim => self.decode_verbatim(buffer),
            _ => Ok(()) // TODO: implement other decoders
        }
    }

    fn decode_constant<Sample>(&mut self, buffer: &mut [Sample])
                               -> FlacResult<()>
                               where Sample: UnsignedInt {
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

    fn decode_verbatim<Sample>(&mut self, buffer: &mut [Sample])
                               -> FlacResult<()>
                               where Sample: UnsignedInt {
        // A verbatim block stores samples without encoding whatsoever.
        for s in buffer.iter_mut() {
            // The unwrap is safe, because it has been verified before that the
            // `Sample` type is wide enough for the bits per sample.
            let sample_u32 = try!(self.input.read_leq_u32(self.bits_per_sample));
            *s = NumCast::from(sample_u32).unwrap();
        }

        Ok(())
    }
}
