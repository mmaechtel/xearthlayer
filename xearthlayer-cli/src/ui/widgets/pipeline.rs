//! Pipeline stage visualization widget.
//!
//! Shows the flow of tiles through pipeline stages:
//! FUSE → DOWNLOAD → ASSEMBLE → ENCODE → CACHE → DONE

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};
use xearthlayer::telemetry::TelemetrySnapshot;

/// Widget displaying pipeline stage status.
pub struct PipelineWidget<'a> {
    snapshot: &'a TelemetrySnapshot,
}

impl<'a> PipelineWidget<'a> {
    pub fn new(snapshot: &'a TelemetrySnapshot) -> Self {
        Self { snapshot }
    }

    /// Create a progress bar string (filled/empty blocks).
    fn progress_bar(active: usize, max_display: usize) -> String {
        let filled = active.min(max_display);
        let empty = max_display.saturating_sub(filled);
        format!("{}{}", "█".repeat(filled), "░".repeat(empty))
    }
}

impl Widget for PipelineWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().borders(Borders::NONE);

        // Calculate stage metrics
        // FUSE waiting = jobs_submitted - jobs_completed - jobs_failed - jobs_active
        let fuse_waiting = self
            .snapshot
            .jobs_submitted
            .saturating_sub(self.snapshot.jobs_completed)
            .saturating_sub(self.snapshot.jobs_failed)
            .saturating_sub(self.snapshot.jobs_active as u64);

        let download_active = self.snapshot.downloads_active;
        let encode_active = self.snapshot.encodes_active;
        let completed = self.snapshot.jobs_completed;

        // Use fixed-width columns for alignment
        // Each column: 12 chars for header, centered
        // FUSE(4) DOWNLOAD(8) ASSEMBLE(8) ENCODE(6) CACHE(5) DONE(4)

        // Pipeline flow line with arrows
        let flow_line = Line::from(vec![
            Span::raw("  "),
            Span::styled("FUSE", Style::default().fg(Color::Cyan)),
            Span::raw(" ──► "),
            Span::styled("DOWNLOAD", Style::default().fg(Color::Yellow)),
            Span::raw(" ──► "),
            Span::styled("ASSEMBLE", Style::default().fg(Color::Magenta)),
            Span::raw(" ──► "),
            Span::styled("ENCODE", Style::default().fg(Color::Blue)),
            Span::raw(" ──► "),
            Span::styled("CACHE", Style::default().fg(Color::Green)),
            Span::raw(" ──► "),
            Span::styled("DONE", Style::default().fg(Color::White)),
        ]);

        // Counts line - align under stage names
        // "  FUSE ──► DOWNLOAD ──► ASSEMBLE ──► ENCODE ──► CACHE ──► DONE"
        //  "   12       ██░░ 128      ██░░ 45     ██░░ 8    ██░░ 2     1247"
        let counts_line = Line::from(vec![
            Span::raw("   "),
            Span::styled(
                format!("{:<4}", fuse_waiting),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw("    "),
            Span::styled(
                format!(
                    "{} {:>3}",
                    Self::progress_bar(download_active, 4),
                    download_active
                ),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("       "),
            Span::styled(
                format!(
                    "{} {:>3}",
                    Self::progress_bar(self.snapshot.jobs_active, 4),
                    self.snapshot.jobs_active
                ),
                Style::default().fg(Color::Magenta),
            ),
            Span::raw("      "),
            Span::styled(
                format!(
                    "{} {:>3}",
                    Self::progress_bar(encode_active, 4),
                    encode_active
                ),
                Style::default().fg(Color::Blue),
            ),
            Span::raw("     "),
            Span::styled(
                format!("{} {:>3}", Self::progress_bar(0, 4), 0),
                Style::default().fg(Color::Green),
            ),
            Span::raw("    "),
            Span::styled(
                format!("{:>6}", completed),
                Style::default().fg(Color::White),
            ),
        ]);

        // Labels line - align under counts
        let labels_line = Line::from(vec![
            Span::styled("  wait", Style::default().fg(Color::DarkGray)),
            Span::raw("       "),
            Span::styled("active", Style::default().fg(Color::DarkGray)),
            Span::raw("          "),
            Span::styled("active", Style::default().fg(Color::DarkGray)),
            Span::raw("         "),
            Span::styled("active", Style::default().fg(Color::DarkGray)),
            Span::raw("        "),
            Span::styled("active", Style::default().fg(Color::DarkGray)),
            Span::raw("     "),
            Span::styled("completed", Style::default().fg(Color::DarkGray)),
        ]);

        let text = vec![
            Line::raw(""),
            flow_line,
            counts_line,
            labels_line,
            Line::raw(""),
        ];

        let paragraph = Paragraph::new(text).block(block);
        paragraph.render(area, buf);
    }
}
