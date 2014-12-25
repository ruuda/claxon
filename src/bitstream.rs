use std::io::IoResult;

/// Wraps a `Reader` to facilitate reading that is not byte-aligned.
pub struct Bitstream<'r> {
    /// The source where bits are read from.
    reader: &'r mut (Reader + 'r),
    /// Data read from the reader, but not yet fully consumed.
    data: u8,
    /// The number of bits of `data` that have not been consumed.
    bits_left: u8
}

impl<'r> Bitstream<'r> {
    /// Wraps the reader with a reader that facilitates reading individual bits.
    pub fn new(reader: &'r mut Reader) -> Bitstream<'r> {
        Bitstream { reader: reader, data: 0, bits_left: 0 }
    }

    /// Generates a bitmask with 1s in the `bits` most significant bits.
    fn mask_u8(bits: u8) -> u8 {
        debug_assert!(bits <= 8);
        0xffu8 << (8 - bits) as uint
    }

    /// Reads at most eight bits.
    pub fn read_leq_u8(&mut self, bits: u8) -> IoResult<u8> {
        // Of course we can read no more than 8 bits, but we do not want the
        // performance overhead of the assertion, so only do it in debug mode.
        debug_assert!(bits <= 8);

        // If not enough bits left, we will need to read the next byte.
        let result = if self.bits_left < bits {
            // Most significant bits are shifted to the right position already.
            let msb = self.data;

            // From the next byte, we take the additional bits that we need.
            // Those start at the most significant bit, so we need to shift so
            // that it does not overlap with what we have already.
            self.data = try!(self.reader.read_byte());
            let lsb = (self.data & Bitstream::mask_u8(bits - self.bits_left))
                    >> self.bits_left as uint;

            // Shift out the bits that we have consumed.
            self.data = self.data << (bits - self.bits_left) as uint;
            self.bits_left = 8 - (bits - self.bits_left);

            msb | lsb
        } else {
            let result = self.data & Bitstream::mask_u8(bits);

            // Shift out the bits that we have consumed.
            self.data = self.data << bits as uint;
            self.bits_left = self.bits_left - bits;

            result
        };

        // The resulting data is padded with zeroes in the least significant
        // bits, but we want to pad in the most significant bits, so shift.
        Ok(result >> (8 - bits) as uint)
    }
}

#[test]
fn verify_bitstream() {
    use std::io::MemReader;

    let mut data = MemReader::new(vec!(0b1010_0101, 0b1110_0001,
                                       0b1101_0010, 0b0101_0101,
                                       0b0111_0011, 0b0011_1111));
    let mut bits = Bitstream::new(&mut data);

    assert_eq!(bits.read_leq_u8(0).unwrap(), 0);
    assert_eq!(bits.read_leq_u8(1).unwrap(), 1);
    assert_eq!(bits.read_leq_u8(1).unwrap(), 0);
    assert_eq!(bits.read_leq_u8(2).unwrap(), 0b10);
    assert_eq!(bits.read_leq_u8(2).unwrap(), 0b01);
    assert_eq!(bits.read_leq_u8(3).unwrap(), 0b011);
    assert_eq!(bits.read_leq_u8(3).unwrap(), 0b110);
    assert_eq!(bits.read_leq_u8(4).unwrap(), 0b0001);
    assert_eq!(bits.read_leq_u8(5).unwrap(), 0b11010);
    assert_eq!(bits.read_leq_u8(6).unwrap(), 0b010010);
    assert_eq!(bits.read_leq_u8(7).unwrap(), 0b1010101);
    assert_eq!(bits.read_leq_u8(8).unwrap(), 0b11001100);
}
