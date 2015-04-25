// Claxon -- A FLAC decoding library in Rust
// Copyright (C) 2015 Ruud van Asseldonk
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

use std::cmp::Eq;
use std::ops::{Add, BitAnd, BitOr, Neg, Shl, Shr, Sub};

/// An trait that allows for interegers to be generic in width.
pub trait Sample: Copy + Clone + Eq +
    Neg<Output = Self> +
    Add<Output = Self> +
    Sub<Output = Self> +
    Shl<usize, Output = Self> +
    Shr<usize, Output = Self> +
    BitOr<Self, Output = Self> +
    BitAnd<Self, Output = Self> {

    type Unsigned: BitAnd<<Self as Sample>::Unsigned,
                          Output = <Self as Sample>::Unsigned>
                 + BitOr<Output = <Self as Sample>::Unsigned>
                 + Shl<usize, Output = <Self as Sample>::Unsigned>
                 + Shr<usize, Output = <Self as Sample>::Unsigned>
                 + Add<Output = <Self as Sample>::Unsigned>
                 + Eq + Copy + Clone;

    /// Returns the maximal value that the type can contain.
    // TODO: is this actually required, can we do without in non-debug versions?
    fn max() -> Self;

    /// Returns the minimal value that the type can contain.
    // TODO: is this actually required, can we do without in non-debug versions?
    fn min() -> Self;

    /// Returns the maximal value that the `Unsigned` type can contain.
    // TODO: is this actually required, can we do without in non-debug versions?
    fn max_unsigned() -> <Self as Sample>::Unsigned;

    /// Returns 0.
    // TODO: could be an associated constant once those land.
    fn zero() -> Self {
        Self::from_i8(0).unwrap()
    }

    /// Returns 1.
    // TODO: could be an associated constant once those land.
    fn one() -> Self {
        Self::from_i8(1).unwrap()
    }

    /// Returns 0 as the unsigned type.
    // TODO: could be an associated constant once those land.
    fn zero_unsigned() -> <Self as Sample>::Unsigned {
        <<Self as Sample>::Unsigned as FromPrimitive>::from_i8(0).unwrap()
    }

    /// Returns 1 as the unsigned type.
    // TODO: could be an associated constant once those land.
    fn one_unsigned() -> <Self as Sample>::Unsigned {
        <<Self as Sample>::Unsigned as FromPrimitive>::from_i8(1).unwrap()
    }

    /// Interprets the unsigned value as a signed number.
    fn from_unsigned(unsigned: <Self as Sample>::Unsigned) -> Self;

    /// Converts an `i32` to the sample, assuming it will not overflow.
    fn from_i32_nofail(x: i32) -> Self;

    /// Converts an `i32` to the sample, returning `None` on overflow.
    fn from_i32(x: i32) -> Option<Self>;

    /// Adds with wraparound on overflow.
    fn wrapping_add(self, other: Self) -> Self;

    /// Subtracts with wraparound on overflow.
    fn wrapping_sub(self, other: Self) -> Self;
}

impl Sample for i8 {

    type Unsigned = u8;

    fn max() -> i8 {
        use std::i8;
        i8::MAX
    }

    fn min() -> i8 {
        use std::i8;
        i8::MIN
    }

    fn max_unsigned() -> u8 {
        use std::u8;
        u8::MAX
    }

    fn from_unsigned(unsigned: u8) -> i8 {
        unsigned as i8
    }

    fn from_i32_nofail(x: i32) -> i8 {
        x as i8
    }

    fn from_i32(x: i32) -> Option<i8> {
        use std::i8;
        if x > i8::MAX || x < i8::MIN {
            None
        } else {
            x as i8
        }
    }

    fn wrapping_add(self, other: i8) -> i8 {
        self.wrapping_add(other)
    }

    fn wrapping_sub(self, other: i8) -> i8 {
        self.wrapping_sub(other)
    }
}

impl Sample for i16 {

    type Unsigned = u16;

    fn max() -> i16 {
        use std::i16;
        i16::MAX
    }

    fn min() -> i16 {
        use std::i16;
        i16::MIN
    }

    fn max_unsigned() -> u16 {
        use std::u16;
        u16::MAX
    }

    fn from_unsigned(unsigned: u16) -> i16 {
        unsigned as i16
    }

    fn from_i32_nofail(x: i32) -> i16 {
        x as i16
    }

    fn from_i32(x: i32) -> Option<i16> {
        use std::i16;
        if x > i16::MAX || x < i16::MIN {
            None
        } else {
            x as i16
        }
    }

    fn wrapping_add(self, other: i16) -> i16 {
        self.wrapping_add(other)
    }

    fn wrapping_sub(self, other: i16) -> i16 {
        self.wrapping_sub(other)
    }
}

impl Sample for i32 {

    type Unsigned = u32;

    fn max() -> i32 {
        use std::i32;
        i32::MAX
    }

    fn min() -> i32 {
        use std::i32;
        i32::MIN
    }

    fn max_unsigned() -> u32 {
        use std::u32;
        u32::MAX
    }

    fn from_unsigned(unsigned: u32) -> i32 {
        unsigned as i32
    }

    fn from_i32_nofail(x: i32) -> i32 {
        x
    }

    fn from_i32(x: i32) -> Option<i32> {
        use std::i32;
        if x > i32::MAX || x < i32::MIN {
            None
        } else {
            x as i32
        }
    }

    fn wrapping_add(self, other: i32) -> i32 {
        self.wrapping_add(other)
    }

    fn wrapping_sub(self, other: i32) -> i32 {
        self.wrapping_sub(other)
    }
}
