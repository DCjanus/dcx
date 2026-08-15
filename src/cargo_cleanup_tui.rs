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
use crate::cargo::{CleanupCandidate, RiskLevel};

type Tui = Terminal<CrosstermBackend<Stderr>>;

pub(crate) fn select_candidates(candidates: Vec<CleanupCandidate>) -> AnyResult<Vec<usize>> {
    let mut terminal = start_terminal()?;
    let _guard = TerminalGuard;
    let mut app = App::new(candidates);

    loop {
        terminal
            .draw(|frame| render(frame, &app))
            .context("无法绘制 Cargo 缓存选择界面")?;
        let Event::Key(key) = event::read().context("无法读取终端输入")? else {
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
    enable_raw_mode().context("无法启用终端 raw mode")?;
    let mut stderr = io::stderr();
    if let Err(error) = execute!(stderr, EnterAlternateScreen, EnableMouseCapture, Hide) {
        let _ = disable_raw_mode();
        return Err(error).context("无法进入终端备用屏幕");
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
            Err(error).context("无法初始化终端")
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
            selected: candidates
                .iter()
                .enumerate()
                .filter_map(|(index, candidate)| {
                    (candidate.kind.risk() == RiskLevel::Low).then_some(index)
                })
                .collect(),
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
            Paragraph::new("终端尺寸太小\n请调整到至少 72 x 18")
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Cargo 缓存清理 "),
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
            " Cargo 缓存清理 ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "  {} 组候选 | 已选 {} 组 | 可释放 {}",
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
            let risk = candidate.kind.risk();
            let risk_style = risk_badge_style(risk);
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {marker} "), Style::default().fg(color)),
                Span::styled(format!(" {} ", risk.label()), risk_style),
                Span::styled(
                    format!(" {} ", candidate.kind.label()),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(format!(
                    "{} · {}",
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
                    .title(" 风险      类别      大小 · 构建配置 ")
                    .title_bottom(" 上/下 移动 · Space 选择 "),
            )
            .highlight_style(cursor_style())
            .highlight_symbol("▶ "),
        area,
        &mut state,
    );
}

fn risk_badge_style(risk: RiskLevel) -> Style {
    match risk {
        RiskLevel::Low => Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD),
        RiskLevel::Medium => Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    }
}

fn cursor_style() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

fn render_details(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let Some(candidate) = app.current() else {
        return;
    };
    let risk = candidate.kind.risk();
    let risk_color = match risk {
        RiskLevel::Low => Color::Green,
        RiskLevel::Medium => Color::Yellow,
    };
    let lines = vec![
        Line::raw(format!("构建配置  {}", candidate.profile)),
        Line::raw(format!("缓存类别  {}", candidate.kind.label())),
        Line::from(vec![
            Span::raw("风险等级  "),
            Span::styled(
                risk.label(),
                Style::default().fg(risk_color).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::raw(format!("占用空间  {}", Size::from_bytes(candidate.size))),
        Line::raw(format!("路径数量  {}", candidate.paths.len())),
        Line::raw(""),
        Line::styled("入选原因", Style::default().add_modifier(Modifier::BOLD)),
        Line::raw(&candidate.reason),
        Line::raw(""),
        Line::styled("风险说明", Style::default().add_modifier(Modifier::BOLD)),
        Line::styled(risk.explanation(), Style::default().fg(risk_color)),
        Line::raw("所有候选缓存均可由 Cargo 重新生成。"),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(" 审计详情 ")),
        area,
    );
}

fn render_footer(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let enter = if app.selected.is_empty() {
        "不可用"
    } else {
        "确认"
    };
    let help = Line::from(vec![
        Span::styled(" a ", key_style()),
        Span::raw("全选  "),
        Span::styled(" x ", key_style()),
        Span::raw("清空  "),
        Span::styled(" Enter ", key_style()),
        Span::raw(format!("{enter}  ")),
        Span::styled(" q ", key_style()),
        Span::raw("退出"),
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
                "确认清理 {} 组缓存（{}）？",
                app.selected.len(),
                Size::from_bytes(app.selected_size())
            ),
            Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::raw("Cargo 可以重新生成这些数据，但下一次构建可能变慢。"),
        Line::raw("相关 target profile 正在使用时，dcx 会拒绝清理。"),
        Line::raw(""),
        Line::from(vec![
            Span::styled(" Enter ", key_style()),
            Span::raw("清理     "),
            Span::styled(" Esc ", key_style()),
            Span::raw("返回"),
        ])
        .alignment(Alignment::Center),
    ];
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::LightRed))
                .title(" 确认清理 "),
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
    fn defaults_to_selecting_only_low_risk_groups() {
        let app = App::new(vec![
            candidate(CandidateKind::Toolchain),
            candidate(CandidateKind::Incremental),
        ]);

        assert_eq!(app.selected, BTreeSet::from([0]));
        assert_eq!(app.selected_size(), 10);
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

    #[test]
    fn cursor_highlight_preserves_the_risk_badge_colors() {
        let highlighted = risk_badge_style(RiskLevel::Low).patch(cursor_style());

        assert_eq!(highlighted.fg, Some(Color::Black));
        assert_eq!(highlighted.bg, Some(Color::Green));
    }
}
