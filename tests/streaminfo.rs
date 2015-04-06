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

#![feature(path_ext)]

extern crate claxon;

use std::fs;
use std::io;
use std::path;

fn run_metaflac(fname: &path::Path) -> String {
    use std::process::Command;

    // Run metaflac on the specified file and print all streaminfo data.
    let output = Command::new("metaflac")
                         .arg("--show-min-blocksize")
                         .arg("--show-max-blocksize")
                         .arg("--show-min-framesize")
                         .arg("--show-max-framesize")
                         .arg("--show-sample-rate")
                         .arg("--show-channels")
                         .arg("--show-bps")
                         .arg("--show-total-samples")
                         .arg("--show-md5sum")
                         .arg(fname.to_str().expect("unsupported filename"))
                         .output().ok().expect("failed to run metaflac");
    String::from_utf8(output.stdout).ok().expect("metaflac wrote invalid UTF-8")
}

fn print_hex(seq: &[u8]) -> String {
    let vec: Vec<String> = seq.iter().map(|x| format!("{:0>2x}", *x)).collect();
    vec.concat()
}

fn read_streaminfo(fname: &path::Path) -> String {
    use claxon::FlacStream;

    // Use a buffered reader, this speeds up the test by 120%.
    let file = fs::File::open(fname).unwrap();
    let mut reader = io::BufReader::new(file);
    let stream = FlacStream::new(&mut reader).unwrap();
    let streaminfo = stream.streaminfo();

    // Format the streaminfo in the same way that metaflac prints it.
    format!("{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n",
            streaminfo.min_block_size,
            streaminfo.max_block_size,
            streaminfo.min_frame_size.unwrap_or(0),
            streaminfo.max_frame_size.unwrap_or(0),
            streaminfo.sample_rate,
            streaminfo.n_channels,
            streaminfo.bits_per_sample,
            streaminfo.n_samples.unwrap_or(0),
            print_hex(&streaminfo.md5sum)) // TODO implement LowerHex for &[u8] and submit a PR.
}

fn compare_metaflac(fname: &path::Path) {
    let metaflac = run_metaflac(fname);
    let streaminfo = read_streaminfo(fname);
    let mut mf_lines = metaflac.lines_any();
    let mut si_lines = streaminfo.lines_any();
    while let (Some(mf), Some(si)) = (mf_lines.next(), si_lines.next()) {
        if mf != si {
            println!("metaflac\n--------\n{}", metaflac);
            println!("streaminfo\n----------\n{}", streaminfo);
            panic!("metaflac disagrees on parsed streaminfo");
        }
    };
}

#[test]
fn verify_streaminfo() {
    use std::ffi::OsStr;
    use std::fs::PathExt;

    // Compare our streaminfo parsing with metaflac on all flac files in the
    // testsamples directory.
    let dir = fs::read_dir("testsamples").ok().expect("failed to enumerate flac files");
    for path in dir {
        let path = path.ok().expect("failed to obtain path info").path();
        if path.is_file() && path.extension() == Some(OsStr::new("flac")) {
            print!("    comparing {} ...", path.to_str().expect("unsupported filename"));
            compare_metaflac(&path);
            println!(" ok");
        }
    }
}
