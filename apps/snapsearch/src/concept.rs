//! The vocabulary shared by the mock library and the local text encoder.
//!
//! Each concept says what a thing *looks like* — a top colour and a bottom
//! colour — plus the words people use for it. `photo.rs` builds the mock
//! library out of these, and [`crate::embed::local`] turns a typed query
//! into a vector by synthesising the same kind of picture and encoding it
//! with the very same image encoder.
//!
//! **That two-way use is worth being upfront about.** It means a query of
//! "sunset" retrieving the orange photos proves the *pipeline* is correctly
//! wired — pixels in, vectors out, cosine ranking, top-k — but not that the
//! local encoder has any real-world semantic power. It cannot: it reads
//! colour, not subject matter. Point it at a real camera roll and "sunset"
//! would return anything orange, a plate of curry included. Understanding
//! what a photo is *of* is what the CLIP backend is for.

use vieww::foundation::Color;

pub struct Concept {
    /// The label the mock library shows for photos built from this.
    pub name: &'static str,
    /// Words in a query that should match it.
    pub words: &'static [&'static str],
    pub top: Color,
    pub bottom: Color,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::rgb(r, g, b)
}

/// Ten concepts, chosen to spread out in colour space so that retrieval has
/// something to actually discriminate between.
pub const CONCEPTS: &[Concept] = &[
    Concept {
        name: "sunset",
        words: &["sunset", "sunrise", "dusk", "dawn", "golden", "evening", "orange"],
        top: rgb(0xff, 0x9a, 0x3c),
        bottom: rgb(0xd6, 0x2f, 0x6d),
    },
    Concept {
        name: "ocean",
        words: &["ocean", "sea", "water", "beach", "coast", "wave", "surf", "blue"],
        top: rgb(0x7e, 0xd0, 0xf7),
        bottom: rgb(0x0b, 0x63, 0xa8),
    },
    Concept {
        name: "lake",
        words: &["lake", "river", "pond", "reflection", "calm", "teal"],
        top: rgb(0x5e, 0xd6, 0xc8),
        bottom: rgb(0x13, 0x6e, 0x8c),
    },
    Concept {
        name: "forest",
        words: &["forest", "tree", "trees", "woods", "hike", "trail", "jungle", "green", "grass", "park"],
        top: rgb(0x6f, 0xc4, 0x54),
        bottom: rgb(0x11, 0x54, 0x2e),
    },
    Concept {
        name: "mountains",
        words: &["mountain", "mountains", "peak", "alpine", "snow", "summit", "ridge"],
        top: rgb(0xcf, 0xe4, 0xf2),
        bottom: rgb(0x59, 0x67, 0x7d),
    },
    Concept {
        name: "city at night",
        words: &["city", "street", "urban", "building", "night", "dark", "downtown", "skyline"],
        top: rgb(0x2a, 0x2f, 0x55),
        bottom: rgb(0x0b, 0x0d, 0x1c),
    },
    Concept {
        name: "food",
        words: &["food", "meal", "dinner", "lunch", "breakfast", "cake", "plate", "red"],
        top: rgb(0xf2, 0x6b, 0x4e),
        bottom: rgb(0x8c, 0x1d, 0x2b),
    },
    Concept {
        name: "dog",
        words: &["dog", "puppy", "pet", "cat", "animal", "porch", "fur"],
        top: rgb(0xd8, 0xa4, 0x6a),
        bottom: rgb(0x6b, 0x45, 0x28),
    },
    Concept {
        name: "flowers",
        words: &["flower", "flowers", "garden", "bloom", "spring", "petal", "pink"],
        top: rgb(0xf9, 0xa8, 0xd4),
        bottom: rgb(0x9d, 0x28, 0x8f),
    },
    Concept {
        name: "desert",
        words: &["desert", "sand", "dune", "canyon", "arid", "tan"],
        top: rgb(0xf5, 0xd1, 0x8c),
        bottom: rgb(0xb5, 0x6f, 0x2e),
    },
];

/// Every concept a query mentions, in the order they appear in [`CONCEPTS`].
///
/// Matching is on whole lowercase alphabetic words, with a crude plural
/// trim, rather than substrings — substring matching makes "cat" fire on
/// "catalogue" and is the classic way a search like this quietly goes wrong.
pub fn concepts_in(query: &str) -> Vec<&'static Concept> {
    let words: Vec<String> = query
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect();

    CONCEPTS
        .iter()
        .filter(|concept| {
            words.iter().any(|word| {
                let stem = word.strip_suffix('s').unwrap_or(word);
                concept
                    .words
                    .iter()
                    .any(|candidate| *candidate == word || *candidate == stem)
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_can_mention_several_concepts() {
        let found = concepts_in("sunset over water");
        let names: Vec<_> = found.iter().map(|c| c.name).collect();
        assert!(names.contains(&"sunset"), "got {names:?}");
        assert!(names.contains(&"ocean"), "got {names:?}");
    }

    #[test]
    fn plurals_match_their_singular_concept() {
        assert!(!concepts_in("mountains").is_empty());
        assert!(!concepts_in("flowers in a garden").is_empty());
    }

    #[test]
    fn matching_is_by_word_not_by_substring() {
        // "cat" is a `dog`-concept word; "catalogue" must not fire it.
        assert!(
            concepts_in("catalogue").is_empty(),
            "substring matching would wrongly find the pet concept here"
        );
    }

    #[test]
    fn a_query_of_nothing_recognisable_matches_nothing() {
        assert!(concepts_in("qwertyuiop").is_empty());
    }
}
