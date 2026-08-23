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
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use thiserror::Error;

use crate::diagnostics::Diagnostics;

const WIDE_TERMINAL_MIN_COLUMNS: u16 = 72;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayoutMode {
    Wide,
    Narrow,
}

impl LayoutMode {
    const fn for_width(width: u16) -> Self {
        if width >= WIDE_TERMINAL_MIN_COLUMNS {
            Self::Wide
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
    let vertical = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(6),
        Constraint::Length(3),
    ])
    .split(area);

    render_header(frame, vertical[0], diagnostics);

    match LayoutMode::for_width(area.width) {
        LayoutMode::Wide => {
            let columns =
                Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .spacing(1)
                    .split(vertical[1]);
            render_environment(frame, columns[0], diagnostics);
            render_foundation(frame, columns[1]);
        }
        LayoutMode::Narrow => {
            let rows = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(vertical[1]);
            render_environment(frame, rows[0], diagnostics);
            render_foundation(frame, rows[1]);
        }
    }

    let footer = Paragraph::new(Line::from(vec![
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(" quit   "),
        Span::styled("esc", Style::default().fg(Color::Yellow)),
        Span::raw(" quit   foundation milestone"),
    ]))
    .block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, vertical[2]);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, diagnostics: &Diagnostics) {
    let mut title = vec![Span::styled(
        "ippo",
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    )];

    if diagnostics.environment != "personal" {
        title.push(Span::raw(" "));
        title.push(Span::styled(
            "[DEVELOPMENT]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }

    frame.render_widget(
        Paragraph::new(Line::from(title)).block(Block::default().borders(Borders::BOTTOM)),
        area,
    );
}

fn render_environment(frame: &mut Frame<'_>, area: Rect, diagnostics: &Diagnostics) {
    let content = vec![
        Line::from(format!("profile       {}", diagnostics.profile.as_str())),
        Line::from(format!("environment   {}", diagnostics.environment)),
        Line::from(format!("schema        v{}", diagnostics.schema_version)),
        Line::from(format!("database      {}", diagnostics.database_path)),
    ];

    frame.render_widget(
        Paragraph::new(content)
            .wrap(Wrap { trim: false })
            .block(Block::bordered().title(" environment ")),
        area,
    );
}

fn render_foundation(frame: &mut Frame<'_>, area: Rect) {
    let content = vec![
        Line::from("The safe local foundation is running."),
        Line::from(""),
        Line::from("Next: create and complete a daily binary habit."),
    ];

    frame.render_widget(
        Paragraph::new(content)
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title(" foundation ")),
        area,
    );
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
    fn development_banner_renders_in_wide_terminal() {
        let output = rendered_text(100, 20);

        assert!(output.contains("[DEVELOPMENT]"));
        assert!(output.contains("environment"));
        assert!(output.contains("foundation"));
    }

    #[test]
    fn development_banner_renders_in_narrow_terminal() {
        let output = rendered_text(50, 24);

        assert!(output.contains("[DEVELOPMENT]"));
        assert!(output.contains("environment"));
        assert!(output.contains("foundation"));
    }

    #[test]
    fn layout_breakpoint_is_explicit() {
        assert_eq!(LayoutMode::for_width(71), LayoutMode::Narrow);
        assert_eq!(LayoutMode::for_width(72), LayoutMode::Wide);
    }
}
