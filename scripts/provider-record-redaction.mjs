const REDACTED = "[REDACTED]";
const SENSITIVE_KEY = /(?:authorization|proxy-authorization|cookie|set-cookie|password|passwd|passcode|secret|token|api[-_]?key|client[-_]?secret|access[-_]?key|session|csrf|xsrf|oauth|code|state)/i;
const URL_KEY = /(?:code|state|token|secret|password|session|key|auth|ticket|credential)/i;

export function artifactDirectoryName(domain, now = new Date()) {
  const safeDomain = String(domain)
    .toLowerCase()
    .replace(/[^a-z0-9.-]+/g, "-")
    .replace(/^[.-]+|[.-]+$/g, "") || "provider";
  const part = (value) => String(value).padStart(2, "0");
  const timestamp = `${now.getFullYear()}${part(now.getMonth() + 1)}${part(now.getDate())}_${part(now.getHours())}${part(now.getMinutes())}${part(now.getSeconds())}`;
  return `${safeDomain}-${timestamp}`;
}

export function redactHeaders(headers, report) {
  return Object.fromEntries(
    Object.entries(headers ?? {}).map(([key, value]) => {
      if (SENSITIVE_KEY.test(key)) {
        report?.redacted("header", key);
        return [key, REDACTED];
      }
      return [key, redactString(String(value), report)];
    }),
  );
}

export function redactUrl(value, report) {
  try {
    const url = new URL(value);
    for (const key of [...url.searchParams.keys()]) {
      if (URL_KEY.test(key)) {
        url.searchParams.set(key, REDACTED);
        report?.redacted("query", key);
      }
    }
    return url.toString();
  } catch {
    return redactString(String(value), report);
  }
}

export function redactQueryParameters(parameters, report) {
  return (parameters ?? []).map(({ name, value }) => {
    if (URL_KEY.test(name)) {
      report?.redacted("query", name);
      return { name, value: REDACTED };
    }
    return { name, value: redactString(String(value), report) };
  });
}

export function redactHtml(html, report) {
  return redactString(String(html), report).replace(/<input\b[^>]*>/gi, (input) => {
    const sensitive = /\btype\s*=\s*["']?password\b/i.test(input)
      || /\b(?:name|id)\s*=\s*["'][^"']*(?:pass|token|secret|code|auth|session)[^"']*["']/i.test(input);
    if (!sensitive) return input;
    const next = input.replace(/\bvalue\s*=\s*(["'])[^"']*\1/i, `value="${REDACTED}"`);
    if (next !== input) report?.redacted("html-input", "value");
    return next;
  });
}

export function redactValue(value, report, key = "") {
  if (typeof value === "string" && /url$/i.test(key)) return redactUrl(value, report);
  if (SENSITIVE_KEY.test(key)) {
    report?.redacted("json", key);
    return REDACTED;
  }
  if (Array.isArray(value)) return value.map((item) => redactValue(item, report));
  if (value && typeof value === "object") {
    if ("name" in value && "value" in value && ("domain" in value || "expires" in value)) {
      report?.redacted("cookie", String(value.name));
      return Object.fromEntries(
        Object.entries(value).map(([childKey, child]) => [
          childKey,
          childKey === "value" ? REDACTED : redactValue(child, report, childKey),
        ]),
      );
    }
    return Object.fromEntries(
      Object.entries(value).map(([childKey, child]) => [childKey, redactValue(child, report, childKey)]),
    );
  }
  return typeof value === "string" ? redactString(value, report) : value;
}

export function redactString(value, report) {
  let output = String(value);
  const patterns = [
    [/\bBearer\s+[A-Za-z0-9._~+\/-]+=*/gi, "Bearer [REDACTED]"],
    [/\b(Basic)\s+[A-Za-z0-9+/]+=*/gi, "$1 [REDACTED]"],
    [/(["']?(?:password|passwd|secret|token|api[-_]?key|authorization|cookie)["']?\s*[:=]\s*["']?)[^\s,"';&<]+/gi, "$1[REDACTED]"],
    [/\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b/g, REDACTED],
  ];
  for (const [pattern, replacement] of patterns) {
    const before = output;
    output = output.replace(pattern, replacement);
    if (before !== output) report?.redacted("text", pattern.source);
  }
  return output;
}

export function scanSanitizedText(text) {
  const findings = [];
  const patterns = [
    ["bearer", /\bBearer\s+(?!\[REDACTED\])[A-Za-z0-9._~+\/-]{6,}/i],
    ["basic", /\bBasic\s+(?!\[REDACTED\])[A-Za-z0-9+/]{8,}=*/i],
    ["sensitive-assignment", /(?:password|secret|token|authorization|cookie)\s*[:=]\s*(?!["']?\[REDACTED\])["']?[^\s,"']{4,}/i],
    ["password-input", /<input\b[^>]*(?:type=["']?password|name=["'][^"']*pass)[^>]*value=["'](?!\[REDACTED\])[^"']+/i],
    ["jwt", /\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b/],
  ];
  for (const [name, pattern] of patterns) if (pattern.test(text)) findings.push(name);
  return findings;
}

export function createRedactionReport() {
  const counts = {};
  return {
    redacted(kind) {
      counts[kind] = (counts[kind] ?? 0) + 1;
    },
    snapshot(findings = []) {
      return {
        redactedCounts: { ...counts },
        sanitizedScanFindings: findings,
        readyForGit: findings.length === 0,
      };
    },
  };
}
