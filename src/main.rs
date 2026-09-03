use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style, Stylize},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};
use std::io;
use std::time::Duration;
use sysinfo::{ProcessesToUpdate, System};


fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let result = run(&mut terminal);
    ratatui::restore();
    result
}


fn run(terminal: &mut ratatui::DefaultTerminal) -> io::Result<()> {
    let mut sys = System::new_all();
    let mut table_state = TableState::default();
    table_state.select(Some(0));


    loop {
        sys.refresh_processes(ProcessesToUpdate::All, true);


        let mut processes: Vec<_> = sys.processes().iter().collect();
        processes.sort_by(|a, b| b.1.memory().cmp(&a.1.memory()));
        let display_len = processes.len();


        terminal.draw(|frame| render(frame, &sys, &mut table_state, &processes))?;


        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Down | KeyCode::Char('j') => {
                        let i = match table_state.selected() {
                            Some(i) => {
                                if i >= display_len.saturating_sub(1) { 0 } else { i + 1 }
                            }
                            None => 0,
                        };
                        table_state.select(Some(i));
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        let i = match table_state.selected() {
                            Some(i) => {
                                if i == 0 { display_len.saturating_sub(1) } else { i - 1 }
                            }
                            None => 0,
                        };
                        table_state.select(Some(i));
                    }
                    _ => {}
                }
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
) {
    // Main vertical layout: Top section for table + inspector, bottom section for status bar
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(frame.area());


    // Horizontal split: Left side for table, Right side for process inspector details
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        ])
        .split(main_chunks[0]);


    let rows: Vec<Row> = processes
        .iter()
        .enumerate()
        .map(|(index, (pid, proc))| {
            let name = proc.name().to_string_lossy();
            let memory = proc.memory() / 1024 / 1024;

            
            let base_style = if index % 2 == 0 {
                Style::default()
            } else {
                Style::default().bg(Color::Rgb(20, 20, 25))
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
                .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                .bottom_margin(1),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" 🚀 rogue-watch: Processes ")
                .title_style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 60, 90))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );


    frame.render_stateful_widget(table, content_chunks[0], table_state);


    // Extract details for the currently selected process
    let selected_index = table_state.selected().unwrap_or(0);
    let inspector_text = if let Some((pid, proc)) = processes.get(selected_index) {
        let name = proc.name().to_string_lossy();
        let exe = proc.exe().map_or("Unknown".to_string(), |p| p.display().to_string());
        let memory = proc.memory() / 1024 / 1024;
        let virtual_memory = proc.virtual_memory() / 1024 / 1024;
        let status = format!("{:?}", proc.status());
        let run_time = proc.run_time();


        format!(
            "Process Inspector\n\n\
             • PID: {}\n\
             • Name: {}\n\
             • Status: {}\n\
             • Memory: {} MB\n\
             • Virtual Mem: {} MB\n\
             • Uptime: {}s\n\n\
             Executable Path:\n{}",
            pid, name, status, memory, virtual_memory, run_time, exe
        )
    } else {
        "No process selected.".to_string()
    };


    let inspector_panel = Paragraph::new(inspector_text)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" 🔍 Inspector ")
                .title_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        );


    frame.render_widget(inspector_panel, content_chunks[1]);


    // Bottom status bar
    let total_memory = sys.total_memory() / 1024 / 1024;
    let used_memory = sys.used_memory() / 1024 / 1024;
    let status_text = format!(
        " System RAM: {}MB / {}MB | [↑/↓ or j/k] Navigate | [q] Quit",
        used_memory, total_memory
    );


    let status_bar = Paragraph::new(status_text)
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL));


    frame.render_widget(status_bar, main_chunks[1]);
}
