use super::{
    Diagnostic, DiagnosticSeverity, ProgressUpdate, ReportStatus, StageEvent, StageLogLevel,
    StageReport, StageReporter,
};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    io::Write,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MessageFormat {
    #[default]
    Human,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Verbosity {
    Quiet,
    #[default]
    Normal,
    Verbose,
}

#[derive(Debug, Clone, Copy)]
pub struct TerminalOutputOptions {
    pub verbosity: Verbosity,
    pub message_format: MessageFormat,
    pub color: bool,
    pub progress: bool,
    pub interactive: bool,
}

impl Default for TerminalOutputOptions {
    fn default() -> Self {
        Self {
            verbosity: Verbosity::Normal,
            message_format: MessageFormat::Human,
            color: false,
            progress: false,
            interactive: false,
        }
    }
}

pub struct TerminalStageReporter<'a> {
    writer: &'a mut dyn Write,
    options: TerminalOutputOptions,
    active_progress: bool,
    last_progress_at: Option<Instant>,
    latest_report: Option<StageReport>,
    rendered_diagnostics: BTreeSet<Diagnostic>,
}

impl<'a> TerminalStageReporter<'a> {
    pub fn new(writer: &'a mut dyn Write, options: TerminalOutputOptions) -> Self {
        Self {
            writer,
            options,
            active_progress: false,
            last_progress_at: None,
            latest_report: None,
            rendered_diagnostics: BTreeSet::new(),
        }
    }

    fn write_human_line(&mut self, line: &str) {
        self.clear_progress();
        let _ = writeln!(self.writer, "{line}");
        let _ = self.writer.flush();
    }

    fn clear_progress(&mut self) {
        if !self.active_progress {
            return;
        }
        if self.options.interactive {
            let _ = write!(self.writer, "\r\x1b[2K");
        } else {
            let _ = writeln!(self.writer);
        }
        self.active_progress = false;
    }

    fn write_progress(&mut self, stage: &str, message: &str, force: bool) {
        if self.options.verbosity == Verbosity::Quiet || !self.options.progress {
            return;
        }
        let now = Instant::now();
        if !force
            && self
                .last_progress_at
                .is_some_and(|last| now.duration_since(last) < Duration::from_millis(100))
        {
            return;
        }
        self.last_progress_at = Some(now);
        if self.options.interactive {
            let _ = write!(
                self.writer,
                "\r{} {:<8} {}\x1b[K",
                self.paint("[..]", "\x1b[36m"),
                title_case(stage),
                message
            );
            let _ = self.writer.flush();
            self.active_progress = true;
        } else if self.options.verbosity >= Verbosity::Verbose || force {
            self.write_human_line(&format!("[..] {:<8} {message}", title_case(stage)));
        }
    }

    fn render_diagnostic(&mut self, diagnostic: &Diagnostic) {
        if !self.rendered_diagnostics.insert(diagnostic.clone()) {
            return;
        }
        let (label, color) = match diagnostic.severity {
            DiagnosticSeverity::Note => ("NOTE", "\x1b[36m"),
            DiagnosticSeverity::Warning => ("WARNING", "\x1b[33m"),
            DiagnosticSeverity::Error => ("ERROR", "\x1b[31m"),
        };
        self.write_human_line(&format!(
            "{} [{}] {}",
            self.paint(label, color),
            diagnostic.code,
            diagnostic.message
        ));
        if let Some(object) = diagnostic.object.as_deref() {
            self.write_human_line(&format!("  object: {object}"));
        }
        if let Some(detail) = diagnostic.detail.as_deref() {
            self.write_human_line(&format!("  detail: {detail}"));
        }
        if let Some(help) = diagnostic.help.as_deref() {
            self.write_human_line(&format!("  help: {help}"));
        }
        if let Some(artifact) = diagnostic.artifact.as_deref() {
            self.write_human_line(&format!("  report: {artifact}"));
        }
    }

    fn render_finished(&mut self, stage: &str, status: ReportStatus, elapsed_ms: u64) {
        let timing_status = self
            .latest_report
            .as_ref()
            .filter(|report| report.stage == stage)
            .and_then(|report| report.metrics.get("timing_status"))
            .and_then(Value::as_str);
        let (marker, color) = match (status, timing_status) {
            (ReportStatus::Success, Some("VIOLATED")) => ("[VIOL]", "\x1b[31m"),
            (
                ReportStatus::Success,
                Some("UNCONSTRAINED" | "PARTIALLY CONSTRAINED" | "NOT ANALYZED"),
            ) => ("[WARN]", "\x1b[33m"),
            (ReportStatus::Success, _) => ("[OK]", "\x1b[32m"),
            (ReportStatus::Failed, _) => ("[FAIL]", "\x1b[31m"),
            (ReportStatus::Skipped, _) => ("[SKIP]", "\x1b[33m"),
        };
        let mut line = format!(
            "{} {:<8} {:>8}",
            self.paint(marker, color),
            title_case(stage),
            format_elapsed(elapsed_ms)
        );
        if let Some(report) = self
            .latest_report
            .as_ref()
            .filter(|report| report.stage == stage)
            && let Some(summary) = compact_stage_summary(report)
        {
            line.push_str("  ");
            line.push_str(&summary);
        }
        self.write_human_line(&line);
        self.latest_report = None;
        self.last_progress_at = None;
    }

    fn paint(&self, value: &str, color: &str) -> String {
        if self.options.color {
            format!("{color}{value}\x1b[0m")
        } else {
            value.to_string()
        }
    }

    fn render_progress_update(&mut self, stage: &str, update: &ProgressUpdate) {
        let mut message = format!(
            "{} {}/{} {} ({:.0}%)",
            update.phase,
            update.current,
            update.total,
            update.unit.as_str(),
            update.percent()
        );
        for (key, value) in &update.metrics {
            message.push_str(&format!("  {key}={value}"));
        }
        self.write_progress(stage, &message, update.current >= update.total);
    }
}

impl StageReporter for TerminalStageReporter<'_> {
    fn on_stage_event(&mut self, event: StageEvent) {
        if self.options.message_format == MessageFormat::Json {
            self.clear_progress();
            let _ = serde_json::to_writer(&mut self.writer, &event);
            let _ = writeln!(self.writer);
            let _ = self.writer.flush();
            return;
        }

        match event {
            StageEvent::FlowStarted { flow, design, seed } => {
                if self.options.verbosity != Verbosity::Quiet {
                    self.write_human_line(&format!(
                        "FDE {}  flow={}  design={}  seed={}",
                        env!("CARGO_PKG_VERSION"),
                        flow,
                        design,
                        seed
                    ));
                    self.write_human_line("");
                }
            }
            StageEvent::Started { stage } => {
                if self.options.verbosity >= Verbosity::Verbose {
                    self.write_human_line(&format!("[..] {:<8} starting", title_case(stage)));
                }
            }
            StageEvent::Log {
                stage,
                level: StageLogLevel::Progress,
                message,
            } => self.write_progress(stage, &message, message.contains("100%")),
            StageEvent::Log {
                stage,
                level: StageLogLevel::Warning,
                message,
            } => self.render_diagnostic(&Diagnostic::warning(
                format!("FDE-{}-WARN", stage.to_ascii_uppercase()),
                message,
            )),
            StageEvent::Log {
                stage,
                level: StageLogLevel::Info,
                message,
            } => {
                if self.options.verbosity >= Verbosity::Verbose {
                    self.write_human_line(&format!("     {:<8} {message}", title_case(stage)));
                }
            }
            StageEvent::Progress { stage, update } => {
                self.render_progress_update(stage, &update);
            }
            StageEvent::Diagnostic { diagnostic, .. } => self.render_diagnostic(&diagnostic),
            StageEvent::Report { report, .. } => {
                for diagnostic in &report.diagnostics {
                    self.render_diagnostic(diagnostic);
                }
                self.latest_report = Some(*report);
            }
            StageEvent::FlowFinished {
                status,
                elapsed_ms,
                error_count,
                warning_count,
                artifacts,
            } => {
                let status_text = match status {
                    ReportStatus::Success => "Completed",
                    ReportStatus::Failed => "Failed",
                    ReportStatus::Skipped => "Skipped",
                };
                self.write_human_line("");
                self.write_human_line(&format!(
                    "{status_text} in {} with {error_count} error(s) and {warning_count} warning(s)",
                    format_elapsed(elapsed_ms)
                ));
                if self.options.verbosity != Verbosity::Quiet {
                    for (label, key) in [
                        ("Timing report", "sta_report"),
                        ("Run report", "report"),
                        ("Bitstream", "bitstream"),
                    ] {
                        if let Some(path) = artifacts.get(key) {
                            self.write_human_line(&format!("{label:<14}: {path}"));
                        }
                    }
                }
            }
            StageEvent::Finished {
                stage,
                status,
                elapsed_ms,
            } => self.render_finished(stage, status, elapsed_ms),
        }
    }
}

fn compact_stage_summary(report: &StageReport) -> Option<String> {
    let metric = |name: &str| report.metrics.get(name);
    match report.stage.as_str() {
        "map" => Some(format!(
            "{} cells, {} nets",
            display_metric(metric("cell_count"))?,
            display_metric(metric("net_count"))?
        )),
        "pack" => Some(format!(
            "{} clusters, avg fill {}",
            display_metric(metric("cluster_count"))?,
            display_metric(metric("average_cluster_fill"))?
        )),
        "place" => Some(format!("cost {}", display_metric(metric("final_cost"))?)),
        "route" => Some(format!(
            "{} nets, {} pips, overuse {}",
            display_metric(metric("device_net_count"))?,
            display_metric(metric("physical_pip_count"))?,
            display_metric(metric("final_overuse_count"))?
        )),
        "sta" => {
            let status = display_metric(metric("timing_status"))?;
            let wns = metric("worst_slack_ns")
                .and_then(Value::as_f64)
                .map(|value| format!(", WNS {value:+.3} ns"))
                .unwrap_or_default();
            Some(format!(
                "{status}{wns}, Fmax {:.2} MHz",
                metric("fmax_mhz")?.as_f64()?
            ))
        }
        "bitgen" => Some(format!(
            "{} bytes, SHA256 {}",
            display_metric(metric("byte_count"))?,
            short_hash(metric("bitstream_sha256")?.as_str()?)
        )),
        _ => None,
    }
}

fn display_metric(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Number(number) => number.as_f64().map(|value| {
            if value.fract().abs() < f64::EPSILON {
                format!("{value:.0}")
            } else {
                format!("{value:.3}")
            }
        }),
        Value::String(value) => Some(value.clone()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

fn short_hash(value: &str) -> &str {
    value.get(..12).unwrap_or(value)
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

fn format_elapsed(elapsed_ms: u64) -> String {
    if elapsed_ms == 0 {
        "<1 ms".to_string()
    } else if elapsed_ms >= 1_000 {
        format!("{:.2} s", elapsed_ms as f64 / 1_000.0)
    } else {
        format!("{elapsed_ms} ms")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::StageReport;

    #[test]
    fn human_reporter_prints_one_compact_stage_completion() {
        let mut output = Vec::new();
        let mut reporter = TerminalStageReporter::new(
            &mut output,
            TerminalOutputOptions {
                progress: false,
                ..TerminalOutputOptions::default()
            },
        );
        let mut report = StageReport::new("map");
        report.metric("cell_count", 3);
        report.metric("net_count", 6);
        reporter.on_stage_event(StageEvent::Report {
            stage: "map",
            report: Box::new(report),
        });
        reporter.on_stage_event(StageEvent::Finished {
            stage: "map",
            status: ReportStatus::Success,
            elapsed_ms: 1,
        });
        drop(reporter);

        assert_eq!(
            String::from_utf8(output).expect("utf8"),
            "[OK] Map          1 ms  3 cells, 6 nets\n"
        );
    }

    #[test]
    fn json_reporter_emits_json_lines() {
        let mut output = Vec::new();
        let mut reporter = TerminalStageReporter::new(
            &mut output,
            TerminalOutputOptions {
                message_format: MessageFormat::Json,
                ..TerminalOutputOptions::default()
            },
        );
        reporter.on_stage_event(StageEvent::Started { stage: "route" });
        drop(reporter);
        let text = String::from_utf8(output).expect("utf8");
        let value: Value = serde_json::from_str(text.trim()).expect("event json");
        assert_eq!(value.get("event").and_then(Value::as_str), Some("started"));
    }

    #[test]
    fn non_color_human_output_contains_no_ansi_sequences() {
        let mut output = Vec::new();
        let mut reporter = TerminalStageReporter::new(
            &mut output,
            TerminalOutputOptions {
                color: false,
                ..TerminalOutputOptions::default()
            },
        );
        reporter.on_stage_event(StageEvent::Diagnostic {
            stage: "sta",
            diagnostic: Diagnostic::warning("FDE-STA-TEST", "check timing"),
        });
        drop(reporter);

        let text = String::from_utf8(output).expect("utf8");
        assert!(text.contains("WARNING [FDE-STA-TEST]"));
        assert!(!text.contains("\x1b["));
    }

    #[test]
    fn noninteractive_progress_coalesces_to_final_update_at_normal_verbosity() {
        let mut output = Vec::new();
        let mut reporter = TerminalStageReporter::new(
            &mut output,
            TerminalOutputOptions {
                progress: true,
                interactive: false,
                ..TerminalOutputOptions::default()
            },
        );
        reporter.on_stage_event(StageEvent::Progress {
            stage: "route",
            update: ProgressUpdate::new("routing", 1, 10, crate::report::WorkUnit::Nets),
        });
        reporter.on_stage_event(StageEvent::Progress {
            stage: "route",
            update: ProgressUpdate::new("routing", 10, 10, crate::report::WorkUnit::Nets),
        });
        drop(reporter);

        let text = String::from_utf8(output).expect("utf8");
        assert!(!text.contains("1/10"));
        assert_eq!(text.matches("10/10").count(), 1);
    }

    #[test]
    fn json_diagnostic_event_has_stable_typed_fields() {
        let mut output = Vec::new();
        let mut reporter = TerminalStageReporter::new(
            &mut output,
            TerminalOutputOptions {
                message_format: MessageFormat::Json,
                ..TerminalOutputOptions::default()
            },
        );
        reporter.on_stage_event(StageEvent::Diagnostic {
            stage: "route",
            diagnostic: Diagnostic::error("FDE-ROUTE-0002", "no path")
                .with_object("net:data")
                .with_help("inspect routing resources"),
        });
        drop(reporter);

        let value: Value = serde_json::from_slice(&output).expect("event json");
        assert_eq!(value["event"], "diagnostic");
        assert_eq!(value["stage"], "route");
        assert_eq!(value["diagnostic"]["severity"], "error");
        assert_eq!(value["diagnostic"]["code"], "FDE-ROUTE-0002");
        assert_eq!(value["diagnostic"]["object"], "net:data");
    }
}
