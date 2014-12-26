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
}
