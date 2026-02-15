#![feature(core_io_borrowed_buf)]
#![feature(read_buf)]
#![feature(maybe_uninit_slice)]

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    thread,
};

use foldhash::HashMap;

use crate::buffer::BufReader;

mod buffer;

const MEASUREMENT_FILE_PATH: &'static str = "measurements.txt";

fn main() {
    // We process the file in chunks using multiple threads.
    // We can't cleanly chunk the file, such that each chunk only contains whole lines,
    // without first parsing the whole thing, which would defeat the purpose of multi
    // threading.
    // Instead, we naively chunk the file, and each thread parses the complete lines in
    // its chunk, storing the partial data at the start/end of its chunk.
    // Finally, we concatenate the unconsumed data of each chunk into a new buffer, which
    // we parse on the main thread, and merge all of the results together.

    let file_len = File::open(MEASUREMENT_FILE_PATH)
        .expect("measurement file not found")
        .metadata()
        .unwrap()
        .len();

    let cpu_count = num_cpus::get() as u64;

    let mut chunk_processing_result = thread::scope(|s| {
        let handles: Vec<_> = chunk_indices(cpu_count, file_len)
            .map(|(start, end)| s.spawn(move || process_chunk(MEASUREMENT_FILE_PATH, start, end)))
            .collect();

        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .fold(ChunkProcessingResult::default(), merge_chunk_results)
    });

    let consumed = parse_buffer(
        0,
        &chunk_processing_result.unconsumed,
        &mut chunk_processing_result.results,
    );

    // The unconsumed portion should always consist of whole measurements, so we should
    // consume all of during the final parse step.
    debug_assert_eq!(consumed, chunk_processing_result.unconsumed.len());

    // Write results, sorted by station name.

    let mut results = chunk_processing_result
        .results
        .into_iter()
        .collect::<Vec<_>>();

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

type Results = HashMap<Vec<u8>, Result>;

#[derive(Default)]
struct ChunkProcessingResult {
    /// Partial measurements from the start/end of the chunk.
    unconsumed: Vec<u8>,
    /// The parsed measurement data for the complete measurements in the chunk.
    results: Results,
}

/// Opens the file at `file_path` and parses measurements from `[chunk_start, chunk_end)`.
fn process_chunk(
    file_path: &'static str,
    chunk_start: u64,
    chunk_end: u64,
) -> ChunkProcessingResult {
    let mut file = File::open(file_path).unwrap();

    if chunk_start != 0 {
        file.seek(SeekFrom::Start(chunk_start)).unwrap();
    }

    // .take() ensures each thread doesn't read past its chunk.
    let mut reader = BufReader::new(file.take(chunk_end - chunk_start));

    let mut results: Results = Results::default();

    let mut bytes = reader.fill_buf().unwrap();

    let mut i = 0;

    let mut unconsumed = Vec::new();

    // We naively chunk the file, so each chunk is likely to start in the
    // middle of a line. We account for this by skipping to the first
    // newline in the chunk, where we can start parsing line-by-line, and
    // storing the skipped/unconsumed content for later re-processing.
    if chunk_start != 0 {
        while i < bytes.len() {
            if bytes[i] == b'\n' {
                i += 1;
                unconsumed.extend_from_slice(&bytes[0..i]);
                break;
            }

            i += 1;
        }
    }

    // Parse lines from the reader. When we parse a line, we mark the
    // input up to that point as consumed. Then, when we've exhausted the
    // buffer, we backshift the unconsumed tail portion to the start of
    // the buffer and refill it up to capacity.
    while bytes.len() > 0 {
        let consumed = parse_buffer(i, bytes, &mut results);

        // Inform the reader of how many bytes we actually 'used'.
        reader.consume(consumed);

        // Shift any unconsumed bytes to the start of the buffer.
        reader.buf.backshift();

        // Fill the buffer up to capacity, or with all remaining bytes from the
        // file.
        let read = reader.buf.read_more(&mut reader.inner).unwrap();
        bytes = reader.buf.buffer();

        if read == 0 {
            break;
        }

        i = 0;
    }

    // Similar to the chunk start, the chunk end is likely to be in the
    // middle of a line, so our line-by-line parsing won't consume the
    // whole buffer, and we need to store the unconsumed portion for later
    // re-processing.
    if bytes.len() > 0 {
        unconsumed.extend_from_slice(bytes);
    }

    ChunkProcessingResult {
        results,
        unconsumed,
    }
}

/// Parses measurements from `buffer`, line-by-line. Returns the number of bytes that were
/// consumed. If the buffer ends in the middle of a measurement, then
/// `consumed != buffer.len()`.
fn parse_buffer(start_index: usize, buffer: &[u8], results: &mut Results) -> usize {
    let mut i = start_index;
    let mut station_start = start_index;

    let mut consumed = 0;

    while i < buffer.len() {
        let byte = buffer[i];

        if byte == b';' {
            let station = &buffer[station_start..i];

            let measurement_start = i + 1;

            let mut j = measurement_start;

            while j < buffer.len() {
                let byte = buffer[j];

                if byte == b'\n' {
                    let measurement_bytes = &buffer[measurement_start..j];

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

    consumed
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

/// Combines the data from two chunks into one.
fn merge_chunk_results(
    mut a: ChunkProcessingResult,
    b: ChunkProcessingResult,
) -> ChunkProcessingResult {
    a.unconsumed.extend_from_slice(&b.unconsumed);

    for (key, value) in b.results {
        let result = if let Some(result) = a.results.get_mut(&key) {
            result
        } else {
            a.results.entry(key).or_default()
        };

        result.sum += value.sum;
        result.count += value.count;

        result.max = f32::max(value.max, result.max);
        result.min = f32::min(value.min, result.min);
    }

    a
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

/// Splits `total_len` evenly into `num_chunks` chunks. If `num_chunks` does not divide
/// `total_len`, the remainder is added to the last chunk.
fn chunk_indices(num_chunks: u64, total_len: u64) -> impl Iterator<Item = (u64, u64)> {
    let chunk_size = total_len / num_chunks;

    (0..num_chunks).map(move |i| {
        let start = i * chunk_size;
        let end = if i == num_chunks - 1 {
            total_len
        } else {
            start + chunk_size
        };
        (start, end)
    })
}
