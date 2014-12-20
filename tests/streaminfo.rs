extern crate flac;

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
    let vec: Vec<String> = seq.iter().map(|x| format!("{:x}", *x)).collect();
    vec.concat()
}

fn read_streaminfo(fname: &Path) -> String {
    use std::io::fs::File;
    use flac::{read_stream_header, read_metadata_block_header, read_streaminfo_block};

    // Open the file and extract its streaminfo block.
    // TODO: avoid repetition in the tests and library;
    // there might be some kind of metadata block iterator in the future.
    let mut file = File::open(fname).ok().expect("failed to open flac file");
    read_stream_header(&mut file).ok().expect("invalid flac header");
    read_metadata_block_header(&mut file).ok().expect("invalid metadata block header");
    let streaminfo = read_streaminfo_block(&mut file).ok().expect("failed to read streaminfo");

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
            println!("metaflac: {}\nstreaminfo: {}", metaflac, streaminfo);
            panic!("metaflac disagrees on parsed streaminfo (run with --nocapture for details)");
        }
    };
}

#[test]
fn test_foo() {
    compare_metaflac(&Path::new("foo.flac"));
}
