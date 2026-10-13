use std::io::{IsTerminal, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use planrisk::finding::Severity;
use planrisk::json::render_json;
use planrisk::markdown::render_markdown;
use planrisk::report::Report;
use planrisk::rules::{all_rules, render_rule_list};
use planrisk::summary::render_text_colored;

/// Exit codes, in one place. Also listed in `--help` (see `EXIT_CODES_HELP`)
/// and PLAN.md §9. Changing them breaks CI pipelines, so treat them as fixed.
mod exit_code {
    /// Ran fine, and no finding is at or above `--fail-on` (or it wasn't set).
    pub const OK: u8 = 0;
    /// Something went wrong: bad input, unreadable file, bad arguments.
    pub const ERROR: u8 = 1;
    /// Ran fine, but at least one finding is at or above `--fail-on`.
    pub const FAIL_ON: u8 = 2;
}

const EXIT_CODES_HELP: &str = "\
Exit codes:
  0  no finding at or above --fail-on (or --fail-on not set)
  1  error: bad input, unreadable file or bad arguments
  2  at least one finding at or above --fail-on";

/// Reads a Terraform plan and flags risky changes.
#[derive(Parser)]
#[command(version, about, after_help = EXIT_CODES_HELP)]
// PLAN_JSON is required, except when a subcommand like `rules` is given; and
// subcommands don't take the plan options. A file named `rules` can still be
// checked as `./rules`.
#[command(subcommand_negates_reqs = true, args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Plan JSON from `terraform show -json plan.out`, or `-` to read stdin.
    #[arg(value_name = "PLAN_JSON", required = true)]
    plan: Option<String>,

    /// Output format [default: terminal when output is a terminal,
    /// markdown otherwise, e.g. in CI].
    #[arg(long, value_enum)]
    format: Option<Format>,

    /// Don't use colors in terminal output. Colors are also off when output
    /// isn't a terminal, or when the NO_COLOR environment variable is set.
    #[arg(long)]
    no_color: bool,

    /// Exit with code 2 if any finding is this severe or worse.
    /// Without it, findings never fail the run.
    #[arg(long, value_enum, value_name = "SEVERITY")]
    fail_on: Option<Severity>,
}

#[derive(Subcommand)]
enum Command {
    /// List every rule with its default severity.
    Rules,
}

/// Output formats for `--format` (PLAN.md §11).
#[derive(Clone, Copy, ValueEnum)]
enum Format {
    /// Human-readable summary.
    Terminal,
    /// Markdown for a pull request comment.
    Markdown,
    /// Machine-readable JSON, for scripts and other tools.
    // The format is documented in src/json.rs (and the README, issue #30).
    Json,
}

fn main() -> ExitCode {
    // `try_parse` instead of `parse`: on bad arguments clap would exit with
    // code 2, which planrisk reserves for `--fail-on`.
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            // Prints usage errors to stderr, and --help/--version to stdout.
            let _ = err.print();
            return ExitCode::from(if err.use_stderr() {
                exit_code::ERROR
            } else {
                exit_code::OK
            });
        }
    };

    match run(&cli) {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            // `{:#}` prints the error and its causes on one line,
            // e.g. "could not read plan file `x`: No such file or directory".
            eprintln!("error: {err:#}");
            ExitCode::from(exit_code::ERROR)
        }
    }
}

/// Runs planrisk and returns the exit code for a successful run.
fn run(cli: &Cli) -> anyhow::Result<u8> {
    if let Some(Command::Rules) = cli.command {
        print!("{}", render_rule_list(&all_rules()));
        return Ok(exit_code::OK);
    }

    // clap guarantees PLAN_JSON is present when there is no subcommand.
    let source = cli.plan.as_deref().expect("PLAN_JSON is required");
    let plan = planrisk::input::read_plan(source)?;
    let report = Report::build(plan, &all_rules());
    // Without --format: a person at a terminal gets the terminal summary;
    // anything else (a pipe, a file, CI) gets Markdown for a PR comment.
    let format = cli.format.unwrap_or(if std::io::stdout().is_terminal() {
        Format::Terminal
    } else {
        Format::Markdown
    });
    match format {
        Format::Terminal => {
            // anstream removes the colors when stdout isn't a terminal, or
            // NO_COLOR / CLICOLOR say so; `--no-color` always removes them.
            let stdout = std::io::stdout();
            let mut out = if cli.no_color {
                anstream::AutoStream::never(stdout)
            } else {
                anstream::AutoStream::auto(stdout)
            };
            write!(out, "{}", render_text_colored(&report))?;
        }
        Format::Markdown => print!("{}", render_markdown(&report)),
        Format::Json => print!("{}", render_json(&report)),
    }

    if let Some(threshold) = cli.fail_on {
        let failing = report
            .findings
            .iter()
            .filter(|finding| finding.severity >= threshold)
            .count();
        if failing > 0 {
            // On stderr, so it shows in CI logs without mixing into JSON on stdout.
            let s = if failing == 1 { "" } else { "s" };
            eprintln!("planrisk: {failing} finding{s} at or above --fail-on {threshold}");
            return Ok(exit_code::FAIL_ON);
        }
    }
    Ok(exit_code::OK)
}
