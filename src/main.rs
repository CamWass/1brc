use std::{
    fs::File,
    io::{BufRead, BufReader},
};

use rustc_hash::FxHashMap;

fn main() {
    let file = File::open("measurements.txt").expect("measurements.txt file not found");
    let mut reader = BufReader::new(file);

    let mut results: FxHashMap<String, Result> = FxHashMap::default();

    let mut station_raw = Vec::new();
    let mut measurement_raw = Vec::new();

    while reader.read_until(b';', &mut station_raw).unwrap() != 0 {
        reader.read_until(b'\n', &mut measurement_raw).unwrap();

        // Last byte is `;`
        let station = std::str::from_utf8(&station_raw[..station_raw.len() - 1]).unwrap();

        // Last byte is `\n`
        let measurement_string =
            std::str::from_utf8(&measurement_raw[..measurement_raw.len() - 1]).unwrap();

        let measurement = measurement_string.parse::<f32>().unwrap();

        let result = if let Some(result) = results.get_mut(station) {
            result
        } else {
            results.entry(station.to_string()).or_default()
        };

        result.sum += measurement;
        result.count += 1;

        result.max = f32::max(measurement, result.max);
        result.min = f32::min(measurement, result.min);

        station_raw.clear();
        measurement_raw.clear();
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
