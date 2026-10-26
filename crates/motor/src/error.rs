//! Error type shared by the engine.

use std::path::PathBuf;

use thiserror::Error;

/// Errors returned by the engine. Display messages may reach the user, so they
/// are in pt-BR; the technical cause is available through `source()`.
#[derive(Debug, Error)]
pub enum MotorError {
    #[error("Não foi possível ler ou gravar o arquivo {}.", .path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    /// The config file exists but is not valid JSON for [`crate::Config`].
    /// `backup` is the copy kept for inspection, if it could be written.
    #[error("O arquivo de configuração {} está danificado e não pôde ser lido.", .path.display())]
    CorruptConfig {
        path: PathBuf,
        backup: Option<PathBuf>,
        source: serde_json::Error,
    },

    #[error("Não foi possível preparar a configuração para salvar.")]
    Serialize(#[source] serde_json::Error),

    #[error(transparent)]
    Filter(#[from] crate::filter::FilterError),

    #[error(transparent)]
    Destination(#[from] crate::destination::DestinationError),
}
