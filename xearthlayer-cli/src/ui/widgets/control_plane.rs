//! Control plane health widget.
//!
//! Displays control plane status including health state,
//! job concurrency, and recovery metrics.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use xearthlayer::pipeline::control_plane::{HealthSnapshot, HealthStatus};

/// Widget displaying control plane health status.
pub struct ControlPlaneWidget<'a> {
    snapshot: &'a HealthSnapshot,
    max_concurrent_jobs: usize,
}

impl<'a> ControlPlaneWidget<'a> {
    pub fn new(snapshot: &'a HealthSnapshot, max_concurrent_jobs: usize) -> Self {
        Self {
            snapshot,
            max_concurrent_jobs,
        }
    }

    /// Get color for health status.
    fn status_color(status: HealthStatus) -> Color {
        match status {
            HealthStatus::Healthy => Color::Green,
            HealthStatus::Degraded => Color::Yellow,
            HealthStatus::Recovering => Color::Cyan,
            HealthStatus::Critical => Color::Red,
        }
    }

    /// Format duration since last success.
    fn format_duration(d: std::time::Duration) -> String {
        let secs = d.as_secs();
        if secs < 60 {
            format!("{}s", secs)
        } else if secs < 3600 {
            format!("{}m{}s", secs / 60, secs % 60)
        } else {
            format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
        }
    }
}

impl Widget for ControlPlaneWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let status = self.snapshot.status;
        let status_color = Self::status_color(status);

        // First line: Status and job concurrency
        let status_line = Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:^10}", status.as_str().to_uppercase()),
                Style::default().fg(status_color),
            ),
            Span::raw("  │  "),
            Span::styled("Jobs: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(
                    "{}/{}",
                    self.snapshot.jobs_in_progress, self.max_concurrent_jobs
                ),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(
                format!(" (peak: {})", self.snapshot.peak_concurrent_jobs),
                Style::default().fg(Color::DarkGray),
            ),
        ]);

        // Second line: Recovery metrics (only show if there are any)
        let has_recoveries = self.snapshot.jobs_recovered > 0
            || self.snapshot.jobs_rejected_capacity > 0
            || self.snapshot.semaphore_timeouts > 0;

        let recovery_line = if has_recoveries {
            let mut spans = vec![Span::styled(
                "Recovery: ",
                Style::default().fg(Color::DarkGray),
            )];

            if self.snapshot.jobs_recovered > 0 {
                spans.push(Span::styled(
                    format!("{} recovered", self.snapshot.jobs_recovered),
                    Style::default().fg(Color::Yellow),
                ));
            }

            if self.snapshot.jobs_rejected_capacity > 0 {
                if self.snapshot.jobs_recovered > 0 {
                    spans.push(Span::raw("  │  "));
                }
                spans.push(Span::styled(
                    format!("{} rejected", self.snapshot.jobs_rejected_capacity),
                    Style::default().fg(Color::Red),
                ));
            }

            if self.snapshot.semaphore_timeouts > 0 {
                if self.snapshot.jobs_recovered > 0 || self.snapshot.jobs_rejected_capacity > 0 {
                    spans.push(Span::raw("  │  "));
                }
                spans.push(Span::styled(
                    format!("{} timeouts", self.snapshot.semaphore_timeouts),
                    Style::default().fg(Color::Red),
                ));
            }

            Line::from(spans)
        } else {
            // Show time since last success if available
            if let Some(elapsed) = self.snapshot.time_since_last_success {
                Line::from(vec![
                    Span::styled("Last success: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!("{} ago", Self::format_duration(elapsed)),
                        Style::default().fg(Color::Green),
                    ),
                ])
            } else {
                Line::from(vec![Span::styled(
                    "Waiting for first job...",
                    Style::default().fg(Color::DarkGray),
                )])
            }
        };

        let text = vec![status_line, recovery_line];
        let paragraph = Paragraph::new(text);
        paragraph.render(area, buf);
    }
}
