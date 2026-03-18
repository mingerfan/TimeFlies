use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::infra::{AppError, AppResult};

const BUILTIN_COMMAND_CATALOG_TOML: &str = include_str!("../config/commands.toml");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandCatalog {
    pub version: u32,
    pub commands: Vec<CommandDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDefinition {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub description: String,
    pub handler: String,
    #[serde(default)]
    pub requires_selected_task: bool,
    #[serde(default)]
    pub insert_trailing_space: bool,
    pub argument: Option<CommandArgumentDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandArgumentDefinition {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub required: bool,
    pub placeholder: Option<String>,
    pub completion: Option<String>,
}

pub fn load_builtin_command_catalog() -> AppResult<CommandCatalog> {
    let catalog = toml::from_str::<CommandCatalog>(BUILTIN_COMMAND_CATALOG_TOML).map_err(|error| {
        AppError::internal(
            "failed to parse command catalog",
            format!("failed to parse builtin command catalog: {error}"),
        )
    })?;
    validate_command_catalog(&catalog)?;
    Ok(catalog)
}

fn validate_command_catalog(catalog: &CommandCatalog) -> AppResult<()> {
    if catalog.version == 0 {
        return Err(AppError::validation("command catalog version must be positive"));
    }

    if catalog.commands.is_empty() {
        return Err(AppError::validation("command catalog must define at least one command"));
    }

    let mut seen_tokens = HashSet::new();
    for command in &catalog.commands {
        validate_command_definition(command, &mut seen_tokens)?;
    }

    Ok(())
}

fn validate_command_definition(
    command: &CommandDefinition,
    seen_tokens: &mut HashSet<String>,
) -> AppResult<()> {
    let command_name = command.name.trim().to_lowercase();
    if command_name.is_empty() {
        return Err(AppError::validation("command name cannot be empty"));
    }
    ensure_command_token_valid(&command_name, "command name")?;
    ensure_unique_token(&command_name, seen_tokens)?;

    if command.description.trim().is_empty() {
        return Err(AppError::validation(format!(
            "command '{command_name}' description cannot be empty"
        )));
    }

    if command.handler.trim().is_empty() {
        return Err(AppError::validation(format!(
            "command '{command_name}' handler cannot be empty"
        )));
    }

    for alias in &command.aliases {
        let alias_name = alias.trim().to_lowercase();
        if alias_name.is_empty() {
            return Err(AppError::validation(format!(
                "command '{command_name}' contains an empty alias"
            )));
        }
        ensure_command_token_valid(&alias_name, "command alias")?;
        ensure_unique_token(&alias_name, seen_tokens)?;
    }

    validate_command_argument(command)?;

    Ok(())
}

fn validate_command_argument(command: &CommandDefinition) -> AppResult<()> {
    let Some(argument) = &command.argument else {
        return Ok(());
    };

    if argument.name.trim().is_empty() {
        return Err(AppError::validation(format!(
            "command '{}' has an empty argument name",
            command.name
        )));
    }

    if argument.kind.trim().is_empty() {
        return Err(AppError::validation(format!(
            "command '{}' has an empty argument kind for '{}'",
            command.name, argument.name
        )));
    }

    if let Some(completion) = &argument.completion {
        if completion.trim().is_empty() {
            return Err(AppError::validation(format!(
                "command '{}' has an empty completion for argument '{}'",
                command.name, argument.name
            )));
        }
    }

    Ok(())
}

fn ensure_command_token_valid(token: &str, label: &str) -> AppResult<()> {
    if token
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
    {
        return Ok(());
    }

    Err(AppError::validation(format!(
        "{label} '{token}' contains unsupported characters"
    )))
}

fn ensure_unique_token(token: &str, seen_tokens: &mut HashSet<String>) -> AppResult<()> {
    if seen_tokens.insert(token.to_string()) {
        return Ok(());
    }

    Err(AppError::validation(format!(
        "duplicate command token '{token}' in command catalog"
    )))
}

#[cfg(test)]
mod tests {
    use super::load_builtin_command_catalog;

    #[test]
    fn builtin_command_catalog_is_valid() {
        load_builtin_command_catalog().expect("builtin command catalog should be valid");
    }
}
