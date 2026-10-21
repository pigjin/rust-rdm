//! Value rules used by the original ContentServerStatus.vue and byte_convert.js.
use crate::model::Info;

pub(super) fn info_integer(info: &Info, section: &str, key: &str) -> f64 {
    sample_integer(
        info.get(section)
            .and_then(|values| values.get(key))
            .map(String::as_str)
            .unwrap_or("0"),
    )
}

pub(super) fn uptime(seconds: u64) -> (u64, &'static str) {
    if seconds < 3600 {
        (seconds / 60, "common.unit_minute")
    } else if seconds < 60 * 3600 {
        (seconds / 3600, "common.unit_hour")
    } else {
        (seconds / 86400, "common.unit_day")
    }
}

/// Redis INFO is textual; the original chart reads its numeric prefixes with
/// parseInt, including decimal network rates, rather than parsing an f64 rate.
pub(super) fn sample_integer(raw: &str) -> f64 {
    let raw = raw.trim_start_matches(|c| {
        matches!(c, '\u{0009}'..='\u{000d}' | ' ' | '\u{00a0}' | '\u{1680}'
            | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
            | '\u{205f}' | '\u{3000}' | '\u{feff}')
    });
    let (sign, raw) = if let Some(rest) = raw.strip_prefix('-') {
        (-1., rest)
    } else {
        (1., raw.strip_prefix('+').unwrap_or(raw))
    };
    if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        let count = hex.bytes().take_while(u8::is_ascii_hexdigit).count();
        if count == 0 {
            return f64::NAN;
        }
        let digits = &hex[..count];
        let value = u128::from_str_radix(digits, 16)
            .map(|n| n as f64)
            .unwrap_or_else(|_| {
                digits.bytes().fold(0., |n, digit| {
                    n * 16. + (digit as char).to_digit(16).unwrap() as f64
                })
            });
        return sign * value;
    }
    let count = raw.bytes().take_while(u8::is_ascii_digit).count();
    sign * raw[..count].parse::<f64>().unwrap_or(f64::NAN)
}

/// The status summary uses two decimals and a separate unit; chart ticks call
/// the original formatBytes with one decimal and no separator.
pub(super) fn format_bytes(bytes: f64, places: u32, separator: &str) -> String {
    if bytes <= 0. {
        return format!("0{separator}B");
    }
    // The original indexes an undefined unit for positive sub-byte values;
    // formatBytes consequently returns NaN. Chart callbacks preserve that.
    if bytes.is_nan() || bytes < 1. {
        return "NaN".into();
    }
    let index = (bytes.log(1024.).floor() as usize).min(4);
    let unit = ["B", "KB", "MB", "GB", "TB"][index];
    let value = bytes / 1024_f64.powi(index as i32);
    format!("{}{separator}{unit}", compact_fixed(value, places))
}

fn compact_fixed(value: f64, places: u32) -> String {
    assert!(places <= 2, "status byte precision is one or two digits");
    if value.is_infinite() {
        return "Infinity".into();
    }
    if value >= 1e21 {
        return format!("{value:e}").replace('e', "e+");
    }
    // ECMAScript toFixed chooses the larger integer on an exact tie. Scale
    // the exact binary value, rather than rounding a floating multiplication
    // or using Rust's ties-to-even formatter (1.125 must become 1.13).
    // https://tc39.es/ecma262/multipage/numbers-and-dates.html#sec-number.prototype.tofixed
    let bits = value.to_bits();
    let mantissa = ((bits & ((1_u64 << 52) - 1)) | (1_u64 << 52)) as u128;
    let exponent = ((bits >> 52) & 0x7ff) as i32 - 1023 - 52;
    let scale = 10_u128.pow(places);
    let scaled = mantissa * scale;
    let integer = if exponent >= 0 {
        scaled << exponent
    } else {
        let shift = (-exponent) as u32;
        if shift >= 128 {
            0
        } else {
            let denominator = 1_u128 << shift;
            scaled / denominator + u128::from(scaled % denominator >= denominator / 2)
        }
    };
    let fraction = integer % scale;
    if fraction == 0 {
        (integer / scale).to_string()
    } else {
        let fraction = format!("{fraction:0width$}", width = places as usize);
        format!("{}.{}", integer / scale, fraction.trim_end_matches('0'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn source() -> Value {
        serde_json::from_str(include_str!(
            "../../tests/fixtures/status-values-source.json"
        ))
        .unwrap()
    }

    #[test]
    fn original_status_uptime_boundaries() {
        for row in source()["records"]["uptime"].as_array().unwrap() {
            let seconds = row["seconds"].as_u64().unwrap();
            assert_eq!(
                uptime(seconds),
                (
                    row["value"].as_u64().unwrap(),
                    row["unitKey"].as_str().unwrap()
                ),
                "{seconds}"
            );
        }
    }

    #[test]
    fn original_status_byte_rounding_and_ticks() {
        let source = source();
        for (group, places, separator) in [("memory", 2, " "), ("chartBytes", 1, "")] {
            for row in source["records"][group].as_array().unwrap() {
                let bytes = row["bytes"].as_f64().unwrap();
                assert_eq!(
                    format_bytes(bytes, places, separator),
                    row["expected"].as_str().unwrap(),
                    "{group} {bytes}"
                );
            }
        }
    }

    #[test]
    fn original_status_info_integer_prefixes() {
        for row in source()["records"]["integerParsing"].as_array().unwrap() {
            let raw = row["raw"].as_str().unwrap();
            let actual = sample_integer(raw);
            if let Some(expected) = row["value"].as_f64() {
                assert_eq!(actual, expected, "{raw:?}");
                assert_eq!(
                    actual == 0. && actual.is_sign_negative(),
                    row["negativeZero"].as_bool().unwrap(),
                    "{raw:?}"
                );
            } else {
                assert!(actual.is_nan(), "{raw:?}");
            }
        }
        // The source's get(info, path, '0') distinguishes an absent INFO field
        // from an explicitly present empty value before calling parseInt.
        let mut info = Info::new();
        assert_eq!(info_integer(&info, "Clients", "connected_clients"), 0.);
        info.insert(
            "Clients".into(),
            [("connected_clients".into(), String::new())].into(),
        );
        assert!(info_integer(&info, "Clients", "connected_clients").is_nan());
    }

    #[test]
    fn original_status_network_sample_values() {
        for row in source()["records"]["samples"].as_array().unwrap() {
            let actual = sample_integer(row["raw"].as_str().unwrap());
            assert_eq!(actual, row["input"].as_f64().unwrap());
            assert_eq!(actual, row["output"].as_f64().unwrap());
        }
    }
}
