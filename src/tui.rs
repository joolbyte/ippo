use std::io::{self, Stdout};

use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Gauge, Paragraph},
};
use thiserror::Error;

use crate::diagnostics::Diagnostics;

const WIDE_TERMINAL_MIN_COLUMNS: u16 = 96;
const MEDIUM_TERMINAL_MIN_COLUMNS: u16 = 64;

mod palette {
    use ratatui::style::Color;

    pub const SUMI: Color = Color::Rgb(22, 24, 23);
    pub const SUMI_LIGHT: Color = Color::Rgb(34, 37, 35);
    pub const WASHI: Color = Color::Rgb(229, 222, 202);
    pub const STONE: Color = Color::Rgb(139, 133, 120);
    pub const VERMILION: Color = Color::Rgb(196, 67, 54);
    pub const VERMILION_DARK: Color = Color::Rgb(112, 52, 45);
    pub const MOSS: Color = Color::Rgb(114, 143, 104);
    pub const MOSS_DARK: Color = Color::Rgb(55, 74, 54);
    pub const INDIGO: Color = Color::Rgb(76, 101, 132);
    pub const GOLD: Color = Color::Rgb(201, 163, 83);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayoutMode {
    Wide,
    Medium,
    Narrow,
}

impl LayoutMode {
    const fn for_width(width: u16) -> Self {
        if width >= WIDE_TERMINAL_MIN_COLUMNS {
            Self::Wide
        } else if width >= MEDIUM_TERMINAL_MIN_COLUMNS {
            Self::Medium
        } else {
            Self::Narrow
        }
    }
}

pub fn run(diagnostics: &Diagnostics) -> Result<(), TuiError> {
    let mut session = TerminalSession::start()?;

    loop {
        session.terminal.draw(|frame| render(frame, diagnostics))?;

        if let Event::Key(key) = event::read()?
            && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
            && (matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
                || (key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL)))
        {
            break;
        }
    }

    Ok(())
}

fn render(frame: &mut Frame<'_>, diagnostics: &Diagnostics) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(palette::SUMI)),
        area,
    );

    let shell = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(12),
        Constraint::Length(2),
    ])
    .split(area);

    render_header(frame, shell[0], diagnostics);

    match LayoutMode::for_width(area.width) {
        LayoutMode::Wide => render_wide_dashboard(frame, shell[1]),
        LayoutMode::Medium => render_medium_dashboard(frame, shell[1]),
        LayoutMode::Narrow => render_narrow_dashboard(frame, shell[1]),
    }

    render_footer(frame, shell[2]);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, diagnostics: &Diagnostics) {
    let rows = Layout::vertical([Constraint::Length(2), Constraint::Length(2)]).split(area);
    let title_columns =
        Layout::horizontal([Constraint::Min(24), Constraint::Length(22)]).split(rows[0]);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " ippo",
                Style::default()
                    .fg(palette::WASHI)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " 一歩",
                Style::default()
                    .fg(palette::VERMILION)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  one step, daily", Style::default().fg(palette::STONE)),
        ]))
        .style(Style::default().bg(palette::SUMI)),
        title_columns[0],
    );

    let environment = if diagnostics.environment == "personal" {
        "LOCAL · PERSONAL"
    } else {
        "PREVIEW · DEVELOPMENT"
    };
    frame.render_widget(
        Paragraph::new(environment)
            .alignment(Alignment::Right)
            .style(
                Style::default()
                    .fg(if diagnostics.environment == "personal" {
                        palette::MOSS
                    } else {
                        palette::GOLD
                    })
                    .add_modifier(Modifier::BOLD),
            ),
        title_columns[1],
    );

    let navigation = Paragraph::new(Line::from(vec![
        Span::styled("  TODAY", selected_tab()),
        Span::styled("   CALENDAR", muted()),
        Span::styled("   HISTORY", muted()),
        Span::styled("   SETTINGS", muted()),
    ]))
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(palette::VERMILION_DARK)),
    );
    frame.render_widget(navigation, rows[1]);
}

fn render_wide_dashboard(frame: &mut Frame<'_>, area: Rect) {
    let columns = Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
        .spacing(1)
        .split(area);
    let primary = Layout::vertical([Constraint::Length(9), Constraint::Min(12)])
        .spacing(1)
        .split(columns[0]);
    let secondary = Layout::vertical([Constraint::Length(17), Constraint::Min(7)])
        .spacing(1)
        .split(columns[1]);

    render_status(frame, primary[0]);
    render_today(frame, primary[1], true);
    render_calendar(frame, secondary[0]);
    render_contributions(frame, secondary[1]);
}

fn render_medium_dashboard(frame: &mut Frame<'_>, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(9),
        Constraint::Min(12),
        Constraint::Length(8),
    ])
    .spacing(1)
    .split(area);

    render_status(frame, rows[0]);
    render_today(frame, rows[1], true);
    render_contributions(frame, rows[2]);
}

fn render_narrow_dashboard(frame: &mut Frame<'_>, area: Rect) {
    let rows = Layout::vertical([Constraint::Length(9), Constraint::Min(12)])
        .spacing(1)
        .split(area);

    render_status(frame, rows[0]);
    render_today(frame, rows[1], false);
}

fn render_status(frame: &mut Frame<'_>, area: Rect) {
    let (inner, block) = panel(area, " いま  STATUS ");
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(1),
    ])
    .split(inner);
    let headline = Layout::horizontal([Constraint::Min(20), Constraint::Length(14)]).split(rows[0]);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "72%",
                Style::default()
                    .fg(palette::WASHI)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  4 of 6 complete", muted()),
        ])),
        headline[0],
    );
    frame.render_widget(
        Paragraph::new(Span::styled("SUN · 23 AUG", accent())).alignment(Alignment::Right),
        headline[1],
    );

    frame.render_widget(
        Gauge::default()
            .gauge_style(
                Style::default()
                    .fg(palette::VERMILION)
                    .bg(palette::SUMI_LIGHT),
            )
            .ratio(0.72)
            .label(""),
        rows[1],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("火  12 day flow", Style::default().fg(palette::GOLD)),
            Span::styled("    level 7", muted()),
            Span::styled("  ·  380 / 500 xp", Style::default().fg(palette::INDIGO)),
        ])),
        rows[2],
    );
}

fn render_today(frame: &mut Frame<'_>, area: Rect, detailed: bool) {
    let (inner, block) = panel(area, " 今日  TODAY ");
    frame.render_widget(block, area);

    let lines = if detailed {
        vec![
            routine_header("MORNING", "3 / 4"),
            habit_line("●", palette::MOSS, "drink water", "6 / 8", false),
            habit_line("●", palette::MOSS, "stretch", "10 min", false),
            habit_line("○", palette::STONE, "read", "12 / 20 pages", true),
            Line::from(""),
            routine_header("EVENING", "1 / 2"),
            habit_line("●", palette::MOSS, "walk outside", "30 min", false),
            habit_line(
                "○",
                palette::STONE,
                "summarize my day",
                "0 / 100 chars",
                false,
            ),
        ]
    } else {
        vec![
            routine_header("MORNING", "3 / 4"),
            habit_line("●", palette::MOSS, "drink water", "6 / 8", false),
            habit_line("●", palette::MOSS, "stretch", "10m", false),
            habit_line("○", palette::STONE, "read", "12 / 20", true),
            Line::from(""),
            routine_header("EVENING", "1 / 2"),
            habit_line("○", palette::STONE, "summarize day", "0 / 100", false),
        ]
    };

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_calendar(frame: &mut Frame<'_>, area: Rect) {
    let (inner, block) = panel(area, " 暦  AUGUST 2026 ");
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(vec![
            Span::styled(" MON TUE WED THU FRI SAT SUN ", muted()),
            Span::styled(" ‹  ›", accent()),
        ]),
        Line::from(Span::styled("                       1   2", muted())),
        Line::from("   3   4   5   6   7   8   9"),
        Line::from("  10  11  12  13  14  15  16"),
        Line::from(vec![
            Span::raw("  17  18  19  20  21  22 "),
            Span::styled(
                "23",
                Style::default().fg(palette::SUMI).bg(palette::VERMILION),
            ),
        ]),
        Line::from(Span::styled("  24  25  26  27  28  29  30", muted())),
        Line::from(Span::styled("  31", muted())),
        Line::from(""),
        Line::from(vec![
            Span::styled(" selected", muted()),
            Span::styled("  72% complete", Style::default().fg(palette::MOSS)),
        ]),
    ];

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
}

fn render_contributions(frame: &mut Frame<'_>, area: Rect) {
    let (inner, block) = panel(area, " 足跡  CONTRIBUTIONS ");
    frame.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(3)]).split(inner);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("20 weeks", muted()),
            Span::styled(
                "  ·  consistency by completion",
                Style::default().fg(palette::STONE),
            ),
        ])),
        rows[0],
    );

    let levels = [
        0, 1, 2, 0, 3, 4, 2, 1, 0, 2, 3, 3, 4, 2, 1, 0, 3, 4, 4, 2, 1, 2, 0, 3, 4, 4, 3, 2, 0, 1,
        3, 2, 4, 4, 3, 1, 0, 2, 3, 4, 2, 1, 3, 3, 4, 4, 2, 1, 0, 2, 4, 3, 1, 2, 3, 4, 4, 2, 1,
    ];
    let heatmap = (0..3)
        .map(|row| {
            let mut spans = vec![Span::styled(
                ["m  ", "w  ", "f  "][row],
                Style::default().fg(palette::STONE),
            )];
            for level in levels.iter().skip(row).step_by(3) {
                spans.push(Span::styled("■ ", Style::default().fg(heat_color(*level))));
            }
            Line::from(spans)
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(heatmap), rows[1]);
}

fn render_footer(frame: &mut Frame<'_>, area: Rect) {
    if area.width < MEDIUM_TERMINAL_MIN_COLUMNS {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                key("j/k"),
                Span::styled(" move  ", muted()),
                key("space"),
                Span::styled(" check  ", muted()),
                key("q"),
                Span::styled(" quit", muted()),
            ]))
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(palette::VERMILION_DARK)),
            ),
            area,
        );
        return;
    }

    let columns = Layout::horizontal([Constraint::Min(24), Constraint::Length(31)]).split(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            key("j/k"),
            Span::styled(" move   ", muted()),
            key("space"),
            Span::styled(" check   ", muted()),
            key("enter"),
            Span::styled(" open   ", muted()),
            key("q"),
            Span::styled(" quit", muted()),
        ]))
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(palette::VERMILION_DARK)),
        ),
        columns[0],
    );
    frame.render_widget(
        Paragraph::new("VISUAL PROTOTYPE · SAMPLE DATA ")
            .alignment(Alignment::Right)
            .style(Style::default().fg(palette::STONE))
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(palette::VERMILION_DARK)),
            ),
        columns[1],
    );
}

fn panel(area: Rect, title: &'static str) -> (Rect, Block<'static>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(palette::VERMILION_DARK))
        .title(Span::styled(
            title,
            Style::default()
                .fg(palette::VERMILION)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    (inner, block)
}

fn routine_header(name: &'static str, progress: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("┌─ {name}"),
            Style::default()
                .fg(palette::INDIGO)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  [{progress}]"), muted()),
    ])
}

fn habit_line(
    marker: &'static str,
    marker_color: Color,
    name: &'static str,
    progress: &'static str,
    selected: bool,
) -> Line<'static> {
    let name_style = if selected {
        Style::default()
            .fg(palette::WASHI)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(palette::WASHI)
    };

    Line::from(vec![
        Span::styled("│  ", Style::default().fg(palette::VERMILION_DARK)),
        Span::styled(marker, Style::default().fg(marker_color)),
        Span::raw(" "),
        Span::styled(name, name_style),
        Span::styled(format!("  {progress}"), muted()),
        if selected {
            Span::styled("  ←", Style::default().fg(palette::VERMILION))
        } else {
            Span::raw("")
        },
    ])
}

fn heat_color(level: u8) -> Color {
    match level {
        0 => palette::SUMI_LIGHT,
        1 => palette::MOSS_DARK,
        2 => Color::Rgb(72, 99, 70),
        3 => palette::MOSS,
        _ => palette::WASHI,
    }
}

fn key(value: &'static str) -> Span<'static> {
    Span::styled(
        value,
        Style::default()
            .fg(palette::GOLD)
            .add_modifier(Modifier::BOLD),
    )
}

fn selected_tab() -> Style {
    Style::default()
        .fg(palette::WASHI)
        .bg(palette::VERMILION)
        .add_modifier(Modifier::BOLD)
}

fn accent() -> Style {
    Style::default()
        .fg(palette::VERMILION)
        .add_modifier(Modifier::BOLD)
}

fn muted() -> Style {
    Style::default().fg(palette::STONE)
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    fn start() -> Result<Self, TuiError> {
        enable_raw_mode()?;

        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen, Hide) {
            let _ = disable_raw_mode();
            return Err(error.into());
        }

        match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => Ok(Self { terminal }),
            Err(error) => {
                let mut stdout = io::stdout();
                let _ = execute!(stdout, LeaveAlternateScreen, Show);
                let _ = disable_raw_mode();
                Err(error.into())
            }
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen, Show);
        let _ = self.terminal.show_cursor();
    }
}

#[derive(Debug, Error)]
pub enum TuiError {
    #[error("terminal I/O failed: {0}")]
    Io(#[from] io::Error),
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use std::path::PathBuf;

    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::config::Profile;

    fn diagnostics() -> Diagnostics {
        Diagnostics {
            profile: Profile::Dev,
            environment: "development".to_owned(),
            database_path: PathBuf::from("/tmp/ippo-dev.db")
                .to_string_lossy()
                .into_owned(),
            database_overridden: false,
            schema_version: 1,
        }
    }

    fn rendered_text(width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render(frame, &diagnostics()))
            .expect("render should succeed");

        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn wide_dashboard_renders_all_primary_views() {
        let output = rendered_text(120, 36);

        assert!(output.contains("ippo"));
        assert!(output.contains('一'));
        assert!(output.contains('歩'));
        assert!(output.contains("PREVIEW · DEVELOPMENT"));
        assert!(output.contains("STATUS"));
        assert!(output.contains("TODAY"));
        assert!(output.contains("AUGUST 2026"));
        assert!(output.contains("CONTRIBUTIONS"));
        assert!(output.contains("summarize my day"));
    }

    #[test]
    fn medium_dashboard_prioritizes_today_and_contributions() {
        let output = rendered_text(80, 36);

        assert!(output.contains("STATUS"));
        assert!(output.contains("TODAY"));
        assert!(output.contains("CONTRIBUTIONS"));
        assert!(!output.contains("AUGUST 2026"));
    }

    #[test]
    fn narrow_dashboard_keeps_core_habit_flow_visible() {
        let output = rendered_text(52, 30);

        assert!(output.contains("ippo"));
        assert!(output.contains('一'));
        assert!(output.contains('歩'));
        assert!(output.contains("72%"));
        assert!(output.contains("MORNING"));
        assert!(output.contains("summarize day"));
        assert!(!output.contains("CONTRIBUTIONS"));
    }

    #[test]
    fn layout_breakpoints_are_explicit() {
        assert_eq!(LayoutMode::for_width(63), LayoutMode::Narrow);
        assert_eq!(LayoutMode::for_width(64), LayoutMode::Medium);
        assert_eq!(LayoutMode::for_width(95), LayoutMode::Medium);
        assert_eq!(LayoutMode::for_width(96), LayoutMode::Wide);
    }
}
