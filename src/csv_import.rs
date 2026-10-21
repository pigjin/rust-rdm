//! CSV record and expiry conversion behavior from the original Go importer.
//!
//! The original importer stops at a malformed record instead of recovering.
//! In particular, BOMs are data, bare CRs are not record delimiters, CRLFs are
//! normalized inside quoted fields, and quotes must start and end a field.
//! Behavior follows Go's reader.go; its BSD license is in assets/licenses/go-csv.txt.
use std::io::{self, BufRead};

/// Match time.Duration wrapping, Time.Sub saturation and go-redis formatMs.
/// Sample the signed Unix nanosecond clock after parsing a valid expiry, as Go does.
pub(crate) fn restore_ttl_ms(
    custom_seconds: i64,
    absolute_expiry: Option<&[u8]>,
    now_unix_nanos: impl FnOnce() -> i128,
) -> i64 {
    let duration_nanos = if custom_seconds > 0 {
        custom_seconds.wrapping_mul(1_000_000_000)
    } else if custom_seconds < 0 {
        absolute_expiry
            .and_then(|value| std::str::from_utf8(value).ok())
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| *value > 0)
            .map(|expiry| {
                (i128::from(expiry) * 1_000_000)
                    .saturating_sub(now_unix_nanos())
                    .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
            })
            .unwrap_or(0)
    } else {
        0
    };
    if duration_nanos > 0 && duration_nanos < 1_000_000 {
        1
    } else {
        // Go integer division truncates negative durations toward zero too.
        duration_nanos / 1_000_000
    }
}

pub(crate) struct Reader<R> {
    input: R,
    fields_per_record: Option<usize>,
}

fn malformed(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

impl<R: BufRead> Reader<R> {
    pub(crate) fn new(input: R) -> Self {
        Self {
            input,
            fields_per_record: None,
        }
    }

    fn read_line(&mut self) -> io::Result<Option<Vec<u8>>> {
        let mut line = Vec::new();
        if self.input.read_until(b'\n', &mut line)? == 0 {
            return Ok(None);
        }
        if line.ends_with(b"\r\n") {
            line.pop();
            *line.last_mut().unwrap() = b'\n';
        } else if line.last() == Some(&b'\r') {
            // Go preserves other bare CRs, but drops the final CR before EOF.
            line.pop();
        }
        Ok(Some(line))
    }

    pub(crate) fn read_record(&mut self) -> io::Result<Option<Vec<Vec<u8>>>> {
        let mut line = loop {
            match self.read_line()? {
                Some(line) if line.is_empty() || line == b"\n" => continue,
                Some(line) => break line,
                None => return Ok(None),
            }
        };
        let mut position = 0;
        let mut fields = Vec::new();
        'field: loop {
            if line.get(position) != Some(&b'"') {
                let comma = line[position..].iter().position(|byte| *byte == b',');
                let end = comma
                    .map(|index| position + index)
                    .unwrap_or_else(|| line.len() - usize::from(line.last() == Some(&b'\n')));
                let field = &line[position..end];
                if field.contains(&b'"') {
                    return Err(malformed("bare quote in non-quoted field"));
                }
                fields.push(field.to_vec());
                if comma.is_some() {
                    position = end + 1;
                    continue;
                }
                break;
            }

            position += 1;
            let mut field = Vec::new();
            loop {
                if let Some(quote) = line[position..].iter().position(|byte| *byte == b'"') {
                    let quote = position + quote;
                    field.extend_from_slice(&line[position..quote]);
                    position = quote + 1;
                    match line.get(position) {
                        Some(b'"') => {
                            field.push(b'"');
                            position += 1;
                        }
                        Some(b',') => {
                            fields.push(field);
                            position += 1;
                            continue 'field;
                        }
                        None => {
                            fields.push(field);
                            break 'field;
                        }
                        Some(b'\n') if position + 1 == line.len() => {
                            fields.push(field);
                            break 'field;
                        }
                        _ => return Err(malformed("extraneous quote in quoted field")),
                    }
                } else {
                    field.extend_from_slice(&line[position..]);
                    line = self
                        .read_line()?
                        .ok_or_else(|| malformed("missing quote in quoted field"))?;
                    position = 0;
                }
            }
        }
        match self.fields_per_record {
            Some(expected) if expected != fields.len() => Err(malformed("wrong number of fields")),
            _ => {
                self.fields_per_record = Some(fields.len());
                Ok(Some(fields))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};

    #[test]
    fn restore_ttl_matches_original_clock_and_wire_samples() {
        let oracle: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/csv-ttl-source.json")).unwrap();
        for sample in oracle["records"].as_array().unwrap() {
            let expiry = sample["expiryHex"]
                .as_str()
                .map(|value| hex::decode(value).unwrap());
            let now = i128::from(sample["nowMs"].as_i64().unwrap()) * 1_000_000
                + i128::from(sample["nowRemainderNs"].as_i64().unwrap());
            assert_eq!(
                restore_ttl_ms(sample["ttl"].as_i64().unwrap(), expiry.as_deref(), || now),
                sample["wireTtlMs"].as_i64().unwrap(),
                "{}: original RESTORE TTL after duration conversion",
                sample["name"]
            );
        }
    }

    #[test]
    fn records_match_original_go_at_different_buffer_boundaries() {
        let samples: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/csv-parser-source.json")).unwrap();
        for sample in samples["records"].as_array().unwrap() {
            let raw = hex::decode(sample["inputHex"].as_str().unwrap()).unwrap();
            for capacity in [1, 7, 4096] {
                let mut reader = Reader::new(BufReader::with_capacity(capacity, Cursor::new(&raw)));
                let mut records = Vec::new();
                let failed = loop {
                    match reader.read_record() {
                        Ok(Some(row)) => {
                            records.push(row.iter().map(hex::encode).collect::<Vec<_>>())
                        }
                        Ok(None) => break false,
                        Err(_) => break true,
                    }
                };
                assert_eq!(
                    serde_json::json!(records),
                    sample["recordsHex"],
                    "{} at buffer size {capacity}",
                    sample["name"]
                );
                assert_eq!(
                    failed,
                    sample["parseError"].as_bool().unwrap(),
                    "{} parse error at buffer size {capacity}",
                    sample["name"]
                );
            }
        }
    }

    #[test]
    fn short_byte_streams_match_original_go_exhaustive_samples() {
        fn bytes(hash: &mut u64, value: &[u8]) {
            for byte in value {
                *hash = (*hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
            }
        }
        fn number(hash: &mut u64, value: usize) {
            bytes(hash, &(value as u64).to_le_bytes());
        }
        let oracle: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/csv-parser-exhaustive.json"))
                .unwrap();
        let alphabet = hex::decode(oracle["alphabetHex"].as_str().unwrap()).unwrap();
        let mut total = 0;
        for sample in oracle["records"].as_array().unwrap() {
            let length = sample["length"].as_u64().unwrap() as usize;
            let count = sample["count"].as_u64().unwrap() as usize;
            assert_eq!(count, alphabet.len().pow(length as u32));
            let mut hash = 0xcbf29ce484222325;
            for index in 0..count {
                let mut raw = vec![0; length];
                let mut value = index;
                for byte in raw.iter_mut().rev() {
                    *byte = alphabet[value % alphabet.len()];
                    value /= alphabet.len();
                }
                let mut reader = Reader::new(BufReader::with_capacity(3, Cursor::new(&raw)));
                let mut rows = Vec::new();
                let failed = loop {
                    match reader.read_record() {
                        Ok(Some(row)) => rows.push(row),
                        Ok(None) => break false,
                        Err(_) => break true,
                    }
                };
                number(&mut hash, raw.len());
                bytes(&mut hash, &raw);
                number(&mut hash, rows.len());
                for row in rows {
                    number(&mut hash, row.len());
                    for field in row {
                        number(&mut hash, field.len());
                        bytes(&mut hash, &field);
                    }
                }
                bytes(&mut hash, &[u8::from(failed)]);
            }
            assert_eq!(
                format!("{hash:016x}"),
                sample["fnv1a64"].as_str().unwrap(),
                "Go CSV records/error fingerprint at input length {length}"
            );
            total += count;
        }
        assert_eq!(total, oracle["totalCases"].as_u64().unwrap() as usize);
    }
}
