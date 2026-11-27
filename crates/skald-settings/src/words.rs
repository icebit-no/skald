//! Numbers and times in the words the window shows them in, the same as `skald` prints them.

/// A size in gigabytes or megabytes with a decimal comma: «3,1 GB», «850 MB».
pub(crate) fn size(bytes: u64) -> String {
    const MB: f64 = 1_000_000.0;
    const GB: f64 = 1_000_000_000.0;
    let bytes = bytes as f64;
    if bytes >= GB {
        format!("{:.1} GB", bytes / GB).replace('.', ",")
    } else {
        format!("{:.0} MB", bytes / MB)
    }
}

/// How much of a file has arrived: «1,2 av 3,1 GB».
pub(crate) fn part_of(received: u64, total: u64) -> String {
    let total_text = size(total);
    let unit = total_text.rsplit(' ').next().unwrap_or("");
    let received_text = size(received);
    match received_text.strip_suffix(&format!(" {unit}")) {
        Some(number) => format!("{number} av {total_text}"),
        None => format!("{received_text} av {total_text}"),
    }
}

pub(crate) fn times_applied(count: u32) -> String {
    match count {
        0 => "ikke brukt ennå".to_owned(),
        1 => "brukt 1 gang".to_owned(),
        n => format!("brukt {n} ganger"),
    }
}

/// How long ago a take ended; `None` when the time is unknown.
pub(crate) fn ago(unix_ms: i64, now_ms: i64) -> Option<String> {
    if unix_ms <= 0 {
        return None;
    }
    let minutes = (now_ms - unix_ms).max(0) / 60_000;
    let (hours, days) = (minutes / 60, minutes / (60 * 24));
    Some(match (days, hours, minutes) {
        (0, 0, 0) => "nå nettopp".to_owned(),
        (0, 0, 1) => "for 1 minutt siden".to_owned(),
        (0, 0, n) => format!("for {n} minutter siden"),
        (0, 1, _) => "for 1 time siden".to_owned(),
        (0, n, _) => format!("for {n} timer siden"),
        (1, _, _) => "for 1 dag siden".to_owned(),
        (n, _, _) => format!("for {n} dager siden"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_have_a_decimal_comma() {
        assert_eq!(size(3_095_033_483), "3,1 GB");
        assert_eq!(size(850_000_000), "850 MB");
        assert_eq!(part_of(1_200_000_000, 3_095_033_483), "1,2 av 3,1 GB");
        assert_eq!(part_of(40_000_000, 3_095_033_483), "40 MB av 3,1 GB");
    }

    #[test]
    fn counts_and_times_read_as_the_command_line_prints_them() {
        assert_eq!(times_applied(0), "ikke brukt ennå");
        assert_eq!(times_applied(1), "brukt 1 gang");
        assert_eq!(times_applied(12), "brukt 12 ganger");
        let now = 1_795_800_000_000;
        assert_eq!(ago(now - 5_000, now).as_deref(), Some("nå nettopp"));
        assert_eq!(
            ago(now - 3 * 60_000, now).as_deref(),
            Some("for 3 minutter siden")
        );
        assert_eq!(
            ago(now - 26 * 3_600_000, now).as_deref(),
            Some("for 1 dag siden")
        );
        assert_eq!(ago(0, now), None);
    }
}
