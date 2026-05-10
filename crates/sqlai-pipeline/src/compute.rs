//! Rust 内置计算：moving average + linear extrapolation。
//!
//! 输入：上一步 SQL 出来的 ExecutionResult，结构假设是 (bucket: String, value: number)。
//! 输出：同形 ExecutionResult，但额外加一列 `kind` ∈ {"actual","ma","forecast"}。

use serde_json::Value;
use sqlai_exec::ExecutionResult;
use sqlai_skills::ComputeFn;

#[derive(Debug, thiserror::Error)]
pub enum ComputeError {
    #[error("missing column '{0}' in input rows")]
    MissingColumn(String),

    #[error("invalid param: {0}")]
    InvalidParam(String),

    #[error("non-numeric value at row {0}")]
    NonNumeric(usize),
}

pub fn run_compute(
    function: ComputeFn,
    params: &Value,
    input: &ExecutionResult,
) -> Result<ExecutionResult, ComputeError> {
    let buckets = collect_string_col(input, "bucket")?;
    let values = collect_number_col(input, "value")?;

    match function {
        ComputeFn::MovingAverage => {
            let window = params
                .get("window")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| ComputeError::InvalidParam("window".into()))?
                as usize;
            if window < 2 {
                return Err(ComputeError::InvalidParam("window must be >= 2".into()));
            }
            let mut rows = Vec::with_capacity(values.len() * 2);
            for (i, b) in buckets.iter().enumerate() {
                rows.push(make_row(b, values[i], "actual"));
            }
            for (i, b) in buckets.iter().enumerate() {
                if i + 1 < window {
                    continue;
                }
                let avg: f64 = values[(i + 1 - window)..=i].iter().sum::<f64>() / window as f64;
                rows.push(make_row(b, avg, "ma"));
            }
            Ok(ExecutionResult {
                columns: vec!["bucket".into(), "value".into(), "kind".into()],
                rows,
                truncated: false,
            })
        }
        ComputeFn::LinearExtrapolation => {
            let horizon = params
                .get("horizon")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| ComputeError::InvalidParam("horizon".into()))?
                as usize;
            let granularity = params
                .get("granularity")
                .and_then(|v| v.as_str())
                .unwrap_or("day");

            let n = values.len();
            if n < 2 {
                return Err(ComputeError::InvalidParam("need >= 2 points to fit".into()));
            }
            let xs: Vec<f64> = (0..n).map(|i| i as f64).collect();
            let (slope, intercept) = lin_reg(&xs, &values);

            let mut rows = Vec::with_capacity(n + horizon);
            for (i, b) in buckets.iter().enumerate() {
                rows.push(make_row(b, values[i], "actual"));
            }
            let last_bucket = buckets.last().cloned().unwrap_or_default();
            for k in 1..=horizon {
                let predicted = intercept + slope * ((n - 1 + k) as f64);
                let label = next_bucket_label(&last_bucket, k, granularity);
                rows.push(make_row(&label, predicted, "forecast"));
            }
            Ok(ExecutionResult {
                columns: vec!["bucket".into(), "value".into(), "kind".into()],
                rows,
                truncated: false,
            })
        }
    }
}

fn make_row(bucket: &str, value: f64, kind: &str) -> Value {
    serde_json::json!({ "bucket": bucket, "value": value, "kind": kind })
}

fn collect_string_col(input: &ExecutionResult, name: &str) -> Result<Vec<String>, ComputeError> {
    if !input.columns.iter().any(|c| c == name) {
        return Err(ComputeError::MissingColumn(name.into()));
    }
    Ok(input
        .rows
        .iter()
        .map(|r| {
            r.get(name)
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default()
        })
        .collect())
}

fn collect_number_col(input: &ExecutionResult, name: &str) -> Result<Vec<f64>, ComputeError> {
    if !input.columns.iter().any(|c| c == name) {
        return Err(ComputeError::MissingColumn(name.into()));
    }
    let mut out = Vec::with_capacity(input.rows.len());
    for (i, r) in input.rows.iter().enumerate() {
        let v = r
            .get(name)
            .and_then(|v| v.as_f64())
            .ok_or(ComputeError::NonNumeric(i))?;
        out.push(v);
    }
    Ok(out)
}

fn lin_reg(xs: &[f64], ys: &[f64]) -> (f64, f64) {
    let n = xs.len() as f64;
    let mean_x = xs.iter().sum::<f64>() / n;
    let mean_y = ys.iter().sum::<f64>() / n;
    let mut num = 0.0;
    let mut den = 0.0;
    for (x, y) in xs.iter().zip(ys.iter()) {
        num += (x - mean_x) * (y - mean_y);
        den += (x - mean_x).powi(2);
    }
    let slope = if den == 0.0 { 0.0 } else { num / den };
    let intercept = mean_y - slope * mean_x;
    (slope, intercept)
}

fn next_bucket_label(last: &str, k: usize, granularity: &str) -> String {
    use chrono::NaiveDateTime;
    let parsed = NaiveDateTime::parse_from_str(last, "%Y-%m-%d %H:%M:%S").or_else(|_| {
        NaiveDateTime::parse_from_str(&format!("{} 00:00:00", last), "%Y-%m-%d %H:%M:%S")
    });
    if let Ok(dt) = parsed {
        let next = match granularity {
            "day" => dt + chrono::Duration::days(k as i64),
            "week" => dt + chrono::Duration::weeks(k as i64),
            "month" => dt + chrono::Duration::days((30 * k) as i64),
            _ => dt + chrono::Duration::days(k as i64),
        };
        next.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        format!("{}+{}{}", last, k, granularity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input(buckets: &[&str], values: &[f64]) -> ExecutionResult {
        let rows: Vec<Value> = buckets
            .iter()
            .zip(values.iter())
            .map(|(b, v)| serde_json::json!({"bucket": b, "value": v}))
            .collect();
        ExecutionResult {
            columns: vec!["bucket".into(), "value".into()],
            rows,
            truncated: false,
        }
    }

    #[test]
    fn moving_average_window_3() {
        let input = make_input(
            &[
                "2025-01-01",
                "2025-01-02",
                "2025-01-03",
                "2025-01-04",
                "2025-01-05",
            ],
            &[10.0, 20.0, 30.0, 40.0, 50.0],
        );
        let out = run_compute(
            ComputeFn::MovingAverage,
            &serde_json::json!({"window":3}),
            &input,
        )
        .unwrap();
        assert_eq!(out.rows.len(), 8);
        let ma_rows: Vec<f64> = out
            .rows
            .iter()
            .filter(|r| r["kind"] == "ma")
            .map(|r| r["value"].as_f64().unwrap())
            .collect();
        assert_eq!(ma_rows, vec![20.0, 30.0, 40.0]);
    }

    #[test]
    fn linear_extrapolation_perfect_line() {
        let input = make_input(
            &[
                "2025-01-01",
                "2025-01-02",
                "2025-01-03",
                "2025-01-04",
                "2025-01-05",
            ],
            &[10.0, 20.0, 30.0, 40.0, 50.0],
        );
        let out = run_compute(
            ComputeFn::LinearExtrapolation,
            &serde_json::json!({"horizon":2,"granularity":"day"}),
            &input,
        )
        .unwrap();
        assert_eq!(out.rows.len(), 7);
        let forecasts: Vec<f64> = out
            .rows
            .iter()
            .filter(|r| r["kind"] == "forecast")
            .map(|r| r["value"].as_f64().unwrap())
            .collect();
        assert!((forecasts[0] - 60.0).abs() < 1e-9);
        assert!((forecasts[1] - 70.0).abs() < 1e-9);
    }

    #[test]
    fn missing_value_column_errors() {
        let input = ExecutionResult {
            columns: vec!["bucket".into()],
            rows: vec![serde_json::json!({"bucket":"2025-01-01"})],
            truncated: false,
        };
        let err = run_compute(
            ComputeFn::MovingAverage,
            &serde_json::json!({"window":2}),
            &input,
        )
        .unwrap_err();
        assert!(matches!(err, ComputeError::MissingColumn(_)));
    }
}
