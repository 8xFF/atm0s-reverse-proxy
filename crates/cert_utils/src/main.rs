use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;

/// A Certs util for quic, which generate der cert and key based on domain
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Domains
    #[arg(env, long)]
    domains: Vec<String>,
}

fn main() {
    let args = Args::parse();
    let cert = rcgen::generate_simple_self_signed(args.domains).expect("Should generate cert");
    let start = SystemTime::now();
    let since_the_epoch = start.duration_since(UNIX_EPOCH).expect("Time went backwards").as_millis();
    std::fs::write(format!("./certificate-{}.cert", since_the_epoch), cert.cert.der()).expect("Should write cert");
    std::fs::write(format!("./certificate-{}.key", since_the_epoch), cert.key_pair.serialize_der()).expect("Should write key");
}
