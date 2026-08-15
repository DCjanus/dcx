use std::collections::BTreeSet;
use std::io::{self, Stderr};

use anyhow::Context;
use crossterm::cursor::{Hide, Show};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use size::Size;

use crate::AnyResult;
use crate::cargo::CleanupCandidate;

type Tui = Terminal<CrosstermBackend<Stderr>>;

pub(crate) fn select_candidates(candidates: Vec<CleanupCandidate>) -> AnyResult<Vec<usize>> {
    let mut terminal = start_terminal()?;
    let _guard = TerminalGuard;
    let mut app = App::new(candidates);

    loop {
        terminal
            .draw(|frame| render(frame, &app))
            .context("failed to draw Cargo cache selector")?;
        let Event::Key(key) = event::read().context("failed to read terminal input")? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match app.handle_key(key) {
            Outcome::Continue => {}
            Outcome::Cancel => return Ok(Vec::new()),
            Outcome::Delete => return Ok(app.selected.into_iter().collect()),
        }
    }
}

fn start_terminal() -> AnyResult<Tui> {
    enable_raw_mode().context("failed to enable terminal raw mode")?;
    let mut stderr = io::stderr();
    if let Err(error) = execute!(stderr, EnterAlternateScreen, EnableMouseCapture, Hide) {
        let _ = disable_raw_mode();
        return Err(error).context("failed to enter alternate screen");
    }
    match Terminal::new(CrosstermBackend::new(stderr)) {
        Ok(terminal) => Ok(terminal),
        Err(error) => {
            let _ = disable_raw_mode();
            let _ = execute!(
                io::stderr(),
                LeaveAlternateScreen,
                DisableMouseCapture,
                Show
            );
            Err(error).context("failed to initialize terminal")
        }
    }
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stderr(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            Show
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Continue,
    Cancel,
    Delete,
}

struct App {
    candidates: Vec<CleanupCandidate>,
    selected: BTreeSet<usize>,
    cursor: usize,
    confirming: bool,
}

impl App {
    fn new(candidates: Vec<CleanupCandidate>) -> Self {
        Self {
            selected: (0..candidates.len()).collect(),
            candidates,
            cursor: 0,
            confirming: false,
        }
    }

    fn current(&self) -> Option<&CleanupCandidate> {
        self.candidates.get(self.cursor)
    }

    fn selected_size(&self) -> u64 {
        self.selected
            .iter()
            .filter_map(|index| self.candidates.get(*index))
            .map(|candidate| candidate.size)
            .sum()
    }

    fn handle_key(&mut self, key: KeyEvent) -> Outcome {
        if self.confirming {
            return match key.code {
                KeyCode::Enter => Outcome::Delete,
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.confirming = false;
                    Outcome::Continue
                }
                _ => Outcome::Continue,
            };
        }
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Outcome::Cancel,
            KeyCode::Esc | KeyCode::Char('q') => Outcome::Cancel,
            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor = self.cursor.saturating_sub(1);
                Outcome::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.cursor = self
                    .cursor
                    .saturating_add(1)
                    .min(self.candidates.len().saturating_sub(1));
                Outcome::Continue
            }
            KeyCode::Home => {
                self.cursor = 0;
                Outcome::Continue
            }
            KeyCode::End => {
                self.cursor = self.candidates.len().saturating_sub(1);
                Outcome::Continue
            }
            KeyCode::Char(' ') => {
                if !self.selected.remove(&self.cursor) {
                    self.selected.insert(self.cursor);
                }
                Outcome::Continue
            }
            KeyCode::Char('a') => {
                if self.selected.len() == self.candidates.len() {
                    self.selected.clear();
                } else {
                    self.selected = (0..self.candidates.len()).collect();
                }
                Outcome::Continue
            }
            KeyCode::Char('x') => {
                self.selected.clear();
                Outcome::Continue
            }
            KeyCode::Enter if !self.selected.is_empty() => {
                self.confirming = true;
                Outcome::Continue
            }
            _ => Outcome::Continue,
        }
    }
}

fn render(frame: &mut ratatui::Frame<'_>, app: &App) {
    let area = frame.area();
    if area.width < 72 || area.height < 18 {
        frame.render_widget(
            Paragraph::new("Terminal is too small\nResize it to at least 72 x 18")
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Cargo cache cleanup "),
                ),
            area,
        );
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(3),
        ])
        .split(area);
    render_header(frame, rows[0], app);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
        .split(rows[1]);
    render_list(frame, columns[0], app);
    render_details(frame, columns[1], app);
    render_footer(frame, rows[2], app);
    if app.confirming {
        render_confirmation(frame, area, app);
    }
}

fn render_header(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let title = Line::from(vec![
        Span::styled(
            " Cargo cache cleanup ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "  {} groups | {} selected | {} reclaimable",
            app.candidates.len(),
            app.selected.len(),
            Size::from_bytes(app.selected_size())
        )),
    ]);
    frame.render_widget(
        Paragraph::new(title).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn render_list(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let items = app
        .candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let (marker, color) = if app.selected.contains(&index) {
                ("[x]", Color::Green)
            } else {
                ("[ ]", Color::Gray)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {marker} "), Style::default().fg(color)),
                Span::styled(
                    format!("{:<11}", candidate.kind.label()),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(format!(
                    "{}  {}",
                    Size::from_bytes(candidate.size),
                    candidate.profile
                )),
            ]))
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(app.cursor.min(items.len() - 1)));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Cache groups ")
                    .title_bottom(" Up/Down move | Space toggle "),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Rgb(35, 45, 60))
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">"),
        area,
        &mut state,
    );
}

fn render_details(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let Some(candidate) = app.current() else {
        return;
    };
    let lines = vec![
        Line::raw(format!("Profile   {}", candidate.profile)),
        Line::raw(format!("Category  {}", candidate.kind.label())),
        Line::raw(format!("Size      {}", Size::from_bytes(candidate.size))),
        Line::raw(format!("Paths     {}", candidate.paths.len())),
        Line::raw(""),
        Line::styled("Reason", Style::default().add_modifier(Modifier::BOLD)),
        Line::raw(&candidate.reason),
        Line::raw(""),
        Line::styled(
            "All selected data can be rebuilt by Cargo.",
            Style::default().fg(Color::Yellow),
        ),
    ];
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Audit details "),
        ),
        area,
    );
}

fn render_footer(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let enter = if app.selected.is_empty() {
        "disabled"
    } else {
        "confirm"
    };
    let help = Line::from(vec![
        Span::styled(" a ", key_style()),
        Span::raw("all  "),
        Span::styled(" x ", key_style()),
        Span::raw("clear  "),
        Span::styled(" Enter ", key_style()),
        Span::raw(format!("{enter}  ")),
        Span::styled(" q ", key_style()),
        Span::raw("quit"),
    ]);
    frame.render_widget(
        Paragraph::new(help)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn key_style() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::Gray)
        .add_modifier(Modifier::BOLD)
}

fn render_confirmation(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let popup = centered_rect(66, 46, area);
    frame.render_widget(Clear, popup);
    let lines = vec![
        Line::styled(
            format!(
                "Remove {} cache groups ({})?",
                app.selected.len(),
                Size::from_bytes(app.selected_size())
            ),
            Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::raw("Cargo can rebuild this data, but the next build may be slower."),
        Line::raw("Cleanup is refused while the target profile is active."),
        Line::raw(""),
        Line::from(vec![
            Span::styled(" Enter ", key_style()),
            Span::raw("remove     "),
            Span::styled(" Esc ", key_style()),
            Span::raw("back"),
        ])
        .alignment(Alignment::Center),
    ];
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::LightRed))
                .title(" Confirm cleanup "),
        ),
        popup,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::cargo::CandidateKind;

    fn candidate(kind: CandidateKind) -> CleanupCandidate {
        CleanupCandidate {
            kind,
            profile: "debug".to_owned(),
            reason: "test".to_owned(),
            size: 10,
            paths: vec![PathBuf::from("target/debug/cache")],
            profile_path: PathBuf::from("target/debug"),
        }
    }

    #[test]
    fn defaults_to_selecting_all_reclaimable_groups() {
        let app = App::new(vec![
            candidate(CandidateKind::Toolchain),
            candidate(CandidateKind::Incremental),
        ]);

        assert_eq!(app.selected, BTreeSet::from([0, 1]));
        assert_eq!(app.selected_size(), 20);
    }

    #[test]
    fn requires_a_second_enter_before_removal() {
        let mut app = App::new(vec![candidate(CandidateKind::Toolchain)]);

        assert_eq!(
            app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Outcome::Continue
        );
        assert!(app.confirming);
        assert_eq!(
            app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Outcome::Delete
        );
    }
}
