use std::{
    collections::BTreeMap,
    io::{self, Stdout},
    time::Duration,
};

use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use jiff::{Span as JiffSpan, civil::Date};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Gauge, Paragraph},
};
use thiserror::Error;

use crate::{
    app::{ApplicationError, HabitApplication},
    clock::{Clock, SystemClock, SystemTimeZone, TimeZoneSource},
    diagnostics::Diagnostics,
    habit::{
        DayProgress, MAX_HABIT_NAME_CHARS, MAX_ROUTINE_NAME_CHARS, ProjectedHabit, Routine,
        TodayHabit,
    },
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
    view: DashboardView,
    mode: InputMode,
    notice: Option<Notice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DashboardView {
    Today,
    Calendar,
    Contributions,
}

impl DashboardView {
    fn next(self) -> Self {
        match self {
            Self::Today => Self::Calendar,
            Self::Calendar => Self::Contributions,
            Self::Contributions => Self::Today,
        }
    }
}

enum InputMode {
    Normal,
    CreatingHabit {
        value: String,
        error: Option<String>,
    },
    CreatingRoutine {
        value: String,
        error: Option<String>,
    },
    EditingHabit {
        habit_id: i64,
        name: String,
        routines: Vec<RoutineChoice>,
        focus: SettingsFocus,
        routine_cursor: usize,
        error: Option<String>,
    },
}

#[derive(Clone)]
struct RoutineChoice {
    routine: Routine,
    selected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsFocus {
    Name,
    Routines,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HabitEntry {
    habit_index: usize,
    routine_id: Option<i64>,
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
            view: DashboardView::Today,
            mode: InputMode::Normal,
            notice: None,
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return true;
        }

        match &mut self.mode {
            InputMode::CreatingHabit { value, error } => match key.code {
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
            InputMode::CreatingRoutine { value, error } => match key.code {
                KeyCode::Esc => {
                    self.mode = InputMode::Normal;
                    self.notice = None;
                }
                KeyCode::Enter => {
                    let name = value.clone();
                    match self.application.create_routine(&name) {
                        Ok(()) => {
                            self.mode = InputMode::Normal;
                            self.notice = Some(Notice {
                                message: format!("created routine ‘{}’", name.trim()),
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
                        && value.chars().count() < MAX_ROUTINE_NAME_CHARS =>
                {
                    value.push(character);
                    *error = None;
                }
                _ => {}
            },
            InputMode::EditingHabit {
                habit_id,
                name,
                routines,
                focus,
                routine_cursor,
                error,
            } => match key.code {
                KeyCode::Esc => {
                    self.mode = InputMode::Normal;
                    self.notice = None;
                }
                KeyCode::Tab => {
                    *focus = match focus {
                        SettingsFocus::Name => SettingsFocus::Routines,
                        SettingsFocus::Routines => SettingsFocus::Name,
                    };
                    *error = None;
                }
                KeyCode::Enter => {
                    let habit_id = *habit_id;
                    let name = name.clone();
                    let routine_ids: Vec<_> = routines
                        .iter()
                        .filter(|choice| choice.selected)
                        .map(|choice| choice.routine.id)
                        .collect();
                    match self
                        .application
                        .update_habit_settings(habit_id, &name, &routine_ids)
                    {
                        Ok(()) => {
                            self.select_habit(habit_id, None);
                            self.mode = InputMode::Normal;
                            self.notice = Some(Notice {
                                message: format!("saved ‘{}’", name.trim()),
                                is_error: false,
                            });
                        }
                        Err(application_error) => {
                            *error = Some(application_error.to_string());
                        }
                    }
                }
                KeyCode::Backspace if *focus == SettingsFocus::Name => {
                    name.pop();
                    *error = None;
                }
                KeyCode::Char(' ') if *focus == SettingsFocus::Routines => {
                    if let Some(choice) = routines.get_mut(*routine_cursor) {
                        choice.selected = !choice.selected;
                    }
                    *error = None;
                }
                KeyCode::Up | KeyCode::Char('k') if *focus == SettingsFocus::Routines => {
                    *routine_cursor = routine_cursor.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') if *focus == SettingsFocus::Routines => {
                    if !routines.is_empty() {
                        *routine_cursor = (*routine_cursor + 1).min(routines.len() - 1);
                    }
                }
                KeyCode::Char(character)
                    if *focus == SettingsFocus::Name
                        && !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && name.chars().count() < MAX_HABIT_NAME_CHARS =>
                {
                    name.push(character);
                    *error = None;
                }
                _ => {}
            },
            InputMode::Normal => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return true,
                KeyCode::Tab => {
                    self.view = self.view.next();
                    self.notice = None;
                }
                KeyCode::Char('n') => {
                    self.begin_create_habit();
                }
                KeyCode::Char('r') => {
                    if self.application.is_viewing_today() {
                        self.mode = InputMode::CreatingRoutine {
                            value: String::new(),
                            error: None,
                        };
                        self.notice = None;
                    } else {
                        self.read_only_notice();
                    }
                }
                KeyCode::Char('e') => self.begin_edit_habit(),
                KeyCode::Up | KeyCode::Char('k') => {
                    if !self.habit_entries().is_empty() {
                        self.selected = self.selected.saturating_sub(1);
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let entries = self.habit_entries();
                    if !entries.is_empty() {
                        self.selected = (self.selected + 1).min(entries.len() - 1);
                    }
                }
                KeyCode::Left | KeyCode::Char('h') => self.move_day(-1),
                KeyCode::Right | KeyCode::Char('l') => self.move_day(1),
                KeyCode::Char('[') => self.move_month(-1),
                KeyCode::Char(']') => self.move_month(1),
                KeyCode::Char('t') => self.select_date(self.application.today()),
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
        let entries = self.habit_entries();
        let Some(entry) = entries.get(self.selected).copied() else {
            return;
        };
        let habit = &self.application.habits()[entry.habit_index];
        let occurrence_id = habit.occurrence_id;
        let habit_name = habit.name.clone();
        let was_completed = habit.completed;
        let previous_selection = self.selected;

        match self.application.toggle(occurrence_id) {
            Ok(()) => {
                let updated_entries = self.habit_entries();
                let toggled_position = updated_entries
                    .iter()
                    .position(|candidate| {
                        let habit = &self.application.habits()[candidate.habit_index];
                        habit.occurrence_id == occurrence_id
                            && candidate.routine_id == entry.routine_id
                    })
                    .unwrap_or(0);

                self.selected = if was_completed {
                    toggled_position
                } else {
                    updated_entries
                        .iter()
                        .enumerate()
                        .skip(previous_selection)
                        .find(|(_, candidate)| {
                            !self.application.habits()[candidate.habit_index].completed
                        })
                        .or_else(|| {
                            updated_entries
                                .iter()
                                .enumerate()
                                .rev()
                                .find(|(_, candidate)| {
                                    !self.application.habits()[candidate.habit_index].completed
                                })
                        })
                        .map_or(toggled_position, |(position, _)| position)
                };
                self.notice = Some(Notice {
                    message: format!("updated ‘{habit_name}’"),
                    is_error: false,
                });
            }
            Err(error) => self.set_error(error),
        }
    }

    fn begin_create_habit(&mut self) {
        if !self.application.is_viewing_today() {
            self.read_only_notice();
            return;
        }
        self.mode = InputMode::CreatingHabit {
            value: String::new(),
            error: None,
        };
        self.notice = None;
    }

    fn begin_edit_habit(&mut self) {
        if !self.application.is_viewing_today() {
            self.read_only_notice();
            return;
        }
        let entries = self.habit_entries();
        let Some(entry) = entries.get(self.selected) else {
            return;
        };
        let habit = &self.application.habits()[entry.habit_index];
        let routines = self
            .application
            .routines()
            .iter()
            .map(|routine| RoutineChoice {
                routine: routine.clone(),
                selected: habit
                    .routines
                    .iter()
                    .any(|assigned| assigned.id == routine.id),
            })
            .collect();
        self.mode = InputMode::EditingHabit {
            habit_id: habit.habit_id,
            name: habit.name.clone(),
            routines,
            focus: SettingsFocus::Name,
            routine_cursor: 0,
            error: None,
        };
        self.notice = None;
    }

    fn habit_entries(&self) -> Vec<HabitEntry> {
        habit_entries(self.application.habits(), self.application.routines())
    }

    fn select_habit(&mut self, habit_id: i64, routine_id: Option<i64>) {
        self.selected = self
            .habit_entries()
            .iter()
            .position(|entry| {
                let habit = &self.application.habits()[entry.habit_index];
                habit.habit_id == habit_id
                    && routine_id.is_none_or(|routine_id| entry.routine_id == Some(routine_id))
            })
            .unwrap_or(0);
    }

    fn move_day(&mut self, offset: i8) {
        let date = if offset < 0 {
            self.application.selected_date().yesterday()
        } else {
            self.application.selected_date().tomorrow()
        };
        if let Ok(date) = date {
            self.select_date(date);
        }
    }

    fn move_month(&mut self, offset: i8) {
        self.select_date(shift_month(self.application.selected_date(), offset));
    }

    fn select_date(&mut self, date: Date) {
        match self.application.select_date(date) {
            Ok(()) => {
                self.selected = 0;
                self.notice = if self.application.is_viewing_today() {
                    Some(Notice {
                        message: "returned to today".to_owned(),
                        is_error: false,
                    })
                } else if self.application.is_viewing_future() {
                    Some(Notice {
                        message: format!("viewing {} · upcoming preview", date),
                        is_error: false,
                    })
                } else {
                    Some(Notice {
                        message: format!("viewing {} · read-only", date),
                        is_error: false,
                    })
                };
            }
            Err(error) => self.set_error(error),
        }
    }

    fn read_only_notice(&mut self) {
        self.notice = Some(Notice {
            message: if self.application.is_viewing_future() {
                format!(
                    "{} is an upcoming preview · press t for today",
                    self.application.selected_date()
                )
            } else {
                format!(
                    "{} is read-only history · press t for today",
                    self.application.selected_date()
                )
            },
            is_error: true,
        });
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

    render_header(frame, shell[0], diagnostics, state.view);

    match state.view {
        DashboardView::Calendar => render_calendar(frame, shell[1], state),
        DashboardView::Contributions => render_contributions(frame, shell[1], state),
        DashboardView::Today => match LayoutMode::for_width(area.width) {
            LayoutMode::Wide => render_wide_dashboard(frame, shell[1], state),
            LayoutMode::Medium => render_medium_dashboard(frame, shell[1], state),
            LayoutMode::Narrow => render_narrow_dashboard(frame, shell[1], state),
        },
    }

    render_footer(frame, shell[2], state);

    match &state.mode {
        InputMode::CreatingHabit { .. } => {
            render_name_dialog(frame, area, state, NameDialogKind::Habit)
        }
        InputMode::CreatingRoutine { .. } => {
            render_name_dialog(frame, area, state, NameDialogKind::Routine)
        }
        InputMode::EditingHabit { .. } => render_settings_dialog(frame, area, state),
        InputMode::Normal => {}
    }
}

fn render_header(
    frame: &mut Frame<'_>,
    area: Rect,
    diagnostics: &Diagnostics,
    view: DashboardView,
) {
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
            Span::styled("  TODAY", tab_style(view == DashboardView::Today)),
            Span::styled("   CALENDAR", tab_style(view == DashboardView::Calendar)),
            Span::styled(
                "   CONTRIBUTIONS",
                tab_style(view == DashboardView::Contributions),
            ),
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
    let total = if state.application.is_viewing_future() {
        state.application.projected_habits().len()
    } else {
        state.application.habits().len()
    };
    let percentage = state.application.completion_percentage();
    let headline_text = if state.application.is_viewing_future() {
        scheduled_habit_label(total)
    } else {
        format!("{percentage}%  {completed} of {total} complete")
    };

    frame.render_widget(
        Paragraph::new(Span::styled(
            headline_text,
            Style::default()
                .fg(palette::WASHI)
                .add_modifier(Modifier::BOLD),
        )),
        headline[0],
    );
    frame.render_widget(
        Paragraph::new(Span::styled(
            state.application.selected_date().to_string(),
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

    let guidance = if state.application.is_viewing_future() {
        Line::from(vec![
            Span::styled("予定", Style::default().fg(palette::INDIGO)),
            Span::styled(
                "  upcoming day · read-only preview · progress begins when this day starts",
                muted(),
            ),
        ])
    } else if !state.application.is_viewing_today() {
        Line::from(vec![
            Span::styled("履歴", Style::default().fg(palette::INDIGO)),
            Span::styled("  historical view · read-only · press ", muted()),
            key("t"),
            Span::styled(" for today", muted()),
        ])
    } else if total == 0 {
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
    let title = if state.application.is_viewing_today() {
        " 今日  TODAY ".to_owned()
    } else if state.application.is_viewing_future() {
        format!(" 予定  UPCOMING · {} ", state.application.selected_date())
    } else {
        format!(" 履歴  {} ", state.application.selected_date())
    };
    let (inner, block) = panel(area, title);
    frame.render_widget(block, area);

    if state.application.is_viewing_future() {
        render_projected_habits(frame, inner, state);
        return;
    }

    let habits = state.application.habits();
    if habits.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    if state.application.is_viewing_today() {
                        "No habits scheduled today."
                    } else {
                        "No habits were scheduled on this day."
                    },
                    Style::default().fg(palette::WASHI),
                )),
                Line::from(""),
                if state.application.is_viewing_today() {
                    Line::from(vec![
                        Span::styled("Press ", muted()),
                        key("n"),
                        Span::styled(" to create a daily binary habit.", muted()),
                    ])
                } else {
                    Line::from(Span::styled("Use h/l to browse nearby days.", muted()))
                },
            ])
            .alignment(Alignment::Center),
            inner,
        );
        return;
    }

    let groups = habit_groups(habits, state.application.routines());
    let mut selectable_index = 0;
    let mut selected_line = 0;
    let mut all_lines = Vec::new();
    for group in groups {
        let group_completed = group
            .habit_indices
            .iter()
            .filter(|index| habits[**index].completed)
            .count();
        all_lines.push(Line::from(vec![
            Span::styled(
                format!("┌─ {}", group.name.to_uppercase()),
                Style::default()
                    .fg(palette::INDIGO)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  [{group_completed} / {}]", group.habit_indices.len()),
                muted(),
            ),
        ]));
        for habit_index in group.habit_indices {
            let selected = selectable_index == state.selected;
            if selected {
                selected_line = all_lines.len();
            }
            all_lines.push(habit_line(&habits[habit_index], selected));
            selectable_index += 1;
        }
    }

    let visible_lines = inner.height as usize;
    let start = selected_line.saturating_sub(visible_lines.saturating_sub(1));
    frame.render_widget(
        Paragraph::new(
            all_lines
                .into_iter()
                .skip(start)
                .take(visible_lines)
                .collect::<Vec<_>>(),
        ),
        inner,
    );
}

fn render_projected_habits<C: Clock, T: TimeZoneSource>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiState<C, T>,
) {
    let habits = state.application.projected_habits();
    if habits.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "No habits are currently scheduled for this day.",
                    Style::default().fg(palette::WASHI),
                )),
                Line::from(""),
                Line::from(Span::styled("Use h/l to browse nearby days.", muted())),
            ])
            .alignment(Alignment::Center),
            area,
        );
        return;
    }

    let groups = habit_groups(habits, state.application.routines());
    let mut lines = Vec::new();
    for group in groups {
        lines.push(Line::from(vec![
            Span::styled(
                format!("┌─ {}", group.name.to_uppercase()),
                Style::default()
                    .fg(palette::INDIGO)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  [{} scheduled]", group.habit_indices.len()),
                muted(),
            ),
        ]));
        for habit_index in group.habit_indices {
            lines.push(projected_habit_line(&habits[habit_index]));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!(
            "{} · progress begins when this day starts",
            scheduled_habit_label(habits.len())
        ),
        muted(),
    )));
    frame.render_widget(
        Paragraph::new(
            lines
                .into_iter()
                .take(area.height as usize)
                .collect::<Vec<_>>(),
        ),
        area,
    );
}

fn render_calendar<C: Clock, T: TimeZoneSource>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiState<C, T>,
) {
    let selected = state.application.selected_date();
    let today = state.application.today();
    let title = format!(
        " 暦  ‹ {} {} › ",
        month_name(selected.month()),
        selected.year()
    );
    let (inner, block) = panel(area, title);
    frame.render_widget(block, area);

    let progress: BTreeMap<_, _> = state
        .application
        .contributions()
        .iter()
        .map(|day| (day.date, day))
        .collect();
    let mut lines = calendar_lines(selected, today, &progress);
    lines.push(Line::from(""));
    let summary = if state.application.is_viewing_future() {
        format!(
            "{selected}  ·  {}",
            scheduled_habit_label(state.application.projected_habits().len())
        )
    } else {
        progress.get(&selected).map_or_else(
            || format!("{selected}  ·  no habits"),
            |day| {
                format!(
                    "{selected}  ·  {}%  [{}/{}]",
                    day.percentage(),
                    day.completed,
                    day.scheduled
                )
            },
        )
    };
    lines.push(Line::from(Span::styled(
        summary,
        Style::default().fg(palette::MOSS),
    )));
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
}

fn render_contributions<C: Clock, T: TimeZoneSource>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiState<C, T>,
) {
    let (inner, block) = panel(area, " 足跡  CONTRIBUTIONS ");
    frame.render_widget(block, area);

    let progress: BTreeMap<_, _> = state
        .application
        .contributions()
        .iter()
        .map(|day| (day.date, day))
        .collect();
    if inner.height < 8 {
        let days = inner.width.saturating_sub(2).min(42) as usize;
        let start = state
            .application
            .today()
            .saturating_sub(JiffSpan::new().days(days.saturating_sub(1) as i64));
        let mut cells = vec![Span::styled("  ", muted())];
        for offset in 0..days {
            let date = start.saturating_add(JiffSpan::new().days(offset as i64));
            cells.push(contribution_cell(
                date,
                state.application.today(),
                progress.get(&date).copied(),
            ));
        }
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled("recent consistency", muted())),
                Line::from(cells),
                Line::from(Span::styled("· none  ░ 0%  ▒▓█ progress", muted())),
            ]),
            inner,
        );
        return;
    }
    let weeks = ((inner.width.saturating_sub(4) / 2) as usize).clamp(4, 26);
    let end = state
        .application
        .today()
        .saturating_add(JiffSpan::new().days(i64::from(
            6 - state.application.today().weekday().to_monday_zero_offset(),
        )));
    let start = end.saturating_sub(JiffSpan::new().days((weeks * 7 - 1) as i64));
    let mut lines = vec![Line::from(vec![
        Span::styled("    ", muted()),
        Span::styled(format!("last {weeks} weeks"), muted()),
    ])];
    for weekday in 0..7 {
        let label = match weekday {
            0 => "M ",
            2 => "W ",
            4 => "F ",
            _ => "  ",
        };
        let mut spans = vec![Span::styled(label, muted())];
        for week in 0..weeks {
            let date = start.saturating_add(JiffSpan::new().days((week * 7 + weekday) as i64));
            spans.push(contribution_cell(
                date,
                state.application.today(),
                progress.get(&date).copied(),
            ));
            spans.push(Span::raw(" "));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_footer<C: Clock, T: TimeZoneSource>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiState<C, T>,
) {
    if !matches!(state.mode, InputMode::Normal) {
        let editing = matches!(state.mode, InputMode::EditingHabit { .. });
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                key("enter"),
                Span::styled(if editing { " save   " } else { " create   " }, muted()),
                if editing { key("tab") } else { Span::raw("") },
                Span::styled(if editing { " field   " } else { "" }, muted()),
                key("esc"),
                Span::styled(" cancel", muted()),
            ]))
            .block(footer_block()),
            area,
        );
        return;
    }

    if area.width < MEDIUM_TERMINAL_MIN_COLUMNS {
        if !state.application.is_viewing_today() {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    key("h/l"),
                    Span::styled(" date  ", muted()),
                    key("t"),
                    Span::styled(" today  ", muted()),
                    key("tab"),
                    Span::styled(" view", muted()),
                ]))
                .block(footer_block()),
                area,
            );
            return;
        }
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                key("n"),
                Span::styled(" new  ", muted()),
                key("r"),
                Span::styled(" routine  ", muted()),
                key("e"),
                Span::styled(" edit  ", muted()),
                key("space"),
                Span::styled(" done", muted()),
                Span::styled("  ", muted()),
                key("tab"),
                Span::styled(" view", muted()),
            ]))
            .block(footer_block()),
            area,
        );
        return;
    }

    if !state.application.is_viewing_today() {
        let columns = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                key("h/l"),
                Span::styled(" date   ", muted()),
                key("t"),
                Span::styled(" today   ", muted()),
                key("tab"),
                Span::styled(" view", muted()),
            ]))
            .block(footer_block()),
            columns[0],
        );
        let message = state.notice.as_ref().map_or_else(
            || {
                if state.application.is_viewing_future() {
                    format!(
                        "viewing {} · upcoming preview",
                        state.application.selected_date()
                    )
                } else {
                    format!("viewing {} · read-only", state.application.selected_date())
                }
            },
            |notice| notice.message.clone(),
        );
        frame.render_widget(
            Paragraph::new(message)
                .alignment(Alignment::Right)
                .style(Style::default().fg(palette::MOSS))
                .block(footer_block()),
            columns[1],
        );
        return;
    }
    let columns = Layout::horizontal([Constraint::Min(48), Constraint::Length(34)]).split(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            key("n"),
            Span::styled(" new   ", muted()),
            key("r"),
            Span::styled(" routine   ", muted()),
            key("e"),
            Span::styled(" edit   ", muted()),
            key("space"),
            Span::styled(" toggle   ", muted()),
            key("h/l"),
            Span::styled(" date   ", muted()),
            key("tab"),
            Span::styled(" view", muted()),
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

#[derive(Clone, Copy)]
enum NameDialogKind {
    Habit,
    Routine,
}

fn render_name_dialog<C: Clock, T: TimeZoneSource>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiState<C, T>,
    kind: NameDialogKind,
) {
    let (value, error, maximum, title) = match (&state.mode, kind) {
        (InputMode::CreatingHabit { value, error }, NameDialogKind::Habit) => {
            (value, error, MAX_HABIT_NAME_CHARS, " NEW DAILY HABIT ")
        }
        (InputMode::CreatingRoutine { value, error }, NameDialogKind::Routine) => {
            (value, error, MAX_ROUTINE_NAME_CHARS, " NEW ROUTINE ")
        }
        _ => return,
    };
    let popup = centered_rect(area, 70, 7);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(palette::VERMILION))
        .style(Style::default().bg(palette::SUMI))
        .title(Span::styled(title, accent()));
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
        || format!("{} characters remaining", maximum - value.chars().count()),
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

fn render_settings_dialog<C: Clock, T: TimeZoneSource>(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &TuiState<C, T>,
) {
    let InputMode::EditingHabit {
        name,
        routines,
        focus,
        routine_cursor,
        error,
        ..
    } = &state.mode
    else {
        return;
    };
    let height = (10 + routines.len() as u16).min(area.height.saturating_sub(2));
    let popup = centered_rect(area, 72, height);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(palette::VERMILION))
        .style(Style::default().bg(palette::SUMI))
        .title(Span::styled(" HABIT SETTINGS ", accent()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut lines = vec![
        Line::from(Span::styled(
            if *focus == SettingsFocus::Name {
                "› NAME"
            } else {
                "  NAME"
            },
            if *focus == SettingsFocus::Name {
                accent()
            } else {
                muted()
            },
        )),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(name.clone(), Style::default().fg(palette::WASHI)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            if *focus == SettingsFocus::Routines {
                "› ROUTINES  [space toggles]"
            } else {
                "  ROUTINES"
            },
            if *focus == SettingsFocus::Routines {
                accent()
            } else {
                muted()
            },
        )),
    ];
    if routines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No routines yet. Save, then press r to create one.",
            muted(),
        )));
    } else {
        lines.extend(routines.iter().enumerate().map(|(index, choice)| {
            let marker = if choice.selected { "●" } else { "○" };
            let cursor = if *focus == SettingsFocus::Routines && index == *routine_cursor {
                "›"
            } else {
                " "
            };
            Line::from(vec![
                Span::styled(cursor, Style::default().fg(palette::VERMILION)),
                Span::raw(" "),
                Span::styled(
                    marker,
                    Style::default().fg(if choice.selected {
                        palette::MOSS
                    } else {
                        palette::STONE
                    }),
                ),
                Span::raw(" "),
                Span::styled(
                    choice.routine.name.clone(),
                    Style::default().fg(palette::WASHI),
                ),
            ])
        }));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        error
            .as_deref()
            .unwrap_or("Tab changes field · Enter saves"),
        Style::default().fg(if error.is_some() {
            palette::VERMILION
        } else {
            palette::STONE
        }),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

fn calendar_lines(
    selected: Date,
    today: Date,
    progress: &BTreeMap<Date, &DayProgress>,
) -> Vec<Line<'static>> {
    let first = Date::new(selected.year(), selected.month(), 1).expect("selected month is valid");
    let offset = first.weekday().to_monday_zero_offset() as usize;
    let days = selected.days_in_month() as usize;
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
            let date = Date::new(selected.year(), selected.month(), day as i8)
                .expect("calendar day is valid");
            if date == selected {
                spans.push(Span::styled(
                    label,
                    Style::default()
                        .fg(palette::SUMI)
                        .bg(palette::VERMILION)
                        .add_modifier(Modifier::BOLD),
                ));
            } else if date > today {
                spans.push(Span::styled(
                    label,
                    Style::default().fg(palette::VERMILION_DARK),
                ));
            } else if let Some(day_progress) = progress.get(&date) {
                spans.push(Span::styled(
                    label,
                    contribution_text_style(day_progress.percentage()),
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

fn projected_habit_line(habit: &ProjectedHabit) -> Line<'static> {
    Line::from(vec![
        Span::styled("│  ", Style::default().fg(palette::VERMILION_DARK)),
        Span::styled("○", Style::default().fg(palette::STONE)),
        Span::raw(" "),
        Span::styled(habit.name.clone(), Style::default().fg(palette::WASHI)),
    ])
}

struct HabitGroup {
    name: String,
    routine_id: Option<i64>,
    habit_indices: Vec<usize>,
}

trait GroupedHabit {
    fn routines(&self) -> &[Routine];
}

impl GroupedHabit for TodayHabit {
    fn routines(&self) -> &[Routine] {
        &self.routines
    }
}

impl GroupedHabit for ProjectedHabit {
    fn routines(&self) -> &[Routine] {
        &self.routines
    }
}

fn habit_groups<H: GroupedHabit>(habits: &[H], routines: &[Routine]) -> Vec<HabitGroup> {
    let mut known_routines: Vec<Routine> = routines.to_vec();
    for routine in habits.iter().flat_map(GroupedHabit::routines) {
        if !known_routines.iter().any(|known| known.id == routine.id) {
            known_routines.push(routine.clone());
        }
    }

    let mut groups = Vec::new();
    for routine in known_routines {
        let habit_indices: Vec<_> = habits
            .iter()
            .enumerate()
            .filter(|(_, habit)| {
                habit
                    .routines()
                    .iter()
                    .any(|assigned| assigned.id == routine.id)
            })
            .map(|(index, _)| index)
            .collect();
        if !habit_indices.is_empty() {
            groups.push(HabitGroup {
                name: routine.name,
                routine_id: Some(routine.id),
                habit_indices,
            });
        }
    }

    let ungrouped: Vec<_> = habits
        .iter()
        .enumerate()
        .filter(|(_, habit)| habit.routines().is_empty())
        .map(|(index, _)| index)
        .collect();
    if !ungrouped.is_empty() {
        groups.push(HabitGroup {
            name: "ungrouped".to_owned(),
            routine_id: None,
            habit_indices: ungrouped,
        });
    }
    groups
}

fn habit_entries(habits: &[TodayHabit], routines: &[Routine]) -> Vec<HabitEntry> {
    habit_groups(habits, routines)
        .into_iter()
        .flat_map(|group| {
            group
                .habit_indices
                .into_iter()
                .map(move |habit_index| HabitEntry {
                    habit_index,
                    routine_id: group.routine_id,
                })
        })
        .collect()
}

fn contribution_cell(date: Date, today: Date, progress: Option<&DayProgress>) -> Span<'static> {
    if date > today {
        return Span::styled("·", Style::default().fg(palette::VERMILION_DARK));
    }
    let Some(progress) = progress else {
        return Span::styled("·", Style::default().fg(palette::STONE));
    };
    let percentage = progress.percentage();
    let symbol = match percentage {
        0 => "░",
        1..=33 => "▒",
        34..=66 => "▓",
        _ => "█",
    };
    Span::styled(symbol, contribution_text_style(percentage))
}

fn contribution_text_style(percentage: u16) -> Style {
    match percentage {
        0 => Style::default().fg(palette::VERMILION_DARK),
        1..=33 => Style::default().fg(palette::INDIGO),
        34..=66 => Style::default().fg(palette::GOLD),
        _ => Style::default()
            .fg(palette::MOSS)
            .add_modifier(Modifier::BOLD),
    }
}

fn scheduled_habit_label(count: usize) -> String {
    format!(
        "{count} habit{} scheduled",
        if count == 1 { "" } else { "s" }
    )
}

fn shift_month(date: Date, offset: i8) -> Date {
    let month_index = date.year() * 12 + i16::from(date.month() - 1) + i16::from(offset);
    let year = month_index.div_euclid(12);
    let month = (month_index.rem_euclid(12) + 1) as i8;
    let first = Date::new(year, month, 1).expect("shifted month is representable");
    Date::new(year, month, date.day().min(first.days_in_month()))
        .expect("clamped shifted date is valid")
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

fn tab_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(palette::WASHI)
            .bg(palette::VERMILION)
            .add_modifier(Modifier::BOLD)
    } else {
        muted()
    }
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
            schema_version: 3,
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
        assert!(output.contains("No habits scheduled today"));
        assert!(output.contains("CONTRIBUTIONS"));
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
    fn completing_a_habit_moves_it_below_unchecked_habits_and_advances_focus() {
        let mut state = state();
        for name in ["first", "second", "third"] {
            state.application.create_daily_binary(name).unwrap();
        }
        state.selected = 1;

        state.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));

        let names: Vec<_> = state
            .application
            .habits()
            .iter()
            .map(|habit| habit.name.as_str())
            .collect();
        assert_eq!(names, ["first", "third", "second"]);
        assert_eq!(state.application.habits()[state.selected].name, "third");
    }

    #[test]
    fn unchecking_a_completed_habit_keeps_it_selected() {
        let mut state = state();
        for name in ["first", "second"] {
            state.application.create_daily_binary(name).unwrap();
        }
        state.selected = 0;
        state.toggle_selected();
        assert_eq!(state.application.habits()[state.selected].name, "second");

        state.selected = 1;
        state.toggle_selected();

        assert_eq!(state.application.habits()[state.selected].name, "first");
        assert!(!state.application.habits()[state.selected].completed);
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
    fn routine_creation_and_habit_settings_assign_a_real_group() {
        let mut state = state();
        state.application.create_daily_binary("read").unwrap();
        state.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        for character in "morning".chars() {
            state.handle_key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
        }
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        state.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        state.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        state.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(state.application.routines()[0].name, "morning");
        assert_eq!(state.application.habits()[0].routines[0].name, "morning");
        assert!(rendered_text(120, 36, &state).contains("MORNING"));
    }

    #[test]
    fn a_habit_can_appear_in_multiple_routine_groups_without_double_counting() {
        let mut state = state();
        state.application.create_daily_binary("stretch").unwrap();
        state.application.create_routine("morning").unwrap();
        state.application.create_routine("recovery").unwrap();
        let habit_id = state.application.habits()[0].habit_id;
        let routine_ids: Vec<_> = state
            .application
            .routines()
            .iter()
            .map(|routine| routine.id)
            .collect();
        state
            .application
            .update_habit_settings(habit_id, "stretch", &routine_ids)
            .unwrap();

        assert_eq!(state.habit_entries().len(), 2);
        assert_eq!(state.application.habits().len(), 1);
        let output = rendered_text(120, 36, &state);
        assert!(output.contains("MORNING"));
        assert!(output.contains("RECOVERY"));
        assert!(output.contains("0 of 1 complete"));
    }

    #[test]
    fn calendar_navigation_loads_read_only_history_and_returns_to_today() {
        let mut state = state();
        state.application.create_daily_binary("read").unwrap();

        state.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        assert_eq!(
            state.application.selected_date(),
            Date::new(2026, 8, 22).unwrap()
        );
        assert!(!state.application.is_viewing_today());
        assert!(rendered_text(120, 36, &state).contains("2026-08-22"));

        state.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(matches!(state.mode, InputMode::Normal));
        assert!(state.notice.as_ref().unwrap().is_error);

        state.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
        assert_eq!(state.application.selected_date(), state.application.today());
        assert!(state.application.is_viewing_today());
    }

    #[test]
    fn future_date_renders_a_grouped_upcoming_preview() {
        let mut state = state();
        state.application.create_daily_binary("read").unwrap();
        state.application.create_routine("evening").unwrap();
        let habit_id = state.application.habits()[0].habit_id;
        let routine_id = state.application.routines()[0].id;
        state
            .application
            .update_habit_settings(habit_id, "read", &[routine_id])
            .unwrap();

        state.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        let output = rendered_text(120, 36, &state);

        assert!(output.contains("UPCOMING · 2026-08-24"));
        assert!(output.contains("upcoming day · read-only preview"));
        assert!(output.contains("EVENING"));
        assert!(output.contains("read"));
        assert!(output.contains("1 habit scheduled"));
        assert!(output.contains("viewing 2026-08-24 · upcoming preview"));
        assert!(state.application.habits().is_empty());
    }

    #[test]
    fn contribution_panel_renders_persisted_completion_intensity() {
        let mut state = state();
        state.application.create_daily_binary("read").unwrap();
        state.toggle_selected();

        let output = rendered_text(120, 36, &state);
        assert!(output.contains("CONTRIBUTIONS"));
        assert!(output.contains("last"));
        assert!(output.contains('█'));
    }

    #[test]
    fn narrow_dashboard_keeps_empty_state_and_create_action_visible() {
        let output = rendered_text(52, 30, &state());

        assert!(output.contains("0%"));
        assert!(output.contains("No habits scheduled today"));
        assert!(output.contains("new"));
        assert!(!output.contains("recent consistency"));
    }

    #[test]
    fn compact_medium_dashboard_omits_secondary_history_panel() {
        let output = rendered_text(80, 24, &state());

        assert!(output.contains("STATUS"));
        assert!(output.contains("TODAY"));
        assert!(output.contains("No habits scheduled today"));
        assert!(!output.contains("recent consistency"));
    }

    #[test]
    fn compact_terminals_can_focus_calendar_and_contribution_views() {
        let mut state = state();
        state.application.create_daily_binary("read").unwrap();

        state.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let calendar = rendered_text(52, 30, &state);
        assert_eq!(state.view, DashboardView::Calendar);
        assert!(calendar.contains("CALENDAR"));
        assert!(calendar.contains("AUGUST 2026"));

        state.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let contributions = rendered_text(52, 30, &state);
        assert_eq!(state.view, DashboardView::Contributions);
        assert!(contributions.contains("CONTRIBUTIONS"));
        assert!(contributions.contains("last"));
    }

    #[test]
    fn layout_breakpoints_are_explicit() {
        assert_eq!(LayoutMode::for_width(63), LayoutMode::Narrow);
        assert_eq!(LayoutMode::for_width(64), LayoutMode::Medium);
        assert_eq!(LayoutMode::for_width(95), LayoutMode::Medium);
        assert_eq!(LayoutMode::for_width(96), LayoutMode::Wide);
    }
}
