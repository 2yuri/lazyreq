//! Interactive terminal UI (lazygit-style): `lazyreq` with no arguments.
//!
//! Scans a directory tree for `.lreq` files and presents three panels —
//! [1] files, [2] requests (cards), [3] history — plus a detail overlay.
//! View/run only for now; editing may come later.

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::history::{self, Record};
use crate::lazyreq::LazyReq;
use crate::timest::format_timestamp;

const SKIP_DIRS: &[&str] = &[
    "node_modules", "target", "dist", "build", "vendor", ".git", "out",
];
const MAX_FILES: usize = 500;
const MAX_DEPTH: usize = 8;
const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧'];

pub struct FileEntry {
    pub path: PathBuf,
    pub name: String,
    pub dir: String,
    pub lazyreq: Option<Arc<LazyReq>>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Panel {
    Files,
    Requests,
    History,
}

enum Overlay {
    /// Detail view of a history record (file index kept for retry).
    Detail { file: usize, record: Record, scroll: u16 },
    Keys,
}

struct Running {
    file: usize,
    id: String,
    /// run kind, purely informational: "run" or "retry"
    kind: &'static str,
}

struct RunOutcome {
    file: usize,
    id: String,
    error: Option<String>,
}

struct HistEntry {
    file: usize,
    record: Record,
}

pub struct App {
    root: String,
    files: Vec<FileEntry>,
    file_idx: usize,
    req_idx: usize,
    hist_idx: usize,
    focus: Panel,
    history: Vec<HistEntry>,
    running: Vec<Running>,
    overlay: Option<Overlay>,
    status: Option<String>,
    tick: usize,
    quit: bool,
}

pub fn scan(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        if depth > MAX_DEPTH || found.len() >= MAX_FILES {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let Ok(file_type) = entry.file_type() else { continue };
            if file_type.is_dir() {
                if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
                    continue;
                }
                stack.push((entry.path(), depth + 1));
            } else if name.ends_with(".lreq") {
                found.push(entry.path());
            }
        }
    }

    found.sort();
    found
}

/// `~` and `~/x` expand to the home directory.
pub fn expand_path(raw: &str) -> PathBuf {
    if raw == "~" {
        return home::home_dir().unwrap_or_else(|| PathBuf::from(raw));
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = home::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(raw)
}

fn shorten_home(path: &str) -> String {
    match home::home_dir() {
        Some(home) => path.replacen(&home.to_string_lossy().to_string(), "~", 1),
        None => path.to_string(),
    }
}

impl App {
    fn new(root: &Path) -> App {
        let files = scan(root)
            .into_iter()
            .map(|path| {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let dir = shorten_home(&path.parent().unwrap_or(Path::new("")).to_string_lossy());
                let mut lazyreq = LazyReq::new();
                match lazyreq.from_file(path.to_string_lossy().to_string()) {
                    Ok(()) => FileEntry {
                        path,
                        name,
                        dir,
                        lazyreq: Some(Arc::new(lazyreq)),
                        error: None,
                    },
                    Err(e) => FileEntry {
                        path,
                        name,
                        dir,
                        lazyreq: None,
                        error: Some(e),
                    },
                }
            })
            .collect();

        let mut app = App {
            root: shorten_home(&root.to_string_lossy()),
            files,
            file_idx: 0,
            req_idx: 0,
            hist_idx: 0,
            focus: Panel::Files,
            history: Vec::new(),
            running: Vec::new(),
            overlay: None,
            status: None,
            tick: 0,
            quit: false,
        };
        app.reload_history();
        app
    }

    fn selected_file(&self) -> Option<&FileEntry> {
        self.files.get(self.file_idx)
    }

    fn request_ids(&self) -> Vec<String> {
        self.selected_file()
            .and_then(|f| f.lazyreq.as_ref())
            .map(|l| l.request_ids().to_vec())
            .unwrap_or_default()
    }

    /// History pane contents: all runs across files while browsing files,
    /// only the selected request's runs once a request is focused.
    fn reload_history(&mut self) {
        let mut entries: Vec<HistEntry> = Vec::new();

        match self.focus {
            Panel::Files => {
                for (idx, file) in self.files.iter().enumerate() {
                    for record in history::all(&file.path.to_string_lossy()) {
                        entries.push(HistEntry { file: idx, record });
                    }
                }
            }
            Panel::Requests | Panel::History => {
                if let (Some(file), Some(id)) = (
                    self.selected_file(),
                    self.request_ids().get(self.req_idx).cloned(),
                ) {
                    let file_idx = self.file_idx;
                    for record in history::all(&file.path.to_string_lossy()) {
                        if record.id == id {
                            entries.push(HistEntry { file: file_idx, record });
                        }
                    }
                }
            }
        }

        entries.sort_by(|a, b| b.record.ts.cmp(&a.record.ts)); // newest first
        self.history = entries;
        self.hist_idx = self.hist_idx.min(self.history.len().saturating_sub(1));
    }

    /// Last run per (file, request id) for the request cards.
    fn last_run(&self, file_idx: usize, id: &str) -> Option<Record> {
        let file = self.files.get(file_idx)?;
        history::all(&file.path.to_string_lossy())
            .into_iter()
            .filter(|r| r.id == id)
            .max_by_key(|r| r.ts)
    }
}

pub async fn run(path: Option<String>) -> Result<(), String> {
    let root = match path {
        Some(raw) => expand_path(&raw),
        None => std::env::current_dir().map_err(|e| format!("cannot read current dir: {}", e))?,
    };
    if !root.is_dir() {
        return Err(format!("`{}` is not a directory", root.display()));
    }

    let mut app = App::new(&root);
    let (tx, mut rx) = mpsc::unbounded_channel::<RunOutcome>();

    let mut terminal = ratatui::init();
    let result = loop {
        app.tick = app.tick.wrapping_add(1);
        if let Err(e) = terminal.draw(|frame| draw(frame, &mut app)) {
            break Err(format!("draw failed: {}", e));
        }

        // Run completions arrive from spawned tasks; pop exactly one
        // matching in-flight entry per completion.
        while let Ok(outcome) = rx.try_recv() {
            if let Some(pos) = app
                .running
                .iter()
                .position(|r| r.file == outcome.file && r.id == outcome.id)
            {
                app.running.remove(pos);
            }
            if let Some(message) = outcome.error {
                app.status = Some(message);
            }
            app.reload_history();
        }

        match event::poll(Duration::from_millis(80)) {
            Ok(true) => {
                if let Ok(Event::Key(key)) = event::read() {
                    if key.kind == KeyEventKind::Press {
                        handle_key(&mut app, key.code, key.modifiers, &tx);
                    }
                }
            }
            Ok(false) => {}
            Err(e) => break Err(format!("input error: {}", e)),
        }

        if app.quit {
            break Ok(());
        }
    };
    ratatui::restore();
    result
}

fn spawn_run(app: &mut App, tx: &mpsc::UnboundedSender<RunOutcome>) {
    let Some(file) = app.selected_file() else { return };
    let Some(lazyreq) = file.lazyreq.clone() else {
        app.status = Some("file has a parse error — fix it before running".to_string());
        return;
    };
    let Some(id) = app.request_ids().get(app.req_idx).cloned() else { return };

    let file_idx = app.file_idx;
    app.running.push(Running { file: file_idx, id: id.clone(), kind: "run" });
    app.status = None;

    let tx = tx.clone();
    tokio::spawn(async move {
        let error = lazyreq.run_request(&id).await.err();
        let _ = tx.send(RunOutcome { file: file_idx, id, error });
    });
}

fn spawn_retry(app: &mut App, tx: &mpsc::UnboundedSender<RunOutcome>) {
    let Some(entry) = app.history.get(app.hist_idx) else { return };
    let file_idx = entry.file;
    let run_id = entry.record.req.clone();
    let id = entry.record.id.clone();
    if run_id.is_empty() {
        app.status = Some("this run predates run ids and cannot be retried".to_string());
        return;
    }
    let Some(lazyreq) = app.files.get(file_idx).and_then(|f| f.lazyreq.clone()) else {
        app.status = Some("file has a parse error — fix it before retrying".to_string());
        return;
    };

    app.running.push(Running { file: file_idx, id: id.clone(), kind: "retry" });
    app.status = None;

    let tx = tx.clone();
    tokio::spawn(async move {
        let error = lazyreq.retry_run(&run_id).await.err();
        let _ = tx.send(RunOutcome { file: file_idx, id, error });
    });
}

fn handle_key(
    app: &mut App,
    code: KeyCode,
    modifiers: KeyModifiers,
    tx: &mpsc::UnboundedSender<RunOutcome>,
) {
    if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
        app.quit = true;
        return;
    }

    // Overlay swallows every key.
    if let Some(overlay) = &mut app.overlay {
        match (overlay, code) {
            (Overlay::Detail { scroll, .. }, KeyCode::Char('j') | KeyCode::Down) => *scroll += 1,
            (Overlay::Detail { scroll, .. }, KeyCode::Char('k') | KeyCode::Up) => {
                *scroll = scroll.saturating_sub(1)
            }
            (_, KeyCode::Esc | KeyCode::Char('q')) => app.overlay = None,
            _ => {}
        }
        return;
    }

    match code {
        KeyCode::Char('q') => app.quit = true,
        KeyCode::Char('?') => app.overlay = Some(Overlay::Keys),
        KeyCode::Char('1') => { app.focus = Panel::Files; app.reload_history(); }
        KeyCode::Char('2') => { app.focus = Panel::Requests; app.reload_history(); }
        KeyCode::Char('3') => app.focus = Panel::History,
        KeyCode::Tab => {
            app.focus = match app.focus {
                Panel::Files => Panel::Requests,
                Panel::Requests => Panel::History,
                Panel::History => Panel::Files,
            };
            app.reload_history();
        }
        KeyCode::Char('j') | KeyCode::Down => move_selection(app, 1),
        KeyCode::Char('k') | KeyCode::Up => move_selection(app, -1),
        KeyCode::Char('h') | KeyCode::Left if app.focus == Panel::Requests => move_selection(app, -1),
        KeyCode::Char('l') | KeyCode::Right if app.focus == Panel::Requests => move_selection(app, 1),
        KeyCode::Enter => match app.focus {
            Panel::Files => { app.focus = Panel::Requests; app.reload_history(); }
            Panel::Requests => spawn_run(app, tx),
            Panel::History => {
                if let Some(entry) = app.history.get(app.hist_idx) {
                    app.overlay = Some(Overlay::Detail {
                        file: entry.file,
                        record: entry.record.clone(),
                        scroll: 0,
                    });
                }
            }
        },
        KeyCode::Char('r') if app.focus == Panel::History => spawn_retry(app, tx),
        _ => {}
    }
}

fn move_selection(app: &mut App, delta: isize) {
    let step = |idx: usize, len: usize| -> usize {
        if len == 0 {
            return 0;
        }
        (idx as isize + delta).rem_euclid(len as isize) as usize
    };

    match app.focus {
        Panel::Files => {
            app.file_idx = step(app.file_idx, app.files.len());
            app.req_idx = 0;
            app.reload_history();
        }
        Panel::Requests => {
            app.req_idx = step(app.req_idx, self_len(app));
            app.reload_history();
        }
        Panel::History => {
            app.hist_idx = step(app.hist_idx, app.history.len() + app.running.len());
        }
    }
}

fn self_len(app: &App) -> usize {
    app.request_ids().len()
}

// ---------------------------------------------------------------- rendering

fn panel_block(title: String, active: bool) -> Block<'static> {
    let border = if active {
        Style::new().fg(Color::Green)
    } else {
        Style::new().fg(Color::DarkGray)
    };
    let title_style = if active {
        Style::new().fg(Color::Green).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(Color::Gray)
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(Span::styled(title, title_style))
}

fn draw(frame: &mut Frame, app: &mut App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(frame.area());

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(30), Constraint::Min(0)])
        .split(outer[0]);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(8)])
        .split(columns[0]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(columns[1]);

    draw_files(frame, app, left[0]);
    draw_shortcuts(frame, left[1]);
    draw_requests(frame, app, right[0]);
    draw_history(frame, app, right[1]);
    draw_status_line(frame, app, outer[1]);

    match &app.overlay {
        Some(Overlay::Detail { file, record, scroll }) => {
            draw_detail(frame, app, *file, record, *scroll)
        }
        Some(Overlay::Keys) => draw_keys(frame),
        None => {}
    }
}

fn draw_files(frame: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .files
        .iter()
        .map(|f| {
            let marker = if f.error.is_some() {
                Span::styled(" ✗", Style::new().fg(Color::Red))
            } else {
                Span::raw("")
            };
            ListItem::new(vec![
                Line::from(vec![Span::styled(f.name.clone(), Style::new().bold()), marker]),
                Line::from(Span::styled(f.dir.clone(), Style::new().fg(Color::DarkGray))),
            ])
        })
        .collect();

    let empty = items.is_empty();
    let list = List::new(items)
        .block(panel_block(format!("[1] files — {}", app.root), app.focus == Panel::Files))
        .highlight_style(Style::new().bg(Color::Rgb(40, 44, 52)).add_modifier(Modifier::BOLD));

    let mut state = ListState::default();
    state.select((!empty).then_some(app.file_idx));
    frame.render_stateful_widget(list, area, &mut state);

    if empty {
        let hint = Paragraph::new(format!(
            "\nno .lreq files under {}\n\ntry:  lazyreq --path <dir>",
            app.root
        ))
        .style(Style::new().fg(Color::DarkGray))
        .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(hint, area.inner(ratatui::layout::Margin::new(1, 1)));
    }
}

fn draw_shortcuts(frame: &mut Frame, area: Rect) {
    let keys = [
        ("1/2/3", "jump panel"),
        ("⏎", "open / run / view"),
        ("r", "retry run"),
        ("j/k h/l", "navigate"),
        ("?", "all keybindings"),
        ("q", "quit"),
    ];
    let lines: Vec<Line> = keys
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(format!(" {:8}", k), Style::new().fg(Color::Yellow)),
                Span::raw(d.to_string()),
            ])
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(panel_block("shortcuts".to_string(), false)),
        area,
    );
}

fn draw_requests(frame: &mut Frame, app: &mut App, area: Rect) {
    let title = match app.selected_file() {
        Some(f) => format!("[2] requests — {}", f.name),
        None => "[2] requests".to_string(),
    };
    let block = panel_block(title, app.focus == Panel::Requests);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some(error) = app.selected_file().and_then(|f| f.error.clone()) {
        let msg = Paragraph::new(error)
            .style(Style::new().fg(Color::Red))
            .wrap(Wrap { trim: false });
        frame.render_widget(msg, inner);
        return;
    }

    let ids = app.request_ids();
    if ids.is_empty() {
        return;
    }

    // Card grid: as many 30-wide, 5-tall cards per row as fit.
    let card_w: u16 = 30;
    let card_h: u16 = 5;
    let cols = (inner.width / card_w).max(1) as usize;
    let visible_rows = (inner.height / card_h).max(1) as usize;

    // Scroll whole rows so the selected card stays visible.
    let sel_row = app.req_idx / cols;
    let first_row = sel_row.saturating_sub(visible_rows.saturating_sub(1));

    for (i, id) in ids.iter().enumerate() {
        let row = i / cols;
        if row < first_row || row >= first_row + visible_rows {
            continue;
        }
        let col = i % cols;
        let card = Rect {
            x: inner.x + (col as u16) * card_w,
            y: inner.y + ((row - first_row) as u16) * card_h,
            width: card_w.min(inner.width.saturating_sub((col as u16) * card_w)),
            height: card_h.min(inner.height.saturating_sub(((row - first_row) as u16) * card_h)),
        };
        if card.width < 10 || card.height < 3 {
            continue;
        }

        let selected = i == app.req_idx;
        let running = app
            .running
            .iter()
            .any(|r| r.file == app.file_idx && r.id == *id);

        let request = app
            .selected_file()
            .and_then(|f| f.lazyreq.as_ref())
            .and_then(|l| l.request(id));
        let method_path = request
            .map(|r| format!("{} {}", r.method, r.path))
            .unwrap_or_default();

        let last_line = if running {
            Line::from(Span::styled(
                format!("{} running…", SPINNER[app.tick % SPINNER.len()]),
                Style::new().fg(Color::Yellow),
            ))
        } else {
            match app.last_run(app.file_idx, id) {
                Some(rec) => {
                    let (status, color) = status_span(&rec);
                    Line::from(vec![
                        Span::styled(status, Style::new().fg(color).bold()),
                        Span::styled(
                            format!(" · {}ms · {}", rec.ms, &format_timestamp(rec.ts)[11..16]),
                            Style::new().fg(Color::DarkGray),
                        ),
                    ])
                }
                None => Line::from(Span::styled("not run yet", Style::new().fg(Color::DarkGray))),
            }
        };

        let border = if selected {
            Style::new().fg(Color::Green)
        } else {
            Style::new().fg(Color::DarkGray)
        };
        let card_block = Block::default()
            .borders(Borders::ALL)
            .border_style(border)
            .title(Span::styled(
                id.clone(),
                if selected {
                    Style::new().fg(Color::Green).bold()
                } else {
                    Style::new().bold()
                },
            ));
        let body = Paragraph::new(vec![Line::from(method_path), last_line]).block(card_block);
        frame.render_widget(body, card);
    }
}

fn status_span(record: &Record) -> (String, Color) {
    match record.status {
        0 => ("ERR".to_string(), Color::Red),
        s @ 200..=299 => (s.to_string(), Color::Green),
        s @ 300..=399 => (s.to_string(), Color::Yellow),
        s => (s.to_string(), Color::Red),
    }
}

fn draw_history(frame: &mut Frame, app: &mut App, area: Rect) {
    let scope = match app.focus {
        Panel::Files => "all files".to_string(),
        _ => app
            .request_ids()
            .get(app.req_idx)
            .cloned()
            .unwrap_or_else(|| "—".to_string()),
    };

    let mut items: Vec<ListItem> = Vec::new();
    for running in &app.running {
        let file_name = app
            .files
            .get(running.file)
            .map(|f| f.name.clone())
            .unwrap_or_default();
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!("{} {}…  ", SPINNER[app.tick % SPINNER.len()], running.kind),
                Style::new().fg(Color::Yellow),
            ),
            Span::styled(running.id.clone(), Style::new().bold()),
            Span::styled(format!("  {}", file_name), Style::new().fg(Color::DarkGray)),
        ])));
    }
    for entry in &app.history {
        let rec = &entry.record;
        let (status, color) = status_span(rec);
        let tail = match &rec.error {
            Some(e) => e.clone(),
            None => history::shape_of(&rec.resp_body),
        };
        items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("{} ", rec.req), Style::new().fg(Color::Cyan)),
            Span::styled(
                format!("{} ", &format_timestamp(rec.ts)[5..16]),
                Style::new().fg(Color::DarkGray),
            ),
            Span::styled(format!("{:12} ", rec.id), Style::new().fg(Color::Green)),
            Span::raw(format!("{:5} ", rec.method)),
            Span::styled(format!("{:4} ", status), Style::new().fg(color).bold()),
            Span::styled(format!("{:>5}ms  ", rec.ms), Style::new().fg(Color::DarkGray)),
            Span::styled(tail, Style::new().fg(Color::DarkGray)),
        ])));
    }

    let empty = items.is_empty();
    let list = List::new(items)
        .block(panel_block(format!("[3] history — {}", scope), app.focus == Panel::History))
        .highlight_style(Style::new().bg(Color::Rgb(40, 44, 52)).add_modifier(Modifier::BOLD));

    let mut state = ListState::default();
    state.select((!empty).then_some(app.hist_idx));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_status_line(frame: &mut Frame, app: &App, area: Rect) {
    let line = match &app.status {
        Some(msg) => Line::from(Span::styled(
            format!(" {}", msg),
            Style::new().fg(Color::Red),
        )),
        None => {
            let hints = match app.focus {
                Panel::Files => " ⏎ open  ·  j/k move  ·  tab next panel  ·  ? keys  ·  q quit",
                Panel::Requests => " ⏎ run  ·  h/l j/k move  ·  tab next panel  ·  ? keys  ·  q quit",
                Panel::History => " ⏎ view  ·  r retry  ·  j/k move  ·  tab next panel  ·  ? keys  ·  q quit",
            };
            Line::from(Span::styled(hints, Style::new().fg(Color::DarkGray)))
        }
    };
    frame.render_widget(Paragraph::new(line), area);
}

fn centered(area: Rect, pct_x: u16, pct_y: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - pct_y) / 2),
            Constraint::Percentage(pct_y),
            Constraint::Percentage((100 - pct_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pct_x) / 2),
            Constraint::Percentage(pct_x),
            Constraint::Percentage((100 - pct_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn draw_detail(frame: &mut Frame, app: &App, file: usize, record: &Record, scroll: u16) {
    let area = centered(frame.area(), 80, 80);
    frame.render_widget(Clear, area);

    let file_name = app.files.get(file).map(|f| f.name.clone()).unwrap_or_default();
    let (status, color) = status_span(record);

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("{} ", record.method), Style::new().bold()),
            Span::raw(record.url.clone()),
        ]),
        Line::from(vec![
            Span::styled(format!("{} ", status), Style::new().fg(color).bold()),
            Span::styled(
                format!("· {}ms · {} · headers:{}", record.ms, format_timestamp(record.ts), record.req_headers.len()),
                Style::new().fg(Color::DarkGray),
            ),
        ]),
        Line::default(),
    ];

    if let Some(error) = &record.error {
        lines.push(Line::from(Span::styled(
            format!("error: {}", error),
            Style::new().fg(Color::Red),
        )));
        lines.push(Line::default());
    }

    if !record.req_body.is_empty() {
        lines.push(Line::from(Span::styled("request:", Style::new().bold())));
        for l in pretty(&record.req_body).lines() {
            lines.push(Line::from(format!("  {}", l)));
        }
        lines.push(Line::default());
    }

    if !record.resp_body.is_empty() {
        lines.push(Line::from(Span::styled("response:", Style::new().bold())));
        for l in pretty(&record.resp_body).lines() {
            lines.push(Line::from(format!("  {}", l)));
        }
    }

    let title = format!(" run {} — {} ({}) · j/k scroll · Esc close ", record.req, record.id, file_name);
    let widget = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::new().fg(Color::Green))
                .title(title),
        )
        .scroll((scroll, 0));
    frame.render_widget(widget, area);
}

fn draw_keys(frame: &mut Frame) {
    let area = centered(frame.area(), 50, 60);
    frame.render_widget(Clear, area);

    let keys = [
        ("1 / 2 / 3", "focus files / requests / history"),
        ("tab", "cycle panels"),
        ("j/k  ↑/↓", "move (h/l too, in the request grid)"),
        ("⏎", "files: open · requests: run · history: view"),
        ("r", "retry the selected run (exact body, fresh auth)"),
        ("j/k in view", "scroll the detail view"),
        ("esc", "close view"),
        ("q / ctrl-c", "quit"),
    ];
    let lines: Vec<Line> = keys
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(format!(" {:14}", k), Style::new().fg(Color::Yellow)),
                Span::raw(d.to_string()),
            ])
        })
        .collect();

    let widget = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(Color::Green))
            .title(" keybindings — Esc close "),
    );
    frame.render_widget(widget, area);
}

fn pretty(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| body.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[test]
    fn scan_finds_lreq_files_and_skips_junk_dirs() {
        let root = std::env::temp_dir().join(format!("lreq-scan-{}", uuid::Uuid::new_v4()));
        for dir in ["a", "b/nested", "node_modules/pkg", ".hidden", "target/debug"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        for file in [
            "a/api.lreq",
            "b/nested/deep.lreq",
            "node_modules/pkg/skip.lreq",
            ".hidden/skip.lreq",
            "target/debug/skip.lreq",
            "a/readme.md",
        ] {
            let mut f = fs::File::create(root.join(file)).unwrap();
            f.write_all(b"ID: x\nGET http://localhost\n").unwrap();
        }

        let found = scan(&root);
        let names: Vec<String> = found
            .iter()
            .map(|p| p.strip_prefix(&root).unwrap().to_string_lossy().to_string())
            .collect();

        assert_eq!(names, vec!["a/api.lreq", "b/nested/deep.lreq"]);
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn expand_path_handles_tilde() {
        let home = home::home_dir().unwrap();
        assert_eq!(expand_path("~"), home);
        assert_eq!(expand_path("~/projects"), home.join("projects"));
        assert_eq!(expand_path("/tmp/x"), PathBuf::from("/tmp/x"));
    }
}
