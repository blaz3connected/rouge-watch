use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton, MouseEvent, MouseEventKind},
    execute,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, Gauge, Paragraph, Row, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Table, TableState,
    },
    Frame,
};
use std::io;
use std::time::{Duration, Instant};
use sysinfo::{ProcessesToUpdate, System};


const DOUBLE_CLICK_MS: u128 = 400;
const REFRESH_INTERVAL: Duration = Duration::from_millis(600);
const POPUP_ANIM_MS: u128 = 130;
const FLASH_DURATION: Duration = Duration::from_secs(2);


// Catppuccin Mocha palette - kept consistent across every widget instead of one-off colors.
const ACCENT_BLUE: Color = Color::Rgb(137, 180, 250);
const ACCENT_GREEN: Color = Color::Rgb(166, 227, 161);
const ACCENT_YELLOW: Color = Color::Rgb(249, 226, 175);
const ACCENT_RED: Color = Color::Rgb(243, 139, 168);
const ACCENT_MUTED: Color = Color::Rgb(147, 154, 183);
const BORDER_DIM: Color = Color::Rgb(88, 91, 112);
const TEXT_MAIN: Color = Color::Rgb(205, 214, 244);
const TEXT_DIM: Color = Color::Rgb(186, 194, 222);
const BG_ROW_A: Color = Color::Rgb(15, 17, 23);
const BG_ROW_B: Color = Color::Rgb(22, 25, 34);
const BG_ROW_HOVER: Color = Color::Rgb(35, 38, 52);
const BG_ROW_SELECTED: Color = Color::Rgb(49, 50, 68);
const SHADOW: Color = Color::Rgb(8, 8, 12);


/// Green under 50%, yellow under 85%, red above - the same semantic colors everywhere
/// instead of icons, which is both more reliable across terminals and reads as more
/// "professional monitor" than decorative emoji.
fn usage_color(ratio: f64) -> Color {
    if ratio >= 0.85 {
        ACCENT_RED
    } else if ratio >= 0.5 {
        ACCENT_YELLOW
    } else {
        ACCENT_GREEN
    }
}


#[derive(PartialEq, Clone, Copy)]
enum AppMode {
    Normal,
    ManageTask { pid: sysinfo::Pid },
}


#[derive(PartialEq, Clone, Copy)]
enum HoveredButton {
    Terminate,
    Cancel,
    CloseX,
}


#[derive(Clone, Copy)]
enum PopupAnim {
    Opening(Instant),
    Closing(Instant),
}


#[derive(Clone, Copy)]
struct Rects {
    table_area: Rect,
    btn_terminate: Rect,
    btn_cancel: Rect,
    btn_close_x: Rect,
}


impl Default for Rects {
    fn default() -> Self {
        let empty = Rect { x: 0, y: 0, width: 0, height: 0 };
        Self {
            table_area: empty,
            btn_terminate: empty,
            btn_cancel: empty,
            btn_close_x: empty,
        }
    }
}


struct App {
    mode: AppMode,
    rects: Rects,
    hovered_row: Option<usize>,
    hovered_button: Option<HoveredButton>,
    last_click: Option<(usize, Instant)>,
    popup_anim: Option<PopupAnim>,
    selected_pid: Option<sysinfo::Pid>,
    table_state: TableState,
    scroll_state: ScrollbarState,
    status_message: Option<(String, Instant)>,
}


impl App {
    fn new() -> Self {
        let mut table_state = TableState::default();
        table_state.select(Some(0));
        Self {
            mode: AppMode::Normal,
            rects: Rects::default(),
            hovered_row: None,
            hovered_button: None,
            last_click: None,
            popup_anim: None,
            selected_pid: None,
            table_state,
            scroll_state: ScrollbarState::default(),
            status_message: None,
        }
    }


    fn sync_selection(&mut self, processes: &[(&sysinfo::Pid, &sysinfo::Process)], display_len: usize) {
        if let Some(pid) = self.selected_pid {
            if let Some(pos) = processes.iter().position(|(p, _)| **p == pid) {
                self.table_state.select(Some(pos));
            } else if display_len > 0 {
                self.table_state.select(Some(0));
                self.selected_pid = processes.first().map(|(p, _)| **p);
            }
        } else if display_len > 0 {
            self.table_state.select(Some(0));
            self.selected_pid = processes.first().map(|(p, _)| **p);
        }


        if let Some(i) = self.table_state.selected() {
            if i < display_len {
                if let Some((pid, _)) = processes.get(i) {
                    self.selected_pid = Some(**pid);
                }
            }
        }


        self.scroll_state = self
            .scroll_state
            .content_length(display_len)
            .position(self.table_state.selected().unwrap_or(0));
    }


    /// Finalizes a completed close animation. Call once per frame, before drawing.
    fn tick(&mut self) {
        if let Some(PopupAnim::Closing(t)) = self.popup_anim {
            if t.elapsed().as_millis() >= POPUP_ANIM_MS {
                self.mode = AppMode::Normal;
                self.popup_anim = None;
                self.hovered_button = None;
            }
        }
    }


    fn is_closing(&self) -> bool {
        matches!(self.popup_anim, Some(PopupAnim::Closing(_)))
    }


    fn open_popup(&mut self, pid: sysinfo::Pid) {
        self.mode = AppMode::ManageTask { pid };
        self.popup_anim = Some(PopupAnim::Opening(Instant::now()));
        self.hovered_button = None;
    }


    /// Starts the close animation instead of closing instantly. Safe to call repeatedly -
    /// a close already in progress is left alone rather than restarted.
    fn request_close(&mut self) {
        if matches!(self.mode, AppMode::ManageTask { .. }) && !self.is_closing() {
            self.popup_anim = Some(PopupAnim::Closing(Instant::now()));
            self.hovered_button = None;
        }
    }


    fn flash(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), Instant::now()));
    }


    fn expire_flash(&mut self) {
        if let Some((_, t)) = self.status_message {
            if t.elapsed() > FLASH_DURATION {
                self.status_message = None;
            }
        }
    }


    fn row_at_position(&self, x: u16, y: u16) -> Option<usize> {
        let r = self.rects.table_area;
        let header_offset = 3;
        if x >= r.x && x < r.x + r.width && y >= r.y + header_offset && y + 1 < r.y + r.height {
            Some((y - (r.y + header_offset)) as usize + self.table_state.offset())
        } else {
            None
        }
    }


    fn button_at(&self, x: u16, y: u16) -> Option<HoveredButton> {
        let hit = |r: Rect| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height;
        if hit(self.rects.btn_close_x) {
            Some(HoveredButton::CloseX)
        } else if hit(self.rects.btn_terminate) {
            Some(HoveredButton::Terminate)
        } else if hit(self.rects.btn_cancel) {
            Some(HoveredButton::Cancel)
        } else {
            None
        }
    }


    fn handle_key(
        &mut self,
        code: KeyCode,
        sys: &mut System,
        pids: &[sysinfo::Pid],
        display_len: usize,
    ) -> bool {
        match self.mode {
            AppMode::Normal => match code {
                KeyCode::Char('q') => return false,
                KeyCode::Down | KeyCode::Char('j') => {
                    let i = match self.table_state.selected() {
                        Some(i) if i < display_len.saturating_sub(1) => i + 1,
                        Some(i) => i,
                        None => 0,
                    };
                    self.table_state.select(Some(i));
                    if let Some(pid) = pids.get(i) {
                        self.selected_pid = Some(*pid);
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    let i = match self.table_state.selected() {
                        Some(i) if i > 0 => i - 1,
                        _ => 0,
                    };
                    self.table_state.select(Some(i));
                    if let Some(pid) = pids.get(i) {
                        self.selected_pid = Some(*pid);
                    }
                }
                KeyCode::Enter => {
                    if let Some(i) = self.table_state.selected() {
                        if let Some(pid) = pids.get(i) {
                            self.open_popup(*pid);
                        }
                    }
                }
                _ => {}
            },
            AppMode::ManageTask { pid } => {
                // Ignore input while the popup is animating shut, so a stray keypress can't
                // re-trigger an action (e.g. double-kill) mid-close.
                if self.is_closing() {
                    return true;
                }
                match code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => self.terminate(pid, sys),
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => self.request_close(),
                    _ => {}
                }
            }
        }
        true
    }


    fn handle_mouse(
        &mut self,
        mouse: MouseEvent,
        sys: &mut System,
        pids: &[sysinfo::Pid],
        display_len: usize,
    ) {
        match mouse.kind {
            MouseEventKind::Moved => {
                self.hovered_row = None;
                self.hovered_button = None;
                match self.mode {
                    AppMode::Normal => {
                        if let Some(row) = self.row_at_position(mouse.column, mouse.row) {
                            if row < display_len {
                                self.hovered_row = Some(row);
                            }
                        }
                    }
                    AppMode::ManageTask { .. } if !self.is_closing() => {
                        self.hovered_button = self.button_at(mouse.column, mouse.row);
                    }
                    AppMode::ManageTask { .. } => {}
                }
            }
            MouseEventKind::Down(MouseButton::Left) => match self.mode {
                AppMode::Normal => self.handle_table_click(mouse.column, mouse.row, pids, display_len),
                AppMode::ManageTask { pid } if !self.is_closing() => {
                    self.handle_popup_click(mouse.column, mouse.row, pid, sys)
                }
                AppMode::ManageTask { .. } => {}
            },
            _ => {}
        }
    }


    fn handle_table_click(&mut self, x: u16, y: u16, pids: &[sysinfo::Pid], display_len: usize) {
        let Some(target) = self.row_at_position(x, y) else { return };
        if target >= display_len {
            return;
        }


        self.table_state.select(Some(target));
        if let Some(pid) = pids.get(target) {
            self.selected_pid = Some(*pid);
        }


        let now = Instant::now();
        let is_double = matches!(
            self.last_click,
            Some((row, t)) if row == target && t.elapsed().as_millis() < DOUBLE_CLICK_MS
        );


        if is_double {
            self.last_click = None;
            if let Some(pid) = pids.get(target) {
                self.open_popup(*pid);
            }
        } else {
            self.last_click = Some((target, now));
        }
    }


    fn handle_popup_click(&mut self, x: u16, y: u16, pid: sysinfo::Pid, sys: &mut System) {
        match self.button_at(x, y) {
            Some(HoveredButton::CloseX) | Some(HoveredButton::Cancel) => self.request_close(),
            Some(HoveredButton::Terminate) => self.terminate(pid, sys),
            None => {}
        }
    }


    fn terminate(&mut self, pid: sysinfo::Pid, sys: &mut System) {
        let name = sys.process(pid).map(|p| p.name().to_string_lossy().to_string());
        if let Some(proc) = sys.process(pid) {
            proc.kill();
        }
        self.request_close();
        if let Some(name) = name {
            self.flash(format!("Terminated {name}"));
        }
    }
}


fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;


    let result = run(&mut terminal);


    execute!(io::stdout(), DisableMouseCapture)?;
    ratatui::restore();
    result
}


fn run(terminal: &mut ratatui::DefaultTerminal) -> io::Result<()> {
    let mut sys = System::new_all();
    let mut app = App::new();
    let mut last_refresh = Instant::now() - REFRESH_INTERVAL;


    loop {
        if last_refresh.elapsed() >= REFRESH_INTERVAL {
            sys.refresh_cpu_all();
            sys.refresh_processes(ProcessesToUpdate::All, true);
            last_refresh = Instant::now();
        }


        let mut processes: Vec<_> = sys.processes().iter().collect();
        processes.sort_by(|a, b| b.1.memory().cmp(&a.1.memory()));
        processes.truncate(50);
        let display_len = processes.len();


        app.sync_selection(&processes, display_len);
        app.expire_flash();
        app.tick();


        terminal.draw(|frame| render(frame, &sys, &mut app, &processes))?;


        let pids: Vec<sysinfo::Pid> = processes.iter().map(|(pid, _)| **pid).collect();


        // Block briefly for the first event, then HANDLE every event already queued -
        // never discard them. Silently dropping queued input is what made double-clicks
        // and button presses feel unreliable ("click several times") before.
        if event::poll(Duration::from_millis(16))? {
            loop {
                match event::read()? {
                    Event::Key(key) => {
                        if !app.handle_key(key.code, &mut sys, &pids, display_len) {
                            return Ok(());
                        }
                    }
                    Event::Mouse(mouse) => app.handle_mouse(mouse, &mut sys, &pids, display_len),
                    _ => {}
                }
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
    }
}


fn render(frame: &mut Frame, sys: &System, app: &mut App, processes: &[(&sysinfo::Pid, &sysinfo::Process)]) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(3)])
        .split(frame.area());


    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(main_chunks[0]);


    app.rects.table_area = content_chunks[0];


    render_table(frame, app, processes, content_chunks[0]);
    render_inspector(frame, sys, processes, app.table_state.selected().unwrap_or(0), content_chunks[1]);
    render_status_bar(frame, sys, app, main_chunks[1]);


    if let AppMode::ManageTask { pid } = app.mode {
        render_popup(frame, sys, app, pid);
    }
}


fn render_table(frame: &mut Frame, app: &mut App, processes: &[(&sysinfo::Pid, &sysinfo::Process)], area: Rect) {
    let rows: Vec<Row> = processes
        .iter()
        .enumerate()
        .map(|(index, (pid, proc))| {
            let name = proc.name().to_string_lossy();
            let memory = proc.memory() / 1024 / 1024;


            let is_selected = app.table_state.selected() == Some(index);
            let base_style = if Some(index) == app.hovered_row && !is_selected {
                Style::default().bg(BG_ROW_HOVER)
            } else if index % 2 == 0 {
                Style::default().bg(BG_ROW_A)
            } else {
                Style::default().bg(BG_ROW_B)
            };


            Row::new(vec![
                Cell::from(pid.to_string()),
                Cell::from(name.to_string()),
                Cell::from(format!("{memory} MB")),
            ])
            .style(base_style)
        })
        .collect();


    let widths = [
        Constraint::Length(10),
        Constraint::Percentage(50),
        Constraint::Percentage(40),
    ];


    let table = Table::new(rows, widths)
        .header(
            Row::new(vec!["PID", "NAME", "MEMORY"])
                .style(Style::default().fg(ACCENT_BLUE).add_modifier(Modifier::BOLD))
                .bottom_margin(1),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(BORDER_DIM))
                .title(" Processes  ·  double-click or Enter to manage ")
                .title_style(Style::default().fg(ACCENT_GREEN).add_modifier(Modifier::BOLD)),
        )
        .row_highlight_style(
            Style::default()
                .bg(BG_ROW_SELECTED)
                .fg(TEXT_MAIN)
                .add_modifier(Modifier::BOLD),
        );


    frame.render_stateful_widget(table, area, &mut app.table_state);


    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .thumb_style(Style::default().fg(ACCENT_BLUE))
        .track_style(Style::default().fg(BORDER_DIM));
    frame.render_stateful_widget(
        scrollbar,
        area.inner(Margin { vertical: 1, horizontal: 0 }),
        &mut app.scroll_state,
    );
}


fn render_inspector(
    frame: &mut Frame,
    sys: &System,
    processes: &[(&sysinfo::Pid, &sysinfo::Process)],
    selected_index: usize,
    area: Rect,
) {
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_DIM))
        .title(" Inspector ")
        .title_style(Style::default().fg(ACCENT_YELLOW).add_modifier(Modifier::BOLD));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);


    let selected = processes.get(selected_index);


    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);


    let text = if let Some((pid, proc)) = selected {
        let name = proc.name().to_string_lossy();
        let exe = proc.exe().map_or("Unknown".to_string(), |p| p.display().to_string());
        let virtual_memory = proc.virtual_memory() / 1024 / 1024;
        let status = format!("{:?}", proc.status());
        let run_time = proc.run_time();


        format!(
            "Name       {name}\n\
             PID        {pid}\n\
             Status     {status}\n\
             Virtual    {virtual_memory} MB\n\
             Uptime     {run_time}s\n\n\
             Path\n{exe}"
        )
    } else {
        "No process selected.".to_string()
    };


    frame.render_widget(Paragraph::new(text).style(Style::default().fg(TEXT_DIM)), chunks[0]);


    let total_mem_mb = (sys.total_memory() / 1024 / 1024).max(1) as f64;


    let (cpu, mem_mb) = selected
        .map(|(_, proc)| (proc.cpu_usage() as f64, (proc.memory() / 1024 / 1024) as f64))
        .unwrap_or((0.0, 0.0));

    let cpu_ratio = (cpu / 100.0).clamp(0.0, 1.0);
    let mem_ratio = (mem_mb / total_mem_mb).clamp(0.0, 1.0);


    frame.render_widget(
        Paragraph::new(format!("CPU   {cpu:.1}%")).style(Style::default().fg(TEXT_DIM)),
        chunks[2],
    );
    frame.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(usage_color(cpu_ratio)))
            .ratio(cpu_ratio)
            .label(""),
        chunks[3],
    );


    frame.render_widget(
        Paragraph::new(format!("MEM   {mem_mb:.0} MB")).style(Style::default().fg(TEXT_DIM)),
        chunks[4],
    );
    frame.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(usage_color(mem_ratio)))
            .ratio(mem_ratio)
            .label(""),
        chunks[5],
    );
}


fn render_status_bar(frame: &mut Frame, sys: &System, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(20), Constraint::Length(24)])
        .split(area);


    let (text, color) = if let Some((msg, _)) = &app.status_message {
        (format!(" {msg}"), ACCENT_GREEN)
    } else {
        let base = match app.mode {
            AppMode::Normal => " Double-click or Enter to manage  ·  q to quit".to_string(),
            AppMode::ManageTask { .. } => " Click a button, or press Y / Esc".to_string(),
        };
        (base, ACCENT_MUTED)
    };


    let status_bar = Paragraph::new(text).style(Style::default().fg(color)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER_DIM)),
    );
    frame.render_widget(status_bar, chunks[0]);


    let total = sys.total_memory().max(1);
    let used = sys.used_memory();
    let ratio = (used as f64 / total as f64).clamp(0.0, 1.0);
    let used_mb = used / 1024 / 1024;
    let total_mb = total / 1024 / 1024;


    let ram_gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(BORDER_DIM))
                .title(" RAM ")
                .title_style(Style::default().fg(ACCENT_MUTED)),
        )
        .gauge_style(Style::default().fg(usage_color(ratio)))
        .ratio(ratio)
        .label(format!("{used_mb}/{total_mb} MB"));


    frame.render_widget(ram_gauge, chunks[1]);
}


fn render_popup(frame: &mut Frame, sys: &System, app: &mut App, pid: sysinfo::Pid) {
    let (progress, ease_in) = match app.popup_anim {
        Some(PopupAnim::Opening(t)) => ((t.elapsed().as_millis() as f32 / POPUP_ANIM_MS as f32).min(1.0), true),
        Some(PopupAnim::Closing(t)) => (1.0 - (t.elapsed().as_millis() as f32 / POPUP_ANIM_MS as f32).min(1.0), false),
        None => (1.0, true),
    };
    let eased = if ease_in {
        1.0 - (1.0 - progress) * (1.0 - progress) // ease-out: quick start, gentle settle
    } else {
        progress * progress // ease-in: gentle start, quick "snap away" finish
    };


    let target = centered_rect(65, 45, frame.area());
    let popup_area = scale_toward(target, frame.area(), eased);


    let shadow_area = Rect {
        x: popup_area.x.saturating_add(1),
        y: popup_area.y.saturating_add(1),
        width: popup_area.width,
        height: popup_area.height,
    }
    .intersection(frame.area());
    frame.render_widget(Clear, shadow_area);
    frame.render_widget(Block::default().style(Style::default().bg(SHADOW)), shadow_area);


    frame.render_widget(Clear, popup_area);

    let popup_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT_RED))
        .title(" Terminate Process ")
        .title_style(Style::default().fg(ACCENT_RED).add_modifier(Modifier::BOLD));
    frame.render_widget(popup_block, popup_area);


    // Mid-animation the box is too small for its contents - skip interior layout until it
    // reaches full size so text never looks squeezed or clipped.
    if progress < 1.0 {
        return;
    }


    app.rects.btn_close_x = Rect {
        x: popup_area.x + popup_area.width.saturating_sub(5),
        y: popup_area.y,
        width: 4,
        height: 1,
    };


    let close_style = if app.hovered_button == Some(HoveredButton::CloseX) {
        Style::default().fg(Color::Black).bg(ACCENT_RED).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(ACCENT_RED)
    };
    frame.render_widget(
        Paragraph::new(" × ").style(close_style).alignment(ratatui::layout::Alignment::Center),
        app.rects.btn_close_x,
    );


    let inner_area = popup_area.inner(Margin { vertical: 1, horizontal: 2 });


    let modal_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // divider
            Constraint::Length(6), // body text
            Constraint::Length(1), // spacer
            Constraint::Length(3), // action buttons
        ])
        .split(inner_area);


    let divider = "─".repeat(inner_area.width as usize);
    frame.render_widget(
        Paragraph::new(divider).style(Style::default().fg(BORDER_DIM)),
        modal_chunks[0],
    );


    let modal_content = if let Some(proc) = sys.process(pid) {
        let name = proc.name().to_string_lossy();
        let memory = proc.memory() / 1024 / 1024;
        let cpu = proc.cpu_usage();
        let exe = proc.exe().map_or("Unknown path".to_string(), |p| p.display().to_string());
        let status = format!("{:?}", proc.status());


        format!(
            "Name        {name}\n\
             PID         {pid}\n\
             Status      {status}\n\
             CPU         {cpu:.1}%\n\
             Memory      {memory} MB\n\
             Executable  {exe}"
        )
    } else {
        format!("Process with PID {pid} is no longer active.")
    };


    frame.render_widget(
        Paragraph::new(modal_content).style(Style::default().fg(TEXT_MAIN)),
        modal_chunks[1],
    );


    let button_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(48), Constraint::Percentage(48)])
        .spacing(ratatui::layout::Spacing::Overlap(0))
        .split(modal_chunks[3]);


    app.rects.btn_terminate = button_chunks[0];
    app.rects.btn_cancel = button_chunks[1];


    let terminate_hovered = app.hovered_button == Some(HoveredButton::Terminate);
    let cancel_hovered = app.hovered_button == Some(HoveredButton::Cancel);


    let terminate_btn = Paragraph::new("Terminate")
        .style(
            Style::default()
                .fg(if terminate_hovered { Color::Black } else { ACCENT_RED })
                .bg(if terminate_hovered { ACCENT_RED } else { Color::Reset })
                .add_modifier(Modifier::BOLD),
        )
        .alignment(ratatui::layout::Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(ACCENT_RED)),
        );


    let cancel_btn = Paragraph::new("Cancel")
        .style(
            Style::default()
                .fg(if cancel_hovered { Color::Black } else { ACCENT_MUTED })
                .bg(if cancel_hovered { ACCENT_MUTED } else { Color::Reset })
                .add_modifier(Modifier::BOLD),
        )
        .alignment(ratatui::layout::Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(ACCENT_MUTED)),
        );


    frame.render_widget(terminate_btn, button_chunks[0]);
    frame.render_widget(cancel_btn, button_chunks[1]);
}


/// Interpolates a Rect from a single point at its center out to `target`'s full size as `t`
/// goes from 0.0 to 1.0 (and back down for a close animation).
fn scale_toward(target: Rect, screen: Rect, t: f32) -> Rect {
    let cx = target.x as f32 + target.width as f32 / 2.0;
    let cy = target.y as f32 + target.height as f32 / 2.0;
    let w = (target.width as f32 * t).max(1.0);
    let h = (target.height as f32 * t).max(1.0);
    let x = (cx - w / 2.0).max(0.0);
    let y = (cy - h / 2.0).max(0.0);


    Rect {
        x: x.round() as u16,
        y: y.round() as u16,
        width: (w.round() as u16).min(screen.width),
        height: (h.round() as u16).min(screen.height),
    }
}


fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);


    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
