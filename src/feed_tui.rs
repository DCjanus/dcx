use std::io;

use anyhow::Context;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};

use crate::AnyResult;
use crate::feed::{Entry, Feed, render_body};

pub(crate) fn run(feed: Feed) -> AnyResult {
    enable_raw_mode().context("failed to enable terminal raw mode")?;
    let _guard = TerminalGuard;
    execute!(io::stderr(), EnterAlternateScreen).context("failed to enter alternate screen")?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stderr()))
        .context("failed to initialize terminal")?;
    let mut app = App::new(feed);
    loop {
        draw(&mut terminal, &mut app)?;
        if let Event::Key(key) = event::read().context("failed to read terminal input")?
            && key.kind == KeyEventKind::Press
            && app.key(key)
        {
            return Ok(());
        }
    }
}

struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stderr(), LeaveAlternateScreen, crossterm::cursor::Show);
    }
}

struct App {
    feed: Feed,
    visible: Vec<usize>,
    selected: usize,
    scroll: u16,
    max_scroll: u16,
    query: String,
    searching: bool,
    reading: bool,
    raw: bool,
    // 仅在条目、宽度或显示模式改变时重新渲染。
    rendered: Option<(usize, u16, Text<'static>)>,
    list: ListState,
}

impl App {
    fn new(feed: Feed) -> Self {
        Self {
            visible: (0..feed.entries.len()).collect(),
            feed,
            selected: 0,
            scroll: 0,
            max_scroll: 0,
            query: String::new(),
            searching: false,
            reading: false,
            raw: false,
            rendered: None,
            list: ListState::default(),
        }
    }

    fn filter(&mut self) {
        let query = self.query.to_lowercase();
        self.visible = self
            .feed
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                entry.title.to_lowercase().contains(&query)
                    || entry.body.to_lowercase().contains(&query)
            })
            .map(|(index, _)| index)
            .collect();
        self.selected = 0;
        self.scroll = 0;
    }

    fn key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return true;
        }
        if self.searching {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.searching = false,
                KeyCode::Backspace => {
                    self.query.pop();
                    self.filter();
                }
                KeyCode::Char(ch) => {
                    self.query.push(ch);
                    self.filter();
                }
                _ => {}
            }
            return false;
        }
        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Esc if self.reading => self.reading = false,
            KeyCode::Esc if !self.query.is_empty() => {
                self.query.clear();
                self.filter();
            }
            KeyCode::Esc => return true,
            KeyCode::Char('/') => self.searching = true,
            KeyCode::Char('r') => {
                self.raw = !self.raw;
                self.rendered = None;
                self.scroll = 0;
            }
            KeyCode::Tab => self.reading = !self.reading,
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.reading = true,
            KeyCode::Left | KeyCode::Char('h') => self.reading = false,
            KeyCode::Down | KeyCode::Char('j') if self.reading => {
                self.scroll = self.scroll.saturating_add(1).min(self.max_scroll)
            }
            KeyCode::Up | KeyCode::Char('k') if self.reading => {
                self.scroll = self.scroll.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = self
                    .selected
                    .saturating_add(1)
                    .min(self.visible.len().saturating_sub(1));
                self.scroll = 0;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                self.scroll = 0;
            }
            KeyCode::PageDown | KeyCode::Char(' ') => {
                self.scroll = self.scroll.saturating_add(10).min(self.max_scroll)
            }
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(10),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => self.scroll = self.max_scroll,
            _ => {}
        }
        false
    }
}

fn draw<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut App) -> AnyResult
where
    B::Error: std::error::Error + Send + Sync + 'static,
{
    terminal.draw(|frame| {
        let rows = Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).split(frame.area());
        let narrow = rows[0].width < 80;
        let panes = Layout::horizontal([Constraint::Percentage(35), Constraint::Percentage(65)]).split(rows[0]);
        let list_area = if narrow { rows[0] } else { panes[0] };
        let body_area = if narrow { rows[0] } else { panes[1] };
        if !narrow || !app.reading {
            let items = app.visible.iter().map(|&index| ListItem::new(app.feed.entries[index].title.clone())).collect::<Vec<_>>();
            app.list.select((!app.visible.is_empty()).then_some(app.selected));
            let title = format!("{} ({}/{})", app.feed.title, app.visible.len(), app.feed.entries.len());
            let list = List::new(items).block(Block::bordered().title(title))
                .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan)).highlight_symbol("> ");
            frame.render_stateful_widget(list, list_area, &mut app.list);
        }
        if !narrow || app.reading {
            let text = if let Some(&index) = app.visible.get(app.selected) {
                let width = body_area.width.saturating_sub(2).max(1);
                if app.rendered.as_ref().is_none_or(|(old, old_width, _)| *old != index || *old_width != width) {
                    let entry = &app.feed.entries[index];
                    app.rendered = Some((index, width, entry_text(entry, width, app.raw)));
                }
                app.rendered.as_ref().unwrap().2.clone()
            } else if app.feed.entries.is_empty() { Text::raw("No entries in this feed.") } else { Text::raw("No matching entries.") };
            let title = app.visible.get(app.selected).map(|&index| {
                let entry = &app.feed.entries[index];
                format!("{} | {}{}{}", if entry.summary { "Summary" } else { "Content" }, entry.format.label(),
                    if app.raw { " | Raw" } else { "" }, if app.reading { " [focused]" } else { "" })
            }).unwrap_or_else(|| "Content".into());
            let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
            app.max_scroll = paragraph.line_count(body_area.width.saturating_sub(2).max(1))
                .saturating_sub(usize::from(body_area.height.saturating_sub(2))).min(u16::MAX as usize) as u16;
            app.scroll = app.scroll.min(app.max_scroll);
            frame.render_widget(paragraph.block(Block::bordered().title(title)).scroll((app.scroll, 0)), body_area);
        }
        let footer = if app.searching { format!("Search: {}_\nEnter/Esc: finish", app.query) }
        else { format!("j/k: move  Enter/Tab: focus  PgUp/PgDn: scroll  /: search  r: raw  q: quit\nFilter: {}", app.query) };
        frame.render_widget(Paragraph::new(footer), rows[1]);
    }).context("failed to draw feed reader")?;
    Ok(())
}

fn entry_text(entry: &Entry, width: u16, raw: bool) -> Text<'static> {
    let mut text = Text::from(vec![
        Line::styled(
            entry.title.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            entry.date.clone(),
            Style::default().add_modifier(Modifier::DIM),
        ),
        Line::styled(
            entry.link.clone(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::UNDERLINED),
        ),
        Line::default(),
    ]);
    let body =
        render_body(entry, width, raw).unwrap_or_else(|error| Text::raw(format!("{error:#}")));
    text.lines.extend(body.lines.into_iter().map(|mut line| {
        line.style = body.style.patch(line.style);
        line.alignment = line.alignment.or(body.alignment);
        line
    }));
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn app() -> App {
        App::new(
            crate::feed::parse(include_bytes!("../tests/fixtures/sample.atom").as_slice()).unwrap(),
        )
    }

    fn press(app: &mut App, code: KeyCode) -> bool {
        app.key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn filters_by_content_and_resets_selection_and_scroll() {
        let mut app = app();
        press(&mut app, KeyCode::Down);
        assert_eq!(app.selected, 1);
        press(&mut app, KeyCode::Char('/'));
        for ch in "列表内容".chars() {
            press(&mut app, KeyCode::Char(ch));
        }
        assert_eq!(app.visible, vec![0]);
        assert_eq!(app.selected, 0);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.visible.len(), 2);
        press(&mut app, KeyCode::Char('/'));
        for ch in "NO MATCH".chars() {
            press(&mut app, KeyCode::Char(ch));
        }
        assert!(app.visible.is_empty());
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Down);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn switches_focus_and_bounds_body_scroll() {
        let mut app = app();
        app.max_scroll = 12;
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Down);
        assert_eq!(app.selected, 0);
        assert_eq!(app.scroll, 1);
        press(&mut app, KeyCode::End);
        press(&mut app, KeyCode::PageDown);
        assert_eq!(app.scroll, 12);
        press(&mut app, KeyCode::Home);
        press(&mut app, KeyCode::Up);
        assert_eq!(app.scroll, 0);
        assert!(!press(&mut app, KeyCode::Esc));
        assert!(!app.reading);
        assert!(press(&mut app, KeyCode::Char('q')));
    }

    #[test]
    fn end_aligns_the_last_content_line_with_the_bottom_of_the_viewport() {
        let mut app = app();
        app.feed.entries[0].format = crate::feed::BodyFormat::PlainText;
        app.feed.entries[0].body = (0..50)
            .map(|index| format!("line-{index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut terminal = Terminal::new(TestBackend::new(120, 12)).unwrap();
        draw(&mut terminal, &mut app).unwrap();
        press(&mut app, KeyCode::End);
        draw(&mut terminal, &mut app).unwrap();
        let last_row = (43..119)
            .map(|x| terminal.backend().buffer()[(x, 8)].symbol())
            .collect::<String>();
        assert!(last_row.contains("line-49"), "{last_row}");
    }

    #[test]
    fn renders_wide_narrow_empty_and_filtered_views() {
        for (width, height) in [(120, 24), (40, 12), (1, 1)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut app = app();
            draw(&mut terminal, &mut app).unwrap();
            press(&mut app, KeyCode::Enter);
            draw(&mut terminal, &mut app).unwrap();
            if width > 1 {
                let rendered = terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect::<String>();
                assert!(
                    rendered.replace(' ', "").contains("阅读示例"),
                    "{width}x{height}: {rendered}"
                );
            }
            app.query = "absent".into();
            app.filter();
            draw(&mut terminal, &mut app).unwrap();
            app.feed.entries.clear();
            draw(&mut terminal, &mut app).unwrap();
        }
    }

    #[test]
    fn renders_format_labels_and_switches_between_rich_and_raw_views() {
        let feed = crate::feed::parse(include_bytes!("../tests/fixtures/formats.atom").as_slice())
            .unwrap();
        let mut app = App::new(feed);
        let mut terminal = Terminal::new(TestBackend::new(120, 24)).unwrap();
        draw(&mut terminal, &mut app).unwrap();
        let screen = |terminal: &Terminal<TestBackend>| {
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
        };
        assert!(screen(&terminal).contains("Content | Markdown"));
        app.scroll = 5;
        press(&mut app, KeyCode::Char('r'));
        assert_eq!(app.scroll, 0);
        draw(&mut terminal, &mut app).unwrap();
        assert!(screen(&terminal).contains("Markdown | Raw"));
        assert!(screen(&terminal).contains("**bold phrase**"));
        press(&mut app, KeyCode::Char('r'));
        draw(&mut terminal, &mut app).unwrap();
        assert!(!screen(&terminal).contains("**bold phrase**"));
        press(&mut app, KeyCode::Down);
        draw(&mut terminal, &mut app).unwrap();
        assert!(screen(&terminal).contains("Content | HTML"));
        press(&mut app, KeyCode::Down);
        draw(&mut terminal, &mut app).unwrap();
        assert!(screen(&terminal).contains("Summary | Plain text"));
    }
}
