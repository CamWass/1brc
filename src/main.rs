use std::{
    fs::File,
    io::{BufRead, BufReader},
};

use rustc_hash::FxHashMap;

fn main() {
    let file = File::open("measurements.txt").expect("measurements.txt file not found");
    let reader = BufReader::new(file);

    let mut results: FxHashMap<String, Result> = FxHashMap::default();

    for line in reader.lines() {
        let line = line.unwrap();
        let mut parts = line.split(';');

        let station = parts.next().unwrap();
        let measurement_string = parts.next().unwrap();

        let measurement = measurement_string.parse::<f32>().unwrap();

        let result = results.entry(station.to_string()).or_default();

        result.sum += measurement;
        result.count += 1;

        result.max = f32::max(measurement, result.max);
        result.min = f32::min(measurement, result.min);
    }

    let mut results = results.into_iter().collect::<Vec<_>>();

    results.sort_unstable_by(|a, b| a.0.cmp(&b.0));

    print!("{{");

    for (
        station,
        Result {
            min,
            sum,
            count,
            max,
        },
    ) in results[..results.len() - 1].iter()
    {
        let avg = sum / *count as f32;
        print!("{station}={min:.1}/{avg:.1}/{max:.1}, ");
    }

    let (
        station,
        Result {
            min,
            sum,
            count,
            max,
        },
    ) = results.last().unwrap();
    let avg = sum / *count as f32;

    print!("{station}={min:.1}/{avg:.1}/{max:.1}}}");
}

struct Result {
    min: f32,
    sum: f32,
    count: u32,
    max: f32,
}

impl Default for Result {
    fn default() -> Self {
        Result {
            min: f32::INFINITY,
            sum: 0.0,
            count: 0,
            max: f32::NEG_INFINITY,
        }
    }
}
