#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkerDef {
    pub token: &'static str,
    pub kind: MarkerKind,
    pub description_pt: &'static str,
    pub example: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    Year4,
    Year2,
    Month2,
    MonthName,
    Day2,
    Name,
    Ext,
}

static MARKERS: &[MarkerDef] = &[
    MarkerDef {
        token: "{YYYY}",
        kind: MarkerKind::Year4,
        description_pt: "Ano com quatro dígitos",
        example: "2026",
    },
    MarkerDef {
        token: "{YY}",
        kind: MarkerKind::Year2,
        description_pt: "Ano com dois dígitos",
        example: "26",
    },
    MarkerDef {
        token: "{MM}",
        kind: MarkerKind::Month2,
        description_pt: "Mês com dois dígitos",
        example: "10",
    },
    MarkerDef {
        token: "{MES}",
        kind: MarkerKind::MonthName,
        description_pt: "Nome do mês em português",
        example: "outubro",
    },
    MarkerDef {
        token: "{DD}",
        kind: MarkerKind::Day2,
        description_pt: "Dia com dois dígitos",
        example: "22",
    },
    MarkerDef {
        token: "{NOME}",
        kind: MarkerKind::Name,
        description_pt: "Nome do arquivo sem a extensão",
        example: "relatório",
    },
    MarkerDef {
        token: "{EXT}",
        kind: MarkerKind::Ext,
        description_pt: "Extensão do arquivo sem o ponto",
        example: "pdf",
    },
];

pub fn list_markers() -> &'static [MarkerDef] {
    MARKERS
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn registry_contains_all_documented_markers() {
        let expected = ["{YYYY}", "{YY}", "{MM}", "{MES}", "{DD}", "{NOME}", "{EXT}"];
        let actual: Vec<_> = list_markers().iter().map(|marker| marker.token).collect();

        assert_eq!(actual, expected);
    }

    #[test]
    fn marker_metadata_is_complete_and_tokens_are_unique() {
        let mut tokens = HashSet::new();

        for marker in list_markers() {
            assert!(tokens.insert(marker.token));
            assert!(marker.token.starts_with('{') && marker.token.ends_with('}'));
            assert!(!marker.description_pt.is_empty());
            assert!(!marker.example.is_empty());
        }
    }
}
