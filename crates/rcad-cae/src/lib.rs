//! External CAE solver adapters.
//!
//! Analyses are executed as subprocess jobs (CalculiX, OpenFOAM, …). This crate
//! defines the request/result types and a trait for adapters; concrete runners
//! land post-MVP.

use std::path::PathBuf;

use rcad_geom::TriangleMesh;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum CaeError {
    #[error("{0}")]
    Message(String),
    #[error("solver not installed: {0}")]
    NotInstalled(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnalysisKind {
    Structural,
    Fatigue,
    Thermal,
    Fluid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRequest {
    pub id: Uuid,
    pub kind: AnalysisKind,
    pub frame: Option<u32>,
    pub mesh: TriangleMesh,
    /// Working directory for solver input/output decks.
    pub workdir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub request_id: Uuid,
    pub kind: AnalysisKind,
    pub success: bool,
    pub log: String,
    /// Optional scalar field sampled at mesh nodes (e.g. von Mises).
    pub nodal_scalars: Vec<f32>,
}

pub trait CaeSolver: Send {
    fn name(&self) -> &str;
    fn supports(&self, kind: AnalysisKind) -> bool;
    fn run(&mut self, request: &AnalysisRequest) -> Result<AnalysisResult, CaeError>;
}

/// CalculiX adapter placeholder (structural / thermal / fatigue decks).
#[derive(Debug, Default)]
pub struct CalculiXAdapter {
    pub executable: PathBuf,
}

impl CalculiXAdapter {
    pub fn new() -> Self {
        Self {
            executable: PathBuf::from("ccx"),
        }
    }
}

impl CaeSolver for CalculiXAdapter {
    fn name(&self) -> &str {
        "CalculiX"
    }

    fn supports(&self, kind: AnalysisKind) -> bool {
        matches!(
            kind,
            AnalysisKind::Structural | AnalysisKind::Fatigue | AnalysisKind::Thermal
        )
    }

    fn run(&mut self, request: &AnalysisRequest) -> Result<AnalysisResult, CaeError> {
        if !self.supports(request.kind) {
            return Err(CaeError::Message(format!(
                "CalculiX does not support {:?}",
                request.kind
            )));
        }
        // Detect binary; do not invent results.
        if std::process::Command::new(&self.executable)
            .arg("-v")
            .output()
            .is_err()
        {
            return Err(CaeError::NotInstalled(self.executable.display().to_string()));
        }
        Ok(AnalysisResult {
            request_id: request.id,
            kind: request.kind,
            success: false,
            log: "CalculiX deck generation not yet implemented".into(),
            nodal_scalars: Vec::new(),
        })
    }
}

/// OpenFOAM adapter placeholder (fluids).
#[derive(Debug, Default)]
pub struct OpenFoamAdapter {
    pub executable: PathBuf,
}

impl OpenFoamAdapter {
    pub fn new() -> Self {
        Self {
            executable: PathBuf::from("simpleFoam"),
        }
    }
}

impl CaeSolver for OpenFoamAdapter {
    fn name(&self) -> &str {
        "OpenFOAM"
    }

    fn supports(&self, kind: AnalysisKind) -> bool {
        matches!(kind, AnalysisKind::Fluid)
    }

    fn run(&mut self, request: &AnalysisRequest) -> Result<AnalysisResult, CaeError> {
        if !self.supports(request.kind) {
            return Err(CaeError::Message("OpenFOAM adapter is fluid-only".into()));
        }
        if std::process::Command::new(&self.executable)
            .arg("-help")
            .output()
            .is_err()
        {
            return Err(CaeError::NotInstalled(self.executable.display().to_string()));
        }
        Ok(AnalysisResult {
            request_id: request.id,
            kind: request.kind,
            success: false,
            log: "OpenFOAM case generation not yet implemented".into(),
            nodal_scalars: Vec::new(),
        })
    }
}

pub fn dispatch(kind: AnalysisKind) -> Box<dyn CaeSolver> {
    match kind {
        AnalysisKind::Fluid => Box::new(OpenFoamAdapter::new()),
        _ => Box::new(CalculiXAdapter::new()),
    }
}
