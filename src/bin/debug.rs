// Snow -- A FLAC decoding library in Rust
// Copyright (C) 2015  Ruud van Asseldonk
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

extern crate snow;

fn main() {
    use std::io::{File, BufferedReader};
    use std::os::args;
    use snow::FlacStream;
    let input = File::open(&Path::new(&args()[1])).unwrap();
    let mut reader = BufferedReader::new(input);
    let mut stream = FlacStream::new(&mut reader).unwrap();
    let mut blocks = stream.blocks::<u16>();
    let block = blocks.read_next().unwrap();
    let left = block.channel(0);
    let right = block.channel(1);
    println!("left: {}\nright: {}", left.slice_to(12), right.slice_to(12));
}
