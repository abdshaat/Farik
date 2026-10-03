//! A port for code that binds it later, shared by every test that needs one.

/// A port nothing is listening on. Taken from below the kernel's ephemeral range (32768 and up),
/// so no other test's port-0 listener can be handed it between this probe and the product's bind;
/// the per-process offset and the counter keep parallel tests and test processes off each other's.
pub fn free_port() -> u16 {
    static NEXT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
    loop {
        let step = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let base = u16::try_from(std::process::id() % 6_000).expect("under 6000");
        let port = 20_000 + base * 2 + step % 2_000;
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
}
