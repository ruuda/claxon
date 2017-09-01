// Claxon -- A FLAC decoding library in Rust
// Copyright 2017 Ruud van Asseldonk
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

// This file contains a minimal example of using Claxon and Hound to decode a
// flac file. This can be done more efficiently, but it is also more verbose.
// See the `decode` example for that.

extern crate claxon;
extern crate walkdir;
extern crate time;

use time::PreciseTime;
use std::io::{Seek, SeekFrom};

fn main() {
    use std::ffi::OsStr;
    let mut file_times_us = Vec::new();
    let mut bytes_per_sec = Vec::new();

    let wd = walkdir::WalkDir::new("testsamples/extra")
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .take(1024);

    for entry in wd {
        let path = entry.path();
        if path.is_file() && path.extension() == Some(OsStr::new("flac")) {
            let epoch = PreciseTime::now();

            let reader = claxon::FlacReader::open(path).unwrap();

            // Note that these are not optimized away even though the results
            // are not used, because the expectation may fail.
            reader.get_tag("DATE").next().expect("DATE");
            reader.get_tag("ORIGINALDATE").next().expect("ORIGINALDATE");
            reader.get_tag("TITLE").next().expect("TITLE");
            reader.get_tag("ALBUM").next().expect("ALBUM");
            reader.get_tag("ALBUMARTIST").next().expect("ALBUMARTIST");
            reader.get_tag("MUSICBRAINZ_TRACKID").next().expect("MUSICBRAINZ_TRACKID");
            reader.get_tag("MUSICBRAINZ_ALBUMID").next().expect("MUSICBRAINZ_ALBUMID");
            reader.get_tag("MUSICBRAINZ_ARTISTID").next().expect("MUSICBRAINZ_ARTISTID");

            let bytes = reader.into_inner().seek(SeekFrom::Current(0)).unwrap();
            let duration_ns = epoch.to(PreciseTime::now()).num_nanoseconds().unwrap();
            file_times_us.push(duration_ns as f64 / 1000.0);
            bytes_per_sec.push(bytes as f64 / (duration_ns as f64 / 1.0e9));
        }
    }

    file_times_us.sort_by(|x, y| x.partial_cmp(y).unwrap());
    bytes_per_sec.sort_by(|x, y| x.partial_cmp(y).unwrap());

    let p10 = file_times_us[10 * file_times_us.len() / 100];
    let p50 = file_times_us[50 * file_times_us.len() / 100];
    let p90 = file_times_us[90 * file_times_us.len() / 100];
    let p10t = bytes_per_sec[10 * bytes_per_sec.len() / 100];

    println!("{:>6.2} {:>6.2} {:>6.2} {:>6.2}", p10, p50, p90, p10t / 1024.0 / 1024.0);
}
