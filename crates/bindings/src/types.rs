use core::fmt::{self, Display};

#[derive(Debug, derive_more::From)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
#[cfg_attr(feature = "uniffi", uniffi::export(Debug, Display))]
pub struct Error(#[from] anyhow::Error);

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl core::error::Error for Error {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        self.0.source()
    }
}

#[cfg(feature = "napi")]
impl From<Error> for napi::JsError {
    fn from(v: Error) -> Self {
        Self::from(v.0)
    }
}

pub type Result<T> = core::result::Result<T, Error>;
