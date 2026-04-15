use std::path::PathBuf;

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ProductionCoverageSummary {
    pub functions_total: usize,
    pub functions_called: usize,
    pub functions_never_called: usize,
    pub functions_coverage_unavailable: usize,
    pub percent_dead_in_production: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProductionCoverageVerdict {
    Clean,
    HotPathChangesNeeded,
    ColdCodeDetected,
    LicenseExpiredGrace,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProductionCoverageState {
    Called,
    NeverCalled,
    CoverageUnavailable,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProductionCoverageConfidence {
    High,
    Medium,
    Low,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProductionCoverageWatermark {
    TrialExpired,
    LicenseExpiredGrace,
    Unknown,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProductionCoverageAction {
    pub kind: String,
    pub description: String,
    pub auto_fixable: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProductionCoverageMessage {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProductionCoverageFinding {
    pub path: PathBuf,
    pub function: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub state: ProductionCoverageState,
    pub invocations: u64,
    pub confidence: ProductionCoverageConfidence,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ProductionCoverageAction>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProductionCoverageHotPath {
    pub path: PathBuf,
    pub function: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub invocations: u64,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ProductionCoverageReport {
    pub verdict: ProductionCoverageVerdict,
    pub summary: ProductionCoverageSummary,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<ProductionCoverageFinding>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hot_paths: Vec<ProductionCoverageHotPath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watermark: Option<ProductionCoverageWatermark>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<ProductionCoverageMessage>,
}
