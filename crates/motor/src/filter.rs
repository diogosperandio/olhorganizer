use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::NaiveDate;
use regex::{Regex, RegexBuilder};

use crate::{list_markers, MarkerKind, MotorError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterTemplate {
    pub name_pt: &'static str,
    pub filter: &'static str,
}

static FILTER_TEMPLATES: &[FilterTemplate] = &[
    FilterTemplate {
        name_pt: "Qualquer arquivo",
        filter: "*",
    },
    FilterTemplate {
        name_pt: "Só PDF",
        filter: "*.pdf",
    },
    FilterTemplate {
        name_pt: "Imagens",
        filter: "*.{jpg,png}",
    },
    FilterTemplate {
        name_pt: "Data no nome (AAAAMMDD)",
        filter: "{YYYY}{MM}{DD}.*",
    },
    FilterTemplate {
        name_pt: "O nome contém…",
        filter: "*texto*",
    },
];

pub fn list_filter_templates() -> &'static [FilterTemplate] {
    FILTER_TEMPLATES
}

#[derive(Debug)]
pub struct Filter {
    regex: Regex,
    bindings: Vec<CaptureBinding>,
    date_groups: Option<DateGroups>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Captures {
    pub year: Option<String>,
    pub month: Option<String>,
    pub day: Option<String>,
    pub custom: HashMap<String, String>,
    pub name: String,
    pub ext: String,
}

impl Captures {
    pub fn get(&self, marker: &str) -> Option<&str> {
        let marker = marker
            .strip_prefix('{')
            .and_then(|value| value.strip_suffix('}'))
            .unwrap_or(marker);

        match marker {
            "YYYY" | "YY" => self.year.as_deref(),
            "MM" | "MES" => self.month.as_deref(),
            "DD" => self.day.as_deref(),
            "NOME" => Some(&self.name),
            "EXT" => Some(&self.ext),
            custom => self.custom.get(custom).map(String::as_str),
        }
    }
}

impl Filter {
    pub fn matches(&self, file_name: &str) -> Option<Captures> {
        if is_temporary_download(file_name) {
            return None;
        }

        let regex_captures = self.regex.captures(file_name)?;

        if let Some(groups) = &self.date_groups {
            let year = captured_number(&regex_captures, &groups.year)?;
            let month = captured_number(&regex_captures, &groups.month)?;
            let day = captured_number(&regex_captures, &groups.day)?;
            NaiveDate::from_ymd_opt(year, month, day)?;
        }

        let path = Path::new(file_name);
        let name = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(file_name)
            .to_owned();
        let ext = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_owned();
        let mut captures = Captures {
            year: None,
            month: None,
            day: None,
            custom: HashMap::new(),
            name,
            ext,
        };

        for binding in &self.bindings {
            let Some(value) = regex_captures.name(&binding.group) else {
                continue;
            };
            let value = value.as_str().to_owned();
            match &binding.target {
                CaptureTarget::Year => captures.year = Some(value),
                CaptureTarget::Month => captures.month = Some(value),
                CaptureTarget::Day => captures.day = Some(value),
                CaptureTarget::Custom(name) => {
                    captures.custom.insert(name.clone(), value);
                }
            }
        }

        Some(captures)
    }
}

#[derive(Debug)]
struct CaptureBinding {
    group: String,
    target: CaptureTarget,
}

#[derive(Debug)]
enum CaptureTarget {
    Year,
    Month,
    Day,
    Custom(String),
}

#[derive(Debug)]
struct DateGroups {
    year: String,
    month: String,
    day: String,
}

pub fn compile(filter: &str) -> Result<Filter, MotorError> {
    let mut pattern = String::from("^");
    let mut literal = String::new();
    let mut bindings = Vec::new();
    let mut custom_names = HashSet::new();
    let mut year4_group = None;
    let mut month2_group = None;
    let mut day2_group = None;
    let mut position = 0;

    while position < filter.len() {
        let remaining = &filter[position..];

        if remaining.starts_with(".{") {
            let opening = position + 1;
            let (contents, next_position) = brace_contents(filter, opening)?;
            if contents.contains(',') {
                reject_reserved_modifier(contents)?;
                flush_literal(&mut pattern, &mut literal);
                pattern.push_str(&extension_list_pattern(contents)?);
                position = next_position;
                continue;
            }
        }

        if remaining.starts_with('{') {
            let (contents, next_position) = brace_contents(filter, position)?;
            reject_reserved_modifier(contents)?;
            flush_literal(&mut pattern, &mut literal);

            if contents == "*" {
                pattern.push_str(".*?");
            } else if let Some(name) = contents.strip_prefix("TEXTO:") {
                add_custom_capture(
                    name,
                    r"[\p{L}\p{N}_-]+",
                    &mut pattern,
                    &mut bindings,
                    &mut custom_names,
                )?;
            } else if let Some(name) = contents.strip_prefix("NUM:") {
                add_custom_capture(
                    name,
                    r"[0-9]+",
                    &mut pattern,
                    &mut bindings,
                    &mut custom_names,
                )?;
            } else {
                let token = &filter[position..next_position];
                let marker = list_markers()
                    .iter()
                    .find(|marker| marker.token == token)
                    .ok_or_else(|| MotorError::UnknownMarker {
                        marker: token.to_owned(),
                    })?;
                let group = add_builtin_marker(marker.kind, &mut pattern, &mut bindings);
                match marker.kind {
                    MarkerKind::Year4 if year4_group.is_none() => year4_group = group,
                    MarkerKind::Month2 if month2_group.is_none() => month2_group = group,
                    MarkerKind::Day2 if day2_group.is_none() => day2_group = group,
                    _ => {}
                }
            }

            position = next_position;
            continue;
        }

        let character = remaining
            .chars()
            .next()
            .ok_or(MotorError::UnclosedBrace { position })?;
        match character {
            '\\' => {
                let escaped = remaining[character.len_utf8()..].chars().next();
                match escaped {
                    Some(escaped) if is_escapable_filter_character(escaped) => {
                        literal.push(escaped);
                        position += escaped.len_utf8();
                    }
                    _ => literal.push(character),
                }
            }
            '*' => {
                flush_literal(&mut pattern, &mut literal);
                pattern.push_str(".*?");
            }
            '?' => {
                flush_literal(&mut pattern, &mut literal);
                pattern.push_str(r"[^/\\]");
            }
            _ => literal.push(character),
        }
        position += character.len_utf8();
    }

    flush_literal(&mut pattern, &mut literal);
    pattern.push('$');

    let regex = RegexBuilder::new(&pattern)
        .case_insensitive(true)
        .dot_matches_new_line(true)
        .build()
        .map_err(MotorError::InvalidFilter)?;
    let date_groups = match (year4_group, month2_group, day2_group) {
        (Some(year), Some(month), Some(day)) => Some(DateGroups { year, month, day }),
        _ => None,
    };

    Ok(Filter {
        regex,
        bindings,
        date_groups,
    })
}

fn is_escapable_filter_character(character: char) -> bool {
    matches!(character, '\\' | '*' | '?' | '{' | '}')
}

fn brace_contents(filter: &str, opening: usize) -> Result<(&str, usize), MotorError> {
    let contents_start = opening + 1;
    let closing_offset = filter[contents_start..]
        .find('}')
        .ok_or(MotorError::UnclosedBrace { position: opening })?;
    let closing = contents_start + closing_offset;
    Ok((&filter[contents_start..closing], closing + 1))
}

fn reject_reserved_modifier(contents: &str) -> Result<(), MotorError> {
    if contents.contains('|') {
        Err(MotorError::ReservedModifier)
    } else {
        Ok(())
    }
}

fn flush_literal(pattern: &mut String, literal: &mut String) {
    if !literal.is_empty() {
        pattern.push_str(&regex::escape(literal));
        literal.clear();
    }
}

fn add_builtin_marker(
    kind: MarkerKind,
    pattern: &mut String,
    bindings: &mut Vec<CaptureBinding>,
) -> Option<String> {
    let (marker_pattern, target) = match kind {
        MarkerKind::Year4 => (r"[0-9]{4}", Some(CaptureTarget::Year)),
        MarkerKind::Year2 => (r"[0-9]{2}", Some(CaptureTarget::Year)),
        MarkerKind::Month2 => (r"(?:0[1-9]|1[0-2])", Some(CaptureTarget::Month)),
        MarkerKind::MonthName => (
            r"(?:janeiro|fevereiro|março|abril|maio|junho|julho|agosto|setembro|outubro|novembro|dezembro)",
            Some(CaptureTarget::Month),
        ),
        MarkerKind::Day2 => (r"[0-9]{2}", Some(CaptureTarget::Day)),
        MarkerKind::Name => (r".*?", None),
        MarkerKind::Ext => (r"[^./\\]+", None),
    };

    if let Some(target) = target {
        let group = format!("capture_{}", bindings.len());
        pattern.push_str(&format!("(?P<{group}>{marker_pattern})"));
        bindings.push(CaptureBinding {
            group: group.clone(),
            target,
        });
        Some(group)
    } else {
        pattern.push_str(marker_pattern);
        None
    }
}

fn add_custom_capture(
    name: &str,
    capture_pattern: &str,
    pattern: &mut String,
    bindings: &mut Vec<CaptureBinding>,
    custom_names: &mut HashSet<String>,
) -> Result<(), MotorError> {
    validate_capture_name(name)?;
    if list_markers().iter().any(|marker| {
        marker
            .token
            .trim_matches(['{', '}'])
            .eq_ignore_ascii_case(name)
    }) {
        return Err(MotorError::BuiltinCaptureCollision {
            name: name.to_owned(),
        });
    }
    if !custom_names.insert(name.to_owned()) {
        return Err(MotorError::DuplicateCapture {
            name: name.to_owned(),
        });
    }

    let group = format!("capture_{}", bindings.len());
    pattern.push_str(&format!("(?P<{group}>{capture_pattern})"));
    bindings.push(CaptureBinding {
        group,
        target: CaptureTarget::Custom(name.to_owned()),
    });
    Ok(())
}

fn validate_capture_name(name: &str) -> Result<(), MotorError> {
    if name.is_empty()
        || !name
            .chars()
            .all(|character| character.is_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(MotorError::InvalidCaptureName {
            name: name.to_owned(),
        });
    }
    Ok(())
}

fn extension_list_pattern(contents: &str) -> Result<String, MotorError> {
    let mut extensions = Vec::new();
    for extension in contents.split(',') {
        if extension.is_empty()
            || !extension
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
        {
            return Err(MotorError::InvalidExtensionList {
                list: contents.to_owned(),
            });
        }

        let extension = extension.to_ascii_lowercase();
        if extension == "jpg" || extension == "jpeg" {
            for equivalent in ["jpg", "jpeg"] {
                if !extensions.contains(&equivalent.to_owned()) {
                    extensions.push(equivalent.to_owned());
                }
            }
        } else if !extensions.contains(&extension) {
            extensions.push(extension);
        }
    }

    let alternatives = extensions
        .iter()
        .map(|extension| regex::escape(extension))
        .collect::<Vec<_>>()
        .join("|");
    Ok(format!(r"\.(?:{alternatives})"))
}

fn captured_number<T>(captures: &regex::Captures<'_>, group: &str) -> Option<T>
where
    T: std::str::FromStr,
{
    captures.name(group)?.as_str().parse().ok()
}

fn is_temporary_download(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    lower.starts_with("~$")
        || lower.ends_with(".part")
        || lower.ends_with(".crdownload")
        || lower.ends_with(".tmp")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_table() {
        struct Case {
            filter: &'static str,
            file_name: &'static str,
            matches: bool,
        }

        let cases = [
            Case {
                filter: "*",
                file_name: "qualquer.txt",
                matches: true,
            },
            Case {
                filter: "*.pdf",
                file_name: "report.pdf",
                matches: true,
            },
            Case {
                filter: "*.pdf",
                file_name: "report.PDF",
                matches: true,
            },
            Case {
                filter: "*.pdf",
                file_name: "report.doc",
                matches: false,
            },
            Case {
                filter: "*ne*.pdf",
                file_name: "canela.pdf",
                matches: true,
            },
            Case {
                filter: "*ne*.pdf",
                file_name: "blaubi.pdf",
                matches: false,
            },
            Case {
                filter: "{YYYY}{MM}{DD}.pdf",
                file_name: "20261022.pdf",
                matches: true,
            },
            Case {
                filter: "{YYYY}{MM}{DD}.pdf",
                file_name: "20261022.PDF",
                matches: true,
            },
            Case {
                filter: "{YYYY}{MM}{DD}.{pdf,jpg}",
                file_name: "20261022.jpeg",
                matches: true,
            },
            Case {
                filter: "{YYYY}{MM}{DD}.pdf",
                file_name: "20261342.pdf",
                matches: false,
            },
            Case {
                filter: "{YYYY}{MM}{DD}.pdf",
                file_name: "20260230.pdf",
                matches: false,
            },
            Case {
                filter: "{YYYY}{MM}{DD}.pdf",
                file_name: "20260228.pdf",
                matches: true,
            },
            Case {
                filter: "{TEXTO:cliente}_{YYYY}{MM}{DD}.pdf",
                file_name: "Acme_20261022.pdf",
                matches: true,
            },
            Case {
                filter: "{TEXTO:cliente}_{YYYY}{MM}{DD}.pdf",
                file_name: "Acme Brasil_20261022.pdf",
                matches: false,
            },
            Case {
                filter: "{TEXTO:cliente}.pdf",
                file_name: "José_Ltda-2.pdf",
                matches: true,
            },
            Case {
                filter: "{NUM:pedido}.pdf",
                file_name: "12345.pdf",
                matches: true,
            },
            Case {
                filter: "{NUM:pedido}.pdf",
                file_name: "12A.pdf",
                matches: false,
            },
            Case {
                filter: "arquivo_{*}.txt",
                file_name: "arquivo_texto livre.txt",
                matches: true,
            },
            Case {
                filter: "relatório {YYYY}.pdf",
                file_name: "relatório 2026.pdf",
                matches: true,
            },
            Case {
                filter: "relatório {YYYY}.pdf",
                file_name: "RELATÓRIO 2026.PDF",
                matches: true,
            },
            Case {
                filter: "*.{jpg,png}",
                file_name: "foto.jpg",
                matches: true,
            },
            Case {
                filter: "*.{jpg,png}",
                file_name: "foto.jpeg",
                matches: true,
            },
            Case {
                filter: "*.{jpg,png}",
                file_name: "foto.PNG",
                matches: true,
            },
            Case {
                filter: "*.{jpg,png}",
                file_name: "foto.gif",
                matches: false,
            },
            Case {
                filter: "{YY}-{MM}-{DD}.*",
                file_name: "26-12-31.txt",
                matches: true,
            },
            Case {
                filter: "{YY}-{MM}-{DD}.*",
                file_name: "26-13-31.txt",
                matches: false,
            },
            Case {
                filter: "{MES}-{DD}.txt",
                file_name: "outubro-22.txt",
                matches: true,
            },
            Case {
                filter: "{MES}-{DD}.txt",
                file_name: "OUTUBRO-22.TXT",
                matches: true,
            },
            Case {
                filter: "{NOME}.{EXT}",
                file_name: "arquivo.tar.gz",
                matches: true,
            },
            Case {
                filter: "arquivo+final.pdf",
                file_name: "arquivo+final.pdf",
                matches: true,
            },
            Case {
                filter: "arquivo+final.pdf",
                file_name: "arquivooooofinal.pdf",
                matches: false,
            },
            Case {
                filter: "*",
                file_name: "report.pdf.part",
                matches: false,
            },
            Case {
                filter: "*",
                file_name: "report.pdf.crdownload",
                matches: false,
            },
            Case {
                filter: "*",
                file_name: "report.pdf.tmp",
                matches: false,
            },
            Case {
                filter: "*",
                file_name: "~$report.pdf",
                matches: false,
            },
        ];

        for case in cases {
            let filter = compile(case.filter).expect("table filters should compile");
            assert_eq!(
                filter.matches(case.file_name).is_some(),
                case.matches,
                "filter {:?}, file {:?}",
                case.filter,
                case.file_name
            );
        }
    }

    #[test]
    fn t201_matches_contains_and_single_character_wildcards() {
        struct Case {
            filter: &'static str,
            file_name: &'static str,
            matches: bool,
        }

        let cases = [
            ("*ne*.pdf", "nesta-semana.pdf", true),
            ("*ne*.pdf", "cabeneta232320.pdf", true),
            ("*ne*.pdf", "carabine.pdf", true),
            ("*ne*.pdf", "Nesta-Semana.PDF", true),
            ("*ne*.pdf", "relatorio.pdf", false),
            ("*ne*.pdf", "ne.docx", false),
            ("*belo*.pdf", "belo-horizonte.pdf", true),
            ("*belo*.pdf", "cabelo.pdf", true),
            ("*belo*.pdf", "abelos.pdf", true),
            ("*belo*.pdf", "bela.pdf", false),
            ("?*beti*.pdf", "alfabetizacao.pdf", true),
            ("?*beti*.pdf", "elizabeti.pdf", true),
            ("?*beti*.pdf", "betina.pdf", false),
            ("?*beti*.pdf", "beti.pdf", false),
            ("ne*.pdf", "nesta.pdf", true),
            ("ne*.pdf", "carabine.pdf", false),
            ("*ne.pdf", "carabine.pdf", true),
            ("*ne.pdf", "nesta.pdf", false),
            ("?.pdf", "a.pdf", true),
            ("?.pdf", "ab.pdf", false),
            ("?.pdf", ".pdf", false),
        ];

        for (filter, file_name, matches) in cases {
            let compiled = compile(filter).expect("T201 filter should compile");
            assert_eq!(
                compiled.matches(file_name).is_some(),
                matches,
                "filter {filter:?}, file {file_name:?}"
            );
        }
    }

    #[test]
    fn t202_escapes_wildcards_and_braces_as_literal_text() {
        let cases = [
            (r"*a\*b*.pdf", "xa*by.pdf", true),
            (r"*a\*b*.pdf", "xaZZby.pdf", false),
            (r"*a\?b*.pdf", "xa?by.pdf", true),
            (r"*a\?b*.pdf", "xaZby.pdf", false),
            (r"*a\{b*.pdf", "xa{by.pdf", true),
            (r"*a\{b*.pdf", "xaby.pdf", false),
        ];

        for (filter, file_name, matches) in cases {
            let compiled = compile(filter).expect("T202 filter should compile");
            assert_eq!(
                compiled.matches(file_name).is_some(),
                matches,
                "filter {filter:?}, file {file_name:?}"
            );
        }
    }

    #[test]
    fn extracts_dates_custom_values_name_and_extension() {
        let filter = compile("{TEXTO:cliente}_{YYYY}{MM}{DD}.pdf").expect("filter should compile");
        let captures = filter
            .matches("Acme_20261022.PDF")
            .expect("file should match");

        assert_eq!(captures.year.as_deref(), Some("2026"));
        assert_eq!(captures.month.as_deref(), Some("10"));
        assert_eq!(captures.day.as_deref(), Some("22"));
        assert_eq!(
            captures.custom.get("cliente").map(String::as_str),
            Some("Acme")
        );
        assert_eq!(captures.name, "Acme_20261022");
        assert_eq!(captures.ext, "PDF");
        assert_eq!(captures.get("{cliente}"), Some("Acme"));
        assert_eq!(captures.get("NOME"), Some("Acme_20261022"));
    }

    #[test]
    fn reports_filter_errors_in_portuguese() {
        let cases = [
            ("{DESCONHECIDO}", "não é conhecido"),
            ("arquivo_{YYYY", "sem fechamento"),
            ("{TEXTO:cliente}_{NUM:cliente}", "está repetido"),
            ("{TEXTO:YYYY}", "marcador padrão"),
            ("{YYYY|algo}", "reservado"),
            ("{TEXTO:nome com espaço}", "é inválido"),
            ("*.{jpg,}", "lista de extensões"),
        ];

        for (source, expected_message) in cases {
            let error = compile(source).expect_err("invalid filter should fail");
            assert!(
                error.to_string().contains(expected_message),
                "unexpected error for {source:?}: {error}"
            );
        }
    }

    #[test]
    fn exposes_the_documented_templates() {
        let templates = list_filter_templates();
        assert_eq!(templates.len(), 5);
        assert_eq!(
            templates[0],
            FilterTemplate {
                name_pt: "Qualquer arquivo",
                filter: "*"
            }
        );
        assert_eq!(
            templates[1],
            FilterTemplate {
                name_pt: "Só PDF",
                filter: "*.pdf"
            }
        );
        assert_eq!(
            templates[2],
            FilterTemplate {
                name_pt: "Imagens",
                filter: "*.{jpg,png}"
            }
        );
        assert_eq!(
            templates[3],
            FilterTemplate {
                name_pt: "Data no nome (AAAAMMDD)",
                filter: "{YYYY}{MM}{DD}.*"
            }
        );
        assert_eq!(
            templates[4],
            FilterTemplate {
                name_pt: "O nome contém…",
                filter: "*texto*"
            }
        );

        for template in templates {
            compile(template.filter).expect("built-in template should compile");
        }
    }
}
