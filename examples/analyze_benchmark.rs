use std::env;
use std::fs;
use std::io::{BufRead, Write};
use std::io;
use std::str::FromStr;

fn load() -> Vec<Vec<f64>> {
    let mut frames = Vec::new();
    let fname = env::args().skip(1).next().unwrap();
    let file = io::BufReader::new(fs::File::open(fname).unwrap());
    for line in file.lines() {
        let mut frame_times = Vec::new();
        for t_str in line.unwrap().split('\t') {
            let t = f64::from_str(t_str).unwrap();
            frame_times.push(t);
        }
        frames.push(frame_times);

        if frames.len() % 256 == 0 {
            print!("\rRead {} frames", frames.len());
            io::stdout().flush();
        }
    }

    print!("\r\x1b[0K"); // Clear the progress update line again.

    frames
}

fn main() {
    let frames = load();
    println!("Loaded {} frames, {} iterations.", frames.len(), frames[0].len());
}
