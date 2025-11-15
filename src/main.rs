#![feature(core_io_borrowed_buf)]
#![feature(read_buf)]
#![feature(maybe_uninit_slice)]

use std::{fs::File, io::Write};

use foldhash::HashMap;

use crate::buffer::BufReader;

mod buffer;

fn main() {
    let file = File::open("measurements.txt").expect("measurements.txt file not found");

    let mut reader = BufReader::new(file);

    let mut results: HashMap<Vec<u8>, Result> = HashMap::default();

    let mut bytes = reader.fill_buf().unwrap();

    // Parse lines from the reader. When we parse a line, we mark the input up
    // to that point as consumed. Then, when we've exhausted the buffer, we
    // backshift the unconsumed tail portion to the start of the buffer and
    // refill it up to capacity.
    while bytes.len() > 0 {
        let mut station_start = 0;

        let mut i = 0;

        let mut consumed = 0;

        while i < bytes.len() {
            let byte = bytes[i];

            if byte == b';' {
                let station = &bytes[station_start..i];

                let measurement_start = i + 1;

                let mut j = measurement_start;

                while j < bytes.len() {
                    let byte = bytes[j];

                    if byte == b'\n' {
                        let measurement_bytes = &bytes[measurement_start..j];

                        let measurement = parse_measurement(measurement_bytes);

                        let result = if let Some(result) = results.get_mut(station) {
                            result
                        } else {
                            results.entry(station.to_vec()).or_default()
                        };

                        result.sum += measurement;
                        result.count += 1;

                        result.max = f32::max(measurement, result.max);
                        result.min = f32::min(measurement, result.min);

                        j += 1;
                        consumed = j;
                        break;
                    }

                    j += 1;
                }

                i = j;

                station_start = i;
            } else {
                i += 1;
            }
        }

        // Inform the reader of how many bytes we actually 'used'.
        reader.consume(consumed);

        // Shift any unconsumed bytes to the start of the buffer.
        reader.buf.backshift();

        // Fill the buffer up to capacity, or with all remaining bytes from the
        // file.
        reader.buf.read_more(&reader.inner).unwrap();
        bytes = reader.buf.buffer();
    }

    let mut results = results.into_iter().collect::<Vec<_>>();

    results.sort_unstable_by(|a, b| a.0.cmp(&b.0));

    let stdout = std::io::stdout();
    let mut lock = stdout.lock();

    lock.write(b"{").unwrap();

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

        lock.write(station).unwrap();
        write!(lock, "={min:.1}/{avg:.1}/{max:.1}, ").unwrap();
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

    lock.write(station).unwrap();
    write!(lock, "={min:.1}/{avg:.1}/{max:.1}}}").unwrap();
}

fn parse_measurement(measurement_bytes: &[u8]) -> f32 {
    // - 1 for the fractional digit - ignore the decimal point.
    let mut whole_bytes = &measurement_bytes[..measurement_bytes.len() - 2];

    let mut negative = false;

    if whole_bytes.first() == Some(&b'-') {
        negative = true;
        whole_bytes = &whole_bytes[1..]
    }

    let fractional = byte_ascii_digit(measurement_bytes.last().unwrap()) as f32;

    let mut whole: f32 = 0.0;

    let mut pow: f32 = 1.0;

    for byte in whole_bytes.iter().rev() {
        whole += byte_ascii_digit(byte) as f32 * pow;
        pow *= 10.0;
    }

    let mut measurement = whole + fractional / 10.0;

    if negative {
        measurement *= -1.0;
    }

    measurement
}

fn byte_ascii_digit(byte: &u8) -> u8 {
    byte - b'0'
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
