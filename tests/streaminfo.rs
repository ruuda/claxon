extern crate snow;

fn run_metaflac(fname: &Path) -> String {
    use std::io::Command;

    // Run metaflac on the specified file and print all streaminfo data.
    let mut child = Command::new("metaflac")
                        .arg("--show-min-blocksize")
                        .arg("--show-max-blocksize")
                        .arg("--show-min-framesize")
                        .arg("--show-max-framesize")
                        .arg("--show-sample-rate")
                        .arg("--show-channels")
                        .arg("--show-bps")
                        .arg("--show-total-samples")
                        .arg("--show-md5sum")
                        .arg(fname.as_str().expect("unsupported filename"))
                        .spawn().ok().expect("failed to run metaflac");

    assert!(child.wait().unwrap().success());

    let output = match child.stdout {
        Some(ref mut stdout) => stdout.read_to_string().ok()
                                      .expect("failed to read metaflac stdout"),
        None => panic!("failed to open metaflac stdout")
    };

    output
}

fn print_hex(seq: &[u8]) -> String {
    let vec: Vec<String> = seq.iter().map(|x| format!("{:0>2x}", *x)).collect();
    vec.concat()
}

fn read_streaminfo(fname: &Path) -> String {
    use std::io::fs::File;
    use std::io::BufferedReader;
    use snow::FlacStream;

    // Use a buffered reader, this speeds up the test by 120%.
    let file = File::open(fname).unwrap();
    let mut reader = BufferedReader::new(file);
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

fn compare_metaflac(fname: &Path) {
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
    use std::io::fs::{readdir, PathExtensions};

    // Compare our streaminfo parsing with metaflac on all flac files in the
    // testsamples directory.
    let dir = readdir(&Path::new("testsamples")).ok().expect("failed to enumerate flac files");
    for path in dir.iter() {
        if path.is_file() && path.extension_str() == Some("flac") {
            print!("    comparing {} ...", path.as_str().expect("unsupported filename"));
            compare_metaflac(path);
            println!(" ok");
        }
    }
}
