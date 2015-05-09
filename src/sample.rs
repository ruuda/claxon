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

//! The `sample` module provides the `Sample` trait and its implementations.
//!
//! The purpose of this module is similar to that of `num::traits` in the `num`
//! crate, but the `Sample` type has been specialised more towards FLAC in
//! particular. For instance, it is only implemented for types that can be
//! encountered in a FLAC stream. (This excludes `i64` and unsigned integers.)

use std::cmp::Eq;
use std::fmt;
use std::ops::{Add, BitAnd, BitOr, Mul, Neg, Shl, Shr, Sub};
use std::num::Zero;

/// A trait that allows decoding into integers of various widths.
///
/// A few observations are important here:
///
/// - In the FLAC format, samples are always signed.
/// - FLAC does not support more than 32 bits per sample.
///   Therefore, converting a sample to `i32` or `i64` can never fail.
///
/// This trait should only be implemented for `i8`, `i16` and `i32`.
pub trait Sample: Copy + Clone + Eq + fmt::Debug +
    Neg<Output = Self> +
    Add<Output = Self> +
    Sub<Output = Self> +
    Shl<usize, Output = Self> +
    Shr<usize, Output = Self> +
    BitOr<Self, Output = Self> +
    BitAnd<Self, Output = Self> {

    /// The signed integer type that is wide enough to store differences.
    ///
    /// The difference between two values of the sample type might not fit in
    /// a sample type any more, so a wider integer type is required. The wide
    /// type must be able to store at least twice a difference, so it must be
    /// two bits wider than the sample type itself.
    type Wide: WideSample;

    /// The zero sample.
    // TODO: associated constants, once those land.
    fn zero() -> Self;

    /// Tries to narrow the sample, returning `None` on overflow.
    fn from_wide(wide: Self::Wide) -> Option<Self>;

    /// Casts the sample to its wide type.
    fn widen(self) -> Self::Wide;
}

pub trait WideSample: Copy + Clone + Eq + fmt::Debug +
    Zero +
    Neg<Output = Self> +
    Add<Output = Self> +
    Sub<Output = Self> +
    Mul<Output = Self> +
    Shl<usize, Output = Self> +
    Shr<usize, Output = Self> +
    BitOr<Self, Output = Self> +
    BitAnd<Self, Output = Self> {

    /// The one sample.
    fn one() -> Self;

    /// The maximum value of the wide sample type.
    fn max() -> Self;

    /// The number of bits that this type can store at most.
    fn width() -> u8;

    /// Casts a `i8` to the wide sample type.
    fn from_i8(from: i8) -> Self;

    /// Casts a `u16` to the wide sample type.
    fn from_u16(from: u16) -> Self;

    /// Casts a `i32` to the wide sample type, assuming it will not overflow.
    fn from_i32_nofail(from: i32) -> Self;

    /// Tries to cast an `i64` to the wide sample type.
    ///
    /// This will return `None` if `2 * from` would overflow the wide sample type.
    fn from_i64_spare_bit(from: i64) -> Option<Self>;

    /// Casts the sample to an `i64`.
    fn to_i64(self) -> i64;
}

macro_rules! impl_sample {
    ($narrow: ident, $wide: ident, $width: expr) => {
        impl Sample for $narrow {
            type Wide = $wide;

            fn zero() -> $narrow {
                0
            }

            fn from_wide(wide: $wide) -> Option<$narrow> {
                use std::$narrow;
                if wide < $narrow::MIN as $wide { return None; }
                if wide > $narrow::MAX as $wide { return None; }
                Some(wide as $narrow)
            }

            fn widen(self) -> $wide {
                self as $wide
            }
        }

        impl WideSample for $wide {

            fn one() -> $wide {
                1
            }

            fn max() -> $wide {
                use std::$wide;
                $wide::MAX
            }

            fn width() -> u8 {
                $width * 2
            }

            fn from_i8(from: i8) -> $wide {
                from as $wide
            }

            fn from_u16(from: u16) -> $wide {
                from as $wide
            }

            fn from_i32_nofail(from: i32) -> $wide {
                from as $wide
            }

            fn from_i64_spare_bit(from: i64) -> Option<$wide> {
                use std::$wide;
                if from < ($wide::MIN as i64) >> 1 { return None; }
                if from > ($wide::MAX as i64) >> 1 { return None; }
                Some(from as $wide)
            }

            fn to_i64(self) -> i64 {
                self as i64
            }
        }
    };
}

impl_sample!(i8, i16, 8);
impl_sample!(i16, i32, 16);
impl_sample!(i32, i64, 32);
