use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;

use crate::util::read_to_string;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReleasePrepVersion {
    tag: String,
    package_version: String,
    policy_key: String,
}

impl ReleasePrepVersion {
    fn parse(input: &str) -> Result<Self, String> {
        let package_version = input
            .strip_prefix('v')
            .ok_or_else(|| "release-prep version must start with `v`".to_owned())?;
        let (numeric, alpha) = package_version
            .split_once("-alpha.")
            .ok_or_else(|| "release-prep version must match vX.Y.Z-alpha.N".to_owned())?;
        let mut parts = numeric.split('.');
        let major = parse_version_part(parts.next(), "major")?;
        let minor = parse_version_part(parts.next(), "minor")?;
        let patch = parse_version_part(parts.next(), "patch")?;
        if parts.next().is_some() {
            return Err("release-prep version must match vX.Y.Z-alpha.N".into());
        }
        let alpha = parse_decimal(alpha, "alpha")?;
        let policy_key = format!("v{major}_{minor}_{patch}_alpha_{alpha}");
        Ok(Self {
            tag: input.to_owned(),
            package_version: package_version.to_owned(),
            policy_key,
        })
    }

    fn policy_section(&self) -> String {
        format!("[release_notes.{}]", self.policy_key)
    }
}

fn parse_version_part(part: Option<&str>, name: &str) -> Result<u16, String> {
    let part = part.ok_or_else(|| format!("release-prep version missing {name} version part"))?;
    parse_decimal(part, name)
}

fn parse_decimal(part: &str, name: &str) -> Result<u16, String> {
    if part.is_empty() || !part.chars().all(|ch| ch.is_ascii_digit()) {
        return Err(format!("release-prep {name} version part must be numeric"));
    }
    part.parse::<u16>()
        .map_err(|err| format!("parse release-prep {name} version part `{part}`: {err}"))
}

pub(crate) fn release_prep(
    root: &Path,
    input: &str,
    date: Option<&str>,
    now: SystemTime,
) -> Result<(), String> {
    let version = ReleasePrepVersion::parse(input)?;
    let target_date = scaffold_release_date(date, now).map_err(|kind| match date {
        Some(value) => format!("{kind:?}: `{value}`"),
        None => format!("{kind:?}"),
    })?;
    let cli_manifest_path = root.join("crates/amblicit-cli/Cargo.toml");
    let facade_manifest_path = root.join("crates/amblicit/Cargo.toml");
    let syntax_manifest_path = root.join("crates/amblicit-syntax/Cargo.toml");
    let lockfile_path = root.join("Cargo.lock");
    let changelog_path = root.join("CHANGELOG.md");
    let policy_path = root.join("docs/topics/release-process/policy.toml");
    let test_plan_path = root.join("docs/topics/release-process/test-plan.md");
    let release_notes_path = release_notes_path(root, &version);

    if release_notes_path.exists() {
        return Err(format!(
            "release notes already exist: {}",
            release_notes_path.display()
        ));
    }

    let policy = read_to_string(&policy_path)?;
    let cli_manifest = replace_first_version_line(
        &read_to_string(&cli_manifest_path)?,
        &version.package_version,
    )?;
    let syntax_manifest = replace_first_version_line(
        &read_to_string(&syntax_manifest_path)?,
        &version.package_version,
    )?;
    let facade_manifest = replace_facade_manifest_version(
        &read_to_string(&facade_manifest_path)?,
        &version.package_version,
    )?;
    let lockfile =
        replace_lock_package_versions(&read_to_string(&lockfile_path)?, &version.package_version)?;
    let changelog = insert_release_changelog_section(
        &read_to_string(&changelog_path)?,
        &version.tag,
        &target_date,
    )?;
    let policy = append_release_policy_block(&policy, &version, &target_date)?;
    let release_notes = render_release_notes_stub(&version, &target_date);
    let test_plan = insert_release_test_plan_rows(&read_to_string(&test_plan_path)?, &version)?;

    write_file(&cli_manifest_path, &cli_manifest)?;
    write_file(&syntax_manifest_path, &syntax_manifest)?;
    write_file(&facade_manifest_path, &facade_manifest)?;
    write_file(&lockfile_path, &lockfile)?;
    write_file(&changelog_path, &changelog)?;
    write_file(&policy_path, &policy)?;
    write_file(&release_notes_path, &release_notes)?;
    write_file(&test_plan_path, &test_plan)?;

    println!(
        "release-prep: scaffolded {} with target date {}",
        version.tag, target_date
    );
    Ok(())
}

fn write_file(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("create {}: {err}", parent.display()))?;
    }
    fs::write(path, text).map_err(|err| format!("write {}: {err}", path.display()))
}

fn replace_first_version_line(text: &str, package_version: &str) -> Result<String, String> {
    let mut replaced = false;
    let mut lines = Vec::new();
    for line in text.lines() {
        if !replaced && line.trim_start().starts_with("version = ") {
            lines.push(format!("version = \"{package_version}\""));
            replaced = true;
        } else {
            lines.push(line.to_owned());
        }
    }
    if !replaced {
        return Err("package manifest missing version field".into());
    }
    Ok(join_lines_preserving_final_newline(&lines, text))
}

fn replace_facade_manifest_version(text: &str, package_version: &str) -> Result<String, String> {
    let text = replace_first_version_line(text, package_version)?;
    // The facade declares its exact implementation requirement in an inline
    // dependency table. Reject an absent or ambiguous requirement before writes.
    let pattern = Regex::new(
        r#"(?m)^([ \t]*amblicit-syntax[ \t]*=[ \t]*\{[^\r\n}]*\bversion[ \t]*=[ \t]*)"[^"]*""#,
    )
    .map_err(|error| format!("facade dependency pattern: {error}"))?;
    if pattern.captures_iter(&text).count() != 1 {
        return Err("facade manifest must declare one inline amblicit-syntax version".into());
    }
    Ok(pattern
        .replace(&text, |captures: &regex::Captures<'_>| {
            format!("{}\"={package_version}\"", &captures[1])
        })
        .into_owned())
}

fn replace_lock_package_versions(text: &str, package_version: &str) -> Result<String, String> {
    let mut current_package = None;
    let mut replaced = BTreeSet::new();
    let mut lines = Vec::new();
    for line in text.lines() {
        if line == "[[package]]" {
            current_package = None;
            lines.push(line.to_owned());
            continue;
        }
        if let Some(name) = line.strip_prefix("name = ") {
            current_package = parse_quoted_string(name);
            lines.push(line.to_owned());
            continue;
        }
        if line.starts_with("version = ")
            && matches!(
                current_package.as_deref(),
                Some("amblicit-cli" | "amblicit-syntax" | "flyingrobots-amblicit")
            )
        {
            if let Some(package) = current_package.as_deref() {
                replaced.insert(package.to_owned());
            }
            lines.push(format!("version = \"{package_version}\""));
            continue;
        }
        lines.push(line.to_owned());
    }
    for package in ["amblicit-cli", "amblicit-syntax", "flyingrobots-amblicit"] {
        if !replaced.contains(package) {
            return Err(format!("Cargo.lock missing package version for {package}"));
        }
    }
    Ok(join_lines_preserving_final_newline(&lines, text))
}

fn parse_quoted_string(text: &str) -> Option<String> {
    let trimmed = text.trim();
    trimmed
        .strip_prefix('"')?
        .strip_suffix('"')
        .map(ToOwned::to_owned)
}

fn join_lines_preserving_final_newline(lines: &[String], original: &str) -> String {
    let mut text = lines.join("\n");
    if original.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn insert_release_changelog_section(
    changelog: &str,
    tag: &str,
    target_date: &str,
) -> Result<String, String> {
    if changelog.contains(&format!("## [{tag}]")) {
        return Err(format!(
            "CHANGELOG.md already contains release section for {tag}"
        ));
    }
    replace_once(
        changelog,
        "## [Unreleased]\n\n",
        &format!("## [Unreleased]\n\n## [{tag}] - {target_date}\n\n"),
        "CHANGELOG.md Unreleased section",
    )
}

fn append_release_policy_block(
    policy: &str,
    version: &ReleasePrepVersion,
    target_date: &str,
) -> Result<String, String> {
    if policy.contains(&version.policy_section()) {
        return Err(format!(
            "release policy already contains {}",
            version.policy_section()
        ));
    }
    let mut output = policy.trim_end().to_owned();
    output.push_str("\n\n");
    output.push_str(&version.policy_section());
    output.push('\n');
    output.push_str("tag = \"");
    output.push_str(&version.tag);
    output.push_str("\"\n");
    output.push_str("target_date = \"");
    output.push_str(target_date);
    output.push_str("\"\n");
    output.push_str(
        "\
status = \"prep\"
release_issue = 0
scope = [
  \"TODO_release_scope\",
]
non_goals = [
  \"TODO_release_non_goal\",
]
",
    );
    Ok(output)
}

fn release_notes_path(root: &Path, version: &ReleasePrepVersion) -> PathBuf {
    root.join("docs")
        .join("releases")
        .join(format!("{}.md", version.tag))
}

fn render_release_notes_stub(version: &ReleasePrepVersion, target_date: &str) -> String {
    format!(
        "\
# {tag}

Target date: {target_date}

## Release Thesis

- TODO: write the release thesis before editing release artifacts.

## Released

- TODO: list what shipped.

## Not Released

- TODO: list explicit non-goals.

## Evidence

- TODO: record local verification, CI, tag, release, and milestone evidence.

## Plan Versus Actual

- TODO: reconcile the plan with what landed.

## Fallout Issues

- TODO: list follow-up issues or state none.

## Next Release Thesis

- TODO: capture the next release thesis.
",
        tag = version.tag,
    )
}

fn insert_release_test_plan_rows(
    test_plan: &str,
    version: &ReleasePrepVersion,
) -> Result<String, String> {
    if test_plan.contains(&version.tag) {
        return Err(format!(
            "release-process test plan already contains {}",
            version.tag
        ));
    }
    let req_id = next_numbered_id(test_plan, "RELEASE-REQ-")?;
    let with_requirement = insert_before_once(
        test_plan,
        "\n## Fixtures\n",
        &format!(
            "| {req_id} | planned | Structured release policy captures the `{tag}` release boundary; replace scaffold placeholders before release. | docs/topics/release-process/policy.toml |\n",
            tag = version.tag,
        ),
        "release-process requirements table",
    )?;
    let with_fixture = insert_before_once(
        &with_requirement,
        "\n## Test Cases\n",
        &format!(
            "| docs/releases/{tag}.md | Release-prep notes stub for `{tag}`. | The release workflow will look up this file by full tag name after the release-prep PR merges. |\n",
            tag = version.tag,
        ),
        "release-process fixtures table",
    )?;
    let tp_id = next_numbered_id(&with_fixture, "RELEASE-TP-")?;
    insert_before_once(
        &with_fixture,
        "\n## Determinism Obligations\n",
        &format!(
            "| {tp_id} | planned | Boundary guard | {req_id} | Review confirms structured policy captures the `{tag}` scope and non-goal boundary after scaffold placeholders are replaced. | - | docs/topics/release-process/policy.toml, docs/releases/{tag}.md | Scaffolded by `cargo xtask release-prep {tag}`; reviewers must replace TODO scope/non-goal values before release. Block structure is checked by `release_policy_blocks_are_structurally_complete`; dates are reconciled against git tags by `cargo xtask release-dates`. |\n",
            tag = version.tag,
        ),
        "release-process test case table",
    )
}

fn next_numbered_id(text: &str, prefix: &str) -> Result<String, String> {
    let mut max_id = None;
    let mut width = 3usize;
    for (index, _) in text.match_indices(prefix) {
        let rest = &text[index + prefix.len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            continue;
        }
        width = width.max(digits.len());
        let value = digits
            .parse::<u16>()
            .map_err(|err| format!("parse numbered id `{prefix}{digits}`: {err}"))?;
        max_id = Some(max_id.map_or(value, |current: u16| current.max(value)));
    }
    let next = max_id.ok_or_else(|| format!("no existing IDs with prefix {prefix}"))? + 1;
    Ok(format!("{prefix}{next:0width$}"))
}

fn insert_before_once(
    text: &str,
    needle: &str,
    insertion: &str,
    description: &str,
) -> Result<String, String> {
    let index = text
        .find(needle)
        .ok_or_else(|| format!("{description} missing insertion marker `{needle}`"))?;
    let mut output = String::with_capacity(text.len() + insertion.len());
    output.push_str(&text[..index]);
    output.push_str(insertion);
    output.push_str(&text[index..]);
    Ok(output)
}

fn replace_once(
    text: &str,
    needle: &str,
    replacement: &str,
    description: &str,
) -> Result<String, String> {
    if !text.contains(needle) {
        return Err(format!("{description} missing marker `{needle}`"));
    }
    Ok(text.replacen(needle, replacement, 1))
}

/// The date a release-prep scaffold records.
///
/// This used to be the last `target_date` plus fourteen days, from an era when
/// the field held a planned date on a biweekly cadence. Now that `target_date`
/// records the date a release was actually tagged, extrapolating from history
/// produces a date in the past: seeded from the realigned record, the next value
/// would be 2026-07-14. An explicit `--date` wins; otherwise today's UTC date is
/// used, so the scaffold is at least current when it is written.
fn scaffold_release_date(
    explicit: Option<&str>,
    now: SystemTime,
) -> Result<String, ReleasePrepDateError> {
    match explicit {
        Some(date) => {
            validate_iso_date(date)?;
            Ok(date.to_owned())
        }
        None => utc_date_at(now),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReleasePrepDateError {
    InvalidIsoDate,
    ClockBeforeEpoch,
    ClockOutOfRange,
}

pub(crate) fn validate_iso_date(date: &str) -> Result<(), ReleasePrepDateError> {
    let bytes = date.as_bytes();
    if bytes.len() != 10
        || !bytes.iter().enumerate().all(|(index, byte)| {
            if matches!(index, 4 | 7) {
                *byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
    {
        return Err(ReleasePrepDateError::InvalidIsoDate);
    }
    // The byte grammar above proves these slices are ASCII and in bounds.
    let year = parse_date_part(&date[..4])?;
    let month = parse_date_part(&date[5..7])?;
    let day = parse_date_part(&date[8..])?;
    if day == 0 || day > days_in_month(year, month)? {
        return Err(ReleasePrepDateError::InvalidIsoDate);
    }
    Ok(())
}

fn utc_date_at(now: SystemTime) -> Result<String, ReleasePrepDateError> {
    let seconds = now
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ReleasePrepDateError::ClockBeforeEpoch)?
        .as_secs();
    let days =
        i64::try_from(seconds / 86_400).map_err(|_| ReleasePrepDateError::ClockOutOfRange)?;
    let date = iso_date_from_days_since_epoch(days);
    validate_iso_date(&date).map_err(|_| ReleasePrepDateError::ClockOutOfRange)?;
    Ok(date)
}

/// Civil date from a day count since 1970-01-01, using Howard Hinnant's
/// `civil_from_days`. Avoids taking a date-library dependency for one call.
fn iso_date_from_days_since_epoch(days: i64) -> String {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_position + 2) / 5 + 1;
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

fn parse_date_part(part: &str) -> Result<u16, ReleasePrepDateError> {
    part.parse::<u16>()
        .map_err(|_| ReleasePrepDateError::InvalidIsoDate)
}

fn days_in_month(year: u16, month: u16) -> Result<u16, ReleasePrepDateError> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Ok(31),
        4 | 6 | 9 | 11 => Ok(30),
        2 if is_leap_year(year) => Ok(29),
        2 => Ok(28),
        _ => Err(ReleasePrepDateError::InvalidIsoDate),
    }
}

fn is_leap_year(year: u16) -> bool {
    (is_divisible_by(year, 4) && !is_divisible_by(year, 100)) || is_divisible_by(year, 400)
}

fn is_divisible_by(value: u16, divisor: u16) -> bool {
    value.rem_euclid(divisor) == 0
}

#[cfg(test)]
mod date_tests {
    use super::{scaffold_release_date, validate_iso_date, ReleasePrepDateError};
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn release_prep_default_date_uses_supplied_utc_clock() {
        for (seconds, expected) in [
            (0, "1970-01-01"),
            (86_399, "1970-01-01"),
            (86_400, "1970-01-02"),
            (951_782_400, "2000-02-29"),
            (253_402_300_799, "9999-12-31"),
        ] {
            assert_eq!(
                scaffold_release_date(None, UNIX_EPOCH + Duration::from_secs(seconds)),
                Ok(expected.to_owned())
            );
        }
    }

    #[test]
    fn release_prep_clock_failures_are_structured() {
        assert_eq!(
            scaffold_release_date(None, UNIX_EPOCH - Duration::from_secs(1)),
            Err(ReleasePrepDateError::ClockBeforeEpoch)
        );
        assert_eq!(
            scaffold_release_date(None, UNIX_EPOCH + Duration::from_hours(70_389_528)),
            Err(ReleasePrepDateError::ClockOutOfRange)
        );
    }

    #[test]
    fn release_prep_explicit_date_overrides_clock() {
        assert_eq!(
            scaffold_release_date(Some("2026-08-04"), UNIX_EPOCH - Duration::from_secs(1)),
            Ok("2026-08-04".to_owned())
        );
    }

    #[test]
    fn release_prep_rejects_noncanonical_iso_date() {
        assert_eq!(
            validate_iso_date("2026-1-001"),
            Err(ReleasePrepDateError::InvalidIsoDate)
        );
    }
    #[test]
    fn release_prep_date_validation_has_one_stable_failure() {
        for date in ["1970-01-01", "2000-02-29", "2026-09-07", "9999-12-31"] {
            assert_eq!(validate_iso_date(date), Ok(()), "{date}");
        }
        for date in [
            "2026-1-001",
            "2026-001-1",
            "2026-1-1",
            "2026-01-1 ",
            "2026-01-01 ",
            " 2026-01-01",
            "+026-01-01",
            "２０２６-01-01",
            "2026-00-01",
            "2026-13-01",
            "2026-01-00",
            "2026-04-31",
            "1900-02-29",
            "2026-02-29",
            "2026-AA-01",
            "",
            "2026/01/01",
        ] {
            assert_eq!(
                validate_iso_date(date),
                Err(ReleasePrepDateError::InvalidIsoDate),
                "{date}"
            );
        }
    }
}
