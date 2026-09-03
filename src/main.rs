use std::thread;
use std::time::Duration;
use sysinfo::{ProcessesToUpdate, System};


fn main() {
    println!("Initializing Silent Background Process Rescuer...");


    let mut sys = System::new_all();
    sys.refresh_all();


    thread::sleep(Duration::from_millis(500));
    sys.refresh_processes(ProcessesToUpdate::All, true);


    println!("Scanning active processes (Top memory hogs):");


    let mut processes: Vec<(&sysinfo::Pid, &sysinfo::Process)> = sys.processes().iter().collect();
    processes.sort_by(|a, b| b.1.memory().cmp(&a.1.memory()));


    for (pid, process) in processes.iter().take(5) {
        let name = process.name();
        let memory_mb = process.memory() / 1024 / 1024;
        let cpu_usage = process.cpu_usage();


        println!(
            "PID: {:<6} | Name: {:<20} | CPU: {:>5.1}% | RAM: {} MB",
            pid, name.to_string_lossy(), cpu_usage, memory_mb
        );
    }
}
