use std::{
    io::{self, Stdout},
    time::Duration,
};

use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use jiff::civil::Date;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Gauge, Paragraph, Wrap},
};
use thiserror::Error;

use crate::{
    app::{ApplicationError, HabitApplication},
    clock::{Clock, SystemClock, SystemTimeZone, TimeZoneSource},
    diagnostics::Diagnostics,
    habit::{MAX_HABIT_NAME_CHARS, TodayHabit},
    storage::Database,
};

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

struct TuiState<C, T> {
    application: HabitApplication<C, T>,
    selected: usize,
    mode: InputMode,
    notice: Option<Notice>,
}

enum InputMode {
    Normal,
    Creating {
        value: String,
        error: Option<String>,
    },
}

struct Notice {
    message: String,
    is_error: bool,
}

impl<C: Clock, T: TimeZoneSource> TuiState<C, T> {
    fn new(application: HabitApplication<C, T>) -> Self {
        Self {
            application,
            selected: 0,
            mode: InputMode::Normal,
            notice: None,
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return true;
        }

        match &mut self.mode {
            InputMode::Creating { value, error } => match key.code {
                KeyCode::Esc => {
                    self.mode = InputMode::Normal;
                    self.notice = None;
                }
                KeyCode::Enter => {
                    let name = value.clone();
                    match self.application.create_daily_binary(&name) {
                        Ok(()) => {
                            self.selected = self.application.habits().len().saturating_sub(1);
                            self.mode = InputMode::Normal;
                            self.notice = Some(Notice {
                                message: format!("created ‘{}’", name.trim()),
                                is_error: false,
                            });
                        }
                        Err(application_error) => {
                            *error = Some(application_error.to_string());
                        }
                    }
                }
                KeyCode::Backspace => {
                    value.pop();
                    *error = None;
                }
                KeyCode::Char(character)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && value.chars().count() < MAX_HABIT_NAME_CHARS =>
                {
                    value.push(character);
                    *error = None;
                }
                _ => {}
            },
            InputMode::Normal => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return true,
                KeyCode::Char('n') => {
                    self.mode = InputMode::Creating {
                        value: String::new(),
                        error: None,
                    };
                    self.notice = None;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if !self.application.habits().is_empty() {
                        self.selected = self.selected.saturating_sub(1);
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if !self.application.habits().is_empty() {
                        self.selected =
                            (self.selected + 1).min(self.application.habits().len() - 1);
                    }
                }
                KeyCode::Char(' ') => self.toggle_selected(),
                _ => {}
            },
        }

        false
    }

    fn refresh_day(&mut self) -> bool {
        match self.application.refresh_day() {
            Ok(changed) => {
                if changed {
                    self.selected = 0;
                    self.notice = Some(Notice {
                        message: "a new day has begun".to_owned(),
                        is_error: false,
                    });
                }
                changed
            }
            Err(error) => {
                self.set_error(error);
                true
            }
        }
    }

    fn toggle_selected(&mut self) {
        let Some(habit) = self.application.habits().get(self.selected) else {
            return;
        };
        let occurrence_id = habit.occurrence_id;
        let habit_name = habit.name.clone();

        match self.application.toggle(occurrence_id) {
            Ok(()) => {
                self.notice = Some(Notice {
                    message: format!("updated ‘{habit_name}’"),
                    is_error: false,
                });
            }
            Err(error) => self.set_error(error),
        }
    }

    fn set_error(&mut self, error: ApplicationError) {
        self.notice = Some(Notice {
            message: error.to_string(),
            is_error: true,
        });
    }
}

pub fn run(database: Database, diagnostics: &Diagnostics) -> Result<(), TuiError> {
    let application = HabitApplication::new(database, SystemClock, SystemTimeZone)?;
    let mut state = TuiState::new(application);
    let mut session = TerminalSession::start()?;
    let mut needs_draw = true;

    loop {
        needs_draw |= state.refresh_day();
        if needs_draw {
            session
                .terminal
                .draw(|frame| render(frame, diagnostics, &state))?;
            needs_draw = false;
        }

        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
            && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        {
            if state.handle_key(key) {
                break;
            }
            needs_draw = true;
        }
    }

    Ok(())
}

fn render<C: Clock, T: TimeZoneSource>(
    frame: &mut Frame<'_>,
    diagnostics: &Diagnostics,
    state: &TuiState<C, T>,
) {
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
        LayoutMode::Wide => render_wide_dashboard(frame, shell[1], state),
        LayoutMode::Medium => render_medium_dashboard(frame, shell[1], state),
        LayoutMode::Narrow => render_narrow_dashboard(frame, shell[1], state),
    }

    render_footer(frame, shell[2], state);

    if matches!(state.mode, InputMode::Creating { .. }) {
        render_creation_dialog(frame, area, state);
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, diagnostics: &Diagnostics) {
    let rows = Layout::vertical([Constraint::Length(2), Constraint::Length(2)]).split(area);
    let title_columns =
        Layout::horizontal([Constraint::Min(24), Constraint::Length(18)]).split(rows[0]);

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
        ])),
        title_columns[0],
    );

    let environment = if diagnostics.environment == "personal" {
        "LOCAL · PERSONAL"
    } else {
        "DEVELOPMENT"
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

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  TODAY", selected_tab()),
            Span::styled("   CALENDAR", muted()),
            Span::styled("   HISTORY", muted()),
            Span::styled("   SETTINGS", muted()),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(palette::VERMILION_DARK)),
        ),
        rows[1],
    );
}

fn render_wide_dashboard<C: Clock, T: TimeZoneSource>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiState<C, T>,
) {
    let columns = Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
        .spacing(1)
        .split(area);
    let primary = Layout::vertical([Constraint::Length(9), Constraint::Min(12)])
        .spacing(1)
        .split(columns[0]);

    render_status(frame, primary[0], state);
    render_today(frame, primary[1], state);
    if area.height < 26 {
        render_calendar(frame, columns[1], state);
    } else {
        let secondary = Layout::vertical([Constraint::Length(17), Constraint::Min(7)])
            .spacing(1)
            .split(columns[1]);
        render_calendar(frame, secondary[0], state);
        render_contributions(frame, secondary[1], state);
    }
}

fn render_medium_dashboard<C: Clock, T: TimeZoneSource>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiState<C, T>,
) {
    if area.height < 30 {
        let rows = Layout::vertical([Constraint::Length(9), Constraint::Min(8)])
            .spacing(1)
            .split(area);
        render_status(frame, rows[0], state);
        render_today(frame, rows[1], state);
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(9),
        Constraint::Min(12),
        Constraint::Length(8),
    ])
    .spacing(1)
    .split(area);

    render_status(frame, rows[0], state);
    render_today(frame, rows[1], state);
    render_contributions(frame, rows[2], state);
}

fn render_narrow_dashboard<C: Clock, T: TimeZoneSource>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiState<C, T>,
) {
    let rows = Layout::vertical([Constraint::Length(9), Constraint::Min(12)])
        .spacing(1)
        .split(area);

    render_status(frame, rows[0], state);
    render_today(frame, rows[1], state);
}

fn render_status<C: Clock, T: TimeZoneSource>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiState<C, T>,
) {
    let (inner, block) = panel(area, " いま  STATUS ");
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Length(1),
    ])
    .split(inner);
    let headline = Layout::horizontal([Constraint::Min(20), Constraint::Length(12)]).split(rows[0]);
    let completed = state.application.completed_count();
    let total = state.application.habits().len();
    let percentage = state.application.completion_percentage();

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{percentage}%"),
                Style::default()
                    .fg(palette::WASHI)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {completed} of {total} complete"), muted()),
        ])),
        headline[0],
    );
    frame.render_widget(
        Paragraph::new(Span::styled(
            state.application.today().to_string(),
            accent(),
        ))
        .alignment(Alignment::Right),
        headline[1],
    );

    frame.render_widget(
        Gauge::default()
            .gauge_style(
                Style::default()
                    .fg(palette::VERMILION)
                    .bg(palette::SUMI_LIGHT),
            )
            .ratio(f64::from(percentage) / 100.0)
            .label(""),
        rows[1],
    );

    let guidance = if total == 0 {
        Line::from(vec![
            Span::styled("一", Style::default().fg(palette::GOLD)),
            Span::styled("  start with one small step · press ", muted()),
            key("n"),
        ])
    } else if completed == total {
        Line::from(Span::styled(
            "今日の一歩  today's habits are complete",
            Style::default().fg(palette::MOSS),
        ))
    } else {
        Line::from(Span::styled("one step at a time", muted()))
    };
    frame.render_widget(Paragraph::new(guidance), rows[2]);
}

fn render_today<C: Clock, T: TimeZoneSource>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiState<C, T>,
) {
    let (inner, block) = panel(area, " 今日  TODAY ");
    frame.render_widget(block, area);

    let habits = state.application.habits();
    if habits.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "No habits yet.",
                    Style::default().fg(palette::WASHI),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Press ", muted()),
                    key("n"),
                    Span::styled(" to create a daily binary habit.", muted()),
                ]),
            ])
            .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let visible_habits = inner.height.saturating_sub(2) as usize;
    let start = state
        .selected
        .saturating_sub(visible_habits.saturating_sub(1));
    let completed = state.application.completed_count();
    let mut lines = vec![Line::from(vec![
        Span::styled(
            "┌─ DAILY",
            Style::default()
                .fg(palette::INDIGO)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  [{completed} / {}]", habits.len()), muted()),
    ])];
    lines.extend(
        habits
            .iter()
            .enumerate()
            .skip(start)
            .take(visible_habits)
            .map(|(index, habit)| habit_line(habit, index == state.selected)),
    );

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_calendar<C: Clock, T: TimeZoneSource>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiState<C, T>,
) {
    let today = state.application.today();
    let title = format!(" 暦  {} {} ", month_name(today.month()), today.year());
    let (inner, block) = panel(area, title);
    frame.render_widget(block, area);

    let mut lines = calendar_lines(today);
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("today", muted()),
        Span::styled(
            format!("  {}% complete", state.application.completion_percentage()),
            Style::default().fg(palette::MOSS),
        ),
    ]));
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
}

fn render_contributions<C: Clock, T: TimeZoneSource>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiState<C, T>,
) {
    let (inner, block) = panel(area, " 足跡  CONTRIBUTIONS ");
    frame.render_widget(block, area);

    let message = if state.application.habits().is_empty() {
        vec![
            Line::from(Span::styled(
                "No history yet.",
                Style::default().fg(palette::WASHI),
            )),
            Line::from(Span::styled("Your consistency will grow here.", muted())),
        ]
    } else {
        vec![
            Line::from(Span::styled(
                "History starts with today's step.",
                Style::default().fg(palette::WASHI),
            )),
            Line::from(Span::styled(
                "The contribution graph arrives with history browsing.",
                muted(),
            )),
        ]
    };
    frame.render_widget(
        Paragraph::new(message)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        inner,
    );
}

fn render_footer<C: Clock, T: TimeZoneSource>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiState<C, T>,
) {
    if matches!(state.mode, InputMode::Creating { .. }) {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                key("enter"),
                Span::styled(" create   ", muted()),
                key("esc"),
                Span::styled(" cancel", muted()),
            ]))
            .block(footer_block()),
            area,
        );
        return;
    }

    if area.width < MEDIUM_TERMINAL_MIN_COLUMNS {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                key("n"),
                Span::styled(" new  ", muted()),
                key("j/k"),
                Span::styled(" move  ", muted()),
                key("space"),
                Span::styled(" toggle  ", muted()),
                key("q"),
                Span::styled(" quit", muted()),
            ]))
            .block(footer_block()),
            area,
        );
        return;
    }

    let columns = Layout::horizontal([Constraint::Min(30), Constraint::Length(34)]).split(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            key("n"),
            Span::styled(" new   ", muted()),
            key("j/k"),
            Span::styled(" move   ", muted()),
            key("space"),
            Span::styled(" toggle   ", muted()),
            key("q"),
            Span::styled(" quit", muted()),
        ]))
        .block(footer_block()),
        columns[0],
    );

    let (message, color) = state.notice.as_ref().map_or_else(
        || ("SQLite · LOCAL".to_owned(), palette::STONE),
        |notice| {
            (
                notice.message.clone(),
                if notice.is_error {
                    palette::VERMILION
                } else {
                    palette::MOSS
                },
            )
        },
    );
    frame.render_widget(
        Paragraph::new(message)
            .alignment(Alignment::Right)
            .style(Style::default().fg(color))
            .block(footer_block()),
        columns[1],
    );
}

fn render_creation_dialog<C: Clock, T: TimeZoneSource>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiState<C, T>,
) {
    let InputMode::Creating { value, error } = &state.mode else {
        return;
    };
    let popup = centered_rect(area, 70, 7);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(palette::VERMILION))
        .style(Style::default().bg(palette::SUMI))
        .title(Span::styled(" NEW DAILY HABIT ", accent()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
    ])
    .split(inner);
    frame.render_widget(Paragraph::new(Span::styled("Name", muted())), rows[0]);
    frame.render_widget(
        Paragraph::new(value.as_str()).block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(palette::INDIGO)),
        ),
        rows[1],
    );
    let hint = error.as_ref().map_or_else(
        || {
            format!(
                "{} characters remaining",
                MAX_HABIT_NAME_CHARS - value.chars().count()
            )
        },
        Clone::clone,
    );
    frame.render_widget(
        Paragraph::new(hint).style(Style::default().fg(if error.is_some() {
            palette::VERMILION
        } else {
            palette::STONE
        })),
        rows[2],
    );

    let cursor_offset = Line::from(value.as_str()).width() as u16;
    let max_offset = rows[1].width.saturating_sub(2);
    frame.set_cursor_position((rows[1].x + cursor_offset.min(max_offset), rows[1].y));
}

fn calendar_lines(today: Date) -> Vec<Line<'static>> {
    let first = Date::new(today.year(), today.month(), 1).expect("today's month is valid");
    let offset = first.weekday().to_monday_zero_offset() as usize;
    let days = today.days_in_month() as usize;
    let mut lines = vec![Line::from(Span::styled(
        "MON TUE WED THU FRI SAT SUN",
        muted(),
    ))];

    for week in 0..6 {
        let mut spans = Vec::new();
        let mut has_day = false;
        for weekday in 0..7 {
            let cell = week * 7 + weekday;
            if cell < offset || cell >= offset + days {
                spans.push(Span::raw("    "));
                continue;
            }

            has_day = true;
            let day = cell - offset + 1;
            let label = format!("{day:>2}  ");
            if day == today.day() as usize {
                spans.push(Span::styled(
                    label,
                    Style::default()
                        .fg(palette::SUMI)
                        .bg(palette::VERMILION)
                        .add_modifier(Modifier::BOLD),
                ));
            } else if weekday >= 5 {
                spans.push(Span::styled(label, muted()));
            } else {
                spans.push(Span::styled(label, Style::default().fg(palette::WASHI)));
            }
        }
        if has_day {
            lines.push(Line::from(spans));
        }
    }
    lines
}

fn habit_line(habit: &TodayHabit, selected: bool) -> Line<'static> {
    let marker = if habit.completed { "●" } else { "○" };
    let marker_color = if habit.completed {
        palette::MOSS
    } else {
        palette::STONE
    };
    let name_style = Style::default()
        .fg(palette::WASHI)
        .add_modifier(if selected {
            Modifier::BOLD
        } else {
            Modifier::empty()
        });

    Line::from(vec![
        Span::styled("│  ", Style::default().fg(palette::VERMILION_DARK)),
        Span::styled(marker, Style::default().fg(marker_color)),
        Span::raw(" "),
        Span::styled(habit.name.clone(), name_style),
        if selected {
            Span::styled("  ←", Style::default().fg(palette::VERMILION))
        } else {
            Span::raw("")
        },
    ])
}

fn panel(area: Rect, title: impl Into<String>) -> (Rect, Block<'static>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(palette::VERMILION_DARK))
        .title(Span::styled(
            title.into(),
            Style::default()
                .fg(palette::VERMILION)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    (inner, block)
}

fn centered_rect(area: Rect, maximum_width: u16, height: u16) -> Rect {
    let width = maximum_width.min(area.width.saturating_sub(4)).max(1);
    let height = height.min(area.height.saturating_sub(2)).max(1);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn month_name(month: i8) -> &'static str {
    [
        "JANUARY",
        "FEBRUARY",
        "MARCH",
        "APRIL",
        "MAY",
        "JUNE",
        "JULY",
        "AUGUST",
        "SEPTEMBER",
        "OCTOBER",
        "NOVEMBER",
        "DECEMBER",
    ][month as usize - 1]
}

fn footer_block() -> Block<'static> {
    Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(palette::VERMILION_DARK))
}

fn key(value: impl Into<String>) -> Span<'static> {
    Span::styled(
        value.into(),
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
    #[error(transparent)]
    Application(#[from] ApplicationError),
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use std::path::PathBuf;

    use jiff::{Timestamp, tz::TimeZone};
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::{
        clock::{FixedClock, FixedTimeZone},
        config::{DataEnvironment, Profile},
    };

    fn diagnostics() -> Diagnostics {
        Diagnostics {
            profile: Profile::Dev,
            environment: "development".to_owned(),
            database_path: PathBuf::from("/tmp/ippo-dev.db")
                .to_string_lossy()
                .into_owned(),
            database_overridden: false,
            schema_version: 2,
        }
    }

    fn state() -> TuiState<FixedClock, FixedTimeZone> {
        let database = Database::open_in_memory(DataEnvironment::Test).unwrap();
        let timestamp: Timestamp = "2026-08-23T12:00:00Z".parse().unwrap();
        let application = HabitApplication::new(
            database,
            FixedClock::new(timestamp),
            FixedTimeZone::new(TimeZone::UTC),
        )
        .unwrap();
        TuiState::new(application)
    }

    fn rendered_text(
        width: u16,
        height: u16,
        state: &TuiState<FixedClock, FixedTimeZone>,
    ) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render(frame, &diagnostics(), state))
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
    fn empty_dashboard_has_no_seeded_activity() {
        let output = rendered_text(120, 36, &state());

        assert!(output.contains("ippo"));
        assert!(output.contains("DEVELOPMENT"));
        assert!(output.contains("0%"));
        assert!(output.contains("No habits yet"));
        assert!(output.contains("No history yet"));
        assert!(!output.contains("drink water"));
        assert!(!output.contains("level 7"));
        assert!(!output.contains("SAMPLE DATA"));
    }

    #[test]
    fn create_input_adds_a_real_habit_and_space_toggles_it() {
        let mut state = state();
        state.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        for character in "read".chars() {
            state.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
        }
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(state.application.habits()[0].name, "read");
        assert!(!state.application.habits()[0].completed);
        assert!(rendered_text(120, 36, &state).contains("read"));

        state.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(state.application.habits()[0].completed);
        assert_eq!(state.application.completion_percentage(), 100);
    }

    #[test]
    fn creation_dialog_reports_validation_without_closing() {
        let mut state = state();
        state.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        let output = rendered_text(80, 30, &state);
        assert!(output.contains("NEW DAILY HABIT"));
        assert!(output.contains("habit name cannot be empty"));
    }

    #[test]
    fn narrow_dashboard_keeps_empty_state_and_create_action_visible() {
        let output = rendered_text(52, 30, &state());

        assert!(output.contains("0%"));
        assert!(output.contains("No habits yet"));
        assert!(output.contains("new"));
        assert!(!output.contains("CONTRIBUTIONS"));
    }

    #[test]
    fn compact_medium_dashboard_omits_secondary_history_panel() {
        let output = rendered_text(80, 24, &state());

        assert!(output.contains("STATUS"));
        assert!(output.contains("TODAY"));
        assert!(output.contains("No habits yet"));
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
