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

use std::io;

pub trait ReadExt where Self : io::Read {
    /// Reads as many bytes as `buf` is long.
    ///
    /// This may issue multiple `read` calls internally. An error is returned
    /// if `read` read 0 bytes before the buffer is full.
    fn read_into(&mut self, buf: &mut [u8]) -> io::Result<()>;

    /// Reads a single byte.
    fn read_u8(&mut self) -> io::Result<u8>;

    /// Reads two bytes and interprets them as a big-endian 16-bit unsigned integer.
    fn read_be_u16(&mut self) -> io::Result<u16>;

    /// Reads three bytes and interprets them as a big-endian 24-bit unsigned integer.
    fn read_be_u24(&mut self) -> io::Result<u32>;

    /// Reads four bytes and interprets them as a big-endian 32-bit unsigned integer.
    fn read_be_u32(&mut self) -> io::Result<u32>;
}

impl<R> ReadExt for R where R: io::Read {
    fn read_into(&mut self, buf: &mut [u8]) -> io::Result<()> {
        let mut n = 0;
        while n < buf.len() {
            let progress = try!(self.read(&mut buf[n ..]));
            if progress > 0 {
                n += progress;
            } else {
                return Err(io::Error::new(io::ErrorKind::Other, "Failed to read enough bytes.", None));
            }
        }
        Ok(())
    }

    fn read_u8(&mut self) -> io::Result<u8> {
        // Read a single byte.
        let mut buf = [0u8; 1];
        if try!(self.read(&mut buf)) != 1 {
            Err(io::Error::new(io::ErrorKind::Other, "Failed to read byte.", None))
        } else {
            Ok(buf[0])
        }
    }

    fn read_be_u16(&mut self) -> io::Result<u16> {
        let mut buf = [0u8; 2];
        try!(self.read_into(&mut buf));
        Ok((buf[0] as u16) << 8 | (buf[1] as u16))
    }

    fn read_be_u24(&mut self) -> io::Result<u32> {
        let mut buf = [0u8; 3];
        try!(self.read_into(&mut buf));
        Ok((buf[0] as u32) << 16 | (buf[1] as u32) << 8 | (buf[2] as u32))
    }

    fn read_be_u32(&mut self) -> io::Result<u32> {
        let mut buf = [0u8; 4];
        try!(self.read_into(&mut buf));
        Ok((buf[0] as u32) << 24 | (buf[1] as u32) << 16 |
           (buf[2] as u32) << 8  | (buf[0] as u32) << 0)
    }
}

#[test]
fn verify_read_into() {
    let mut reader = io::Cursor::new(vec!(2u8, 3, 5, 7, 11, 13, 17, 19));
    let mut buf1 = [0u8; 3];
    let mut buf2 = [0u8, 8];
    reader.read_into(&mut buf1);
    reader.read_into(&mut buf2);
    assert_eq!(buf1, [2u8, 3, 5]);
    assert_eq!(buf2, [7u8, 11, 13, 17, 19]);
}

#[test]
fn verify_read_be_u16() {
    let mut reader = io::Cursor::new(vec!(0u8, 2, 129, 89, 122));
    assert_eq!(reader.read_be_u16(), 2);
    assert_eq!(reader.read_be_u16(), 331133);
    assert!(reader.read_be_u16().is_err());
}

#[test]
fn verify_read_be_u24() {
    let mut reader = io::Cursor::new(vec!(0u8, 0, 2, 0x8f, 0xff, 0xf3, 122));
    assert_eq!(reader.read_be_u24(), 2);
    assert_eq!(reader.read_be_u24(), 9_437_171);
    assert!(reader.read_be_u24().is_err());
}

#[test]
fn verify_read_be_u32() {
    let mut reader = io::Cursor::new(vec!(0u8, 0, 0, 2, 0x80, 0x01, 0xff, 0xe9, 0));
    assert_eq!(reader.read_be_u32(), 2);
    assert_eq!(reader.read_be_u32(), 2_147_614_697);
    assert!(reader.read_be_u32().is_err());
}

/// Wraps a `Reader` to facilitate reading that is not byte-aligned.
pub struct Bitstream<'r> {
    /// The source where bits are read from.
    reader: &'r mut (io::Read + 'r),
    /// Data read from the reader, but not yet fully consumed.
    data: u8,
    /// The number of bits of `data` that have not been consumed.
    bits_left: u8
}

impl<'r> Bitstream<'r> {
    /// Wraps the reader with a reader that facilitates reading individual bits.
    pub fn new(reader: &'r mut io::Read) -> Bitstream<'r> {
        Bitstream { reader: reader, data: 0, bits_left: 0 }
    }

    /// Generates a bitmask with 1s in the `bits` most significant bits.
    fn mask_u8(bits: u8) -> u8 {
        debug_assert!(bits <= 8);
        0xffu8 << (8 - bits) as usize
    }

    /// Skips bits such that the next read will be byte-aligned.
    pub fn align_to_byte(&mut self) {
        self.bits_left = 0;
    }

    // TODO: Remove this method.
    pub fn dump_some(&mut self, bytes: u8) -> io::Result<()> {
        use std::iter::repeat;
        println!(">=== begin bitstream dump ===>");
        println!("     data = {:x}, {} bits left", self.data, self.bits_left);
        print!("     more: [");
        let mut buf: Vec<u8> = repeat(0).take(bytes as usize - 1).collect();
        assert_eq!(try!(self.reader.read(&mut buf)), bytes as usize - 1);
        for i in 0 .. bytes as usize - 1 {
            print!("{:x}, ", buf[i]);
        }
        println!("{:x}]", buf[bytes as usize - 1]);
        println!("<=== end bitstream dump <===");
        Ok(())
    }

    /// Reads at most eight bits.
    pub fn read_leq_u8(&mut self, bits: u8) -> io::Result<u8> {
        // Of course we can read no more than 8 bits, but we do not want the
        // performance overhead of the assertion, so only do it in debug mode.
        debug_assert!(bits <= 8);

        // If not enough bits left, we will need to read the next byte.
        let result = if self.bits_left < bits {
            // Most significant bits are shifted to the right position already.
            let msb = self.data;

            // Read a single byte.
            self.data = try!(self.reader.read_u8());

            // From the next byte, we take the additional bits that we need.
            // Those start at the most significant bit, so we need to shift so
            // that it does not overlap with what we have already.
            let lsb = (self.data & Bitstream::mask_u8(bits - self.bits_left))
                    >> self.bits_left as usize;

            // Shift out the bits that we have consumed.
            self.data = self.data << (bits - self.bits_left) as usize;
            self.bits_left = 8 - (bits - self.bits_left);

            msb | lsb
        } else {
            let result = self.data & Bitstream::mask_u8(bits);

            // Shift out the bits that we have consumed.
            self.data = self.data << bits as usize;
            self.bits_left = self.bits_left - bits;

            result
        };

        // If there are more than 8 bits left, we read too far.
        debug_assert!(self.bits_left < 8);

        // The least significant bits should be zero.
        debug_assert_eq!(self.data & !Bitstream::mask_u8(self.bits_left), 0u8);

        // The resulting data is padded with zeroes in the least significant
        // bits, but we want to pad in the most significant bits, so shift.
        Ok(result >> (8 - bits) as usize)
    }

    /// Reads at most 16 bits.
    pub fn read_leq_u16(&mut self, bits: u8) -> io::Result<u16> {
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
            Ok((msb << (bits - 8) as usize) | lsb)
        }
    }

    /// Reads at most 32 bits.
    pub fn read_leq_u32(&mut self, bits: u8) -> io::Result<u32> {
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
            Ok((msb << (bits - 16) as usize) | lsb)
        }
    }
}

#[test]
fn verify_read_leq_u8() {
    use std::old_io::MemReader;

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
    use std::old_io::MemReader;

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
    use std::old_io::MemReader;

    let mut data = MemReader::new(vec!(0b1010_0101, 0b1110_0001,
                                       0b1101_0010, 0b0101_0101));
    let mut bits = Bitstream::new(&mut data);

    assert_eq!(bits.read_leq_u32(1).unwrap(), 1);
    assert_eq!(bits.read_leq_u32(17).unwrap(), 0b010_0101_1110_0001_11);
    assert_eq!(bits.read_leq_u32(14).unwrap(), 0b01_0010_0101_0101);
}

#[test]
fn verify_read_mixed() {
    use std::old_io::MemReader;

    // These test data are warm-up samples from an actual stream.
    let mut data = MemReader::new(vec!(0x03, 0xc7, 0xbf, 0xe5, 0x9b, 0x74,
                                       0x1e, 0x3a, 0xdd, 0x7d, 0xc5, 0x5e,
                                       0xf6, 0xbf, 0x78, 0x1b, 0xbd));
    let mut bits = Bitstream::new(&mut data);

    assert_eq!(bits.read_leq_u8(6).unwrap(), 0);
    assert_eq!(bits.read_leq_u8(1).unwrap(), 1);
    let minus = 1u32 << 16;
    assert_eq!(bits.read_leq_u32(17).unwrap(), minus | (-14401_i16 as u16 as u32));
    assert_eq!(bits.read_leq_u32(17).unwrap(), minus | (-13514_i16 as u16 as u32));
    assert_eq!(bits.read_leq_u32(17).unwrap(), minus | (-12168_i16 as u16 as u32));
    assert_eq!(bits.read_leq_u32(17).unwrap(), minus | (-10517_i16 as u16 as u32));
    assert_eq!(bits.read_leq_u32(17).unwrap(), minus | (-09131_i16 as u16 as u32));
    assert_eq!(bits.read_leq_u32(17).unwrap(), minus | (-08489_i16 as u16 as u32));
    assert_eq!(bits.read_leq_u32(17).unwrap(), minus | (-08698_i16 as u16 as u32));
}

#[test]
fn verify_align() {
    use std::old_io::MemReader;

    let mut data = MemReader::new(vec!(0x00, 0xff));
    let mut bits = Bitstream::new(&mut data);

    assert_eq!(bits.read_leq_u8(5).unwrap(), 0);
    bits.align_to_byte();
    assert_eq!(bits.read_leq_u8(3).unwrap(), 7);
}
