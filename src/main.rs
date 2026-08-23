use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ippo::{
    config::{Profile, RuntimeConfig, RuntimeOptions},
    diagnostics::Diagnostics,
    storage::Database,
    tui,
};

#[derive(Debug, Parser)]
#[command(
    name = "ippo",
    version,
    about = "A fast, local-first habit tracker for your terminal"
)]
struct Cli {
    #[cfg_attr(
        debug_assertions,
        doc = "Select the personal or isolated development data profile."
    )]
    #[cfg_attr(not(debug_assertions), doc = "Select the personal data profile.")]
    #[arg(long, global = true, value_enum)]
    profile: Option<Profile>,

    /// Use an explicit SQLite database path.
    #[arg(long, global = true, value_name = "PATH")]
    database: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show the active data environment without exposing habit contents.
    Doctor {
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = RuntimeConfig::resolve(RuntimeOptions {
        profile: cli.profile,
        database: cli.database,
    })
    .context("could not resolve ippo's data environment")?;

    let database = Database::open(&config.database_path, config.environment)
        .with_context(|| format!("could not open {}", config.database_path.display()))?;
    let diagnostics = Diagnostics::collect(
        config.profile,
        &config.database_path,
        config.database_overridden,
        &database,
    )
    .context("could not inspect the ippo database")?;

    match cli.command {
        Some(Command::Doctor { json }) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&diagnostics)
                        .context("could not serialize diagnostics")?
                );
            } else {
                println!("{}", diagnostics.human_readable());
            }
        }
        None => tui::run(&diagnostics).context("ippo could not run its terminal interface")?,
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(debug_assertions)]
    fn doctor_accepts_global_profile_before_subcommand() {
        let cli =
            Cli::try_parse_from(["ippo", "--profile", "dev", "doctor"]).expect("CLI should parse");

        assert_eq!(cli.profile, Some(Profile::Dev));
        assert!(matches!(cli.command, Some(Command::Doctor { json: false })));
    }

    #[test]
    fn doctor_accepts_global_profile_after_subcommand() {
        let cli = Cli::try_parse_from(["ippo", "doctor", "--profile", "personal", "--json"])
            .expect("CLI should parse");

        assert_eq!(cli.profile, Some(Profile::Personal));
        assert!(matches!(cli.command, Some(Command::Doctor { json: true })));
    }
}
