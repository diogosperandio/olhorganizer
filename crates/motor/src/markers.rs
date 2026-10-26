//! Registry of built-in markers. The UI builds its marker buttons from
//! [`list_markers`]; the filter compiler looks markers up here by token.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MarkerKind {
    Year4,
    Year2,
    Month2,
    MonthName,
    Day2,
    Name,
    Ext,
}

impl MarkerKind {
    /// Name and extension describe the whole file name, so they only make
    /// sense in a destination, not in a filter.
    pub fn usable_in_filter(self) -> bool {
        !matches!(self, Self::Name | Self::Ext)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MarkerDef {
    /// Text between the braces: `YYYY` for `{YYYY}`.
    pub token: &'static str,
    pub kind: MarkerKind,
    pub description_pt: &'static str,
    pub example: &'static str,
}

static MARKERS: &[MarkerDef] = &[
    MarkerDef {
        token: "YYYY",
        kind: MarkerKind::Year4,
        description_pt: "Ano com 4 dígitos",
        example: "2026",
    },
    MarkerDef {
        token: "YY",
        kind: MarkerKind::Year2,
        description_pt: "Ano com 2 dígitos",
        example: "26",
    },
    MarkerDef {
        token: "MM",
        kind: MarkerKind::Month2,
        description_pt: "Mês com 2 dígitos",
        example: "10",
    },
    MarkerDef {
        token: "MES",
        kind: MarkerKind::MonthName,
        description_pt: "Nome do mês",
        example: "outubro",
    },
    MarkerDef {
        token: "DD",
        kind: MarkerKind::Day2,
        description_pt: "Dia com 2 dígitos",
        example: "22",
    },
    MarkerDef {
        token: "NOME",
        kind: MarkerKind::Name,
        description_pt: "Nome do arquivo, sem a extensão",
        example: "20261022",
    },
    MarkerDef {
        token: "EXT",
        kind: MarkerKind::Ext,
        description_pt: "Extensão do arquivo, sem o ponto",
        example: "pdf",
    },
];

pub fn list_markers() -> &'static [MarkerDef] {
    MARKERS
}

/// Looks a marker up by token, ignoring case.
pub fn find_marker(token: &str) -> Option<&'static MarkerDef> {
    MARKERS.iter().find(|m| m.token.eq_ignore_ascii_case(token))
}

pub const MONTH_NAMES_PT: [&str; 12] = [
    "janeiro",
    "fevereiro",
    "março",
    "abril",
    "maio",
    "junho",
    "julho",
    "agosto",
    "setembro",
    "outubro",
    "novembro",
    "dezembro",
];

/// Month number (1-12) for a pt-BR month name, ignoring case. "marco" is
/// accepted for "março", since users often drop the accent.
pub fn month_from_name(name: &str) -> Option<u32> {
    let name = name.to_lowercase();
    if name == "marco" {
        return Some(3);
    }
    let index = MONTH_NAMES_PT.iter().position(|m| *m == name)?;
    u32::try_from(index + 1).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_unique() {
        for (i, a) in MARKERS.iter().enumerate() {
            for b in &MARKERS[i + 1..] {
                assert!(!a.token.eq_ignore_ascii_case(b.token), "{}", a.token);
            }
        }
    }

    #[test]
    fn find_marker_ignores_case() {
        assert_eq!(find_marker("yyyy").map(|m| m.kind), Some(MarkerKind::Year4));
        assert_eq!(
            find_marker("Mes").map(|m| m.kind),
            Some(MarkerKind::MonthName)
        );
        assert!(find_marker("ANO").is_none());
    }

    #[test]
    fn month_names() {
        assert_eq!(month_from_name("janeiro"), Some(1));
        assert_eq!(month_from_name("MARÇO"), Some(3));
        assert_eq!(month_from_name("marco"), Some(3));
        assert_eq!(month_from_name("Dezembro"), Some(12));
        assert_eq!(month_from_name("october"), None);
    }
}
