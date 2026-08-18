//! In-process clock and local-time handling.
//!
//! Replaces subprocess spawning of `date` with in-process glib/system time formatting.

pub fn current_time_str() -> String {
    if let Ok(now) = glib::DateTime::now_local() {
        if let Ok(formatted) = now.format("%H:%M") {
            return formatted.to_string();
        }
    }
    "--:--".to_string()
}

pub fn current_date_str() -> String {
    if let Ok(now) = glib::DateTime::now_local() {
        if let Ok(formatted) = now.format("%A, %B %e, %Y") {
            return formatted.to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_produces_valid_time_format() {
        let time = current_time_str();
        assert_eq!(time.len(), 5);
        assert_eq!(&time[2..3], ":");
        let hours: u32 = time[0..2].parse().expect("hours numeric");
        let minutes: u32 = time[3..5].parse().expect("minutes numeric");
        assert!(hours < 24);
        assert!(minutes < 60);
    }
}
