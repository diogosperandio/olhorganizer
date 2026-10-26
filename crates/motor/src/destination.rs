//! Destination builder: expands the markers in a rule's destination template.

use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Datelike, Local};
use thiserror::Error;

use crate::error::MotorError;
use crate::filter::Captures;
use crate::markers::{self, MarkerKind};
use crate::rule::{DateSource, Rule};

/// File facts the destination may depend on, besides the name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileMeta {
    pub modified: Option<SystemTime>,
    /// When the file was detected; used by [`DateSource::Detected`].
    pub detected: SystemTime,
}

/// Problems building a destination. Display messages are shown to the user.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DestinationError {
    #[error(
        "A regra usa a data do nome do arquivo, mas o filtro não tem data. Use {{YYYY}}, {{MM}} e {{DD}} no filtro ou escolha outra origem para a data."
    )]
    MissingDate,
    #[error("A pasta de destino usa {{{0}}}, mas o filtro não captura essa parte da data.")]
    MissingDatePart(String),
    #[error("Não foi possível ler a data de modificação do arquivo.")]
    NoModifiedTime,
    #[error("O marcador {{{0}}} da pasta de destino não existe.")]
    UnknownMarker(String),
    #[error("A chave aberta em “{0}” na pasta de destino não foi fechada com }}.")]
    UnclosedBrace(String),
    #[error("A pasta de destino tem uma chave }} sem a chave {{ correspondente.")]
    UnmatchedClose,
    #[error("A pasta de destino tem um par de chaves vazio {{}}.")]
    EmptyBraces,
    #[error("O caractere | dentro de chaves é reservado e ainda não pode ser usado.")]
    ReservedPipe,
    #[error(
        "A pasta de destino precisa ser um caminho completo, começando pela unidade ou pela raiz."
    )]
    NotAbsolute,
}

/// Characters Windows does not allow in file names, plus path separators so a
/// value can never add folders.
const INVALID_CHARS: &[char] = &['<', '>', ':', '"', '|', '?', '*', '/', '\\'];

/// Builds the destination folder for a matched file.
pub fn build_destination(
    rule: &Rule,
    captures: &Captures,
    meta: &FileMeta,
) -> Result<PathBuf, MotorError> {
    Ok(build(rule, captures, meta)?)
}

fn build(rule: &Rule, captures: &Captures, meta: &FileMeta) -> Result<PathBuf, DestinationError> {
    let date = file_date(rule.date_source, captures, meta)?;

    let mut out = String::new();
    let mut rest = rule.destination.as_str();
    while let Some(open) = rest.find(['{', '}']) {
        out.push_str(&rest[..open]);
        rest = &rest[open..];
        if rest.starts_with('}') {
            return Err(DestinationError::UnmatchedClose);
        }
        let close = rest
            .find('}')
            .ok_or_else(|| DestinationError::UnclosedBrace(rest.to_owned()))?;
        let token = &rest[1..close];
        if token.contains('{') {
            return Err(DestinationError::UnclosedBrace(rest[..=close].to_owned()));
        }
        out.push_str(&sanitize(&expand(token, &date, captures)?));
        rest = &rest[close + 1..];
    }
    out.push_str(rest);

    let path = clean_segments(Path::new(&out));
    if !path.is_absolute() {
        return Err(DestinationError::NotAbsolute);
    }
    Ok(path)
}

/// Date parts available for expansion; `None` when the filter did not capture them.
struct FileDate {
    year: Option<i32>,
    month: Option<u32>,
    day: Option<u32>,
}

fn file_date(
    source: DateSource,
    captures: &Captures,
    meta: &FileMeta,
) -> Result<FileDate, DestinationError> {
    match source {
        DateSource::FileName => {
            if captures.year.is_none() && captures.month.is_none() && captures.day.is_none() {
                return Err(DestinationError::MissingDate);
            }
            Ok(FileDate {
                year: captures.year,
                month: captures.month,
                day: captures.day,
            })
        }
        DateSource::Modified => meta
            .modified
            .map(local_date)
            .ok_or(DestinationError::NoModifiedTime),
        DateSource::Detected => Ok(local_date(meta.detected)),
    }
}

fn local_date(time: SystemTime) -> FileDate {
    let date = DateTime::<Local>::from(time).date_naive();
    FileDate {
        year: Some(date.year()),
        month: Some(date.month()),
        day: Some(date.day()),
    }
}

fn expand(token: &str, date: &FileDate, captures: &Captures) -> Result<String, DestinationError> {
    if token.contains('|') {
        return Err(DestinationError::ReservedPipe);
    }
    if token.is_empty() {
        return Err(DestinationError::EmptyBraces);
    }
    let missing = || DestinationError::MissingDatePart(token.to_owned());

    let Some(def) = markers::find_marker(token) else {
        return captures
            .custom
            .get(token)
            .or_else(|| {
                captures
                    .custom
                    .iter()
                    .find(|(name, _)| name.to_lowercase() == token.to_lowercase())
                    .map(|(_, value)| value)
            })
            .cloned()
            .ok_or_else(|| DestinationError::UnknownMarker(token.to_owned()));
    };

    Ok(match def.kind {
        MarkerKind::Year4 => format!("{:04}", date.year.ok_or_else(missing)?),
        MarkerKind::Year2 => format!("{:02}", date.year.ok_or_else(missing)?.rem_euclid(100)),
        MarkerKind::Month2 => format!("{:02}", date.month.ok_or_else(missing)?),
        MarkerKind::MonthName => {
            let month = date.month.ok_or_else(missing)?;
            let name = usize::try_from(month)
                .ok()
                .and_then(|m| markers::MONTH_NAMES_PT.get(m.wrapping_sub(1)))
                .ok_or_else(missing)?;
            capitalize(name)
        }
        MarkerKind::Day2 => format!("{:02}", date.day.ok_or_else(missing)?),
        MarkerKind::Name => captures.name.clone(),
        MarkerKind::Ext => captures.ext.clone(),
    })
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if INVALID_CHARS.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// Trims trailing dots and spaces from every folder name (Windows drops them
/// silently); a name left empty becomes `_`.
fn clean_segments(path: &Path) -> PathBuf {
    path.components()
        .map(|component| match component {
            Component::Normal(name) => {
                let name = name.to_string_lossy();
                let trimmed = name.trim_end_matches(['.', ' ']);
                if trimmed.is_empty() {
                    "_".into()
                } else {
                    trimmed.into()
                }
            }
            other => other.as_os_str().to_owned(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use super::*;
    use crate::rule::{Action, CheckMode, ConflictPolicy};

    fn abs(rel: &str) -> String {
        if cfg!(windows) {
            format!("C:\\{}", rel.replace('/', "\\"))
        } else {
            format!("/{rel}")
        }
    }

    fn rule(destination: &str, date_source: DateSource) -> Rule {
        Rule {
            id: uuid::Uuid::new_v4(),
            name: "Teste".into(),
            enabled: true,
            source_dir: PathBuf::from(abs("origem")),
            filter: "*".into(),
            destination: destination.into(),
            action: Action::Move,
            date_source,
            check_mode: CheckMode::Instant,
            on_conflict: ConflictPolicy::Rename,
            priority: 0,
        }
    }

    fn captures(year: Option<i32>, month: Option<u32>, day: Option<u32>) -> Captures {
        Captures {
            year,
            month,
            day,
            name: "20261022".into(),
            ext: "pdf".into(),
            custom: BTreeMap::new(),
        }
    }

    fn meta() -> FileMeta {
        FileMeta {
            modified: None,
            detected: SystemTime::now(),
        }
    }

    fn build_err(r: &Rule, c: &Captures, m: &FileMeta) -> DestinationError {
        match build_destination(r, c, m) {
            Err(MotorError::Destination(e)) => e,
            other => panic!("expected a destination error, got {other:?}"),
        }
    }

    #[test]
    fn expands_every_builtin_marker() {
        let r = rule(
            &abs("d/{YYYY}/{YY}/{MM}/{MES}/{DD}/{NOME}.{EXT}"),
            DateSource::FileName,
        );
        let c = captures(Some(2026), Some(3), Some(5));
        assert_eq!(
            build_destination(&r, &c, &meta()).unwrap(),
            PathBuf::from(abs("d/2026/26/03/Março/05/20261022.pdf"))
        );
    }

    #[test]
    fn expands_custom_captures() {
        let r = rule(&abs("Clientes/{cliente}/{YYYY}"), DateSource::FileName);
        let mut c = captures(Some(2026), Some(10), Some(22));
        c.custom.insert("cliente".into(), "Acme".into());
        assert_eq!(
            build_destination(&r, &c, &meta()).unwrap(),
            PathBuf::from(abs("Clientes/Acme/2026"))
        );
    }

    #[test]
    fn replaces_invalid_characters_and_trims_segments() {
        let r = rule(&abs("d/{x}/{NOME}"), DateSource::Detected);
        let mut c = captures(None, None, None);
        c.custom.insert("x".into(), "a<b>c:d\"e|f?g*h/i\\j".into());
        c.name = "relatório. . ".into();
        assert_eq!(
            build_destination(&r, &c, &meta()).unwrap(),
            PathBuf::from(abs("d/a_b_c_d_e_f_g_h_i_j/relatório"))
        );

        c.name = "...".into();
        assert_eq!(
            build_destination(&r, &c, &meta()).unwrap(),
            PathBuf::from(abs("d/a_b_c_d_e_f_g_h_i_j/_"))
        );
    }

    #[test]
    fn date_from_modified_time() {
        let r = rule(&abs("d/{YYYY}"), DateSource::Modified);
        let modified = SystemTime::now() - Duration::from_secs(400 * 24 * 3600);
        let m = FileMeta {
            modified: Some(modified),
            detected: SystemTime::now(),
        };
        let year = DateTime::<Local>::from(modified).year();
        assert_eq!(
            build_destination(&r, &captures(None, None, None), &m).unwrap(),
            PathBuf::from(abs(&format!("d/{year}")))
        );
        assert_eq!(
            build_err(&r, &captures(None, None, None), &meta()),
            DestinationError::NoModifiedTime
        );
    }

    #[test]
    fn date_from_detection_time() {
        let r = rule(&abs("d/{YYYY}"), DateSource::Detected);
        let year = Local::now().year();
        assert_eq!(
            build_destination(&r, &captures(None, None, None), &meta()).unwrap(),
            PathBuf::from(abs(&format!("d/{year}")))
        );
    }

    #[test]
    fn missing_date_with_file_name_source() {
        let r = rule(&abs("d/{YYYY}/{MM}"), DateSource::FileName);
        assert_eq!(
            build_err(&r, &captures(None, None, None), &meta()),
            DestinationError::MissingDate
        );
        assert_eq!(
            build_err(&r, &captures(Some(2026), None, None), &meta()),
            DestinationError::MissingDatePart("MM".into())
        );
    }

    #[test]
    fn template_errors() {
        let c = captures(Some(2026), Some(10), Some(22));
        let cases = [
            ("d/{ANO}", DestinationError::UnknownMarker("ANO".into())),
            ("d/{YYYY", DestinationError::UnclosedBrace("{YYYY".into())),
            ("d/}", DestinationError::UnmatchedClose),
            ("d/{}", DestinationError::EmptyBraces),
            ("d/{YYYY|x}", DestinationError::ReservedPipe),
        ];
        for (template, expected) in cases {
            let r = rule(&abs(template), DateSource::FileName);
            assert_eq!(build_err(&r, &c, &meta()), expected, "{template}");
        }
        let r = rule("{YYYY}/{MM}", DateSource::FileName);
        assert_eq!(build_err(&r, &c, &meta()), DestinationError::NotAbsolute);
    }
}
