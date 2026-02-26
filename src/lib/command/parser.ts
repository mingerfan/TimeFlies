export const COMMAND_NAMES = [
  "rename",
  "parent",
  "start",
  "pause",
  "resume",
  "stop",
  "sub",
] as const;

export type CommandName = (typeof COMMAND_NAMES)[number];

type EmptyParseResult = {
  kind: "empty";
  raw: string;
  tags: string[];
  invalidTags: string[];
};

type InvalidParseResult = {
  kind: "invalid";
  raw: string;
  message: string;
  tags: string[];
  invalidTags: string[];
};

type CreateParseResult = {
  kind: "create";
  raw: string;
  title: string;
  tags: string[];
  invalidTags: string[];
};

type CommandParseResult = {
  kind: "command";
  raw: string;
  name: CommandName;
  argument: string;
  tags: string[];
  invalidTags: string[];
};

type UnknownCommandParseResult = {
  kind: "unknown-command";
  raw: string;
  name: string;
  argument: string;
  tags: string[];
  invalidTags: string[];
};

export type ParsedCommandInput =
  | EmptyParseResult
  | InvalidParseResult
  | CreateParseResult
  | CommandParseResult
  | UnknownCommandParseResult;

type TagParseResult = {
  text: string;
  tags: string[];
  invalidTags: string[];
};

const COMMAND_SET = new Set<string>(COMMAND_NAMES);

export function parseCommandInput(rawInput: string): ParsedCommandInput {
  const raw = rawInput;
  const trimmed = rawInput.trim();
  if (!trimmed) {
    return {
      kind: "empty",
      raw,
      tags: [],
      invalidTags: [],
    };
  }

  if (trimmed.startsWith("/")) {
    const body = trimmed.slice(1).trim();
    if (!body) {
      return {
        kind: "invalid",
        raw,
        message: "命令不能为空",
        tags: [],
        invalidTags: [],
      };
    }

    const [rawName, ...restParts] = body.split(/\s+/);
    const name = rawName.toLowerCase();
    const { text, tags, invalidTags } = parseTags(restParts.join(" "));

    if (!COMMAND_SET.has(name)) {
      return {
        kind: "unknown-command",
        raw,
        name,
        argument: text,
        tags,
        invalidTags,
      };
    }

    return {
      kind: "command",
      raw,
      name: name as CommandName,
      argument: text,
      tags,
      invalidTags,
    };
  }

  const { text, tags, invalidTags } = parseTags(trimmed);
  const title = text.trim();
  if (!title) {
    return {
      kind: "invalid",
      raw,
      message: "请输入任务标题或命令",
      tags,
      invalidTags,
    };
  }

  return {
    kind: "create",
    raw,
    title,
    tags,
    invalidTags,
  };
}

function parseTags(input: string): TagParseResult {
  const words = input
    .split(/\s+/)
    .map((item) => item.trim())
    .filter(Boolean);

  const content: string[] = [];
  const tags: string[] = [];
  const invalidTags: string[] = [];
  const seenTags = new Set<string>();

  for (const word of words) {
    if (!word.startsWith("#")) {
      content.push(word);
      continue;
    }

    const rawTag = word.slice(1).trim();
    if (!rawTag || rawTag.includes("#")) {
      invalidTags.push(word);
      continue;
    }

    if (seenTags.has(rawTag)) {
      continue;
    }

    seenTags.add(rawTag);
    tags.push(rawTag);
  }

  return {
    text: content.join(" "),
    tags,
    invalidTags,
  };
}
