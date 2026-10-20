use std::{error::Error, fmt, io};

#[derive(Debug)]
pub enum CoreError {
    Cancelled,
    Timeout,
    Unavailable,
    PermissionDenied,
    InvalidInput(String),
    Io(io::Error),
}

impl fmt::Display for CoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("operation cancelled"),
            Self::Timeout => formatter.write_str("operation timed out"),
            Self::Unavailable => formatter.write_str("capability unavailable"),
            Self::PermissionDenied => formatter.write_str("permission denied"),
            Self::InvalidInput(message) => write!(formatter, "invalid input: {message}"),
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
        }
    }
}

impl Error for CoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for CoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_errors_have_specific_messages_and_no_source() {
        for (error, expected) in [
            (CoreError::Cancelled, "operation cancelled"),
            (CoreError::Timeout, "operation timed out"),
            (CoreError::Unavailable, "capability unavailable"),
            (CoreError::PermissionDenied, "permission denied"),
            (
                CoreError::InvalidInput("bad value".into()),
                "invalid input: bad value",
            ),
        ] {
            assert_eq!(error.to_string(), expected);
            assert!(error.source().is_none());
        }
    }

    #[test]
    fn io_conversion_preserves_kind_raw_code_and_error_source() {
        let original = io::Error::from_raw_os_error(13);
        let original_kind = original.kind();
        let original_message = original.to_string();
        let error = CoreError::from(original);
        let source = error.source().unwrap().downcast_ref::<io::Error>().unwrap();
        assert_eq!(source.kind(), original_kind);
        assert_eq!(source.raw_os_error(), Some(13));
        assert_eq!(source.to_string(), original_message);
        assert_eq!(error.to_string(), format!("I/O error: {original_message}"));
        assert!(matches!(error, CoreError::Io(_)));
    }

    #[test]
    fn io_conversion_preserves_custom_error_payload() {
        let error = CoreError::from(io::Error::other(fmt::Error));
        let source = error.source().unwrap().downcast_ref::<io::Error>().unwrap();
        assert_eq!(source.kind(), io::ErrorKind::Other);
        assert!(source
            .get_ref()
            .unwrap()
            .downcast_ref::<fmt::Error>()
            .is_some());
    }
}
