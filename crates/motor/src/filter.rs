//! Filter compiler: turns a rule's filter string into a matcher that checks
//! file names and extracts marker values from them.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use regex::{Regex, RegexBuilder};
use serde::Serialize;
use thiserror::Error;

use crate::error::MotorError;
use crate::markers::{self, MarkerDef, MarkerKind};

/// A ready-made filter offered by the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FilterTemplate {
    pub name_pt: &'static str,
    pub filter: &'static str,
}

static TEMPLATES: &[FilterTemplate] = &[
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
];

pub fn filter_templates() -> &'static [FilterTemplate] {
    TEMPLATES
}

/// Extensions that count as the same in an extension list.
const EQUIVALENT_EXTENSIONS: &[&[&str]] = &[&["jpg", "jpeg"]];

/// Problems in a filter string. Display messages are shown to the user.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FilterError {
    #[error("O filtro está vazio. Use * para aceitar qualquer arquivo.")]
    Empty,
    #[error("O marcador {{{0}}} não existe.")]
    UnknownMarker(String),
    #[error("A chave aberta em “{0}” não foi fechada com }}.")]
    UnclosedBrace(String),
    #[error("Há uma chave }} sem a chave {{ correspondente.")]
    UnmatchedClose,
    #[error("Há um par de chaves vazio {{}} no filtro.")]
    EmptyBraces,
    #[error("O caractere | dentro de chaves é reservado e ainda não pode ser usado.")]
    ReservedPipe,
    #[error("A lista de extensões tem um item em branco.")]
    EmptyExtension,
    #[error("Falta o nome da captura em {{{0}:}}.")]
    EmptyCaptureName(String),
    #[error("O nome de captura “{0}” só pode ter letras, números e _.")]
    InvalidCaptureName(String),
    #[error("O nome de captura “{0}” foi usado mais de uma vez no filtro.")]
    DuplicateCapture(String),
    #[error("O nome de captura “{0}” já é um marcador do sistema. Escolha outro nome.")]
    ReservedName(String),
    #[error("O marcador {{{0}}} repete uma informação (ano, mês ou dia) que já está no filtro.")]
    DuplicateMarker(String),
    #[error("O marcador {{{0}}} só pode ser usado na pasta de destino.")]
    DestinationOnly(String),
    #[error("O filtro é complexo demais.")]
    TooComplex,
}

/// Values extracted from a file name that matched a filter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Captures {
    pub year: Option<i32>,
    pub month: Option<u32>,
    pub day: Option<u32>,
    /// {NOME}: the file name without its extension.
    pub name: String,
    /// {EXT}: the extension without the dot, as written in the file name.
    pub ext: String,
    /// Values of {TEXTO:x} and {NUM:x}, by capture name.
    pub custom: BTreeMap<String, String>,
}

/// What each regex capture group (in order) holds.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Slot {
    Year4,
    Year2,
    Month2,
    MonthName,
    Day2,
    Custom(String),
}

/// A compiled filter. Matching ignores case.
#[derive(Debug, Clone)]
pub struct Filter {
    source: String,
    regex: Regex,
    slots: Vec<Slot>,
}

/// Compiles a filter string such as `{TEXTO:cliente}_{YYYY}{MM}{DD}.{pdf,jpg}`.
pub fn compile(filter: &str) -> Result<Filter, MotorError> {
    Ok(Compiler::default().run(filter)?)
}

impl Filter {
    /// The filter string this was compiled from.
    pub fn as_str(&self) -> &str {
        &self.source
    }

    /// Whether the filter captures any part of a date ({YYYY}, {MM}, {DD}...).
    pub fn has_date(&self) -> bool {
        self.slots
            .iter()
            .any(|slot| !matches!(slot, Slot::Custom(_)))
    }

    /// Matches a file name (any folder part is ignored). Temporary download
    /// names and impossible dates never match.
    pub fn matches(&self, file_name: &str) -> Option<Captures> {
        let name = file_name
            .rsplit(std::path::is_separator)
            .next()
            .unwrap_or(file_name);
        if is_temporary(name) {
            return None;
        }
        let groups = self.regex.captures(name)?;

        let (stem, ext) = match name.rfind('.') {
            Some(dot) if dot > 0 => (&name[..dot], &name[dot + 1..]),
            _ => (name, ""),
        };
        let mut captures = Captures {
            name: stem.to_owned(),
            ext: ext.to_owned(),
            ..Captures::default()
        };

        for (i, slot) in self.slots.iter().enumerate() {
            let text = groups.get(i + 1)?.as_str();
            match slot {
                Slot::Year4 => captures.year = Some(text.parse().ok()?),
                Slot::Year2 => captures.year = Some(2000 + text.parse::<i32>().ok()?),
                Slot::Month2 => captures.month = Some(text.parse().ok()?),
                Slot::MonthName => captures.month = Some(markers::month_from_name(text)?),
                Slot::Day2 => captures.day = Some(text.parse().ok()?),
                Slot::Custom(key) => {
                    captures.custom.insert(key.clone(), text.to_owned());
                }
            }
        }

        // Without a year, check against a leap year so 29/02 is accepted.
        if let (Some(month), Some(day)) = (captures.month, captures.day) {
            NaiveDate::from_ymd_opt(captures.year.unwrap_or(2000), month, day)?;
        }
        Some(captures)
    }
}

/// Names of files still being downloaded or written by other programs.
pub fn is_temporary(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    lower.ends_with(".part")
        || lower.ends_with(".crdownload")
        || lower.ends_with(".tmp")
        || file_name.starts_with("~$")
}

#[derive(Default)]
struct Compiler {
    pattern: String,
    literal: String,
    slots: Vec<Slot>,
    has_year: bool,
    has_month: bool,
    has_day: bool,
    /// Custom capture names, lowercased, to catch duplicates.
    names: Vec<String>,
}

impl Compiler {
    fn run(mut self, filter: &str) -> Result<Filter, FilterError> {
        if filter.trim().is_empty() {
            return Err(FilterError::Empty);
        }

        let mut rest = filter;
        while let Some(c) = rest.chars().next() {
            match c {
                '*' => {
                    self.push_pattern(".*?");
                    rest = &rest[1..];
                }
                '}' => return Err(FilterError::UnmatchedClose),
                '{' => {
                    let close = rest
                        .find('}')
                        .ok_or_else(|| FilterError::UnclosedBrace(rest.to_owned()))?;
                    let content = &rest[1..close];
                    if content.contains('{') {
                        return Err(FilterError::UnclosedBrace(rest[..=close].to_owned()));
                    }
                    self.brace(content)?;
                    rest = &rest[close + 1..];
                }
                _ => {
                    self.literal.push(c);
                    rest = &rest[c.len_utf8()..];
                }
            }
        }
        self.flush_literal();

        let regex = RegexBuilder::new(&format!("^{}$", self.pattern))
            .case_insensitive(true)
            .build()
            .map_err(|_| FilterError::TooComplex)?;
        Ok(Filter {
            source: filter.to_owned(),
            regex,
            slots: self.slots,
        })
    }

    fn brace(&mut self, content: &str) -> Result<(), FilterError> {
        if content.contains('|') {
            return Err(FilterError::ReservedPipe);
        }
        if content.is_empty() {
            return Err(FilterError::EmptyBraces);
        }
        if content == "*" {
            self.push_pattern(".*?");
            return Ok(());
        }

        if let Some((keyword, name)) = content.split_once(':') {
            let fragment = if keyword.eq_ignore_ascii_case("TEXTO") {
                r"[\p{L}\p{N}_-]+"
            } else if keyword.eq_ignore_ascii_case("NUM") {
                "[0-9]+"
            } else {
                return Err(FilterError::UnknownMarker(content.to_owned()));
            };
            self.claim_name(keyword, name)?;
            self.push_capture(fragment, Slot::Custom(name.to_owned()));
            return Ok(());
        }

        if let Some(def) = markers::find_marker(content) {
            return self.marker(def);
        }
        // `.{pdf,jpg}`: an extension list follows a dot.
        if self.literal.ends_with('.') {
            return self.extension_list(content);
        }
        Err(FilterError::UnknownMarker(content.to_owned()))
    }

    fn claim_name(&mut self, keyword: &str, name: &str) -> Result<(), FilterError> {
        if name.is_empty() {
            return Err(FilterError::EmptyCaptureName(keyword.to_owned()));
        }
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(FilterError::InvalidCaptureName(name.to_owned()));
        }
        if markers::find_marker(name).is_some() {
            return Err(FilterError::ReservedName(name.to_owned()));
        }
        let lower = name.to_lowercase();
        if self.names.contains(&lower) {
            return Err(FilterError::DuplicateCapture(name.to_owned()));
        }
        self.names.push(lower);
        Ok(())
    }

    fn marker(&mut self, def: &MarkerDef) -> Result<(), FilterError> {
        let (claimed, fragment, slot) = match def.kind {
            MarkerKind::Year4 => (&mut self.has_year, "[0-9]{4}".to_owned(), Slot::Year4),
            MarkerKind::Year2 => (&mut self.has_year, "[0-9]{2}".to_owned(), Slot::Year2),
            MarkerKind::Month2 => (
                &mut self.has_month,
                "0[1-9]|1[0-2]".to_owned(),
                Slot::Month2,
            ),
            MarkerKind::MonthName => {
                let names: Vec<String> = markers::MONTH_NAMES_PT
                    .iter()
                    .chain(&["marco"])
                    .map(|m| regex::escape(m))
                    .collect();
                (&mut self.has_month, names.join("|"), Slot::MonthName)
            }
            MarkerKind::Day2 => (
                &mut self.has_day,
                "0[1-9]|[12][0-9]|3[01]".to_owned(),
                Slot::Day2,
            ),
            MarkerKind::Name | MarkerKind::Ext => {
                return Err(FilterError::DestinationOnly(def.token.to_owned()));
            }
        };
        if *claimed {
            return Err(FilterError::DuplicateMarker(def.token.to_owned()));
        }
        *claimed = true;
        self.push_capture(&fragment, slot);
        Ok(())
    }

    fn extension_list(&mut self, content: &str) -> Result<(), FilterError> {
        let mut extensions = Vec::new();
        for item in content.split(',') {
            let item = item.trim().to_lowercase();
            if item.is_empty() {
                return Err(FilterError::EmptyExtension);
            }
            match EQUIVALENT_EXTENSIONS
                .iter()
                .find(|group| group.contains(&item.as_str()))
            {
                Some(group) => extensions.extend(group.iter().map(|e| regex::escape(e))),
                None => extensions.push(regex::escape(&item)),
            }
        }
        extensions.sort();
        extensions.dedup();
        self.push_pattern(&format!("(?:{})", extensions.join("|")));
        Ok(())
    }

    fn push_capture(&mut self, fragment: &str, slot: Slot) {
        self.push_pattern(&format!("({fragment})"));
        self.slots.push(slot);
    }

    fn push_pattern(&mut self, fragment: &str) {
        self.flush_literal();
        self.pattern.push_str(fragment);
    }

    fn flush_literal(&mut self) {
        self.pattern.push_str(&regex::escape(&self.literal));
        self.literal.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captures as a compact string: date parts and custom captures, in order.
    fn describe(c: &Captures) -> String {
        let mut parts = Vec::new();
        if let Some(y) = c.year {
            parts.push(format!("y={y}"));
        }
        if let Some(m) = c.month {
            parts.push(format!("m={m}"));
        }
        if let Some(d) = c.day {
            parts.push(format!("d={d}"));
        }
        for (k, v) in &c.custom {
            parts.push(format!("{k}={v}"));
        }
        parts.join(" ")
    }

    fn filter_error(filter: &str) -> FilterError {
        match compile(filter) {
            Err(MotorError::Filter(e)) => e,
            other => panic!("{filter}: expected a filter error, got {other:?}"),
        }
    }

    #[test]
    fn matching_table() {
        const DATE_PDF: &str = "{YYYY}{MM}{DD}.pdf";
        const CLIENTE: &str = "{TEXTO:cliente}_{YYYY}{MM}{DD}.pdf";
        // (filter, file name, expected captures or None for no match)
        let cases: &[(&str, &str, Option<&str>)] = &[
            ("*", "qualquer coisa.txt", Some("")),
            ("*", "report.pdf.part", None),
            ("*", "video.mp4.crdownload", None),
            ("*", "dados.TMP", None),
            ("*", "~$planilha.xlsx", None),
            ("*.pdf", "report.pdf", Some("")),
            ("*.pdf", "REPORT.PDF", Some("")),
            ("*.pdf", "report.pdf.part", None),
            ("*.pdf", "report.docx", None),
            ("*.pdf", "/home/u/NEWS/report.pdf", Some("")),
            (DATE_PDF, "20261022.pdf", Some("y=2026 m=10 d=22")),
            (DATE_PDF, "20261022.PDF", Some("y=2026 m=10 d=22")),
            (DATE_PDF, "20261342.pdf", None),
            (DATE_PDF, "20260230.pdf", None),
            (DATE_PDF, "20240229.pdf", Some("y=2024 m=2 d=29")),
            (DATE_PDF, "20250229.pdf", None),
            (DATE_PDF, "2026102.pdf", None),
            (DATE_PDF, "x20261022.pdf", None),
            (
                "{YYYY}{MM}{DD}.{pdf,jpg}",
                "20261022.jpeg",
                Some("y=2026 m=10 d=22"),
            ),
            ("*.{pdf,jpg}", "foto.JPG", Some("")),
            ("*.{pdf,jpg}", "foto.Jpeg", Some("")),
            ("*.{pdf,jpg}", "foto.png", None),
            ("*.{jpeg}", "foto.jpg", Some("")),
            (
                CLIENTE,
                "Acme_20261022.pdf",
                Some("y=2026 m=10 d=22 cliente=Acme"),
            ),
            (CLIENTE, "Acme Corp_20261022.pdf", None),
            (
                "{TEXTO:cliente}_*.pdf",
                "São-Paulo_x.pdf",
                Some("cliente=São-Paulo"),
            ),
            ("NF{NUM:numero}.xml", "NF000123.xml", Some("numero=000123")),
            ("NF{NUM:numero}.xml", "NFabc.xml", None),
            ("relatório *.pdf", "relatório 2026.pdf", Some("")),
            ("relatório *.pdf", "relatorio 2026.pdf", None),
            ("* {YYYY}.pdf", "relatório anual 2026.pdf", Some("y=2026")),
            (
                "{DD}-{MES}-{YYYY}.pdf",
                "05-março-2026.pdf",
                Some("y=2026 m=3 d=5"),
            ),
            (
                "{DD}-{MES}-{YYYY}.pdf",
                "05-MARCO-2026.pdf",
                Some("y=2026 m=3 d=5"),
            ),
            ("{DD}-{MES}-{YYYY}.pdf", "31-fevereiro-2026.pdf", None),
            ("{YY}{MM}.csv", "2610.csv", Some("y=2026 m=10")),
            ("{MM}.txt", "13.txt", None),
            ("{DD}{MM}.txt", "3002.txt", None),
            ("{*}_final.docx", "contrato_v2_final.docx", Some("")),
            ("a+b (1).txt", "a+b (1).txt", Some("")),
            ("a+b (1).txt", "aab (1).txt", None),
            (
                "{YYYY}{MM}{DD}.*",
                "20261022.tar.gz",
                Some("y=2026 m=10 d=22"),
            ),
        ];

        for (filter, name, expected) in cases {
            let compiled = compile(filter).unwrap();
            let got = compiled.matches(name).map(|c| describe(&c));
            assert_eq!(
                got.as_deref(),
                *expected,
                "filter {filter:?}, file {name:?}"
            );
        }
    }

    #[test]
    fn captures_name_and_extension() {
        let c = compile("*").unwrap().matches("relatório 2026.PDF").unwrap();
        assert_eq!((c.name.as_str(), c.ext.as_str()), ("relatório 2026", "PDF"));
        let c = compile("*").unwrap().matches("LEIAME").unwrap();
        assert_eq!((c.name.as_str(), c.ext.as_str()), ("LEIAME", ""));
        let c = compile("*").unwrap().matches(".config").unwrap();
        assert_eq!((c.name.as_str(), c.ext.as_str()), (".config", ""));
    }

    #[test]
    fn error_table() {
        use FilterError::*;
        let cases: &[(&str, FilterError)] = &[
            ("", Empty),
            ("{ANO}.pdf", UnknownMarker("ANO".into())),
            ("{pdf,jpg}", UnknownMarker("pdf,jpg".into())),
            ("{FOO:x}", UnknownMarker("FOO:x".into())),
            ("{YYYY.pdf", UnclosedBrace("{YYYY.pdf".into())),
            ("{YYYY{MM}.pdf", UnclosedBrace("{YYYY{MM}".into())),
            ("x}.pdf", UnmatchedClose),
            ("{}.pdf", EmptyBraces),
            ("{YYYY|2}", ReservedPipe),
            ("*.{pdf,}", EmptyExtension),
            ("{TEXTO:}", EmptyCaptureName("TEXTO".into())),
            ("{NUM:meu nome}", InvalidCaptureName("meu nome".into())),
            ("{TEXTO:a}_{NUM:a}", DuplicateCapture("a".into())),
            ("{TEXTO:a}_{NUM:A}", DuplicateCapture("A".into())),
            ("{TEXTO:yyyy}", ReservedName("yyyy".into())),
            ("{NUM:NOME}", ReservedName("NOME".into())),
            ("{YYYY}{YYYY}", DuplicateMarker("YYYY".into())),
            ("{YYYY}{YY}", DuplicateMarker("YY".into())),
            ("{MM}{MES}", DuplicateMarker("MES".into())),
            ("{NOME}.pdf", DestinationOnly("NOME".into())),
            ("*.{EXT}", DestinationOnly("EXT".into())),
        ];
        for (filter, expected) in cases {
            assert_eq!(&filter_error(filter), expected, "filter {filter:?}");
        }
    }

    #[test]
    fn error_messages_name_the_problem() {
        let message = filter_error("{ANO}.pdf").to_string();
        assert_eq!(message, "O marcador {ANO} não existe.");
        let message = filter_error("{TEXTO:a}_{NUM:a}").to_string();
        assert!(message.contains("“a”"), "{message}");
        let message = filter_error("{YYYY.pdf").to_string();
        assert!(message.contains("{YYYY.pdf"), "{message}");
    }

    #[test]
    fn every_filter_marker_matches_its_example() {
        for def in markers::list_markers() {
            let filter = format!("{{{}}}", def.token);
            if def.kind.usable_in_filter() {
                let compiled = compile(&filter).unwrap();
                assert!(compiled.matches(def.example).is_some(), "{filter}");
            } else {
                assert_eq!(
                    filter_error(&filter),
                    FilterError::DestinationOnly(def.token.into())
                );
            }
        }
    }

    #[test]
    fn templates_compile() {
        for template in filter_templates() {
            assert!(compile(template.filter).is_ok(), "{}", template.name_pt);
        }
        let date = compile("{YYYY}{MM}{DD}.*").unwrap();
        assert!(date.matches("20261022.pdf").is_some());
    }

    #[test]
    fn temporary_names() {
        for name in ["a.part", "a.PART", "a.crdownload", "a.tmp", "~$a.docx"] {
            assert!(is_temporary(name), "{name}");
        }
        for name in ["a.pdf", "partes.pdf", "a~$.docx", "tmp.pdf"] {
            assert!(!is_temporary(name), "{name}");
        }
    }
}
