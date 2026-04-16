type ParseOk = {
  ok: true;
  deltaSeconds: number;
};

type ParseError = {
  ok: false;
  message: string;
};

const UNIT_SECONDS = {
  h: 60 * 60,
  m: 60,
  s: 1,
} as const;

const SEGMENT_PATTERN = /(\d+)\s*(h|m|s)/gy;

export function parseFocusAdjustmentInput(raw: string, commandName = "adjust"): ParseOk | ParseError {
  const text = raw.trim().toLowerCase();
  if (!text) {
    return {
      ok: false,
      message: `命令错误：/${commandName} 需要时长参数，如 -15m 或 +10m`,
    };
  }

  let sign = 1;
  let cursor = 0;
  if (text.startsWith("+")) {
    cursor = 1;
  } else if (text.startsWith("-")) {
    sign = -1;
    cursor = 1;
  }

  let totalSeconds = 0;
  let matched = false;

  while (cursor < text.length) {
    while (cursor < text.length && /\s/.test(text[cursor])) {
      cursor += 1;
    }
    if (cursor >= text.length) break;

    SEGMENT_PATTERN.lastIndex = cursor;
    const match = SEGMENT_PATTERN.exec(text);
    if (!match || match.index !== cursor) {
      return {
        ok: false,
        message: `命令错误：/${commandName} 只支持 h/m/s，如 -15m、+1h30m、90s`,
      };
    }

    matched = true;
    totalSeconds += Number(match[1]) * UNIT_SECONDS[match[2] as keyof typeof UNIT_SECONDS];
    cursor = SEGMENT_PATTERN.lastIndex;
  }

  if (!matched || totalSeconds <= 0) {
    return {
      ok: false,
      message: `命令错误：/${commandName} 调整值必须大于 0 秒`,
    };
  }

  return {
    ok: true,
    deltaSeconds: sign * totalSeconds,
  };
}

export function formatFocusAdjustmentDelta(deltaSeconds: number): string {
  const prefix = deltaSeconds >= 0 ? "+" : "-";
  return `${prefix}${formatDurationParts(Math.abs(deltaSeconds)).join(" ")}`;
}

function formatDurationParts(totalSeconds: number): string[] {
  const hours = Math.floor(totalSeconds / UNIT_SECONDS.h);
  const minutes = Math.floor((totalSeconds % UNIT_SECONDS.h) / UNIT_SECONDS.m);
  const seconds = totalSeconds % UNIT_SECONDS.m;
  const parts: string[] = [];

  if (hours > 0) parts.push(`${hours}h`);
  if (minutes > 0) parts.push(`${minutes}m`);
  if (seconds > 0 || parts.length === 0) parts.push(`${seconds}s`);

  return parts;
}
