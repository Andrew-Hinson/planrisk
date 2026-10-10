use std::process::ExitCode;

use clap::Parser;
use planrisk::report::Report;
use planrisk::rules::all_rules;
use planrisk::summary::render_text;

/// Reads a Terraform plan and flags risky changes.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Plan JSON from `terraform show -json plan.out`, or `-` to read stdin.
    #[arg(value_name = "PLAN_JSON")]
    plan: String,
}

/// Exit code for any tool error: bad input, unreadable file, bad arguments.
/// Exit codes are listed in PLAN.md §9; 2 is reserved for `--fail-on`.
const EXIT_ERROR: u8 = 1;

fn main() -> ExitCode {
    // `try_parse` instead of `parse`: on bad arguments clap would exit with
    // code 2, which planrisk reserves for "risk threshold exceeded".
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            // Prints usage errors to stderr, and --help/--version to stdout.
            let _ = err.print();
            return if err.use_stderr() {
                ExitCode::from(EXIT_ERROR)
            } else {
                ExitCode::SUCCESS
            };
        }
    };

    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // `{:#}` prints the error and its causes on one line,
            // e.g. "could not read plan file `x`: No such file or directory".
            eprintln!("error: {err:#}");
            ExitCode::from(EXIT_ERROR)
        }
    }
}

fn run(cli: &Cli) -> anyhow::Result<()> {
    let plan = planrisk::input::read_plan(&cli.plan)?;
    let report = Report::build(plan, &all_rules());
    print!("{}", render_text(&report));
    Ok(())
}
