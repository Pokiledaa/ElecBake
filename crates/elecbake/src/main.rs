use clap::Parser;

#[derive(Parser)]
#[command(name = "elecbake", version, about = "AI copilot for electronic circuit design")]
struct Cli {}

fn main() {
    Cli::parse();
}
