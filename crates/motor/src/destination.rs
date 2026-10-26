use std::fs::Metadata;
use std::path::{Component, PathBuf};

use chrono::{DateTime, Datelike, Local, NaiveDate};

use crate::{Captures, DateSource, MotorError, Rule};

const MONTH_NAMES_PT: [&str; 12] = [
    "Janeiro",
    "Fevereiro",
    "Março",
    "Abril",
    "Maio",
    "Junho",
    "Julho",
    "Agosto",
    "Setembro",
    "Outubro",
    "Novembro",
    "Dezembro",
];

pub fn build_destination(
    rule: &Rule,
    captures: &Captures,
    file_meta: &Metadata,
) -> Result<PathBuf, MotorError> {
    let date = date_for_rule(rule, captures, file_meta)?;
    let mut rendered = String::new();
    let mut position = 0;

    while position < rule.destination.len() {
        let remaining = &rule.destination[position..];
        if remaining.starts_with('{') {
            let contents_start = position + 1;
            let closing_offset = rule.destination[contents_start..]
                .find('}')
                .ok_or(MotorError::UnclosedDestinationMarker { position })?;
            let closing = contents_start + closing_offset;
            let marker = &rule.destination[contents_start..closing];
            let value = marker_value(marker, captures, date)?;
            rendered.push_str(&sanitize_expanded_value(&value));
            position = closing + 1;
            continue;
        }

        let character = remaining
            .chars()
            .next()
            .ok_or(MotorError::UnclosedDestinationMarker { position })?;
        rendered.push(character);
        position += character.len_utf8();
    }

    let mut destination = trim_path_segments(PathBuf::from(rendered));
    let name = sanitize_expanded_value(&captures.name)
        .trim_end_matches([' ', '.'])
        .to_owned();
    let ext = sanitize_expanded_value(&captures.ext)
        .trim_end_matches([' ', '.'])
        .to_owned();
    let file_name = if ext.is_empty() {
        name
    } else {
        format!("{name}.{ext}")
    };
    destination.push(file_name);
    Ok(destination)
}

fn date_for_rule(
    rule: &Rule,
    captures: &Captures,
    file_meta: &Metadata,
) -> Result<NaiveDate, MotorError> {
    match rule.date_source {
        DateSource::FileName => date_from_captures(captures),
        DateSource::Modified => {
            let modified = file_meta.modified().map_err(MotorError::ModifiedTime)?;
            Ok(DateTime::<Local>::from(modified).date_naive())
        }
        DateSource::Detected => Ok(Local::now().date_naive()),
    }
}

fn date_from_captures(captures: &Captures) -> Result<NaiveDate, MotorError> {
    let year = captures
        .year
        .as_deref()
        .ok_or(MotorError::MissingFileNameDate)?;
    let month = captures
        .month
        .as_deref()
        .ok_or(MotorError::MissingFileNameDate)?;
    let day = captures
        .day
        .as_deref()
        .ok_or(MotorError::MissingFileNameDate)?;

    let mut year: i32 = year.parse().map_err(|_| MotorError::InvalidCapturedDate)?;
    if year < 100 {
        year += 2000;
    }
    let month = parse_month(month).ok_or(MotorError::InvalidCapturedDate)?;
    let day = day
        .parse::<u32>()
        .map_err(|_| MotorError::InvalidCapturedDate)?;

    NaiveDate::from_ymd_opt(year, month, day).ok_or(MotorError::InvalidCapturedDate)
}

fn parse_month(value: &str) -> Option<u32> {
    if let Ok(month) = value.parse::<u32>() {
        return Some(month);
    }

    let value = value.to_lowercase();
    MONTH_NAMES_PT
        .iter()
        .position(|month| month.to_lowercase() == value)
        .map(|index| index as u32 + 1)
}

fn marker_value(marker: &str, captures: &Captures, date: NaiveDate) -> Result<String, MotorError> {
    let value = match marker {
        "YYYY" => format!("{:04}", date.year()),
        "YY" => format!("{:02}", date.year().rem_euclid(100)),
        "MM" => format!("{:02}", date.month()),
        "DD" => format!("{:02}", date.day()),
        "MES" => MONTH_NAMES_PT[date.month0() as usize].to_owned(),
        "NOME" => captures.name.clone(),
        "EXT" => captures.ext.clone(),
        custom => {
            captures
                .custom
                .get(custom)
                .cloned()
                .ok_or_else(|| MotorError::UnknownMarker {
                    marker: format!("{{{custom}}}"),
                })?
        }
    };
    Ok(value)
}

fn sanitize_expanded_value(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
                '_'
            } else {
                character
            }
        })
        .collect()
}

fn trim_path_segments(path: PathBuf) -> PathBuf {
    let mut trimmed = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let value = value.to_string_lossy();
                trimmed.push(value.trim_end_matches([' ', '.']));
            }
            _ => trimmed.push(component.as_os_str()),
        }
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use tempfile::tempdir;
    use uuid::Uuid;

    use super::*;
    use crate::{Action, CheckMode, ConflictPolicy};

    fn rule(destination: String) -> Rule {
        Rule {
            id: Uuid::new_v4(),
            name: "Teste".to_owned(),
            enabled: true,
            source_dir: PathBuf::from("/entrada"),
            filter: "*".to_owned(),
            destination,
            action: Action::Move,
            date_source: DateSource::FileName,
            check_mode: CheckMode::Instant,
            include_subfolders: false,
            on_conflict: ConflictPolicy::Rename,
            priority: 0,
        }
    }

    fn metadata() -> (tempfile::TempDir, Metadata) {
        let directory = tempdir().expect("temp directory should be created");
        let path = directory.path().join("arquivo.txt");
        fs::write(&path, b"content").expect("fixture should be written");
        let metadata = fs::metadata(path).expect("metadata should be available");
        (directory, metadata)
    }

    fn captures() -> Captures {
        Captures {
            year: Some("2026".to_owned()),
            month: Some("10".to_owned()),
            day: Some("22".to_owned()),
            custom: HashMap::new(),
            name: "20261022".to_owned(),
            ext: "pdf".to_owned(),
        }
    }

    #[test]
    fn expands_date_name_extension_and_month_name() {
        let (_directory, metadata) = metadata();
        let rule = rule("/arquivo/{YYYY}/{YY}/{MM}/{DD}/{MES}/{NOME}.{EXT}".to_owned());

        let destination =
            build_destination(&rule, &captures(), &metadata).expect("destination should build");

        assert_eq!(
            destination,
            PathBuf::from("/arquivo/2026/26/10/22/Outubro/20261022.pdf/20261022.pdf")
        );
    }

    #[test]
    fn sanitizes_expanded_values_and_trims_segments() {
        let (_directory, metadata) = metadata();
        let mut captures = captures();
        captures.name = "relatório:final?. ".to_owned();
        captures
            .custom
            .insert("cliente".to_owned(), "Acme:* . ".to_owned());
        let rule = rule("/arquivo/{cliente}/{NOME}".to_owned());

        let destination =
            build_destination(&rule, &captures, &metadata).expect("destination should build");

        assert_eq!(
            destination,
            PathBuf::from("/arquivo/Acme__/relatório_final_/relatório_final_.pdf")
        );
    }

    #[test]
    fn rejects_missing_file_name_date() {
        let (_directory, metadata) = metadata();
        let mut captures = captures();
        captures.year = None;
        captures.month = None;
        captures.day = None;

        let error = build_destination(&rule("/arquivo/{YYYY}".to_owned()), &captures, &metadata)
            .expect_err("missing date should fail");

        assert!(matches!(error, MotorError::MissingFileNameDate));
        assert!(error.to_string().contains("não capturou uma data"));
    }

    #[test]
    fn rejects_unknown_destination_marker() {
        let (_directory, metadata) = metadata();
        let error = build_destination(
            &rule("/arquivo/{DESCONHECIDO}".to_owned()),
            &captures(),
            &metadata,
        )
        .expect_err("unknown marker should fail");

        assert!(matches!(error, MotorError::UnknownMarker { .. }));
    }
}
