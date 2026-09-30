//! Display names. The file and the label rule live in `skillstar-gateway`.

use skillstar_gateway::SaveModelNameError;

/// Why the display name was not saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveModelNameControlError {
    Name,
    Store,
}

impl std::fmt::Display for SaveModelNameControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Name => "model_name",
            Self::Store => "model_store",
        })
    }
}

/// Save the name shown for one catalog model.
pub fn save_model_name(id: &str, name: &str) -> Result<(), SaveModelNameControlError> {
    skillstar_gateway::save_model_name(id, name).map_err(|error| match error {
        SaveModelNameError::Name => SaveModelNameControlError::Name,
        SaveModelNameError::Store => SaveModelNameControlError::Store,
    })
}
