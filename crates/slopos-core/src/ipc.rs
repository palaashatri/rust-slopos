use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IpcObjectPath(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcObjectPathError {
    MissingLeadingSlash,
    EmptyElement { segment: usize },
    InvalidCharacter { index: usize, character: char },
}

impl IpcObjectPath {
    pub fn new(value: impl Into<String>) -> Result<Self, IpcObjectPathError> {
        let value = value.into();
        if !value.starts_with('/') {
            return Err(IpcObjectPathError::MissingLeadingSlash);
        }
        if value == "/" {
            return Ok(Self(value));
        }

        let mut offset = 1;
        for (segment, element) in value[1..].split('/').enumerate() {
            if element.is_empty() {
                return Err(IpcObjectPathError::EmptyElement { segment });
            }
            for (index, character) in element.char_indices() {
                if !character.is_ascii_alphanumeric() && character != '_' {
                    return Err(IpcObjectPathError::InvalidCharacter {
                        index: offset + index,
                        character,
                    });
                }
            }
            offset += element.len() + 1;
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IpcObjectPathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingLeadingSlash => formatter.write_str("object path must start with '/'")?,
            Self::EmptyElement { segment } => {
                write!(formatter, "object path segment {segment} is empty")?
            }
            Self::InvalidCharacter { index, character } => write!(
                formatter,
                "invalid object path character {character:?} at byte {index}"
            )?,
        }
        Ok(())
    }
}

impl Error for IpcObjectPathError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_paths_enforce_dbus_syntax() {
        for path in ["/", "/org/slopos/Shell", "/org/example_1/device2"] {
            assert_eq!(IpcObjectPath::new(path).unwrap().as_str(), path);
        }
        assert!(
            IpcObjectPath::new(format!("/{}", "segment/".repeat(40)).trim_end_matches('/')).is_ok()
        );

        for (path, expected) in [
            ("", IpcObjectPathError::MissingLeadingSlash),
            ("org/slopos", IpcObjectPathError::MissingLeadingSlash),
            ("//org", IpcObjectPathError::EmptyElement { segment: 0 }),
            ("/org/", IpcObjectPathError::EmptyElement { segment: 1 }),
            (
                "/org/name-part",
                IpcObjectPathError::InvalidCharacter {
                    index: 9,
                    character: '-',
                },
            ),
            (
                "/org/é",
                IpcObjectPathError::InvalidCharacter {
                    index: 5,
                    character: 'é',
                },
            ),
        ] {
            assert_eq!(IpcObjectPath::new(path), Err(expected));
        }
    }
}
