// Claxon -- A FLAC decoding library in Rust
// Copyright (C) 2014-2015 Ruud van Asseldonk
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

#![feature(test)]

extern crate claxon;
extern crate test;

use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use test::Bencher;

fn bench_decode(path: &Path, bencher: &mut Bencher) {
    // Read the file into memory. We want to measure decode speed, not IO
    // overhead. (There is no stable form of mmap in Rust that I know of, so
    // we read manually.)
    let mut file = File::open(path).unwrap();
    let mut data = Vec::new();
    file.read_to_end(&mut data).unwrap();
    let mut cursor = Cursor::new(data);

    let mut stream = claxon::FlacStream::new(&mut cursor).unwrap();

    // Use the more space-efficient 16-bit integers if it is sufficient,
    // otherwise decode into 32-bit integers, which is always sufficient.
    // TODO: If the closure gets called more often than the number of blocks
    // in the file, the measurement is wrong.
    match stream.streaminfo().bits_per_sample {
        n if n <= 16 => {
            let mut blocks = stream.blocks::<i16>();
            bencher.iter(|| { test::black_box(blocks.read_next().unwrap()); });
        }
        _ => {
            let mut blocks = stream.blocks::<i32>();
            bencher.iter(|| { test::black_box(blocks.read_next().unwrap()); });
        }
    }
}

#[bench]
fn bench_p0(bencher: &mut Bencher) {
    bench_decode(Path::new("testsamples/p0.flac"), bencher);
}

#[bench]
fn bench_p1(bencher: &mut Bencher) {
    bench_decode(Path::new("testsamples/p1.flac"), bencher);
}

#[bench]
fn bench_p2(bencher: &mut Bencher) {
    bench_decode(Path::new("testsamples/p2.flac"), bencher);
}

#[bench]
fn bench_p3(bencher: &mut Bencher) {
    bench_decode(Path::new("testsamples/p3.flac"), bencher);
}

#[bench]
fn bench_p4(bencher: &mut Bencher) {
    bench_decode(Path::new("testsamples/p4.flac"), bencher);
}
