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
        .max_open(1024) // Prefer more file descriptors over allocating memory.
        .into_iter()
        .filter_map(|e| e.ok())
        .take(1024);

    for entry in wd {
        let path = entry.path();
        if path.is_file() && path.extension() == Some(OsStr::new("flac")) {
            let epoch = PreciseTime::now();
            let mut bytes = 0;

            // Read the file multiple times to amortize the walkdir cost.
            for _ in 0..10 {
                let mut reader = claxon::MetadataReader::open(path).unwrap();
                let vc = reader.next_vorbis_comment().unwrap();

                // Note that these are not optimized away even though the results
                // are not used, because the expectation may fail.
                vc.get_tag("date").next().expect("date");
                vc.get_tag("originaldate").next().expect("originaldate");
                vc.get_tag("tracknumber").next().expect("tracknumber");
                vc.get_tag("tracktotal").next().expect("tracktotal");
                vc.get_tag("discnumber").next().expect("discnumber");
                vc.get_tag("disctotal").next().expect("disctotal");

                vc.get_tag("title").next().expect("title");
                vc.get_tag("album").next().expect("album");
                vc.get_tag("artist").next().expect("artist");
                vc.get_tag("albumartist").next().expect("albumartist");
                vc.get_tag("artistsort").next().expect("artistsort");
                vc.get_tag("albumartistsort").next().expect("albumartistsort");

                vc.get_tag("musicbrainz_trackid").next().expect("musicbrainz_trackid");
                vc.get_tag("musicbrainz_albumid").next().expect("musicbrainz_albumid");
                vc.get_tag("musicbrainz_artistid").next().expect("musicbrainz_artistid");
                vc.get_tag("musicbrainz_albumartistid").next().expect("musicbrainz_albumartistid");

                bytes += reader.into_inner().seek(SeekFrom::Current(0)).unwrap();
            }

            let duration_ns = epoch.to(PreciseTime::now()).num_nanoseconds().unwrap();
            file_times_us.push(duration_ns as f64 / 1000.0 / 10.0);
            bytes_per_sec.push(bytes as f64 / (duration_ns as f64 / 1.0e9) / 10.0);
        }
    }

    file_times_us.sort_by(|x, y| x.partial_cmp(y).unwrap());
    bytes_per_sec.sort_by(|x, y| x.partial_cmp(y).unwrap());

    let p10 = file_times_us[10 * file_times_us.len() / 100];
    let p50 = file_times_us[50 * file_times_us.len() / 100];
    let p90 = file_times_us[90 * file_times_us.len() / 100];
    let mean = file_times_us.iter().sum::<f64>() / (file_times_us.len() as f64);
    let p10_mibs = bytes_per_sec[10 * bytes_per_sec.len() / 100] / (1024.0 * 1024.0);

    // Output numbers compatible with tools/compare_benches.r.
    println!("{:>6.2} {:>6.2} {:>6.2} {:>6.2} {:>6.2}", p10, p50, p90, mean, p10_mibs);
}
