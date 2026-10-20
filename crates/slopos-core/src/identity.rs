use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApplicationId(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationIdError {
    Empty,
    TooLong { length: usize },
    MissingSeparator,
    EmptySegment { segment: usize },
    LeadingDigit { segment: usize },
    InvalidCharacter { index: usize, character: char },
}

impl ApplicationId {
    pub fn new(value: impl Into<String>) -> Result<Self, ApplicationIdError> {
        let value = value.into();
        if value.is_empty() {
            return Err(ApplicationIdError::Empty);
        }
        if value.len() > 255 {
            return Err(ApplicationIdError::TooLong {
                length: value.len(),
            });
        }
        if !value.contains('.') {
            return Err(ApplicationIdError::MissingSeparator);
        }
        let mut offset = 0;
        for (segment, part) in value.split('.').enumerate() {
            if part.is_empty() {
                return Err(ApplicationIdError::EmptySegment { segment });
            }
            if part.as_bytes()[0].is_ascii_digit() {
                return Err(ApplicationIdError::LeadingDigit { segment });
            }
            for (index, character) in part.char_indices() {
                if !character.is_ascii_alphanumeric() && character != '_' && character != '-' {
                    return Err(ApplicationIdError::InvalidCharacter {
                        index: offset + index,
                        character,
                    });
                }
            }
            offset += part.len() + 1;
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ApplicationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl fmt::Display for ApplicationIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("application ID is empty"),
            Self::TooLong { length } => {
                write!(
                    formatter,
                    "application ID is {length} bytes; maximum is 255"
                )
            }
            Self::MissingSeparator => {
                formatter.write_str("application ID requires at least two dot-separated segments")
            }
            Self::EmptySegment { segment } => {
                write!(
                    formatter,
                    "application ID segment {segment} is empty (zero-based index)"
                )
            }
            Self::LeadingDigit { segment } => {
                write!(
                    formatter,
                    "application ID segment {segment} starts with a digit (zero-based index)"
                )
            }
            Self::InvalidCharacter { index, character } => {
                write!(
                    formatter,
                    "invalid application ID character {character:?} at byte {index}"
                )
            }
        }
    }
}

impl Error for ApplicationIdError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashSet};

    #[test]
    fn valid_ids_preserve_spelling_without_requiring_reverse_dns() {
        for value in ["a.b", "App._part-2", "-.a", "_.B9", "org.example.App"] {
            let identity = ApplicationId::new(value).unwrap();
            assert_eq!(identity.as_str(), value);
            assert_eq!(identity.to_string(), value);
            assert!(format!("{identity:?}").contains(value));
        }
    }

    #[test]
    fn invalid_ids_report_exact_errors() {
        for (value, expected) in [
            ("", ApplicationIdError::Empty),
            ("App", ApplicationIdError::MissingSeparator),
            (".App", ApplicationIdError::EmptySegment { segment: 0 }),
            ("App.", ApplicationIdError::EmptySegment { segment: 1 }),
            ("a..b", ApplicationIdError::EmptySegment { segment: 1 }),
            ("1a.b", ApplicationIdError::LeadingDigit { segment: 0 }),
            ("a.2b", ApplicationIdError::LeadingDigit { segment: 1 }),
            (
                "a.b c",
                ApplicationIdError::InvalidCharacter {
                    index: 3,
                    character: ' ',
                },
            ),
            (
                "a.b/c",
                ApplicationIdError::InvalidCharacter {
                    index: 3,
                    character: '/',
                },
            ),
            (
                "a.b\0",
                ApplicationIdError::InvalidCharacter {
                    index: 3,
                    character: '\0',
                },
            ),
            (
                "a.é",
                ApplicationIdError::InvalidCharacter {
                    index: 2,
                    character: 'é',
                },
            ),
            (
                ":a.b",
                ApplicationIdError::InvalidCharacter {
                    index: 0,
                    character: ':',
                },
            ),
        ] {
            let error = ApplicationId::new(value).unwrap_err();
            assert_eq!(error, expected);
            assert!(!error.to_string().is_empty());
            assert!(error.source().is_none());
        }
    }

    #[test]
    fn length_limit_is_255_bytes() {
        let accepted = format!("a.{}", "b".repeat(253));
        assert_eq!(ApplicationId::new(accepted).unwrap().as_str().len(), 255);
        let rejected = format!("a.{}", "b".repeat(254));
        assert_eq!(
            ApplicationId::new(rejected),
            Err(ApplicationIdError::TooLong { length: 256 })
        );
        let multibyte = format!("a.{}", "é".repeat(127));
        assert_eq!(
            ApplicationId::new(multibyte),
            Err(ApplicationIdError::TooLong { length: 256 })
        );
    }

    #[test]
    fn equality_order_and_hash_use_exact_identity() {
        let lower = ApplicationId::new("a.b").unwrap();
        let equal = ApplicationId::new(String::from("a.b")).unwrap();
        let upper = ApplicationId::new("A.b").unwrap();
        assert_eq!(lower, equal);
        assert!(upper < lower);
        let identities = [lower, equal, upper];
        assert_eq!(identities.iter().cloned().collect::<HashSet<_>>().len(), 2);
        assert_eq!(identities.into_iter().collect::<BTreeSet<_>>().len(), 2);
    }
}
