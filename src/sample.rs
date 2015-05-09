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
use std::ops::{Add, BitAnd, BitOr, Neg, Shl, Shr, Sub};

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
    /// a sample type any more, so a wider integer type is required.
    type Wide: WideSample;

    /// The zero sample.
    fn zero() -> Self;

    /// Casts the sample to its wide type.
    fn widen(self) -> Sample::Wide;
}

pub trait WideSample: Copy + Clone + Eq + fmt::Debug +
    Add<Output = Self> +
    Sub<Output = Self> +
    Shl<usize, Output = Self> +
    Shr<usize, Output = Self> +
    BitOr<Self, Output = Self> +
    BitAnd<Self, Output = Self> {

    /// The signed integer type that this is the wide version of.
    type Narrow: Sample;

    /// The zero sample.
    fn zero() -> Self;

    /// Tries to cast the sample to its narrow type, returning `None` on overflow.
    fn narrow(self) -> Option<Sample::Narrow>;
}

macro_rules! impl_sample {
    ($narrow: ident, $wide: ident) => {
        impl Sample for $narrow {
            type Wide = $wide;

            fn zero() -> $narrow {
                0
            }

            fn widen(self) -> $wide {
                self as $wide
            }
        }

        impl WideSample for $wide {
            type Narrow = $narrow;

            fn zero() -> $wide {
                0
            }

            fn narrow(self) -> Option<$narrow> {
                use std::$narrow;
                if self < $narrow::MIN as $wide { return None; }
                if self > $narrow::MAX as $wide { return None; }
                Ok(self as $narrow)
            }
        }
    };
}

impl_sample!(i8, i16);
impl_sample!(i16, i32);
impl_sample!(i32, i64);
