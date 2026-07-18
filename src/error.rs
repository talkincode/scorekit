use serde_json::json;

/// Source location inside a scene file (1-based).
#[derive(Debug, Clone, Copy)]
pub struct Location {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot access `{path}`: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid scene: {message}")]
    Parse {
        message: String,
        location: Option<Location>,
    },
    #[error("invalid value at `{path}`: {message}")]
    Validation { path: String, message: String },
    #[error("{count} grammar violation(s) against `{grammar}`")]
    Lint {
        grammar: String,
        count: usize,
        /// Pre-rendered porcelain lines, one per violation.
        porcelain: Vec<String>,
        /// Structured violations for `--json`.
        json: Vec<serde_json::Value>,
    },
    #[error("{count} renderer profile check failure(s) in `{profile}`")]
    ProfileCheck {
        profile: String,
        count: usize,
        status_code: u8,
        /// Pre-rendered human-readable lines, one per failed patch.
        porcelain: Vec<String>,
        /// Complete machine-readable report, including passing patches.
        report: serde_json::Value,
    },
    #[error("environment is not ready for audio builds")]
    Doctor {
        /// Complete human-readable diagnostics and platform-specific help.
        porcelain: Vec<String>,
        /// Machine-readable environment report.
        report: serde_json::Value,
    },
    #[error("missing dependency `{tool}`: {hint}")]
    MissingDependency { tool: String, hint: String },
    #[error("`{tool}` failed ({status}): {stderr}")]
    ToolFailure {
        tool: String,
        status: String,
        stderr: String,
    },
}

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Error::Io { .. } => "io",
            Error::Parse { .. } => "parse",
            Error::Validation { .. } => "validation",
            Error::Lint { .. } => "lint",
            Error::ProfileCheck { .. } => "profile_check",
            Error::Doctor { .. } => "doctor",
            Error::MissingDependency { .. } => "missing_dependency",
            Error::ToolFailure { .. } => "tool_failure",
        }
    }

    /// Stable exit codes: 1 io, 2 invalid input, 3 missing dependency, 4 external tool failure.
    pub fn exit_code(&self) -> u8 {
        match self {
            Error::Io { .. } => 1,
            Error::Parse { .. } | Error::Validation { .. } | Error::Lint { .. } => 2,
            Error::ProfileCheck { status_code, .. } => *status_code,
            Error::Doctor { .. } => 3,
            Error::MissingDependency { .. } => 3,
            Error::ToolFailure { .. } => 4,
        }
    }

    /// Print the error to stderr, machine-readable when `json` is set.
    pub fn report(&self, json: bool) {
        if let Error::Lint {
            porcelain,
            json: violations,
            ..
        } = self
        {
            if json {
                let payload = json!({
                    "code": self.code(),
                    "message": self.to_string(),
                    "violations": violations,
                    "exit_code": self.exit_code(),
                });
                eprintln!("{payload}");
            } else {
                for line in porcelain {
                    eprintln!("{line}");
                }
                eprintln!("error[{}]: {self}", self.code());
            }
            return;
        }
        if let Error::ProfileCheck {
            porcelain, report, ..
        } = self
        {
            if json {
                let payload = json!({
                    "code": self.code(),
                    "message": self.to_string(),
                    "report": report,
                    "exit_code": self.exit_code(),
                });
                eprintln!("{payload}");
            } else {
                for line in porcelain {
                    eprintln!("{line}");
                }
                eprintln!("error[{}]: {self}", self.code());
            }
            return;
        }
        if let Error::Doctor {
            porcelain, report, ..
        } = self
        {
            if json {
                let payload = json!({
                    "code": self.code(),
                    "message": self.to_string(),
                    "report": report,
                    "exit_code": self.exit_code(),
                });
                eprintln!("{payload}");
            } else {
                for line in porcelain {
                    eprintln!("{line}");
                }
                eprintln!("error[{}]: {self}", self.code());
            }
            return;
        }
        if json {
            let location = match self {
                Error::Parse {
                    location: Some(loc),
                    ..
                } => json!({ "line": loc.line, "column": loc.column }),
                _ => json!(null),
            };
            let field = match self {
                Error::Validation { path, .. } => json!(path),
                _ => json!(null),
            };
            let payload = json!({
                "code": self.code(),
                "message": self.to_string(),
                "location": location,
                "field": field,
                "exit_code": self.exit_code(),
            });
            eprintln!("{payload}");
        } else {
            match self {
                Error::Parse {
                    location: Some(loc),
                    ..
                } => eprintln!(
                    "error[{}]: {self} (line {}, column {})",
                    self.code(),
                    loc.line,
                    loc.column
                ),
                _ => eprintln!("error[{}]: {self}", self.code()),
            }
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
