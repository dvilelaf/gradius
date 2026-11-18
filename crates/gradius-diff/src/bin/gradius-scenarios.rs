//! `gradius-scenarios`: run the oracle-driven user-action matrix.
//!
//! Each scenario drives the cartridge and the port from reset with one input
//! script, compares state (and sampled pixels) frame by frame, then checks the
//! oracle-side user-flow invariant. Clean scenarios fail the run; documented
//! mismatches are allowed only inside their measured divergence budget and are
//! reported with their first divergence and evidence files.
//!
//! ```text
//! gradius-scenarios --rom ROM --pack assets/gradius.pak --out DIR [--only NAME]
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use gradius_diff::scenario::{run as run_scenario, Expected, ScenarioResult, SCENARIOS};

fn usage() -> &'static str {
    "\
usage: gradius-scenarios --rom PATH [--pack PATH] [--out DIR] [--only NAME]

  --rom PATH    original cartridge (iNES); GRADIUS_ROM is used when omitted
  --pack PATH   native asset pack (default assets/gradius.pak)
  --out DIR     evidence directory (default /tmp/opencode/gradius-scenarios)
  --only NAME   run one scenario from the matrix
  --list        print the matrix and exit
"
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("gradius-scenarios: {message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let mut rom: Option<PathBuf> = None;
    let mut pack = PathBuf::from("assets/gradius.pak");
    let mut out = PathBuf::from("/tmp/opencode/gradius-scenarios");
    let mut only: Option<String> = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let arg = arg
            .into_string()
            .map_err(|_| "argument is not valid UTF-8")?;
        match arg.as_str() {
            "--rom" => rom = Some(PathBuf::from(next(&mut args, "--rom")?)),
            "--pack" => pack = PathBuf::from(next(&mut args, "--pack")?),
            "--out" => out = PathBuf::from(next(&mut args, "--out")?),
            "--only" => only = Some(next(&mut args, "--only")?),
            "--list" => {
                for scenario in SCENARIOS {
                    println!(
                        "{:<24} {:>5} frames  {}  {}",
                        scenario.name,
                        scenario.frames,
                        scenario.expected.label(),
                        scenario.script
                    );
                }
                return Ok(ExitCode::SUCCESS);
            }
            "-h" | "--help" => {
                print!("{}", usage());
                return Ok(ExitCode::SUCCESS);
            }
            other => return Err(format!("unknown argument `{other}`\n\n{}", usage())),
        }
    }
    let rom = rom
        .or_else(|| std::env::var_os("GRADIUS_ROM").map(PathBuf::from))
        .ok_or("missing --rom (or GRADIUS_ROM)")?;
    if !pack.is_file() {
        return Err(format!("asset pack not found: {}", pack.display()));
    }
    std::fs::create_dir_all(&out)
        .map_err(|error| format!("cannot create {}: {error}", out.display()))?;

    let selected: Vec<_> = SCENARIOS
        .iter()
        .filter(|scenario| only.as_deref().map_or(true, |name| name == scenario.name))
        .collect();
    if selected.is_empty() {
        return Err(format!(
            "no scenario matches {:?}; try --list",
            only.unwrap_or_default()
        ));
    }

    println!(
        "{:<24} {:<6} {:<6} {:<10} {:<8} first divergence",
        "scenario", "expect", "result", "pixels", "checks"
    );
    let mut failures = 0;
    let mut clean_pass = 0;
    let mut known = 0;
    let mut summary = String::from("[\n");
    for (index, scenario) in selected.iter().enumerate() {
        let dir = out.join(scenario.name);
        let result = run_scenario(&rom, &pack, scenario, Some(&dir))?;
        write_evidence(&dir, &result)?;
        let check_passed = result.checks.iter().filter(|check| check.pass).count();
        let first = result.first_divergence();
        let pixels = result
            .pixels
            .match_percent()
            .map_or("-".to_string(), |rate| format!("{rate:.1}%"));
        let verdict = if result.passed { "PASS" } else { "FAIL" };
        println!(
            "{:<24} {:<6} {:<6} {:<10} {:<8} {}",
            scenario.name,
            scenario.expected.label(),
            verdict,
            pixels,
            format!("{check_passed}/{}", result.checks.len()),
            first
        );
        if scenario.expected == Expected::Clean {
            if result.passed {
                clean_pass += 1;
            } else {
                failures += 1;
            }
        } else {
            known += 1;
            // A known mismatch is allowed to diverge only inside its measured
            // budget, and its oracle checks must still prove the scenario
            // exercised the user flow.
            if !result.passed {
                failures += 1;
            }
        }
        if !result.check_passed() {
            for check in result
                .checks
                .iter()
                .filter(|check| check.hard && !check.pass)
            {
                println!("  check failed: {}: {}", check.name, check.detail);
            }
        }
        for note in &result.notes {
            println!("  note: {note}");
        }
        if index > 0 {
            summary.push_str(",\n");
        }
        summary.push_str(&scenario_json(scenario.name, &result));
    }
    summary.push_str("\n]\n");
    std::fs::write(out.join("summary.json"), summary)
        .map_err(|error| format!("cannot write summary: {error}"))?;

    println!(
        "\nscenarios: {} run, {} clean pass, {} known, {} failed; evidence in {}",
        selected.len(),
        clean_pass,
        known,
        failures,
        out.display()
    );
    Ok(if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn next(args: &mut impl Iterator<Item = std::ffi::OsString>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} needs a value"))?
        .into_string()
        .map_err(|_| format!("{flag} value is not valid UTF-8"))
}

fn write_evidence(dir: &std::path::Path, result: &ScenarioResult) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    std::fs::write(dir.join("state.json"), result.report.to_json())
        .map_err(|error| format!("cannot write state.json: {error}"))?;
    std::fs::write(
        dir.join("pixels.json"),
        format!("{}\n", pixel_object(&result.pixels)),
    )
    .map_err(|error| format!("cannot write pixels.json: {error}"))?;
    let mut checks = String::new();
    for check in &result.checks {
        checks.push_str(&format!(
            "{} {}: {}\n",
            if check.pass { "PASS" } else { "FAIL" },
            check.name,
            check.detail
        ));
    }
    for note in &result.notes {
        checks.push_str(&format!("note: {note}\n"));
    }
    std::fs::write(dir.join("checks.txt"), checks)
        .map_err(|error| format!("cannot write checks.txt: {error}"))?;
    std::fs::write(dir.join("oracle.txt"), lines(&result.oracle_lines))
        .map_err(|error| format!("cannot write oracle.txt: {error}"))?;
    std::fs::write(dir.join("native.txt"), lines(&result.native_lines))
        .map_err(|error| format!("cannot write native.txt: {error}"))?;
    Ok(())
}

fn lines(snapshots: &[gradius_diff::Snapshot]) -> String {
    let mut out = String::new();
    for snapshot in snapshots {
        out.push_str(&snapshot.to_line());
        out.push('\n');
    }
    out
}

fn scenario_json(name: &str, result: &ScenarioResult) -> String {
    let mut out = String::from("  {\n");
    out.push_str(&format!("    \"name\": \"{name}\",\n"));
    out.push_str(&format!(
        "    \"expected\": \"{}\",\n",
        result.expected.label()
    ));
    out.push_str(&format!("    \"passed\": {},\n", result.passed));
    out.push_str(&format!(
        "    \"first_divergence\": {},\n",
        match result.report.first() {
            Some(divergence) => format!(
                "{{ \"frame\": {}, \"field\": \"{}\", \"oracle\": \"{}\", \"native\": \"{}\" }}",
                divergence.frame, divergence.field, divergence.oracle, divergence.native
            ),
            None => "null".to_string(),
        }
    ));
    out.push_str(&format!(
        "    \"checks_passed\": {},\n",
        result.checks.iter().filter(|check| check.pass).count()
    ));
    out.push_str(&format!("    \"checks\": {},\n", result.checks.len()));
    out.push_str(&format!(
        "    \"pixels\": {},\n",
        result
            .pixels
            .match_percent()
            .map_or("null".to_string(), |rate| format!("{rate:.3}"))
    ));
    out.push_str("    \"state\": ");
    out.push_str(&indent(&result.report.to_json(), 4));
    out.push_str(",\n    \"pixel_report\": ");
    out.push_str(&indent(&pixel_object(&result.pixels), 4));
    out.push_str("\n  }");
    out
}

/// `PixelSummary::to_json` is a `"pixels": { ... }` member for `merge_json`;
/// the scenario evidence stores the object itself.
fn pixel_object(pixels: &gradius_diff::PixelSummary) -> String {
    pixels
        .to_json()
        .trim_start()
        .strip_prefix("\"pixels\": ")
        .unwrap_or("null")
        .to_string()
}

fn indent(text: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    let mut out = String::new();
    for (index, line) in text.trim_end().lines().enumerate() {
        if index > 0 {
            out.push('\n');
            out.push_str(&pad);
        }
        out.push_str(line);
    }
    out
}
