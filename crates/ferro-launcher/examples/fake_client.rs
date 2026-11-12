//! Stand-in for the client in end-to-end tests: stays alive for a few
//! seconds so the loader can be injected and initialise.

fn main() {
    let secs = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(3);
    std::thread::sleep(std::time::Duration::from_secs(secs));
}
