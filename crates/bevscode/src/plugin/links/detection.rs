//! URL scanner — a port of Monaco's `linkComputer.ts` state machine (no
//! regex dep). Recognized schemes: `http://`, `https://`, `file://`.
//! Bracket/paren/curly characters balance against the char preceding the
//! link start, so URLs inside `(…)`, `[…]`, `{…}` round-trip while the
//! wrapping bracket is excluded; quotes terminate the link except when the
//! link itself is quote-wrapped; trailing `.,;:` is stripped.

/// Port of Monaco's `linkComputer.ts` state machine — yields
/// `(start_char, end_char)` pairs for every URL in `line`. Char offsets
/// are 0-based.
///
/// Recognized schemes: `http://`, `https://`, `file://`. Driven by a
/// `(state × char) → state` transition table. After reaching `Accept`,
/// force-termination characters end the link; trailing punctuation in the
/// `CannotEndIn` class (`.,;:`) is stripped; brackets/parens/braces
/// balanced against the char preceding the link don't terminate, so URLs
/// inside `(…)` or `[…]` round-trip cleanly.
pub fn find_urls(line: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut out: Vec<(usize, usize)> = Vec::new();

    let mut j = 0usize;
    let mut state = State::Start;
    let mut link_begin: usize = 0;
    let mut link_begin_ch: char = '\0';
    let mut has_open_paren = false;
    let mut has_open_square = false;
    let mut in_square = false;
    let mut has_open_curly = false;

    while j < len {
        let ch = chars[j];
        let mut reset = false;

        if state == State::Accept {
            let class = accept_state_class(
                ch,
                link_begin_ch,
                &mut has_open_paren,
                &mut has_open_square,
                &mut in_square,
                &mut has_open_curly,
            );
            if class == CharClass::ForceTermination {
                push_link(&chars, link_begin, j, &mut out);
                reset = true;
            }
        } else if state == State::End {
            let class = if ch == '[' {
                has_open_square = true;
                CharClass::None
            } else {
                classify(ch)
            };
            if class == CharClass::ForceTermination {
                reset = true;
            } else {
                state = State::Accept;
            }
        } else {
            state = next_state(state, ch);
            if state == State::Invalid {
                reset = true;
            }
        }

        if reset {
            state = State::Start;
            has_open_paren = false;
            has_open_square = false;
            in_square = false;
            has_open_curly = false;
            link_begin = j + 1;
            link_begin_ch = ch;
        }
        j += 1;
    }

    if state == State::Accept {
        push_link(&chars, link_begin, len, &mut out);
    }

    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Invalid,
    Start,
    H,
    HT,
    Htt,
    Http,
    F,
    FI,
    Fil,
    BeforeColon,
    AfterColon,
    AlmostThere,
    End,
    Accept,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CharClass {
    None,
    ForceTermination,
    CannotEndIn,
}

fn next_state(state: State, ch: char) -> State {
    match (state, ch) {
        (State::Start, 'h' | 'H') => State::H,
        (State::Start, 'f' | 'F') => State::F,
        (State::H, 't' | 'T') => State::HT,
        (State::HT, 't' | 'T') => State::Htt,
        (State::Htt, 'p' | 'P') => State::Http,
        (State::Http, 's' | 'S') => State::BeforeColon,
        (State::Http, ':') => State::AfterColon,
        (State::F, 'i' | 'I') => State::FI,
        (State::FI, 'l' | 'L') => State::Fil,
        (State::Fil, 'e' | 'E') => State::BeforeColon,
        (State::BeforeColon, ':') => State::AfterColon,
        (State::AfterColon, '/') => State::AlmostThere,
        (State::AlmostThere, '/') => State::End,
        _ => State::Invalid,
    }
}

/// Classify a character for the post-Accept walk. Mirrors Monaco's
/// `FORCE_TERMINATION_CHARACTERS` + `CANNOT_END_WITH_CHARACTERS` tables
/// (plus the Unicode CJK punctuation Monaco terminates on).
fn classify(ch: char) -> CharClass {
    match ch {
        ' ' | '\t' | '<' | '>' | '\'' | '"' | '`' | '|' | '\u{3001}' | '\u{3002}' | '\u{ff61}'
        | '\u{ff64}' | '\u{ff0c}' | '\u{ff0e}' | '\u{ff1a}' | '\u{ff1b}' | '\u{2018}'
        | '\u{3008}' | '\u{300c}' | '\u{300e}' | '\u{3014}' | '\u{ff08}' | '\u{ff3b}'
        | '\u{ff5b}' | '\u{ff62}' | '\u{ff63}' | '\u{ff5d}' | '\u{ff3d}' | '\u{ff09}'
        | '\u{3015}' | '\u{300f}' | '\u{300d}' | '\u{3009}' | '\u{2019}' | '\u{ff40}'
        | '\u{ff5e}' | '\u{2026}' => CharClass::ForceTermination,
        '.' | ',' | ';' | ':' => CharClass::CannotEndIn,
        _ => CharClass::None,
    }
}

/// Per-character class lookup while in `Accept`, with bracket/quote
/// balancing state. Updates the in/out bracket flags inline since their
/// transitions are tied to which character we're classifying.
fn accept_state_class(
    ch: char,
    link_begin_ch: char,
    has_open_paren: &mut bool,
    has_open_square: &mut bool,
    in_square: &mut bool,
    has_open_curly: &mut bool,
) -> CharClass {
    match ch {
        '(' => {
            *has_open_paren = true;
            CharClass::None
        }
        ')' => {
            if *has_open_paren {
                CharClass::None
            } else {
                CharClass::ForceTermination
            }
        }
        '[' => {
            *in_square = true;
            *has_open_square = true;
            CharClass::None
        }
        ']' => {
            *in_square = false;
            if *has_open_square {
                CharClass::None
            } else {
                CharClass::ForceTermination
            }
        }
        '{' => {
            *has_open_curly = true;
            CharClass::None
        }
        '}' => {
            if *has_open_curly {
                CharClass::None
            } else {
                CharClass::ForceTermination
            }
        }
        '\'' | '"' | '`' => {
            if link_begin_ch == ch {
                CharClass::ForceTermination
            } else if matches!(link_begin_ch, '\'' | '"' | '`') {
                CharClass::None
            } else {
                CharClass::ForceTermination
            }
        }
        '*' => {
            if link_begin_ch == '*' {
                CharClass::ForceTermination
            } else {
                CharClass::None
            }
        }
        ' ' => {
            if *in_square {
                CharClass::None
            } else {
                CharClass::ForceTermination
            }
        }
        _ => classify(ch),
    }
}

/// Emit a link covering `chars[begin..end]` after trimming `CannotEndIn`
/// trailing punctuation and shrinking by one when the link is wrapped in a
/// balanced bracket whose closer immediately follows.
fn push_link(chars: &[char], begin: usize, end: usize, out: &mut Vec<(usize, usize)>) {
    let mut last_included = end.saturating_sub(1);
    while last_included > begin && classify(chars[last_included]) == CharClass::CannotEndIn {
        last_included -= 1;
    }
    if begin > 0 && last_included > begin {
        let before = chars[begin - 1];
        let last = chars[last_included];
        let wraps = matches!((before, last), ('(', ')') | ('[', ']') | ('{', '}'));
        if wraps {
            last_included -= 1;
        }
    }
    let final_end = last_included + 1;
    if final_end > begin {
        out.push((begin, final_end));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slice(line: &str, range: (usize, usize)) -> String {
        line.chars().skip(range.0).take(range.1 - range.0).collect()
    }

    #[test]
    fn finds_http_url() {
        let line = "see http://example.com for info";
        let r = find_urls(line);
        assert_eq!(r.len(), 1);
        assert_eq!(slice(line, r[0]), "http://example.com");
    }

    #[test]
    fn finds_https_url() {
        let line = "https://example.com/path?x=1";
        let r = find_urls(line);
        assert_eq!(r.len(), 1);
        assert_eq!(slice(line, r[0]), "https://example.com/path?x=1");
    }

    #[test]
    fn strips_trailing_punctuation() {
        let line = "visit https://example.com.";
        let r = find_urls(line);
        assert_eq!(r.len(), 1);
        assert_eq!(slice(line, r[0]), "https://example.com");
    }

    #[test]
    fn ignores_bare_text() {
        let r = find_urls("no urls here even if i mention example.com");
        assert!(r.is_empty());
    }

    #[test]
    fn finds_multiple_urls() {
        let r = find_urls("a http://a.com b http://b.com");
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn balanced_parens_kept_inside() {
        let line = "see https://en.wikipedia.org/wiki/Rust_(programming_language) ok";
        let r = find_urls(line);
        assert_eq!(r.len(), 1);
        assert_eq!(
            slice(line, r[0]),
            "https://en.wikipedia.org/wiki/Rust_(programming_language)"
        );
    }

    #[test]
    fn wrapping_paren_excluded() {
        let line = "see (https://example.com) ok";
        let r = find_urls(line);
        assert_eq!(r.len(), 1);
        assert_eq!(slice(line, r[0]), "https://example.com");
    }

    #[test]
    fn finds_file_url() {
        let line = "file:///etc/hosts";
        let r = find_urls(line);
        assert_eq!(r.len(), 1);
        assert_eq!(slice(line, r[0]), "file:///etc/hosts");
    }
}
