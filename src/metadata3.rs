// Claxon -- A FLAC decoding library in Rust
// Copyright 2021 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

//! The `metadata` module deals with metadata at the beginning of a FLAC stream.

use std::io;

use error::{Error, Result, fmt_err};
use input::ReadBytes;

/// The different kinds of metadata block defined by the FLAC format.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum BlockType {
    StreamInfo = 0,
    Padding = 1,
    Application = 2,
    SeekTable = 3,
    VorbisComment = 4,
    CueSheet = 5,
    Picture = 6,
}

#[derive(Clone, Copy)]
pub struct BlockHeader {
    /// Whether this is the last metadata block before the audio data.
    is_last: bool,
    /// The type of metadata block.
    block_type: BlockType,
    /// Length of the metadata block in bytes, excluding this header.
    length: u32,
}

#[inline]
pub fn read_block_header<R: io::Read>(input: &mut R) -> Result<BlockHeader> {
    let byte = input.read_u8()?;

    // The first bit specifies whether this is the last block, the next 7 bits
    // specify the type of the metadata block to follow.
    let is_last = (byte >> 7) == 1;
    let block_type_u8 = byte & 0b0111_1111;

    // The length field is 24 bits, or 3 bytes.
    let length = input.read_be_u24()?;

    let block_type = match block_type_u8 {
        0 => BlockType::StreamInfo,
        1 => BlockType::Padding,
        2 => BlockType::Application,
        3 => BlockType::SeekTable,
        4 => BlockType::VorbisComment,
        5 => BlockType::CueSheet,
        6 => BlockType::Picture,
        127 => {
            // This code is invalid to avoid confusion with a frame sync code.
            return fmt_err("invalid metadata block type");
        }
        _ => {
            // Any other block type is 'reserved' at the moment of writing.
            return fmt_err("invalid metadata block, encountered reserved block type");
        }
    };

    let header = BlockHeader {
        is_last,
        block_type,
        length,
    };
    Ok(header)
}
