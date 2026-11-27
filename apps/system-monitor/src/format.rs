//! Human-readable sizes, rates and percentages. Binary units (KiB, MiB, ...)
//! because the kernel's memory figures are binary.

/// `1536` as "1.5 KiB"; bytes below 1 KiB are exact ("512 B").
pub fn bytes(value: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if value < 1024 {
        return format!("{value} B");
    }
    let mut scaled = value as f64;
    let mut unit = 0;
    while scaled >= 1024.0 && unit + 1 < UNITS.len() {
        scaled /= 1024.0;
        unit += 1;
    }
    format!("{scaled:.1} {}", UNITS[unit])
}

/// A bytes-per-second rate, such as "1.2 MiB/s". Negative and non-finite
/// rates are not meaningful and are shown as unavailable by the caller.
pub fn rate(bytes_per_second: f64) -> String {
    format!("{}/s", bytes(bytes_per_second.max(0.0).round() as u64))
}

/// A percentage with one decimal, such as "37.5%".
pub fn percent(value: f64) -> String {
    format!("{value:.1}%")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_use_binary_units_with_one_decimal() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1536), "1.5 KiB");
        assert_eq!(bytes(16 * 1024 * 1024 * 1024), "16.0 GiB");
        // The largest value stops at the largest unit the table knows.
        assert_eq!(bytes(u64::MAX), "16384.0 PiB");
    }

    #[test]
    fn rates_and_percentages() {
        assert_eq!(rate(2.0 * 1024.0 * 1024.0), "2.0 MiB/s");
        assert_eq!(rate(-5.0), "0 B/s");
        assert_eq!(percent(37.46), "37.5%");
    }
}
