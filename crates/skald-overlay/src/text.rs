//! The words the pill shows, shortened to fit.

use std::time::Duration;

/// The elapsed time of a take as minutes and seconds, «0:07».
pub fn elapsed(time: Duration) -> String {
    let seconds = time.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

const ELLIPSIS: &str = "…";

/// The start of a text in whole words, ending in « …» when it does not all fit in `room`.
pub fn start_of(text: &str, room: f32, mut width: impl FnMut(&str) -> f32) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let whole = words.join(" ");
    if width(&whole) <= room {
        return whole;
    }
    for count in (1..words.len()).rev() {
        let start = words[..count].join(" ");
        let shortened = format!(
            "{} {ELLIPSIS}",
            start.trim_end_matches([',', '.', ';', ':'])
        );
        if width(&shortened) <= room {
            return shortened;
        }
    }
    cut(&whole, room, &mut width)
}

/// A name cut to fit in `room`, ending in «…» when it is cut.
pub fn cut(name: &str, room: f32, mut width: impl FnMut(&str) -> f32) -> String {
    if width(name) <= room {
        return name.to_owned();
    }
    let mut ends: Vec<usize> = name.char_indices().map(|(i, _)| i).skip(1).collect();
    while let Some(end) = ends.pop() {
        let shortened = format!("{}{ELLIPSIS}", name[..end].trim_end());
        if width(&shortened) <= room {
            return shortened;
        }
    }
    ELLIPSIS.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character is one unit wide.
    fn chars(text: &str) -> f32 {
        text.chars().count() as f32
    }

    #[test]
    fn the_time_is_minutes_and_seconds() {
        assert_eq!(elapsed(Duration::ZERO), "0:00");
        assert_eq!(elapsed(Duration::from_millis(7_900)), "0:07");
        assert_eq!(elapsed(Duration::from_secs(65)), "1:05");
        assert_eq!(elapsed(Duration::from_secs(120)), "2:00");
    }

    #[test]
    fn a_text_that_fits_is_shown_whole() {
        assert_eq!(
            start_of("Hei, dette er en prøve.", 40.0, chars),
            "Hei, dette er en prøve."
        );
    }

    #[test]
    fn a_long_text_shows_its_first_words() {
        let text = "Kan du sende meg notatene fra møtet i går, så ser jeg på dem før lunsj.";
        assert_eq!(start_of(text, 30.0, chars), "Kan du sende meg notatene …");
        assert_eq!(
            start_of(text, 44.0, chars),
            "Kan du sende meg notatene fra møtet i går …"
        );
    }

    #[test]
    fn line_breaks_and_runs_of_spaces_become_one_space() {
        assert_eq!(start_of("Hei\n\nder  du", 40.0, chars), "Hei der du");
    }

    #[test]
    fn a_first_word_too_long_is_cut() {
        assert_eq!(
            start_of("Donaudampfschifffahrt er langt", 8.0, chars),
            "Donauda…"
        );
    }

    #[test]
    fn a_name_is_cut_where_it_runs_out_of_room() {
        assert_eq!(cut("Shure MV7", 20.0, chars), "Shure MV7");
        assert_eq!(cut("Jabra Evolve2 65 Flex", 8.0, chars), "Jabra E…");
        assert_eq!(cut("Jabra Evolve2 65 Flex", 7.0, chars), "Jabra…");
        assert_eq!(cut("Å", 0.0, chars), "…");
    }
}
