export type CommandCatalog = {
  version: number;
  commands: CommandDefinition[];
};

export type RawCommandCatalog = {
  version: number;
  commands: RawCommandDefinition[];
};

export type CommandDefinition = {
  name: string;
  aliases: string[];
  description: string;
  handler: CommandHandler;
  requires_selected_task: boolean;
  insert_trailing_space: boolean;
  argument: CommandArgumentDefinition | null;
};

export type RawCommandDefinition = Omit<CommandDefinition, "handler"> & {
  handler: string;
  argument: RawCommandArgumentDefinition | null;
};

export type CommandArgumentDefinition = {
  name: string;
  kind: CommandArgumentKind;
  required: boolean;
  placeholder?: string | null;
  completion?: CommandCompletion | null;
};

export type RawCommandArgumentDefinition = Omit<
  CommandArgumentDefinition,
  "kind" | "completion"
> & {
  kind: string;
  completion?: string | null;
};

export const KNOWN_COMMAND_HANDLERS = [
  "create_root_and_start",
  "rename_selected",
  "reparent_selected",
  "start_selected",
  "pause_selected",
  "resume_selected",
  "stop_selected",
  "create_or_insert_subtask",
] as const;

export type CommandHandler = (typeof KNOWN_COMMAND_HANDLERS)[number];

export const KNOWN_COMMAND_ARGUMENT_KINDS = ["text", "parent_target"] as const;
export type CommandArgumentKind = (typeof KNOWN_COMMAND_ARGUMENT_KINDS)[number];

export const KNOWN_COMMAND_COMPLETIONS = ["parent_target"] as const;
export type CommandCompletion = (typeof KNOWN_COMMAND_COMPLETIONS)[number];

export function isCommandHandler(value: string): value is CommandHandler {
  return (KNOWN_COMMAND_HANDLERS as readonly string[]).includes(value);
}

export function isCommandArgumentKind(value: string): value is CommandArgumentKind {
  return (KNOWN_COMMAND_ARGUMENT_KINDS as readonly string[]).includes(value);
}

export function isCommandCompletion(value: string): value is CommandCompletion {
  return (KNOWN_COMMAND_COMPLETIONS as readonly string[]).includes(value);
}
