//! Regex support for QGIS expression functions.
//!
//! Re-exports the `regex` crate (already a workspace dependency) for use by
//! `regexp_match` and `regexp_replace`.

pub use regex::{Error as RegexError, Regex};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_match() {
        let re = Regex::new("[0-9]+").unwrap();
        assert_eq!(re.find("abc123").unwrap().as_str(), "123");
    }

    #[test]
    fn test_replace_all() {
        let re = Regex::new("[0-9]+").unwrap();
        assert_eq!(re.replace_all("a1b22c333", "X"), "aXbXcX");
    }
}
