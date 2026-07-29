use crate::prelude::*;

#[derive(Debug)]
pub(crate) enum AppError
{
    /// A filesystem or OS error surfaced by the standard library.
    Io(io::Error),
    /// An external tool (e.g. 7-Zip) failed or was unavailable.
    Tool(String),
    /// The program was invoked incorrectly by the user.
    Usage(String),
    /// A higher-level failure that annotates its underlying cause with the
    /// operation that failed, preserving the original error as the source.
    Context {
        message: String,
        source: Box<AppError>,
    },
}

impl AppError
{
    /// Wrap this error with a description of the operation that produced it,
    /// keeping the original as the chained [`std::error::Error::source`].
    pub(crate) fn context(self, message: impl Into<String>) -> Self
    {
        return AppError::Context {
            message: message.into(),
            source: Box::new(self),
        };
    }

    /// The kind of failure, independent of its message. Lets callers and the
    /// CLI sink branch on the category even though `Display` stays user-facing.
    pub(crate) fn kind(&self) -> AppErrorKind
    {
        return match self {
            AppError::Io(_) => AppErrorKind::Io,
            AppError::Tool(_) => AppErrorKind::Tool,
            AppError::Usage(_) => AppErrorKind::Usage,
            AppError::Context { source, .. } => source.kind(),
        };
    }
}

/// The category of an [`AppError`], surfaced through [`AppError::kind`] so a
/// `Tool` failure and a `Usage` mistake are distinguishable even when their
/// messages read alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppErrorKind
{
    Io,
    Tool,
    Usage,
}

impl fmt::Display for AppError
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        return match self {
            AppError::Io(err) => write!(f, "{err}"),
            AppError::Tool(message) | AppError::Usage(message) => write!(f, "{message}"),
            AppError::Context { message, source } => write!(f, "{message}: {source}"),
        };
    }
}

impl std::error::Error for AppError
{
    // rust-lifetime: allow: the standard Error trait requires source errors to be object-safe and static
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)>
    {
        return match self {
            AppError::Io(err) => Some(err),
            AppError::Context { source, .. } => Some(source.as_ref()),
            AppError::Tool(_) | AppError::Usage(_) => None,
        };
    }
}

impl From<io::Error> for AppError
{
    fn from(value: io::Error) -> Self
    {
        return AppError::Io(value);
    }
}

/// Attach operation context to a fallible result, chaining the original error
/// as the source. Works for any error convertible into [`AppError`], including
/// `io::Error` and `AppError` itself.
pub(crate) trait ErrorContext<Output>
{
    /// Add an eagerly-built context message. Prefer [`ErrorContext::with_context`]
    /// on hot paths so the message is only formatted when an error occurs.
    fn context(self, message: impl Into<String>) -> Result<Output, AppError>;

    /// Add a context message that is only built when the result is an error.
    fn with_context<F, S>(self, message: F) -> Result<Output, AppError>
    where
        F: FnOnce() -> S,
        S: Into<String>,;
}

impl<Output, Failure> ErrorContext<Output> for Result<Output, Failure>
where
    Failure: Into<AppError>,
{
    fn with_context<F, S>(self, message: F) -> Result<Output, AppError>
    where
        F: FnOnce() -> S,
        S: Into<String>,
    {
        return self.map_err(|err| err.into().context(message()));
    }

    fn context(self, message: impl Into<String>) -> Result<Output, AppError>
    {
        return self.map_err(|err| err.into().context(message));
    }
}

#[cfg(test)]
mod tests
{
    use super::*;
    use std::error::Error;

    #[test]
    fn context_chains_source_and_prefixes_display()
    {
        let io = io::Error::new(io::ErrorKind::NotFound, "no such file");
        let err = AppError::from(io).context("reading install lock");

        assert_eq!(err.to_string(), "reading install lock: no such file");
        let source = err.source().expect("context error keeps a source");
        assert!(source.to_string().contains("no such file"));
    }

    #[test]
    fn result_with_context_only_wraps_errors()
    {
        const SUCCESS_VALUE: u8 = 7;
        let ok: Result<u8, io::Error> = Ok(SUCCESS_VALUE);
        assert_eq!(
            ok.with_context(|| "should not run")
                .expect("the test fixture is created before this assertion reads it"),
            SUCCESS_VALUE
        );

        let failed: Result<(), io::Error> =
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
        let err = failed
            .with_context(|| format!("writing {}", "backup"))
            .unwrap_err();
        assert_eq!(err.to_string(), "writing backup: denied");
    }

    #[test]
    fn kind_sees_through_context_wrapping()
    {
        let usage = AppError::Usage("bad flag".to_string());
        assert_eq!(usage.kind(), AppErrorKind::Usage);

        let wrapped = AppError::from(io::Error::other("boom")).context("during install");
        assert_eq!(wrapped.kind(), AppErrorKind::Io);

        let tool = AppError::Tool("7-Zip missing".to_string());
        assert_ne!(tool.kind(), AppErrorKind::Usage);
    }
}
