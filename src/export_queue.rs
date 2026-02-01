use crate::ui::TrimMode;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Status of an export job
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed(String),
}

/// Type of export operation
#[derive(Debug, Clone)]
pub enum ExportOperation {
    Trim {
        start: f64,
        end: f64,
        mode: TrimMode,
    },
    Concat {
        inputs: Vec<PathBuf>,
    },
}

/// A single export job
#[derive(Debug, Clone)]
pub struct ExportJob {
    pub id: u32,
    pub input: PathBuf,
    pub output: PathBuf,
    pub operation: ExportOperation,
    pub status: JobStatus,
    pub progress: f32,
    pub segment_label: String,
}

impl ExportJob {
    pub fn new_trim(id: u32, input: PathBuf, output: PathBuf, start: f64, end: f64, mode: TrimMode) -> Self {
        Self {
            id,
            input,
            output,
            operation: ExportOperation::Trim { start, end, mode },
            status: JobStatus::Pending,
            progress: 0.0,
            segment_label: String::new(),
        }
    }

    pub fn new_trim_with_label(id: u32, input: PathBuf, output: PathBuf, start: f64, end: f64, mode: TrimMode, label: String) -> Self {
        Self {
            id,
            input,
            output,
            operation: ExportOperation::Trim { start, end, mode },
            status: JobStatus::Pending,
            progress: 0.0,
            segment_label: label,
        }
    }

    pub fn description(&self) -> String {
        match &self.operation {
            ExportOperation::Trim { start, end, mode } => {
                let duration = end - start;
                let label_part = if self.segment_label.is_empty() {
                    String::new()
                } else {
                    format!("[{}] ", self.segment_label)
                };
                format!(
                    "{}{} -> {} ({:.1}s, {})",
                    label_part,
                    self.input.file_name().unwrap_or_default().to_string_lossy(),
                    self.output.file_name().unwrap_or_default().to_string_lossy(),
                    duration,
                    mode.name()
                )
            }
            ExportOperation::Concat { inputs } => {
                format!(
                    "Merge {} files -> {}",
                    inputs.len(),
                    self.output.file_name().unwrap_or_default().to_string_lossy(),
                )
            }
        }
    }

    pub fn status_text(&self) -> &str {
        match &self.status {
            JobStatus::Pending => "En attente",
            JobStatus::Running => "En cours...",
            JobStatus::Completed => "Termine",
            JobStatus::Failed(_) => "Echec",
        }
    }
}

/// Queue of export jobs
#[derive(Default)]
pub struct ExportQueue {
    pub jobs: Vec<ExportJob>,
    next_id: u32,
    pub is_processing: bool,
}

impl ExportQueue {
    pub fn new() -> Self {
        Self {
            jobs: Vec::new(),
            next_id: 0,
            is_processing: false,
        }
    }

    /// Add a trim job to the queue
    pub fn add_trim(&mut self, input: PathBuf, output: PathBuf, start: f64, end: f64, mode: TrimMode) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        let job = ExportJob::new_trim(id, input, output, start, end, mode);
        self.jobs.push(job);
        id
    }

    /// Add a concat job to the queue
    pub fn add_concat(&mut self, inputs: Vec<PathBuf>, output: PathBuf, label: String) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        let first_input = inputs.first().cloned().unwrap_or_default();
        let job = ExportJob {
            id,
            input: first_input,
            output,
            operation: ExportOperation::Concat { inputs },
            status: JobStatus::Pending,
            progress: 0.0,
            segment_label: label,
        };
        self.jobs.push(job);
        id
    }

    /// Add a trim job with a segment label
    pub fn add_trim_with_label(&mut self, input: PathBuf, output: PathBuf, start: f64, end: f64, mode: TrimMode, label: String) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        let job = ExportJob::new_trim_with_label(id, input, output, start, end, mode, label);
        self.jobs.push(job);
        id
    }

    /// Get the next pending job
    pub fn next_pending(&mut self) -> Option<&mut ExportJob> {
        self.jobs.iter_mut().find(|j| j.status == JobStatus::Pending)
    }

    /// Get job by ID
    pub fn get_job(&self, id: u32) -> Option<&ExportJob> {
        self.jobs.iter().find(|j| j.id == id)
    }

    /// Get mutable job by ID
    pub fn get_job_mut(&mut self, id: u32) -> Option<&mut ExportJob> {
        self.jobs.iter_mut().find(|j| j.id == id)
    }

    /// Cancel all pending jobs (running job will finish on its own)
    pub fn cancel_all(&mut self) {
        for job in &mut self.jobs {
            if job.status == JobStatus::Pending {
                job.status = JobStatus::Failed("Cancelled".to_string());
            }
        }
    }

    /// Remove completed/failed jobs
    pub fn clear_finished(&mut self) {
        self.jobs.retain(|j| matches!(j.status, JobStatus::Pending | JobStatus::Running));
    }

    /// Remove a specific job
    pub fn remove_job(&mut self, id: u32) {
        self.jobs.retain(|j| j.id != id);
    }

    /// Count pending jobs
    pub fn pending_count(&self) -> usize {
        self.jobs.iter().filter(|j| j.status == JobStatus::Pending).count()
    }

    /// Count completed jobs
    pub fn completed_count(&self) -> usize {
        self.jobs.iter().filter(|j| j.status == JobStatus::Completed).count()
    }

    /// Check if queue has pending work
    pub fn has_pending(&self) -> bool {
        self.jobs.iter().any(|j| j.status == JobStatus::Pending)
    }

    /// Total progress: (completed, total)
    pub fn total_progress(&self) -> (usize, usize) {
        let total = self.jobs.len();
        let completed = self.jobs.iter().filter(|j| {
            matches!(j.status, JobStatus::Completed | JobStatus::Failed(_))
        }).count();
        (completed, total)
    }
}

/// Shared queue type for async access
pub type SharedQueue = Arc<Mutex<ExportQueue>>;

pub fn create_shared_queue() -> SharedQueue {
    Arc::new(Mutex::new(ExportQueue::new()))
}
