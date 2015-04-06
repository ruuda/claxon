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

extern crate claxon;

fn main() {
    use std::env;
    use std::fs;
    use std::io;
    use std::path;
    use claxon::FlacStream;
    let input = fs::File::open(path::Path::new(&env::args().nth(1).unwrap())).unwrap();
    let mut reader = io::BufReader::new(input);
    let mut stream = FlacStream::new(&mut reader).unwrap();
    let n_samples = stream.streaminfo().n_samples.unwrap();
    let mut blocks = stream.blocks::<i16>();
    let mut sample = 0u64;
    let mut i = 0u64;
    while sample < n_samples {
        let block = blocks.read_next().unwrap();
        let left = block.channel(0);
        let right = block.channel(1);
        println!("block {} decoded\nleft: {:?} .. {:?}\nright: {:?} .. {:?}",
                 i, &left[..12], &left[block.len() as usize - 12 ..],
                 &right[..12], &right[block.len() as usize - 12 ..]);
        sample = sample + block.len() as u64;
        i = i + 1;
    }
}
