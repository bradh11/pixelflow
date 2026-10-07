//! Plain-language validation issues shared by model and mapping checks.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Warning,
    Error,
}

/// Machine-readable issue kind, so the UI can offer one-click fixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IssueCode {
    // Structural checks (pf-model).
    DuplicateId,
    UnknownPropReference,
    RegionOutOfBounds,
    DuplicateRegionName,
    InvalidRegion,
    SegmentOutOfBounds,
    EmptyProp,
    InvalidBrightness,
    InvalidFrameRate,
    InvalidGamma,
    LimitExceeded,
    /// A prop's shape settings PixelFlow can't build (a number that isn't finite, a custom
    /// grid whose cells don't match its size, a setting out of range).
    InvalidShape,
    /// A prop's position, rotation or scale isn't a finite number.
    InvalidTransform,
    InvalidBackground,
    InvalidHouseModel,
    /// Port 0: ports are numbered from 1, as printed on the controller.
    InvalidPortNumber,
    /// Two ports on one controller with the same number.
    DuplicatePort,
    /// A controller with no IP address or hostname.
    MissingAddress,
    /// A controller address PixelFlow can't send to: an IPv6 address, or one with a space in it.
    InvalidAddress,
    // Wiring checks (pf-mapping).
    PortOverCapacity,
    UnassignedNodes,
    NodeAssignedTwice,
    UniverseOutOfRange,
    UniverseCollision,
}

/// One problem found in a show, written for end users.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Issue {
    pub severity: Severity,
    pub code: IssueCode,
    pub message: String,
    /// Suggested fix, in plain language.
    pub fix: Option<String>,
}

impl Issue {
    pub fn error(code: IssueCode, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code,
            message: message.into(),
            fix: None,
        }
    }

    pub fn warning(code: IssueCode, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code,
            message: message.into(),
            fix: None,
        }
    }

    pub fn with_fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }
}

/// A list of issues.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ValidationReport {
    pub issues: Vec<Issue>,
}

impl ValidationReport {
    pub fn push(&mut self, issue: Issue) {
        self.issues.push(issue);
    }

    pub fn extend(&mut self, other: ValidationReport) {
        self.issues.extend(other.issues);
    }

    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::Error)
    }

    pub fn count(&self, severity: Severity) -> usize {
        self.issues.iter().filter(|i| i.severity == severity).count()
    }

    pub fn has_code(&self, code: IssueCode) -> bool {
        self.issues.iter().any(|i| i.code == code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_counts_by_severity() {
        let mut report = ValidationReport::default();
        assert!(!report.has_errors());
        report.push(Issue::warning(IssueCode::EmptyProp, "empty").with_fix("add pixels"));
        assert!(!report.has_errors());
        report.push(Issue::error(IssueCode::DuplicateId, "dup"));
        assert!(report.has_errors());
        assert_eq!(report.count(Severity::Warning), 1);
        assert_eq!(report.count(Severity::Error), 1);
        assert!(report.has_code(IssueCode::DuplicateId));
        assert_eq!(report.issues[0].fix.as_deref(), Some("add pixels"));
    }
}
