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
use super::state::{LoadingPhase, LoadingProgress};

/// Render the loading UI.
pub fn render_loading_ui(frame: &mut Frame, progress: &LoadingProgress, spinner: char) {
    let size = frame.area();

    // Calculate centered box dimensions
    let box_width = 60u16.min(size.width.saturating_sub(4));
    let box_height = 12u16; // Increased for phase indicator
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
    let title = if progress.using_cache {
        "Loading Cached Index..."
    } else {
        "Building Scenery Index..."
    };

    let mut lines = vec![
        Line::from(vec![Span::styled(
            title,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
    ];

    // Spinner + phase/current package line
    let status_text = match progress.phase {
        LoadingPhase::Discovering => "Discovering packages...".to_string(),
        LoadingPhase::CheckingCache => "Checking cache...".to_string(),
        LoadingPhase::Scanning => {
            if !progress.current_package.is_empty() {
                format!("Scanning: {}", progress.current_package)
            } else {
                "Scanning sources...".to_string()
            }
        }
        LoadingPhase::Merging => "Merging index...".to_string(),
        LoadingPhase::SavingCache => "Saving cache...".to_string(),
        LoadingPhase::Complete => "Complete!".to_string(),
    };

    let status_color = if progress.using_cache {
        Color::Magenta
    } else {
        Color::Green
    };

    lines.push(Line::from(vec![
        Span::styled(format!("{} ", spinner), Style::default().fg(Color::Yellow)),
        Span::styled(status_text, Style::default().fg(status_color)),
    ]));

    lines.push(Line::from(""));

    // Progress bar
    let progress_width = (inner.width.saturating_sub(14)) as usize; // "Sources:  " + "XX/XX"
    let filled = (progress.progress_fraction() * progress_width as f64) as usize;
    let empty = progress_width.saturating_sub(filled);
    let progress_bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(empty));

    lines.push(Line::from(vec![
        Span::styled("Sources:  ", Style::default().fg(Color::DarkGray)),
        Span::styled(progress_bar, Style::default().fg(Color::Cyan)),
        Span::styled(
            format!(" {}/{}", progress.packages_scanned, progress.total_packages),
            Style::default().fg(Color::White),
        ),
    ]));

    // Files indexed line
    lines.push(Line::from(vec![
        Span::styled("Files indexed: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format_number(progress.tiles_indexed),
            Style::default().fg(Color::White),
        ),
    ]));

    lines.push(Line::from(""));

    // Elapsed time and cache indicator
    let elapsed = progress.elapsed();
    let mut time_spans = vec![
        Span::styled("Elapsed: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}s", elapsed.as_secs()),
            Style::default().fg(Color::White),
        ),
    ];

    if progress.using_cache {
        time_spans.push(Span::styled("  ", Style::default()));
        time_spans.push(Span::styled(
            "✓ cached",
            Style::default().fg(Color::Magenta),
        ));
    }

    lines.push(Line::from(time_spans));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

/// Format a number with thousands separators.
fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    let chars: Vec<_> = s.chars().collect();

    for (i, ch) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i) % 3 == 0 {
            result.push(',');
        }
        result.push(*ch);
    }

    result
}
