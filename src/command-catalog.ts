import { invoke } from "@tauri-apps/api/core";

import {
  isCommandHandler,
  isCommandArgumentKind,
  isCommandCompletion,
  type CommandCatalog,
  type CommandArgumentDefinition,
  type CommandDefinition,
  type RawCommandCatalog,
  type RawCommandArgumentDefinition,
  type RawCommandDefinition,
} from "./command-types";

let cachedCatalog: CommandCatalog | null = null;
let catalogPromise: Promise<CommandCatalog> | null = null;

export async function ensureCommandCatalog(): Promise<CommandCatalog> {
  if (cachedCatalog) return cachedCatalog;
  if (!catalogPromise) {
    catalogPromise = invoke<RawCommandCatalog>("get_command_catalog")
      .then((catalog) => {
        const normalized = normalizeCommandCatalog(catalog);
        cachedCatalog = normalized;
        return normalized;
      })
      .catch((error) => {
        const detail = error instanceof Error ? error.message : String(error);
        throw new Error(`failed to load command catalog: ${detail}`);
      })
      .finally(() => {
        catalogPromise = null;
      });
  }
  return catalogPromise;
}

function normalizeCommandCatalog(rawCatalog: RawCommandCatalog): CommandCatalog {
  const invalidCommands: string[] = [];
  const commands = rawCatalog.commands.flatMap((command) => {
    const normalized = normalizeCommandDefinition(command);
    if (normalized) return [normalized];
    invalidCommands.push(`${command.name} -> ${command.handler}`);
    return [];
  });

  if (invalidCommands.length > 0) {
    console.warn(
      `[command-catalog] filtered commands with unsupported handlers: ${invalidCommands.join(", ")}`
    );
  }

  if (commands.length === 0) {
    throw new Error("command catalog does not contain any supported commands");
  }

  return {
    version: rawCatalog.version,
    commands,
  };
}

function normalizeCommandDefinition(command: RawCommandDefinition): CommandDefinition | null {
  if (!isCommandHandler(command.handler)) {
    return null;
  }

  const argument = normalizeCommandArgument(command.argument);
  if (command.argument && !argument) {
    return null;
  }

  return {
    ...command,
    handler: command.handler,
    argument,
  };
}

function normalizeCommandArgument(
  argument: RawCommandArgumentDefinition | null
): CommandArgumentDefinition | null {
  if (!argument) return null;
  if (!isCommandArgumentKind(argument.kind)) {
    return null;
  }

  if (argument.completion != null && !isCommandCompletion(argument.completion)) {
    return null;
  }

  if (argument.kind === "text" && argument.completion != null) {
    return null;
  }

  if (argument.kind === "parent_target") {
    if (argument.completion != null && argument.completion !== "parent_target") {
      return null;
    }
  }

  return {
    ...argument,
    kind: argument.kind,
    completion: argument.completion ?? null,
  };
}

export function findCommandDefinitionByToken(
  catalog: CommandCatalog,
  rawToken: string
): CommandDefinition | null {
  const token = rawToken.trim().toLowerCase();
  if (!token) return null;

  for (const command of catalog.commands) {
    if (command.name.toLowerCase() === token) return command;
    if (command.aliases.some((alias) => alias.toLowerCase() === token)) return command;
  }

  return null;
}

export function listPrimaryCommandNames(catalog: CommandCatalog): string[] {
  return catalog.commands.map((command) => command.name);
}
