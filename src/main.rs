use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton, MouseEventKind},
    execute,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Table, TableState},
    Frame,
};
use std::io;
use std::time::Duration;
use sysinfo::{ProcessesToUpdate, System};


static mut TABLE_AREA_X: u16 = 0;
static mut TABLE_AREA_Y: u16 = 0;
static mut TABLE_AREA_WIDTH: u16 = 0;
static mut TABLE_AREA_HEIGHT: u16 = 0;
static mut SCREEN_AREA: Rect = Rect { x: 0, y: 0, width: 0, height: 0 };


static mut BTN_TERMINATE_RECT: Rect = Rect { x: 0, y: 0, width: 0, height: 0 };
static mut BTN_CANCEL_RECT: Rect = Rect { x: 0, y: 0, width: 0, height: 0 };
static mut BTN_CLOSE_X_RECT: Rect = Rect { x: 0, y: 0, width: 0, height: 0 };


#[derive(PartialEq)]
enum AppMode {
    Normal,
    ManageTask { pid: sysinfo::Pid },
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
    let mut table_state = TableState::default();
    table_state.select(Some(0));
    let mut selected_pid: Option<sysinfo::Pid> = None;
    let mut mode = AppMode::Normal;


    loop {
        sys.refresh_cpu_all();
        sys.refresh_processes(ProcessesToUpdate::All, true);


        let mut processes: Vec<_> = sys.processes().iter().collect();
        processes.sort_by(|a, b| b.1.memory().cmp(&a.1.memory()));
        processes.truncate(50);
        let display_len = processes.len();


        // Maintain stable selection by tracking PID instead of volatile index
        if let Some(pid) = selected_pid {
            if let Some(pos) = processes.iter().position(|(p, _)| **p == pid) {
                table_state.select(Some(pos));
            } else if display_len > 0 {
                table_state.select(Some(0));
                selected_pid = processes.first().map(|(p, _)| **p);
            }
        } else if display_len > 0 {
            table_state.select(Some(0));
            selected_pid = processes.first().map(|(p, _)| **p);
        }


        if let Some(i) = table_state.selected() {
            if i < display_len {
                if let Some((pid, _)) = processes.get(i) {
                    selected_pid = Some(**pid);
                }
            }
        }


        terminal.draw(|frame| render(frame, &sys, &mut table_state, &processes, &mode))?;


        // Slower 500ms tick rate stabilizes CPU delta calculations and reduces UI jitter
        if event::poll(Duration::from_millis(500))? {
            match event::read()? {
                Event::Key(key) => {
                    match mode {
                        AppMode::Normal => {
                            match key.code {
                                KeyCode::Char('q') => break,
                                KeyCode::Down | KeyCode::Char('j') => {
                                    let i = match table_state.selected() {
                                        Some(i) => if i < display_len.saturating_sub(1) { i + 1 } else { i },
                                        None => 0,
                                    };
                                    table_state.select(Some(i));
                                    if let Some((pid, _)) = processes.get(i) {
                                        selected_pid = Some(**pid);
                                    }
                                }
                                KeyCode::Up | KeyCode::Char('k') => {
                                    let i = match table_state.selected() {
                                        Some(i) => if i > 0 { i - 1 } else { 0 },
                                        None => 0,
                                    };
                                    table_state.select(Some(i));
                                    if let Some((pid, _)) = processes.get(i) {
                                        selected_pid = Some(**pid);
                                    }
                                }
                                KeyCode::Enter => {
                                    if let Some(i) = table_state.selected() {
                                        if let Some((pid, _)) = processes.get(i) {
                                            mode = AppMode::ManageTask { pid: **pid };
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        AppMode::ManageTask { pid } => {
                            match key.code {
                                KeyCode::Char('y') | KeyCode::Char('Y') => {
                                    if let Some(proc) = sys.process(pid) {
                                        proc.kill();
                                    }
                                    mode = AppMode::Normal;
                                }
                                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                    mode = AppMode::Normal;
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Event::Mouse(mouse) => {
                    if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                        let x = mouse.column;
                        let y = mouse.row;


                        match mode {
                            AppMode::Normal => {
                                unsafe {
                                    let header_offset = 3;
                                    if x >= TABLE_AREA_X 
                                        && x < TABLE_AREA_X + TABLE_AREA_WIDTH 
                                        && y >= TABLE_AREA_Y + header_offset 
                                        && y < TABLE_AREA_Y + TABLE_AREA_HEIGHT - 1 
                                    {
                                        let clicked_visual_row = (y - (TABLE_AREA_Y + header_offset)) as usize;
                                        let target_index = clicked_visual_row + table_state.offset(); 
                                        if target_index < display_len {
                                            let previous_selection = table_state.selected();
                                            table_state.select(Some(target_index));
                                            if let Some((pid, _)) = processes.get(target_index) {
                                                selected_pid = Some(**pid);
                                            }
                                            
                                            if previous_selection == Some(target_index) {
                                                if let Some((pid, _)) = processes.get(target_index) {
                                                    mode = AppMode::ManageTask { pid: **pid };
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            AppMode::ManageTask { pid } => {
                                unsafe {
                                    if x >= BTN_CLOSE_X_RECT.x && x < BTN_CLOSE_X_RECT.x + BTN_CLOSE_X_RECT.width
                                        && y >= BTN_CLOSE_X_RECT.y && y < BTN_CLOSE_X_RECT.y + BTN_CLOSE_X_RECT.height 
                                    {
                                        mode = AppMode::Normal;
                                    }
                                    else if x >= BTN_TERMINATE_RECT.x && x < BTN_TERMINATE_RECT.x + BTN_TERMINATE_RECT.width
                                        && y >= BTN_TERMINATE_RECT.y && y < BTN_TERMINATE_RECT.y + BTN_TERMINATE_RECT.height 
                                    {
                                        if let Some(proc) = sys.process(pid) {
                                            proc.kill();
                                        }
                                        mode = AppMode::Normal;
                                    }
                                    else if x >= BTN_CANCEL_RECT.x && x < BTN_CANCEL_RECT.x + BTN_CANCEL_RECT.width
                                        && y >= BTN_CANCEL_RECT.y && y < BTN_CANCEL_RECT.y + BTN_CANCEL_RECT.height 
                                    {
                                        mode = AppMode::Normal;
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }


            while event::poll(Duration::ZERO)? {
                let _ = event::read();
            }
        }
    }
    Ok(())
}


fn render(
    frame: &mut Frame,
    sys: &System,
    table_state: &mut TableState,
    processes: &[(&sysinfo::Pid, &sysinfo::Process)],
    mode: &AppMode,
) {
    unsafe {
        SCREEN_AREA = frame.area();
    }


    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(3)])
        .split(frame.area());


    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(main_chunks[0]);


    unsafe {
        TABLE_AREA_X = content_chunks[0].x;
        TABLE_AREA_Y = content_chunks[0].y;
        TABLE_AREA_WIDTH = content_chunks[0].width;
        TABLE_AREA_HEIGHT = content_chunks[0].height;
    }


    let rows: Vec<Row> = processes
        .iter()
        .enumerate()
        .map(|(index, (pid, proc))| {
            let name = proc.name().to_string_lossy();
            let memory = proc.memory() / 1024 / 1024;

            
            let base_style = if index % 2 == 0 {
                Style::default().bg(Color::Rgb(15, 17, 23))
            } else {
                Style::default().bg(Color::Rgb(22, 25, 34))
            };


            Row::new(vec![
                Cell::from(pid.to_string()),
                Cell::from(name.to_string()),
                Cell::from(format!("{} MB", memory)),
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
            Row::new(vec!["PID", "Name", "Memory"])
                .style(Style::default().fg(Color::Rgb(137, 180, 250)).add_modifier(Modifier::BOLD))
                .bottom_margin(1),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Rgb(88, 91, 112)))
                .title(" ⚡ Active Processes (Double-Click or Enter to Manage) ")
                .title_style(Style::default().fg(Color::Rgb(166, 227, 161)).add_modifier(Modifier::BOLD)),
        )
        .row_highlight_style(
            Style::default()
                .bg(Color::Rgb(49, 50, 68))
                .fg(Color::Rgb(205, 214, 244))
                .add_modifier(Modifier::BOLD),
        );


    frame.render_stateful_widget(table, content_chunks[0], table_state);


    let selected_index = table_state.selected().unwrap_or(0);
    let inspector_text = if let Some((pid, proc)) = processes.get(selected_index) {
        let name = proc.name().to_string_lossy();
        let exe = proc.exe().map_or("Unknown".to_string(), |p| p.display().to_string());
        let memory = proc.memory() / 1024 / 1024;
        let virtual_memory = proc.virtual_memory() / 1024 / 1024;
        let cpu = proc.cpu_usage();
        let status = format!("{:?}", proc.status());
        let run_time = proc.run_time();


        format!(
            "Process Details\n\n\
             • PID: {}\n\
             • Name: {}\n\
             • Status: {}\n\
             • CPU Usage: {:.1}%\n\
             • Memory: {} MB\n\
             • Virtual Mem: {} MB\n\
             • Uptime: {}s\n\n\
             Path:\n{}",
            pid, name, status, cpu, memory, virtual_memory, run_time, exe
        )
    } else {
        "No process selected.".to_string()
    };


    let inspector_panel = Paragraph::new(inspector_text)
        .style(Style::default().fg(Color::Rgb(186, 194, 222)))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Rgb(88, 91, 112)))
                .title(" 🔍 Inspector ")
                .title_style(Style::default().fg(Color::Rgb(250, 179, 135)).add_modifier(Modifier::BOLD)),
        );

    frame.render_widget(inspector_panel, content_chunks[1]);


    let total_memory = sys.total_memory() / 1024 / 1024;
    let used_memory = sys.used_memory() / 1024 / 1024;
    let status_text = match mode {
        AppMode::Normal => format!(
            " RAM: {}MB / {}MB | [Double-Click / Enter] Manage Task | [q] Quit",
            used_memory, total_memory
        ),
        AppMode::ManageTask { .. } => {
            format!(" RAM: {}MB / {}MB | Click buttons or press [Y/Esc]", used_memory, total_memory)
        }
    };

    let status_bar = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Rgb(147, 154, 183)))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Rgb(88, 91, 112)))
        );


    frame.render_widget(status_bar, main_chunks[1]);


    // Render Modal with explicit non-collapsing layout constraints for buttons
    if let AppMode::ManageTask { pid } = mode {
        let popup_area = centered_rect(65, 45, frame.area());
        frame.render_widget(Clear, popup_area);


        let popup_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(243, 139, 168)))
            .title(" ⚠️ Task Management Center ")
            .title_style(Style::default().fg(Color::Rgb(243, 139, 168)).add_modifier(Modifier::BOLD));


        frame.render_widget(popup_block, popup_area);


        unsafe {
            BTN_CLOSE_X_RECT = Rect {
                x: popup_area.x + popup_area.width - 6,
                y: popup_area.y,
                width: 5,
                height: 1,
            };
        }


        let close_x_para = Paragraph::new("[ X ]")
            .style(Style::default().fg(Color::Rgb(243, 139, 168)).add_modifier(Modifier::BOLD));
        unsafe {
            frame.render_widget(close_x_para, BTN_CLOSE_X_RECT);
        }


        let inner_area = popup_area.inner(ratatui::layout::Margin { vertical: 1, horizontal: 2 });

        
        // Use explicit Length constraints so buttons never get clipped out
        let modal_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(7), // Text body info
                Constraint::Length(1), // Spacer gap
                Constraint::Length(3), // Action buttons row
            ])
            .split(inner_area);


        let modal_content = if let Some(proc) = sys.process(*pid) {
            let name = proc.name().to_string_lossy();
            let memory = proc.memory() / 1024 / 1024;
            let cpu = proc.cpu_usage();
            let exe = proc.exe().map_or("Unknown path".to_string(), |p| p.display().to_string());
            let status = format!("{:?}", proc.status());


            format!(
                "• Process Name: {}\n\
                 • Target PID:   {}\n\
                 • Status:       {}\n\
                 • CPU Usage:    {:.1}%\n\
                 • Memory:       {} MB\n\
                 • Executable:   {}",
                name, pid, status, cpu, memory, exe
            )
        } else {
            format!("Process with PID {} no longer active.", pid)
        };


        let body_paragraph = Paragraph::new(modal_content)
            .style(Style::default().fg(Color::Rgb(205, 214, 244)));
        frame.render_widget(body_paragraph, modal_chunks[0]);


        let button_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(48), Constraint::Percentage(48)])
            .spacing(ratatui::layout::Spacing::Overlap(0))
            .split(modal_chunks[2]); // Guaranteed rendering slot


        let term_rect = button_chunks[0];
        let cancel_rect = button_chunks[1];


        unsafe {
            BTN_TERMINATE_RECT = term_rect;
            BTN_CANCEL_RECT = cancel_rect;
        }


        let terminate_btn = Paragraph::new("🔴 Terminate Task")
            .style(Style::default().fg(Color::Rgb(243, 139, 168)).add_modifier(Modifier::BOLD))
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Rgb(243, 139, 168))));


        let cancel_btn = Paragraph::new("✖ Cancel")
            .style(Style::default().fg(Color::Rgb(147, 154, 183)).add_modifier(Modifier::BOLD))
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Rgb(147, 154, 183))));


        frame.render_widget(terminate_btn, term_rect);
        frame.render_widget(cancel_btn, cancel_rect);
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
