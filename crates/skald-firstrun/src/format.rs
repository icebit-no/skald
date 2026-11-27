//! Sizes, speeds and times as a Norwegian reads them: a decimal comma, decimal units as Finder counts them.

use std::time::Duration;

const KB: f64 = 1e3;
const MB: f64 = 1e6;
const GB: f64 = 1e9;

/// «3,1 GB», «850 MB», «12 kB».
pub fn bytes(count: u64) -> String {
    let count = count as f64;
    if count >= GB {
        format!("{} GB", decimal(count / GB, 1))
    } else if count >= MB {
        format!("{} MB", decimal(count / MB, 0))
    } else if count >= KB {
        format!("{} kB", decimal(count / KB, 0))
    } else {
        format!("{count} byte")
    }
}

/// «1,2 av 3,1 GB», or «850 MB av 3,1 GB» while the two need different units.
pub fn bytes_of(received: u64, total: u64) -> String {
    let (received_text, total_text) = (bytes(received), bytes(total));
    if received == 0 {
        return format!("0 av {total_text}");
    }
    match (unit(&received_text), unit(&total_text)) {
        (Some(a), Some(b)) if a == b => {
            format!(
                "{} av {total_text}",
                received_text.trim_end_matches(a).trim_end()
            )
        }
        _ => format!("{received_text} av {total_text}"),
    }
}

/// «24 MB/s», «2,4 MB/s», «850 kB/s».
pub fn speed(bytes_per_second: f64) -> String {
    if bytes_per_second >= 10.0 * MB {
        format!("{} MB/s", decimal(bytes_per_second / MB, 0))
    } else if bytes_per_second >= MB {
        format!("{} MB/s", decimal(bytes_per_second / MB, 1))
    } else {
        format!("{} kB/s", decimal(bytes_per_second / KB, 0))
    }
}

/// «under ett minutt igjen», «ca. 4 min igjen», «ca. 1 t 20 min igjen».
pub fn time_left(left: Duration) -> String {
    let minutes = (left.as_secs_f64() / 60.0).round() as u64;
    if left < Duration::from_secs(60) {
        "under ett minutt igjen".to_owned()
    } else if minutes < 60 {
        format!("ca. {minutes} min igjen")
    } else if minutes.is_multiple_of(60) {
        format!("ca. {} t igjen", minutes / 60)
    } else {
        format!("ca. {} t {} min igjen", minutes / 60, minutes % 60)
    }
}

/// The share as a whole percent, never 100 before the last byte: «42 %».
pub fn percent(received: u64, total: u64) -> String {
    let share = fraction(received, total) * 100.0;
    let whole = if received < total {
        share.floor().min(99.0)
    } else {
        100.0
    };
    format!("{whole} %")
}

pub fn fraction(received: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.0;
    }
    (received as f64 / total as f64).clamp(0.0, 1.0) as f32
}

/// The start and end of a long hash: «0f2f66f2…a33e76».
pub fn short_hash(hash: &str) -> String {
    match (hash.get(..8), hash.get(hash.len().saturating_sub(6)..)) {
        (Some(start), Some(end)) if hash.len() > 16 => format!("{start}…{end}"),
        _ => hash.to_owned(),
    }
}

fn decimal(value: f64, places: usize) -> String {
    format!("{value:.places$}").replace('.', ",")
}

fn unit(text: &str) -> Option<&str> {
    text.rsplit(' ').next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_take_a_decimal_comma_and_decimal_units() {
        assert_eq!(bytes(3_095_033_483), "3,1 GB");
        assert_eq!(bytes(1_000_000_000), "1,0 GB");
        assert_eq!(bytes(850_400_000), "850 MB");
        assert_eq!(bytes(12_300), "12 kB");
        assert_eq!(bytes(512), "512 byte");
    }

    #[test]
    fn progress_names_the_unit_once_when_both_share_it() {
        assert_eq!(bytes_of(1_240_000_000, 3_095_033_483), "1,2 av 3,1 GB");
        assert_eq!(bytes_of(850_000_000, 3_095_033_483), "850 MB av 3,1 GB");
        assert_eq!(bytes_of(0, 3_095_033_483), "0 av 3,1 GB");
    }

    #[test]
    fn speed_keeps_two_figures() {
        assert_eq!(speed(24_400_000.0), "24 MB/s");
        assert_eq!(speed(2_440_000.0), "2,4 MB/s");
        assert_eq!(speed(850_000.0), "850 kB/s");
    }

    #[test]
    fn time_left_is_rounded_to_minutes() {
        assert_eq!(time_left(Duration::from_secs(35)), "under ett minutt igjen");
        assert_eq!(time_left(Duration::from_secs(60)), "ca. 1 min igjen");
        assert_eq!(time_left(Duration::from_secs(250)), "ca. 4 min igjen");
        assert_eq!(time_left(Duration::from_secs(3600)), "ca. 1 t igjen");
        assert_eq!(time_left(Duration::from_secs(4800)), "ca. 1 t 20 min igjen");
    }

    #[test]
    fn the_percent_reaches_100_only_with_the_last_byte() {
        assert_eq!(percent(0, 100), "0 %");
        assert_eq!(percent(42, 100), "42 %");
        assert_eq!(percent(999_999, 1_000_000), "99 %");
        assert_eq!(percent(100, 100), "100 %");
        assert_eq!(fraction(5, 0), 0.0);
    }

    #[test]
    fn a_hash_is_shortened_in_the_middle() {
        assert_eq!(
            short_hash("0f2f66f22e11a7c7da3c582d8e5c89cb2c0011753ba9c7c9731e320a4ba33e76"),
            "0f2f66f2…a33e76"
        );
        assert_eq!(short_hash("abc"), "abc");
    }
}
