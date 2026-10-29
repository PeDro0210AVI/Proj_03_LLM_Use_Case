//! Extracts numbers from free text (model answers and tool outputs) so they
//! can be compared against reference values regardless of formatting:
//! `287640`, `$287,640.00`, `287.640` (es), `$287.6K`, `2,14 millones`, `10,1%`.

/// One number as written in the text. Ambiguous spellings (`1.842` can be
/// one thousand eight hundred forty-two or one point eight four two) keep
/// every reading.
#[derive(Debug, Clone)]
pub struct Num {
    pub raw: String,
    /// `(value, tolerance)` pairs. The tolerance is half a unit of the last
    /// written digit, so `$287.6K` matches anything in 287,550..287,650.
    pub readings: Vec<(f64, f64)>,
}

impl Num {
    pub fn matches(&self, expected: f64) -> bool {
        self.readings.iter().any(|&(value, tol)| {
            let slack = tol + expected.abs() * 1e-3 + 1e-9;
            (value - expected).abs() <= slack
        })
    }

    /// Small integers are mostly list numbering ("1.", "top 3") and carry
    /// no factual claim on their own.
    pub fn is_trivial(&self) -> bool {
        self.readings
            .iter()
            .all(|&(value, tol)| tol >= 0.5 && value.fract() == 0.0 && value <= 10.0)
    }
}

pub fn extract(text: &str) -> Vec<Num> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        let prev_is_word = i > 0 && chars[i - 1].is_alphanumeric();
        if !c.is_ascii_digit() || prev_is_word {
            i += 1;
            continue;
        }

        // Digits plus ',' / '.' separators that sit between digits.
        let start = i;
        while i < chars.len()
            && (chars[i].is_ascii_digit()
                || ((chars[i] == ',' || chars[i] == '.')
                    && i + 1 < chars.len()
                    && chars[i + 1].is_ascii_digit()))
        {
            i += 1;
        }
        let token: String = chars[start..i].iter().collect();

        // A digit glued to letters ("Q3", "v2", "2xl") is an identifier.
        if i < chars.len() && chars[i].is_alphabetic() && !starts_with_multiplier(&chars[i..]) {
            while i < chars.len() && chars[i].is_alphanumeric() {
                i += 1;
            }
            continue;
        }

        let (multiplier, consumed) = multiplier_after(&chars[i..]);
        i += consumed;

        let readings = interpret(&token)
            .into_iter()
            .map(|(value, unit)| (value * multiplier, 0.5 * unit * multiplier))
            .collect();
        out.push(Num { raw: token, readings });
    }
    out
}

/// Possible `(value, unit_of_last_digit)` readings of a digit/separator token.
fn interpret(token: &str) -> Vec<(f64, f64)> {
    let commas = token.matches(',').count();
    let dots = token.matches('.').count();
    let digits_only = |s: &str| s.replace([',', '.'], "");
    let parse = |s: &str| s.parse::<f64>().unwrap_or(0.0);

    match (commas, dots) {
        (0, 0) => vec![(parse(token), 1.0)],
        // Both kinds: the last separator is the decimal point.
        (c, d) if c > 0 && d > 0 => {
            let last = token.rfind([',', '.']).unwrap();
            let int_part = digits_only(&token[..last]);
            let frac = &token[last + 1..];
            vec![decimal(&int_part, frac)]
        }
        // Repeated separator: thousands grouping ("1,620,000" / "1.620.000").
        (c, d) if c > 1 || d > 1 => vec![(parse(&digits_only(token)), 1.0)],
        // Exactly one separator.
        _ => {
            let sep = token.find([',', '.']).unwrap();
            let int_part = &token[..sep];
            let frac = &token[sep + 1..];
            if frac.len() == 3 {
                // "1,842" / "3.840": thousands in one locale, decimal in the other.
                vec![
                    (parse(&digits_only(token)), 1.0),
                    decimal(int_part, frac),
                ]
            } else {
                vec![decimal(int_part, frac)]
            }
        }
    }
}

fn decimal(int_part: &str, frac: &str) -> (f64, f64) {
    let value = format!("{int_part}.{frac}").parse::<f64>().unwrap_or(0.0);
    (value, 10f64.powi(-(frac.len() as i32)))
}

const MULTIPLIERS: &[(&str, f64)] = &[
    ("millones", 1e6),
    ("millón", 1e6),
    ("millon", 1e6),
    ("millions", 1e6),
    ("million", 1e6),
    ("mil", 1e3),
    ("mm", 1e6),
    ("m", 1e6),
    ("k", 1e3),
];

fn starts_with_multiplier(rest: &[char]) -> bool {
    multiplier_after(rest).1 > 0
}

/// Detects a magnitude suffix right after a number ("K", " millones", ...),
/// returning the multiplier and how many characters it spans.
fn multiplier_after(rest: &[char]) -> (f64, usize) {
    let skip = usize::from(rest.first() == Some(&' '));
    let tail: String = rest[skip..].iter().take(10).collect::<String>().to_lowercase();

    for &(word, factor) in MULTIPLIERS {
        if let Some(after) = tail.strip_prefix(word) {
            // Must end the word ("5 meses" is not "5M"; "10 mil" is 10,000).
            if after.chars().next().is_none_or(|c| !c.is_alphabetic()) {
                // Single-letter suffixes only count when glued to the number.
                if word.len() <= 2 && skip == 1 {
                    continue;
                }
                return (factor, skip + word.chars().count());
            }
        }
    }
    (1.0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first(text: &str) -> Num {
        extract(text).into_iter().next().expect("a number")
    }

    #[test]
    fn plain_and_grouped() {
        assert!(first("$287640").matches(287640.0));
        assert!(first("$287,640.00").matches(287640.0));
        assert!(first("287.640 USD").matches(287640.0));
        assert!(first("1,620,000").matches(1_620_000.0));
        assert!(first("1.620.000").matches(1_620_000.0));
    }

    #[test]
    fn decimals_in_both_locales() {
        assert!(first("10.1%").matches(10.1));
        assert!(first("10,1%").matches(10.1));
        assert!(first("99.94%").matches(99.94));
        assert!(!first("10.1%").matches(10.3));
    }

    #[test]
    fn rounding_follows_written_precision() {
        assert!(first("$287.6K").matches(287640.0));
        assert!(first("$2.1M").matches(2_140_000.0));
        assert!(first("2,14 millones").matches(2_140_000.0));
        assert!(first("26%").matches(25.85));
        assert!(!first("$2.1M").matches(2_200_000.0));
        assert!(first("287,6 mil").matches(287640.0));
        assert!(!first("290 mil").matches(287640.0));
    }

    #[test]
    fn identifiers_and_words_are_skipped() {
        assert!(extract("Q3 Marketing Campaign").is_empty());
        let nums = extract("18 meses");
        assert_eq!(nums.len(), 1);
        assert!(nums[0].matches(18.0));
        assert!(!nums[0].matches(18e6));
    }
}
