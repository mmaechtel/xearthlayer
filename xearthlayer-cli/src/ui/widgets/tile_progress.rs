//! Tile progress widget for displaying active tile generation.
//!
//! Shows a scrolling list of tiles currently being generated with progress bars.
//!
//! ```text
//! ┌ Active Tiles ─────────────────────────┐
//! │ 140E,35S  ████████░░░░░░░░  50%       │
//! │ 140E,36S  ████░░░░░░░░░░░░  25%       │
//! │ 140E,37S  ░░░░░░░░░░░░░░░░   0%       │
//! └───────────────────────────────────────┘
//! ```
//!
//! Each tile entry shows:
//! - Coordinate in compact format (e.g., "140E,35S")
//! - Progress bar using block characters
//! - Percentage complete

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use xearthlayer::runtime::TileProgressEntry;

/// Characters for progress bar rendering.
const PROGRESS_FULL: char = '█';
const PROGRESS_EMPTY: char = '░';

/// Width of the progress bar in characters.
const PROGRESS_BAR_WIDTH: usize = 16;

/// Widget displaying active tile generation progress.
pub struct TileProgressWidget<'a> {
    /// Progress entries to display.
    entries: &'a [TileProgressEntry],
    /// Maximum entries to show (default: 4).
    max_entries: usize,
}

impl<'a> TileProgressWidget<'a> {
    /// Create a new tile progress widget.
    pub fn new(entries: &'a [TileProgressEntry]) -> Self {
        Self {
            entries,
            max_entries: 4,
        }
    }

    /// Set the maximum number of entries to display.
    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
    }

    /// Render a progress bar string.
    fn render_progress_bar(percent: u8) -> String {
        let filled = ((percent as usize * PROGRESS_BAR_WIDTH) / 100).min(PROGRESS_BAR_WIDTH);
        let empty = PROGRESS_BAR_WIDTH - filled;

        format!(
            "{}{}",
            PROGRESS_FULL.to_string().repeat(filled),
            PROGRESS_EMPTY.to_string().repeat(empty)
        )
    }

    /// Get color based on progress percentage.
    fn progress_color(percent: u8) -> Color {
        match percent {
            0..=25 => Color::Yellow,
            26..=75 => Color::Cyan,
            _ => Color::Green,
        }
    }
}

impl Widget for TileProgressWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.entries.is_empty() {
            // Show placeholder when no tiles are being processed
            let placeholder = Line::from(Span::styled(
                "No tiles in progress",
                Style::default().fg(Color::DarkGray),
            ));
            Paragraph::new(placeholder).render(area, buf);
            return;
        }

        // Render each entry as a line
        for (i, entry) in self.entries.iter().take(self.max_entries).enumerate() {
            if i as u16 >= area.height {
                break;
            }

            let row_area = Rect {
                x: area.x,
                y: area.y + i as u16,
                width: area.width,
                height: 1,
            };

            let percent = entry.progress_percent();
            let progress_bar = Self::render_progress_bar(percent);
            let color = Self::progress_color(percent);

            // Format: "140E,35S  ████████░░░░░░░░  50%"
            let coord = entry.format_coordinate();
            let line = Line::from(vec![
                Span::styled(format!("{:<10}", coord), Style::default().fg(Color::White)),
                Span::styled(progress_bar, Style::default().fg(color)),
                Span::styled(format!(" {:>3}%", percent), Style::default().fg(color)),
            ]);

            Paragraph::new(line).render(row_area, buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xearthlayer::coord::TileCoord;

    fn test_entry(row: u32, col: u32, tasks_completed: u8) -> TileProgressEntry {
        let mut entry = TileProgressEntry::new(TileCoord { row, col, zoom: 16 });
        entry.tasks_completed = tasks_completed;
        entry
    }

    #[test]
    fn test_render_progress_bar_empty() {
        let bar = TileProgressWidget::render_progress_bar(0);
        assert_eq!(bar.chars().filter(|&c| c == PROGRESS_EMPTY).count(), 16);
        assert_eq!(bar.chars().filter(|&c| c == PROGRESS_FULL).count(), 0);
    }

    #[test]
    fn test_render_progress_bar_half() {
        let bar = TileProgressWidget::render_progress_bar(50);
        assert_eq!(bar.chars().filter(|&c| c == PROGRESS_FULL).count(), 8);
        assert_eq!(bar.chars().filter(|&c| c == PROGRESS_EMPTY).count(), 8);
    }

    #[test]
    fn test_render_progress_bar_full() {
        let bar = TileProgressWidget::render_progress_bar(100);
        assert_eq!(bar.chars().filter(|&c| c == PROGRESS_FULL).count(), 16);
        assert_eq!(bar.chars().filter(|&c| c == PROGRESS_EMPTY).count(), 0);
    }

    #[test]
    fn test_progress_color() {
        assert_eq!(TileProgressWidget::progress_color(0), Color::Yellow);
        assert_eq!(TileProgressWidget::progress_color(25), Color::Yellow);
        assert_eq!(TileProgressWidget::progress_color(50), Color::Cyan);
        assert_eq!(TileProgressWidget::progress_color(75), Color::Cyan);
        assert_eq!(TileProgressWidget::progress_color(100), Color::Green);
    }

    #[test]
    fn test_widget_creation() {
        let entries = vec![test_entry(100, 200, 1)];
        let widget = TileProgressWidget::new(&entries);
        assert_eq!(widget.max_entries, 4);
    }

    #[test]
    fn test_widget_with_max_entries() {
        let entries = vec![test_entry(100, 200, 1)];
        let widget = TileProgressWidget::new(&entries).with_max_entries(2);
        assert_eq!(widget.max_entries, 2);
    }
}
