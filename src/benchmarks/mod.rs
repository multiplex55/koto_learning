use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::runtime::logging;

const NS_PER_MS: f64 = 1_000_000.0;

#[derive(Clone, Debug)]
pub struct ExampleBenchmarkSummary {
    pub example_id: String,
    pub measurements: Vec<BenchmarkMeasurement>,
    pub report_url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct BenchmarkMeasurement {
    pub benchmark_id: String,
    pub parameter: Option<String>,
    pub mean: EstimateSummary,
    pub std_dev_ms: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct EstimateSummary {
    pub point_estimate_ms: f64,
    pub lower_bound_ms: f64,
    pub upper_bound_ms: f64,
    pub confidence_level: f64,
}

#[derive(Deserialize)]
struct CriterionEstimates {
    mean: Estimate,
    #[serde(default)]
    std_dev: Option<Estimate>,
}

#[derive(Deserialize)]
struct Estimate {
    point_estimate: f64,
    confidence_interval: ConfidenceInterval,
}

#[derive(Deserialize)]
struct ConfidenceInterval {
    confidence_level: f64,
    lower_bound: f64,
    upper_bound: f64,
}

pub fn load_example_summary(example_id: &str) -> Option<ExampleBenchmarkSummary> {
    let base = Path::new("target").join("criterion").join(example_id);
    if !base.exists() {
        return None;
    }

    match collect_measurements(&base) {
        Ok(measurements) => {
            let report_url = report_path(&base).map(file_url);
            if measurements.is_empty() && report_url.is_none() {
                None
            } else {
                Some(ExampleBenchmarkSummary {
                    example_id: example_id.to_string(),
                    measurements,
                    report_url,
                })
            }
        }
        Err(error) => {
            logging::with_runtime_subscriber(|| {
                tracing::warn!(
                    target: "runtime.benchmarks",
                    example_id,
                    %error,
                    "Failed to load Criterion benchmark summary"
                );
            });
            None
        }
    }
}

fn collect_measurements(base: &Path) -> Result<Vec<BenchmarkMeasurement>> {
    let mut measurements = Vec::new();
    collect_recursive(base, &mut Vec::new(), &mut measurements)?;
    measurements.sort_by(|a, b| {
        a.benchmark_id
            .cmp(&b.benchmark_id)
            .then_with(|| a.parameter.cmp(&b.parameter))
    });
    Ok(measurements)
}

fn collect_recursive(
    dir: &Path,
    parts: &mut Vec<String>,
    output: &mut Vec<BenchmarkMeasurement>,
) -> Result<()> {
    let estimates_path = dir.join("new").join("estimates.json");
    if estimates_path.exists() {
        let estimates = load_estimates(&estimates_path)?;
        if let Some(measurement) = build_measurement(parts, estimates) {
            output.push(measurement);
        }
        return Ok(());
    }

    for entry in fs::read_dir(dir).with_context(|| format!("Failed to read directory {dir:?}"))? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if matches!(name.as_str(), "base" | "new" | "report" | "old") {
            continue;
        }
        parts.push(name);
        collect_recursive(&entry.path(), parts, output)?;
        parts.pop();
    }

    Ok(())
}

fn load_estimates(path: &Path) -> Result<CriterionEstimates> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read estimates at {path:?}"))?;
    let estimates = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse Criterion estimates from {path:?}"))?;
    Ok(estimates)
}

fn build_measurement(
    parts: &[String],
    estimates: CriterionEstimates,
) -> Option<BenchmarkMeasurement> {
    if parts.is_empty() {
        return None;
    }

    let benchmark_id = parts.first().cloned().unwrap_or_default();
    let parameter = if parts.len() > 1 {
        Some(parts[1..].join(" / "))
    } else {
        None
    };

    let mean = summary_from_estimate(&estimates.mean);
    let std_dev_ms = estimates
        .std_dev
        .map(|estimate| estimate.point_estimate / NS_PER_MS);

    Some(BenchmarkMeasurement {
        benchmark_id,
        parameter,
        mean,
        std_dev_ms,
    })
}

fn summary_from_estimate(estimate: &Estimate) -> EstimateSummary {
    EstimateSummary {
        point_estimate_ms: estimate.point_estimate / NS_PER_MS,
        lower_bound_ms: estimate.confidence_interval.lower_bound / NS_PER_MS,
        upper_bound_ms: estimate.confidence_interval.upper_bound / NS_PER_MS,
        confidence_level: estimate.confidence_interval.confidence_level,
    }
}

fn report_path(base: &Path) -> Option<PathBuf> {
    let path = base.join("report").join("index.html");
    path.exists().then_some(path)
}

fn file_url(path: PathBuf) -> String {
    match path.canonicalize() {
        Ok(canonical) => format!("file://{}", canonical.display()),
        Err(_) => format!("file://{}", path.display()),
    }
}
