//! Human names, parsed.
//!
//! `authors` is `Vec<String>` exactly as an origin spelled it — calibre and
//! Goodreads write `Borges, Jorge Luis`, a bare epub writes `Jorge Luis
//! Borges`, and a mononym writes `Colette`. Two questions follow from that and
//! neither is phrasing:
//!
//! - **What do you file this under?** [`last_name`]. "Alphabetically by last
//!   name" is a parse of a human name, and SQLite has nothing to parse one
//!   with.
//! - **Which half is the surname?** [`display_order`]. Flipping `Surname,
//!   Given` is the same parse read the other way.
//!
//! Both lived in `crates/tui/src/ui/library.rs` until item 17, where they were
//! one frontend's private knowledge — and a GUI without them files *The
//! Overstory* under nothing and prints `Borges, Jorge Luis` on a book jacket.
//! **Joining a list of names with `", "` is still the frontend's**: that is
//! wording, and this module returns names one at a time on purpose.
//!
//! Nothing here is a lookup against a table of real people. Every rule is
//! structural, every one of them is wrong for somebody, and the failure mode
//! that matters is the one that files a book under nothing.

/// Name particles that belong to the surname rather than to the given names —
/// "Ursula K. **Le Guin**", "Vincent **van Gogh**", "Simone **de Beauvoir**".
///
/// Matched case-insensitively, because the same particle is capitalised in one
/// language and not in the next, and a person writing their own name does not
/// consult a table.
const PARTICLES: [&str; 18] = [
    "de", "del", "della", "der", "den", "di", "da", "du", "la", "le", "van", "von", "ten", "ter",
    "af", "av", "bin", "ibn",
];

/// Honorific and generational suffixes, which are never the name to file under.
const SUFFIXES: [&str; 8] = ["jr", "jr.", "sr", "sr.", "ii", "iii", "iv", "phd"];

fn is_suffix(word: &str) -> bool {
    SUFFIXES.contains(&word.trim_matches(',').to_lowercase().as_str())
}

/// True when `after` — everything past the first comma — is nothing but
/// suffixes, which makes the comma a *suffix* comma rather than an inversion.
///
/// "Martin Luther King, Jr." is the same name as "Martin Luther King" and files
/// under King, not under Martin. Both readers of a comma need this answer and
/// they must not disagree about it, which is why it is one function.
fn comma_is_only_a_suffix(after: &str) -> bool {
    after.split_whitespace().all(is_suffix)
}

/// The name to file an author under.
///
/// Three shapes, in the order they are decided:
///
/// 1. **`Surname, Given`** — the author already said which half is the surname,
///    so nothing is guessed. This is the form a Goodreads export and half the
///    providers use, so it is worth handling first rather than as a curiosity.
/// 2. **Suffixes are dropped** — "Kurt Vonnegut Jr." files under Vonnegut.
/// 3. **The last word, plus any particle in front of it.** "Emily St. John
///    Mandel" → Mandel, because "John" is not a particle; "Ursula K. Le Guin"
///    → Le Guin, because "Le" is.
///
/// A single word is itself: a mononym is a surname as far as this is concerned,
/// and the alternative is filing Homer under nothing.
pub fn last_name(author: &str) -> String {
    let author = author.trim();
    if let Some((head, tail)) = author.split_once(',') {
        let head = head.trim();
        // **Unless the comma is only holding a suffix.** See
        // [`comma_is_only_a_suffix`] — a tail that is nothing but suffixes is
        // not the inverted form, however much it looks like one from the comma.
        if !head.is_empty() && !comma_is_only_a_suffix(tail) {
            return head.to_string();
        }
    }
    // Commas are separators here, not part of a word: the only ones left are
    // the suffix-holding kind the branch above declined.
    let mut words: Vec<&str> = author
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|w| !w.is_empty())
        .collect();
    while words.len() > 1 && is_suffix(words[words.len() - 1]) {
        words.pop();
    }
    match words.len() {
        0 => String::new(),
        1 => words[0].to_string(),
        n => {
            let particle = PARTICLES.contains(&words[n - 2].to_lowercase().as_str());
            if particle {
                format!("{} {}", words[n - 2], words[n - 1])
            } else {
                words[n - 1].to_string()
            }
        }
    }
}

/// The name written the way a person says it: `Jorge Luis Borges`, never
/// `Borges, Jorge Luis`.
///
/// The **inverse** of the question [`last_name`] answers, off the same reading
/// of the comma — which is the reason they are neighbours. A frontend that flips
/// the pair itself is a frontend that will eventually disagree with the sort
/// about which half was the surname, and the disagreement looks like a bug in
/// the sort.
///
/// Four inputs it must not mangle, all of them real in `make dev-db`:
///
/// - **A mononym** — `Colette` — has no comma and is returned untouched.
/// - **An uninverted name** — `Jorge Luis Borges` — likewise. There is no
///   attempt to detect and "fix" one; a name with no comma has told us nothing
///   about which half is the surname, and rearranging it on a guess is how
///   `Emily St. John Mandel` becomes `Mandel Emily St. John`.
/// - **A suffix comma** — `Martin Luther King, Jr.` — is not an inversion, and
///   is returned untouched rather than read as a surname of "King".
/// - **A suffix past an inversion** — `King, Martin Luther, Jr.` — puts the
///   suffix back where it was: `Martin Luther King, Jr.`.
///
/// An empty string stays empty. Absence is the caller's to word, not this
/// module's to invent a placeholder for.
pub fn display_order(author: &str) -> String {
    let author = author.trim();
    let Some((head, rest)) = author.split_once(',') else {
        return author.to_string();
    };
    let head = head.trim();
    if head.is_empty() {
        return author.to_string();
    }

    // Everything past the first comma, as comma-separated segments: the given
    // names, then any suffixes. `King, Martin Luther, Jr.` is two segments and
    // is the reason this is not a second `split_once`.
    let mut segments: Vec<&str> = rest
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let mut suffixes: Vec<&str> = Vec::new();
    while segments.last().is_some_and(|s| comma_is_only_a_suffix(s)) {
        suffixes.push(segments.pop().expect("checked by is_some_and"));
    }
    suffixes.reverse();

    let given = segments.join(", ");
    if given.is_empty() {
        // Nothing but suffixes past the comma — a suffix comma, not an
        // inversion. Untouched, exactly as `last_name` treats it.
        return author.to_string();
    }

    let mut out = format!("{given} {head}");
    for s in suffixes {
        out.push_str(", ");
        out.push_str(s);
    }
    out
}

/// The sort key for "alphabetically by author": `(no-author flag, last name,
/// full name)`.
///
/// The flag is what keeps an authorless book at the bottom rather than at the
/// top under an empty string — a book with no author is not a book by an author
/// called "". The full name breaks ties between two people who share a surname,
/// and it is the *stored* spelling rather than the display order so that a tie
/// is broken by something stable rather than by this module's own parse.
///
/// Takes the whole list and reads the first: a book's authors arrive in the
/// order its origin listed them, which is the order that means "the one whose
/// name is on the spine".
pub fn sort_key(authors: &[String]) -> (u8, String, String) {
    match authors.iter().find(|a| !a.trim().is_empty()) {
        Some(a) => (0, last_name(a).to_lowercase(), a.trim().to_lowercase()),
        None => (1, String::new(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_an_inverted_name_under_the_half_it_named() {
        assert_eq!(last_name("Borges, Jorge Luis"), "Borges");
        assert_eq!(last_name("Le Guin, Ursula K."), "Le Guin");
    }

    #[test]
    fn files_a_plain_name_under_its_last_word() {
        assert_eq!(last_name("Emily St. John Mandel"), "Mandel");
        assert_eq!(last_name("Richard Powers"), "Powers");
    }

    #[test]
    fn a_particle_travels_with_the_surname() {
        assert_eq!(last_name("Ursula K. Le Guin"), "Le Guin");
        assert_eq!(last_name("Simone de Beauvoir"), "de Beauvoir");
        assert_eq!(last_name("Vincent van Gogh"), "van Gogh");
    }

    #[test]
    fn suffixes_are_never_the_filing_name() {
        assert_eq!(last_name("Kurt Vonnegut Jr."), "Vonnegut");
        // The comma holds a suffix, not an inversion.
        assert_eq!(last_name("Martin Luther King, Jr."), "King");
    }

    #[test]
    fn a_mononym_is_its_own_surname() {
        assert_eq!(last_name("Colette"), "Colette");
        assert_eq!(last_name("Homer"), "Homer");
        assert_eq!(last_name(""), "");
        assert_eq!(last_name("   "), "");
    }

    /// The shapes a real library actually contains, kept verbatim from the TUI
    /// test this module replaced. Every one is a book somebody owns rather than
    /// an invented edge case, which is the only reason to handle any of them.
    #[test]
    fn a_last_name_survives_particles_initials_and_suffixes() {
        assert_eq!(last_name("Min Jin Lee"), "Lee");
        assert_eq!(last_name("Mandel, Emily St. John"), "Mandel");
        assert_eq!(last_name("King, Martin Luther Jr."), "King");
        // A trailing comma with nothing after it is a typo, not an inversion.
        assert_eq!(last_name("Min Jin Lee,"), "Lee");
    }

    /// A name that is *only* a suffix keeps it. The loop stops at one word left,
    /// so there is no input that strips a name down to nothing.
    #[test]
    fn stripping_suffixes_never_empties_a_name() {
        assert_eq!(last_name("Jr."), "Jr.");
        assert_eq!(last_name("III"), "III");
    }

    #[test]
    fn display_order_flips_only_a_real_inversion() {
        assert_eq!(display_order("Borges, Jorge Luis"), "Jorge Luis Borges");
        assert_eq!(display_order("Le Guin, Ursula K."), "Ursula K. Le Guin");
        // No comma: nothing has been said about which half is the surname, and
        // guessing is how a three-part name gets mangled.
        assert_eq!(
            display_order("Emily St. John Mandel"),
            "Emily St. John Mandel"
        );
        assert_eq!(display_order("Colette"), "Colette");
        assert_eq!(display_order(""), "");
    }

    #[test]
    fn display_order_keeps_a_suffix_where_it_belongs() {
        // A suffix comma is not an inversion.
        assert_eq!(
            display_order("Martin Luther King, Jr."),
            "Martin Luther King, Jr."
        );
        // An inversion *with* a suffix past it puts the suffix back at the end.
        assert_eq!(
            display_order("King, Martin Luther, Jr."),
            "Martin Luther King, Jr."
        );
    }

    #[test]
    fn an_authorless_book_sorts_last_rather_than_under_nothing() {
        let none: Vec<String> = Vec::new();
        assert_eq!(sort_key(&none).0, 1);
        // A blank string is absence wearing a value, and is treated as one.
        assert_eq!(sort_key(&["   ".to_string()]).0, 1);
        assert_eq!(sort_key(&["Colette".to_string()]).0, 0);
    }

    #[test]
    fn sort_key_files_under_the_first_named_author() {
        let k = sort_key(&["Le Guin, Ursula K.".to_string(), "Someone Else".to_string()]);
        assert_eq!(k.1, "le guin");
    }
}

#[cfg(test)]
mod props {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Arbitrary input must be parsed, not panic. Author strings arrive from
        /// four importers and a text field, and none of them promise a shape.
        #[test]
        fn arbitrary_input_never_panics(s in ".{0,80}") {
            let _ = last_name(&s);
            let _ = display_order(&s);
            let _ = sort_key(&[s]);
        }

        /// A name with no comma has told us nothing about which half is the
        /// surname, so [`display_order`] must be the identity on it (modulo the
        /// trim). This is the property that stops a future "smarter" version
        /// rearranging `Emily St. John Mandel`.
        #[test]
        fn display_order_never_rearranges_an_uninverted_name(s in "[A-Za-z. ]{0,40}") {
            prop_assert_eq!(display_order(&s), s.trim().to_string());
        }

        /// The two readings of a comma agree. Whatever [`display_order`]
        /// produces, filing *it* gives the same answer as filing the original —
        /// which is the whole reason they share `comma_is_only_a_suffix`. A
        /// version where they disagree is a library whose sort and whose labels
        /// name different surnames.
        #[test]
        fn filing_survives_the_flip(
            given in "[A-Za-z][a-z]{1,8}",
            surname in "[A-Za-z][a-z]{1,8}",
        ) {
            // Scoped honestly. A *given* name that is also a particle ("de") and
            // a *surname* that is also a suffix ("Ii") are both genuinely
            // ambiguous once the comma is gone — `de Surname` files under "de
            // Surname" and it is right to, because that is what the string now
            // says. The round trip is a property of the comma, not a claim that
            // the flip is lossless for every input.
            prop_assume!(!PARTICLES.contains(&given.to_lowercase().as_str()));
            prop_assume!(!is_suffix(&given) && !is_suffix(&surname));
            let inverted = format!("{surname}, {given}");
            prop_assert_eq!(last_name(&inverted), surname.clone());
            prop_assert_eq!(last_name(&display_order(&inverted)), surname);
        }
    }
}
