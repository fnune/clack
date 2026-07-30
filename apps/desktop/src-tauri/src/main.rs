#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // polkit re-executes this binary, not the command line one, so the
    // privileged half of a change is answered here before any window exists.
    if let Some(change) = clack::access::privileged_change(std::env::args()) {
        if let Err(error) = clack::access::apply_as_root(change) {
            eprintln!("clack-desktop: {error}");
            std::process::exit(1);
        }

        return;
    }

    clack_desktop_lib::run();
}
