//! What the speaker would get on this machine's default communications
//! output, for a few buffer requests: the buffer WASAPI gives, the device
//! period and the stream latency. The client is initialised and closed
//! without being started, so nothing plays.
//!
//! `cargo run -p yap-speaker --example probe`

fn main() {
    #[cfg(windows)]
    for ms in [10u64, 20, 40] {
        match yap_speaker::probe_default(std::time::Duration::from_millis(ms)) {
            Ok(p) => println!(
                "request {ms:>2} ms: {} buffer {:.1} ms, period {:.1} ms, latency (stream + period) {:.1} ms",
                p.name, p.buffer_ms, p.period_ms, p.latency_ms
            ),
            Err(error) => println!("request {ms} ms: {error}"),
        }
    }
    #[cfg(not(windows))]
    println!("WASAPI only");
}
