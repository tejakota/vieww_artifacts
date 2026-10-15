//! The gallery's command line, parsed once.
//!
//! # Why this exists
//!
//! `main` took "the first argument" as the output directory and "the second
//! argument that is not a flag" as the filter, and `census` separately took
//! "the first argument that is not a flag" as *its* output directory. That
//! held while every flag was a bare word. The first flag with a value
//! (`--receipt run.json`) breaks both readings: `main` would write PNGs into
//! a directory called `--receipt`, and `census` would write its diffs into a
//! directory called `run.json`. So the arguments are now read in one place,
//! and both modes take the same [`Options`].
//!
//! ```console
//! fixtures [OUT] [FILTER] [--census] [--receipt FILE] [--shadow FILE] [--runs N]
//! ```

use std::path::PathBuf;

/// How many timed runs a still fixture gets when `--runs` is not given.
///
/// Small on purpose — see `runner::RUNS`' history: the gallery exists to find
/// costs that differ by orders of magnitude. A receipt someone intends to
/// compare across machines should pass `--runs 30` or more, so its p50 is a
/// median of something.
pub(crate) const DEFAULT_RUNS: usize = 3;

#[derive(Debug, Clone)]
pub(crate) struct Options {
    /// Where PNGs (gallery) or diff triptychs (census) go.
    pub out: Option<PathBuf>,
    /// A substring of fixture names; see `main`'s comment for why.
    pub only: Option<String>,
    pub census: bool,
    /// Write a machine-readable receipt of this run here.
    pub receipt: Option<PathBuf>,
    /// Run the hybrid planner in shadow mode beside every timed frame and
    /// write one JSON line per fixture here. See `shadow.rs`.
    pub shadow: Option<PathBuf>,
    /// Timed runs per fixture, when given. The gallery defaults to
    /// [`DEFAULT_RUNS`] and the census to its own, larger, number.
    pub runs: Option<usize>,
}

impl Options {
    pub(crate) fn from_env() -> Result<Self, String> {
        Self::parse(std::env::args().skip(1))
    }

    pub(crate) fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut options = Self {
            out: None,
            only: None,
            census: false,
            receipt: None,
            shadow: None,
            runs: None,
        };
        let mut positional = Vec::new();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let mut value = |flag: &str| args.next().ok_or_else(|| format!("{flag} needs a value"));
            match arg.as_str() {
                "--census" => options.census = true,
                "--receipt" => options.receipt = Some(value("--receipt")?.into()),
                "--shadow" => options.shadow = Some(value("--shadow")?.into()),
                "--runs" => {
                    let raw = value("--runs")?;
                    options.runs = Some(
                        raw.parse::<usize>()
                            .ok()
                            .filter(|n| *n > 0)
                            .ok_or_else(|| {
                                format!("--runs takes a positive integer, not {raw:?}")
                            })?,
                    );
                }
                flag if flag.starts_with("--") => {
                    return Err(format!("unknown flag {flag}"));
                }
                _ => positional.push(arg),
            }
        }
        let mut positional = positional.into_iter();
        options.out = positional.next().map(PathBuf::from);
        options.only = positional.next();
        if let Some(extra) = positional.next() {
            return Err(format!("unexpected argument {extra:?}"));
        }
        Ok(options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, String> {
        Options::parse(args.iter().map(|s| (*s).to_owned()))
    }

    #[test]
    fn a_flag_value_is_never_mistaken_for_the_output_directory() {
        let o = parse(&["--receipt", "run.json", "out", "editor"]).unwrap();
        assert_eq!(o.receipt.as_deref(), Some(std::path::Path::new("run.json")));
        assert_eq!(o.out.as_deref(), Some(std::path::Path::new("out")));
        assert_eq!(o.only.as_deref(), Some("editor"));
    }

    #[test]
    fn the_historical_forms_still_mean_what_they_meant() {
        let o = parse(&["out", "23-", "--census"]).unwrap();
        assert!(o.census);
        assert_eq!(o.only.as_deref(), Some("23-"));
        let o = parse(&[]).unwrap();
        assert!(o.out.is_none() && o.only.is_none());
        assert_eq!(o.runs, None);
        assert_eq!(parse(&["--runs", "30"]).unwrap().runs, Some(30));
    }

    #[test]
    fn bad_input_is_refused_rather_than_guessed_at() {
        assert!(parse(&["--runs", "0"]).is_err());
        assert!(parse(&["--runs"]).is_err());
        assert!(parse(&["--gpu-please"]).is_err());
        assert!(parse(&["a", "b", "c"]).is_err());
    }
}
