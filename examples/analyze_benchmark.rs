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
            io::stdout().flush().unwrap();
        }
    }

    print!("\r\x1b[0K"); // Clear the progress update line again.

    frames
}

fn min<I: IntoIterator<Item = f64>>(xs: I) -> f64 {
    // We have to implement this manually because Rust insists on not being able
    // to compare floats in an ergonomic manner, which is good for robustness,
    // and bad for a program like this one.
    let mut iter = xs.into_iter();
    let mut m = iter.next().unwrap();
    for x in iter {
        if x < m {
            m = x;
        }
    }
    m
}

fn sum<I: IntoIterator<Item = f64>>(xs: I) -> f64 {
    let mut acc = 0.0;
    let mut res = 0.0;
    for x in xs {
        let new = acc + (res + x);
        res = (res + x) - (new - acc);
        acc = new;
    }
    acc
}

fn mean(xs: &[f64]) -> f64 {
    sum(xs.iter().cloned()) / (xs.len() as f64)
}

fn var(xs: &[f64]) -> f64 {
    let sx2 = sum(xs.iter().map(|&x| x * x));
    let sx = sum(xs.iter().cloned());
    let n = xs.len() as f64;
    (sx2 / n) - (sx / n) * (sx / n)
}

fn skewness(xs: &[f64]) -> f64 {
    let m = mean(xs);
    let sd = var(xs).sqrt();
    let s = sum(xs.iter().map(|&x| ((x - m) / sd)).map(|x| x * x * x));
    s / (xs.len() as f64)
}

/// For every frame, remove all measurements that are more than 5% slower than
/// the minimum time observed for that frame. In typical measurements there are
/// two sources of noise: modest, relatively well-behaved noise, the median of
/// this noise is around 1.4% of the frame time (around 0.2 ns per sample). Then
/// there are other sources of noise that cause extreme outliers, which add a
/// tail to the distribution, and distort the mean by a lot. I don't know how to
/// properly model that noise, so we exclude it.
fn discard_outliers(mut frames: Vec<Vec<f64>>) -> Vec<Vec<f64>> {
    let mut num_total = 0;
    let mut num_remain = 0;
    let mut total_time = 0.0;
    let mut mins = Vec::with_capacity(frames.len());

    for frame in frames.iter_mut() {
        // NOTE: Should not be based on the min, that is not stable when more
        // data comes in.
        num_total += frame.len();
        let min = min(frame.iter().cloned());
        let threshold = min * 1.05;
        frame.retain(|&t| t < threshold);
        num_remain += frame.len();
        total_time += min;
        mins.push(min);
    }

    println!(
        "{:0.2}% of data left after removing extreme noise.",
        100.0 * (num_remain as f64) / (num_total as f64)
    );

    // Some frames are very fast to decode, we also exclude those. It's not that
    // the measurements are incorrect, it's just that we are not interested in
    // these frames (they are mostly silence, or short frames). It's a tiny
    // amount of data, but it distorts the mean frame time, so we exclude it.

    let mean_time_per_sample = total_time / (frames.len() as f64);
    let threshold = mean_time_per_sample * 0.75;

    let mut frames_left = Vec::with_capacity(frames.len());
    num_remain = 0;

    for (frame, min) in frames.drain(..).zip(mins) {
        if min > threshold {
            num_remain += frame.len();
            frames_left.push(frame);
        }
    }

    println!(
        "{:0.2}% of data left after excluding fast frames.",
        100.0 * (num_remain as f64) / (num_total as f64)
    );

    println!("Time per sample: {:0.3} ns.", mean_time_per_sample);

    frames_left
}

fn main() {
    let mut frames = load();
    println!("Loaded {} frames, {} iterations.", frames.len(), frames[0].len());

    frames = discard_outliers(frames);
    let skews: Vec<f64> = frames.iter().map(|f| skewness(&f[..])).collect();
    let sk = mean(&skews[..]);
    println!("{:0.3} -> {:0.3}", sk, 4.0 / (sk * sk));
}
