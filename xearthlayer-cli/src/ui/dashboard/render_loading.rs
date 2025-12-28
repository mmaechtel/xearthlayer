//! Loading state rendering.
//!
//! This module contains rendering functions for the loading and prewarm states.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::render_sections::inner_rect;
use super::state::LoadingProgress;

/// Render the loading UI.
pub fn render_loading_ui(frame: &mut Frame, progress: &LoadingProgress, spinner: char) {
    let size = frame.area();

    // Calculate centered box dimensions
    let box_width = 60u16.min(size.width.saturating_sub(4));
    let box_height = 11u16;
    let x = (size.width.saturating_sub(box_width)) / 2;
    let y = (size.height.saturating_sub(box_height)) / 2;

    let area = Rect {
        x,
        y,
        width: box_width,
        height: box_height,
    };

    // Main container block
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            format!(" X-Plane Earth Layer {} ", xearthlayer::VERSION),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));

    frame.render_widget(block, area);

    // Inner content area
    let inner = inner_rect(area, 2, 1);

    // Build content lines
    let mut lines = vec![
        Line::from(vec![Span::styled(
            "Building Scenery Index...",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
    ];

    // Spinner + current package line
    if !progress.current_package.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", spinner), Style::default().fg(Color::Yellow)),
            Span::styled("Scanning: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&progress.current_package, Style::default().fg(Color::Green)),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", spinner), Style::default().fg(Color::Yellow)),
            Span::styled("Initializing...", Style::default().fg(Color::DarkGray)),
        ]));
    }

    lines.push(Line::from(""));

    // Progress bar
    let progress_width = (inner.width.saturating_sub(14)) as usize; // "Packages: " + "XX/XX"
    let filled = (progress.progress_fraction() * progress_width as f64) as usize;
    let empty = progress_width.saturating_sub(filled);
    let progress_bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(empty));

    lines.push(Line::from(vec![
        Span::styled("Packages: ", Style::default().fg(Color::DarkGray)),
        Span::styled(progress_bar, Style::default().fg(Color::Cyan)),
        Span::styled(
            format!(" {}/{}", progress.packages_scanned, progress.total_packages),
            Style::default().fg(Color::White),
        ),
    ]));

    // Tiles indexed line
    lines.push(Line::from(vec![
        Span::styled("Tiles indexed: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}", progress.tiles_indexed),
            Style::default().fg(Color::White),
        ),
    ]));

    lines.push(Line::from(""));

    // Elapsed time
    let elapsed = progress.elapsed();
    lines.push(Line::from(vec![
        Span::styled("Elapsed: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}s", elapsed.as_secs()),
            Style::default().fg(Color::White),
        ),
    ]));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}
