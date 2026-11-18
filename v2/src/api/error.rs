//! The typed errors of the library calls (C2-T-006, C2-T-040).

use crate::reader::TreeError;

/// Why a call refused. Each variant is one library error of C2-T-040, and
/// [`Error::category`] and [`Error::status`] give the command line row.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// `CarveOptionError`: an unknown option, a value the option refuses, or
    /// an option that no stage of the call reads (C2-H-137).
    Option { option: String, message: String },
    /// `CarveInputError`: the source is longer than the `max_length` of its
    /// profile (C2-H-163), or than the positions of the tree can count.
    Input { profile: String, limit: usize },
    /// `CarveRenderError`: a loss under `strictLosses` (C2-H-130), one code
    /// per loss in document order.
    Render { codes: Vec<String> },
    /// `CarveTreeError`: a tree the reader refuses (C2-A-188).
    Tree(TreeError),
    /// `CarveWriteError`: a writer loss (C2-W-134) or a failing writer.
    Write { target: String, message: String },
    /// `ImportInputError`: an HTML input the importer refuses (C2-M-006).
    ImportInput { message: String },
    /// The call is part of the 0.2 library but this engine does not offer
    /// it yet. Not a row of C2-T-040; the command line reports it as `usage`.
    Unsupported { call: &'static str },
}

impl Error {
    pub(crate) fn option(option: &str, message: impl Into<String>) -> Self {
        Self::Option {
            option: option.to_string(),
            message: message.into(),
        }
    }

    /// The error name of C2-T-040, as `data/cli.json` spells it.
    #[must_use]
    pub const fn spec_name(&self) -> &'static str {
        match self {
            Self::Option { .. } | Self::Unsupported { .. } => "CarveOptionError",
            Self::Input { .. } => "CarveInputError",
            Self::Render { .. } => "CarveRenderError",
            Self::Tree(_) => "CarveTreeError",
            Self::Write { .. } => "CarveWriteError",
            Self::ImportInput { .. } => "ImportInputError",
        }
    }

    /// The failure category of the command line (C2-T-040).
    #[must_use]
    pub const fn category(&self) -> &'static str {
        match self {
            Self::Option { .. } | Self::Unsupported { .. } => "usage",
            Self::Input { .. } => "input-invalid",
            Self::Render { .. } => "render-loss",
            Self::Tree(_) => "ast-json-invalid",
            Self::Write { .. } => "writer-refused",
            Self::ImportInput { .. } => "import-input-invalid",
        }
    }

    /// The exit status of the category (C2-T-040).
    #[must_use]
    pub const fn status(&self) -> i32 {
        match self {
            Self::Render { .. } | Self::Write { .. } => 1,
            _ => 2,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Option { option, message } => write!(f, "option {option}: {message}"),
            Self::Input { profile, limit } => write!(
                f,
                "the input is longer than the {limit} bytes the profile {profile} accepts"
            ),
            Self::Render { codes } => write!(f, "the render lost content: {}", codes.join(", ")),
            Self::Tree(e) => write!(f, "{e}"),
            Self::Write { target, message } => write!(f, "writer {target}: {message}"),
            Self::ImportInput { message } => write!(f, "{message}"),
            Self::Unsupported { call } => {
                write!(f, "{call} is not available in this engine yet")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Tree(e) => Some(e),
            _ => None,
        }
    }
}

impl From<TreeError> for Error {
    fn from(e: TreeError) -> Self {
        Self::Tree(e)
    }
}

impl From<crate::RenderError> for Error {
    fn from(e: crate::RenderError) -> Self {
        match e {
            crate::RenderError::TooLong { profile, limit } => Self::Input {
                profile: profile.to_string(),
                limit,
            },
            crate::RenderError::Losses(codes) => Self::Render {
                codes: codes.into_iter().map(str::to_string).collect(),
            },
        }
    }
}
