use clap::Parser;
use lore_cli::arguments::Cli;

#[test]
fn query_terms_are_ored() {
    let cli = Cli::parse_from(["lore", "hashmap", "serde"]);
    assert_eq!(cli.query, vec!["hashmap", "serde"]);
}

#[test]
fn walk_yield_interval_default_and_override() {
    let cli = Cli::parse_from(["lore"]);
    assert_eq!(cli.walk_yield_interval, 1024);

    let cli = Cli::parse_from(["lore", "--walk-yield-interval", "1"]);
    assert_eq!(cli.walk_yield_interval, 1);
}

#[test]
fn workers_default_and_override() {
    let cli = Cli::parse_from(["lore"]);
    assert_eq!(cli.workers, num_cpus::get().saturating_sub(2).max(1));

    let cli = Cli::parse_from(["lore", "-j", "2"]);
    assert_eq!(cli.workers, 2);
}
