use serde::Serialize;

/// Every command returns this so the UI always gets a message it can show,
/// rather than a panic or an opaque string.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error(
        "The archive was written by a newer version of this app (schema {found}, this version \
         supports up to {supported}). Update the app to open it. Nothing has been changed."
    )]
    DatabaseTooNew { found: i64, supported: i64 },
    #[error("Could not find your home folder, so the Claude Code session folder is unknown.")]
    NoHomeDir,
    #[error("{0}")]
    Other(String),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerialisedError<'a> {
    kind: &'a str,
    message: String,
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let kind = match self {
            AppError::Io(_) => "io",
            AppError::Db(_) => "db",
            AppError::DatabaseTooNew { .. } => "databaseTooNew",
            AppError::NoHomeDir => "noHomeDir",
            AppError::Other(_) => "other",
        };
        SerialisedError {
            kind,
            message: self.to_string(),
        }
        .serialize(serializer)
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialises_to_kind_and_message() {
        let err = AppError::from(std::io::Error::new(std::io::ErrorKind::NotFound, "gone"));
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["kind"], "io");
        assert_eq!(json["message"], "I/O error: gone");
    }
}
