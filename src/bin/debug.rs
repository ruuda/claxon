// Claxon -- A FLAC decoding library in Rust
// Copyright (C) 2015 Ruud van Asseldonk
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

#![allow(unstable)]

extern crate claxon;

fn main() {
    use std::old_io::{File, BufferedReader};
    use std::os::args;
    use claxon::FlacStream;
    let input = File::open(&Path::new(&args()[1])).unwrap();
    let mut reader = BufferedReader::new(input);
    let mut stream = FlacStream::new(&mut reader).unwrap();
    let n_samples = stream.streaminfo().n_samples.unwrap();
    let mut blocks = stream.blocks::<u16>();
    let mut sample = 0u64;
    let mut i = 0u64;
    while sample < n_samples {
        let block = blocks.read_next().unwrap();
        let left = block.channel(0);
        let right = block.channel(1);
        println!("block {} decoded\nleft: {:?} .. {:?}\nright: {:?} .. {:?}",
                 i, left.slice_to(12), left.slice_from(block.len() as usize - 12),
                 right.slice_to(12), right.slice_from(block.len() as usize - 12));
        sample = sample + block.len() as u64;
        i = i + 1;
    }
}
