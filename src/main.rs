fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("-h" | "--help") => {
            print_help();
        }
        Some("--mcp") => {
            if args.next().is_some() {
                eprintln!("{} --mcp takes no other arguments.", rustmix::PROCESS_NAME);
                std::process::exit(2);
            }
            if let Err(err) = rustmix::mcp::serve() {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        Some(other) => {
            eprintln!("Unknown argument: {other}");
            print_help();
            std::process::exit(2);
        }
        None => {
            #[cfg(feature = "ui")]
            match rustmix::ui::run() {
                Ok(rustmix::ui::Exit::Quit) => {}
                Ok(rustmix::ui::Exit::AlreadyRunning) => {
                    eprintln!("{} is already running.", rustmix::APP_NAME);
                }
                Err(err) => {
                    eprintln!("{err}");
                    std::process::exit(1);
                }
            }
            #[cfg(not(feature = "ui"))]
            {
                eprintln!("This build has no window. Rebuild with --features ui.");
                std::process::exit(1);
            }
        }
    }
}

fn print_help() {
    let socket = rustmix::ipc::socket_path();
    println!(
        "\
RustMix Crimson Edition
A native mixer and patchbay for the MOTU 828.

RustMix               window
RustMix --mcp         MCP on stdin for the window that is already running

MCP is off until Control turns it on. The address and port are set there.

Socket: {}
",
        socket.display()
    );
}
