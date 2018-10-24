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

/// Estimate the scale parameter of the Gamma distribution.
fn estimate_scale(k: f64, offset: f64, xs: &[f64]) -> f64 {
    (mean(xs) - offset) / k
}

/// Estimate the shape parameter of the Gamma distribution.
fn estimate_shape(offset: f64, xs: &[f64]) -> f64 {
    let ln_mean = (mean(xs) - offset).ln();
    let mean_ln = sum(xs.iter().map(|&x| (x - offset).max(1e-15).ln())) / (xs.len() as f64);
    let s = ln_mean - mean_ln;
    (3.0 - s + ((s - 3.0) * (s - 3.0) + 24.0 * s).sqrt()) / (12.0 * s)
}

fn erlang_ln_likelihood(k: u32, scale: f64, offset: f64, xs: &[f64]) -> f64 {
    let kfact: u32 = (1..k).product();
    let kpred = (k - 1) as f64;
    let ln_const = -(kfact as f64).ln() - scale.ln() * (k as f64);
    // NOTE: These constants are not relevant for optimization, unless we want
    // to optimize k and scale. But k we should fix, and for the scale we have
    // an estimator already. So we should probably remove the constants.
    ln_const * (xs.len() as f64) +
        sum(xs.iter().map(|x| (x - offset).max(1e-15).ln() * kpred - ((x - offset) / scale)))
}

fn estimate_offset(k: u32, scale: f64, offset: f64, xs: &[f64]) -> f64 {
    let mut off = offset;
    let m = min(xs.iter().cloned());
    for i in 0..10 {
        let oa = off * 0.999;
        let ob = 0.99 * off + 0.01 * m;
        let llka = erlang_ln_likelihood(k, scale, oa, xs);
        let llkb = erlang_ln_likelihood(k, scale, ob, xs);
        let dllk_do = (llkb - llka) / (ob - oa);
        //println!("{} {} {} {}", i, off, llka, llkb);
        off += (dllk_do / (xs.len() as f64)) * 0.0005;
        off = off.min(m);
        off = off.max(0.0);
    }
    off
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

    let mut offs: Vec<f64> = Vec::new();
    let mut ks: Vec<f64> = Vec::new();
    let mut scales: Vec<f64> = Vec::new();

    for frame in frames.iter() {
        // NOTE: The outcome of the estimate depends very much on the initial
        // offset we take here. We can't use 0.0, the shape estimation would go
        // wrong (although we should fix the shape parameter nonetheless). So we
        // need a really good estimate for the offset to make this work.
        let off = min(frame.iter().cloned()) * 0.99;
        let k = estimate_shape(off, &frame[..]);
        let scale = estimate_scale(12.0, off, &frame[..]);
        offs.push(off);
        ks.push(k);
        scales.push(scale);
    }

    let mut mk = mean(&ks[..]);
    let mut mscale = mean(&scales[..]);
    let mut moff = mean(&offs[..]);

    for i in 0..50 {
        println!("i: {}, k: {:0.3}, scale: {:0.4}, off: {:0.3}", i, mk, mscale, moff);

        ks.clear();
        scales.clear();

        for (i, frame) in frames.iter().enumerate() {
            // We fix k=12 for now.
            offs[i] = estimate_offset(12, mscale, offs[i], &frame[..]);
            let scale = estimate_scale(12.0, offs[i], &frame[..]);
            let k = estimate_shape(offs[i], &frame[..]);
            ks.push(k);
            scales.push(scale);
            if i % 16 == 0 {
                print!("\rFitting frame {} of {}", i, frames.len());
                io::stdout().flush().unwrap();
            }
        }

        print!("\r\x1b[0K"); // Clear the progress update line again.

        moff = mean(&offs[..]);
        mk = mean(&ks[..]);
        mscale = mean(&scales[..]);
    }
    // After a while, the parameters converge. Example (fetted with k=12):
    // i: 28, k: 12.255, scale: 0.030, off: 13.681
    // i: 29, k: 12.307, scale: 0.029, off: 13.680
    // i: 30, k: 12.262, scale: 0.030, off: 13.681
    // i: 31, k: 12.301, scale: 0.029, off: 13.680
    // i: 32, k: 12.267, scale: 0.030, off: 13.681
    // i: 33, k: 12.296, scale: 0.029, off: 13.680
    // i: 34, k: 12.271, scale: 0.030, off: 13.681
    println!("Final k: {:0.3}, scale: {:0.4}, off: {:0.3}", mk, mscale, moff);
}
