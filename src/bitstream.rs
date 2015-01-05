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

    /// Skips bits such that the next read will be byte-aligned.
    pub fn align_to_byte(&mut self) {
        self.bits_left = 0;
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

        // If there are more than 8 bits left, we read too far.
        debug_assert!(self.bits_left < 8);

        // The resulting data is padded with zeroes in the least significant
        // bits, but we want to pad in the most significant bits, so shift.
        Ok(result >> (8 - bits) as uint)
    }

    /// Reads at most 16 bits.
    pub fn read_leq_u16(&mut self, bits: u8) -> IoResult<u16> {
        // As with read_leq_u8, this only makes sense if we read <= 16 bits.
        debug_assert!(bits <= 16);

        // Note: the following is not the most efficient implementation
        // possible, but it avoids duplicating the complexity of `read_leq_u8`.

        if bits <= 8 {
            let result = try!(self.read_leq_u8(bits));
            Ok(result as u16)
        } else {
            // First read the 8 most significant bits, then read what is left.
            let msb = try!(self.read_leq_u8(8)) as u16;
            let lsb = try!(self.read_leq_u8(bits - 8)) as u16;
            Ok((msb << (bits - 8) as uint) | lsb)
        }
    }

    /// Reads at most 32 bits.
    pub fn read_leq_u32(&mut self, bits: u8) -> IoResult<u32> {
        // As with read_leq_u8, this only makes sense if we read <= 32 bits.
        debug_assert!(bits <= 32);

        // Note: the following is not the most efficient implementation
        // possible, but it avoids duplicating the complexity of `read_leq_u8`.

        if bits <= 16 {
            let result = try!(self.read_leq_u16(bits));
            Ok(result as u32)
        } else {
            // First read the 16 most significant bits, then read what is left.
            let msb = try!(self.read_leq_u16(16)) as u32;
            let lsb = try!(self.read_leq_u16(bits - 16)) as u32;
            Ok((msb << (bits - 16) as uint) | lsb)
        }
    }
}

#[test]
fn verify_read_leq_u8() {
    use std::io::MemReader;

    let mut data = MemReader::new(vec!(0b1010_0101, 0b1110_0001,
                                       0b1101_0010, 0b0101_0101,
                                       0b0111_0011, 0b0011_1111,
                                       0b1010_1010, 0b0000_1100));
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
    assert_eq!(bits.read_leq_u8(6).unwrap(), 0b111111);
    assert_eq!(bits.read_leq_u8(8).unwrap(), 0b10101010);
    assert_eq!(bits.read_leq_u8(4).unwrap(), 0b0000);
    assert_eq!(bits.read_leq_u8(1).unwrap(), 1);
    assert_eq!(bits.read_leq_u8(1).unwrap(), 1);
    assert_eq!(bits.read_leq_u8(2).unwrap(), 0b00);
}

#[test]
fn verify_read_leq_u16() {
    use std::io::MemReader;

    let mut data = MemReader::new(vec!(0b1010_0101, 0b1110_0001,
                                       0b1101_0010, 0b0101_0101));
    let mut bits = Bitstream::new(&mut data);

    assert_eq!(bits.read_leq_u16(0).unwrap(), 0);
    assert_eq!(bits.read_leq_u16(1).unwrap(), 1);
    assert_eq!(bits.read_leq_u16(13).unwrap(), 0b010_0101_1110_00);
    assert_eq!(bits.read_leq_u16(9).unwrap(), 0b01_1101_001);
}

#[test]
fn verify_read_leq_u32() {
    use std::io::MemReader;

    let mut data = MemReader::new(vec!(0b1010_0101, 0b1110_0001,
                                       0b1101_0010, 0b0101_0101));
    let mut bits = Bitstream::new(&mut data);

    assert_eq!(bits.read_leq_u32(1).unwrap(), 1);
    assert_eq!(bits.read_leq_u32(17).unwrap(), 0b010_0101_1110_0001_11);
    assert_eq!(bits.read_leq_u32(14).unwrap(), 0b01_0010_0101_0101);
}

#[test]
fn verify_align() {
    use std::io::MemReader;

    let mut data = MemReader::new(vec!(0x00, 0xff));
    let mut bits = Bitstream::new(&mut data);

    assert_eq!(bits.read_leq_u8(5).unwrap(), 0);
    bits.align_to_byte();
    assert_eq!(bits.read_leq_u8(3).unwrap(), 7);
}
