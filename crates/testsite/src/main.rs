fn main() {
    let port = std::env::args().nth(1).and_then(|p| p.parse().ok()).unwrap_or(18080);
    testsite::spawn(port);
    eprintln!("testsite on http://127.0.0.1:{port}");
    loop {
        std::thread::park();
    }
}
