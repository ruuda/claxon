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
    fn zero() -> Self;

    /// Returns 1.
    // TODO: could be an associated constant once those land.
    fn one() -> Self;

    /// Returns 0 as the unsigned type.
    // TODO: could be an associated constant once those land.
    fn zero_unsigned() -> <Self as Sample>::Unsigned;

    /// Returns 1 as the unsigned type.
    // TODO: could be an associated constant once those land.
    fn one_unsigned() -> <Self as Sample>::Unsigned;

    /// Interprets the unsigned value as a signed number.
    fn from_unsigned(unsigned: <Self as Sample>::Unsigned) -> Self;

    /// Converts an `u16` to the unsigned sample, assuming it will not overflow.
    fn from_u16_nofail(x: u16) -> <Self as Sample>::Unsigned;

    /// Converts an `i32` to the sample, assuming it will not overflow.
    fn from_i32_nofail(x: i32) -> Self;

    /// Converts an `i32` to the sample, returning `None` on overflow.
    fn from_i32(x: i32) -> Option<Self>;

    /// Converts an `i64` to the sample, returning `None` on overflow.
    fn from_i64(x: i64) -> Option<Self>;

    /// Adds with wraparound on overflow.
    fn wrapping_add(self, other: Self) -> Self;

    /// Subtracts with wraparound on overflow.
    fn wrapping_sub(self, other: Self) -> Self;
}

macro_rules! impl_sample {
    ($signed: ident, $unsigned: ident) => {
        impl Sample for $signed {

            type Unsigned = $unsigned;

            fn max() -> $signed {
                use std::$signed;
                $signed::MAX
            }

            fn min() -> $signed {
                use std::$signed;
                $signed::MIN
            }

            fn max_unsigned() -> $unsigned {
                use std::$unsigned;
                $unsigned::MAX
            }

            fn zero() -> $signed {
                0
            }

            fn one() -> $signed {
                1
            }

            fn zero_unsigned() -> $unsigned {
                0
            }

            fn one_unsigned() -> $unsigned {
                1
            }

            fn from_unsigned(unsigned: $unsigned) -> $signed {
                unsigned as $signed
            }

            fn from_u16_nofail(x: u16) -> $unsigned {
                x as $unsigned
            }

            fn from_i32_nofail(x: i32) -> $signed {
                x as $signed
            }

            fn from_i32(x: i32) -> Option<$signed> {
                use std::$signed;
                if x > $signed::MAX || x < $signed::MIN {
                    None
                } else {
                    x as $signed
                }
            }

            fn from_i64(x: i64) -> Option<$signed> {
                use std::$signed;
                if x > $signed::MAX || x < $signed::MIN {
                    None
                } else {
                    x as $signed
                }
            }

            fn wrapping_add(self, other: $signed) -> $signed {
                self.wrapping_add(other)
            }

            fn wrapping_sub(self, other: $signed) -> $signed {
                self.wrapping_sub(other)
            }
        }
    };
}

impl_sample!(i8, u8);
impl_sample!(i16, u16);
impl_sample!(i32, u32);
