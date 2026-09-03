use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    style::Stylize,
    widgets::{Block, Cell, Row, Table},
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


    loop {
        sys.refresh_processes(ProcessesToUpdate::All, true);


        terminal.draw(|frame| render(frame, &sys))?;


        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if let KeyCode::Char('q') = key.code {
                    break;
                }
            }
        }
    }
    Ok(())
}


fn render(frame: &mut Frame, sys: &System) {
    let block = Block::bordered()
        .title(" rogue-watch | Silent Background Process Rescuer ");


    // Collect processes into a vector so we can sort them by memory
    let mut processes: Vec<_> = sys.processes().iter().collect();


    // Sort by memory usage descending (heaviest first)
    processes.sort_by(|a, b| b.1.memory().cmp(&a.1.memory()));


    let rows: Vec<Row> = processes
        .into_iter()
        .take(15)
        .map(|(pid, proc)| {
            let name = proc.name().to_string_lossy();
            let memory = proc.memory() / 1024 / 1024; // Convert to MB
            Row::new(vec![
                Cell::from(pid.to_string()),
                Cell::from(name.to_string()),
                Cell::from(format!("{} MB", memory)),
            ])
        })
        .collect();


    let widths = [
        ratatui::layout::Constraint::Length(10),
        ratatui::layout::Constraint::Percentage(60),
        ratatui::layout::Constraint::Percentage(30),
    ];


    let table = Table::new(rows, widths)
        .header(
            Row::new(vec!["PID", "Process Name", "Memory"])
                .style(ratatui::style::Style::default().bold()),
        )
        .block(block);


    frame.render_widget(table, frame.area());
}
