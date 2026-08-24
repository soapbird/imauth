import assert from "node:assert/strict";
import test from "node:test";

import {
  artifactDirectoryName,
  redactHeaders,
  redactHtml,
  redactQueryParameters,
  redactUrl,
  redactValue,
  scanSanitizedText,
} from "./provider-record-redaction.mjs";

test("builds a filesystem-safe domain and timestamp directory", () => {
  assert.equal(
    artifactDirectoryName("Login.Example.com", new Date(2026, 7, 24, 14, 5, 9)),
    "login.example.com-20260824_140509",
  );
  assert.deepEqual(
    redactQueryParameters([{ name: "code", value: "live-code" }, { name: "lang", value: "ko" }]),
    [{ name: "code", value: "[REDACTED]" }, { name: "lang", value: "ko" }],
  );
});

test("redacts authentication material without hiding ordinary network metadata", () => {
  assert.deepEqual(
    redactHeaders({
      authorization: "Bearer live-token",
      cookie: "session=live-cookie",
      "content-type": "application/json",
    }),
    {
      authorization: "[REDACTED]",
      cookie: "[REDACTED]",
      "content-type": "application/json",
    },
  );
  assert.equal(
    redactUrl("https://example.com/callback?code=live-code&lang=ko"),
    "https://example.com/callback?code=%5BREDACTED%5D&lang=ko",
  );
});

test("redacts form values and reports remaining secrets", () => {
  const html = '<input name="password" value="hunter2"><input name="q" value="naver">';
  const redacted = redactHtml(html);
  assert.match(redacted, /name="password" value="\[REDACTED\]"/);
  assert.match(redacted, /name="q" value="naver"/);
  assert.deepEqual(scanSanitizedText(redacted), []);
  assert.notDeepEqual(scanSanitizedText("Authorization: Bearer live-token"), []);
});

test("always removes browser cookie values", () => {
  const cookies = redactValue([
    { name: "NID_AUT", value: "live-cookie", domain: ".naver.com", path: "/" },
  ]);
  assert.equal(cookies[0].name, "NID_AUT");
  assert.equal(cookies[0].value, "[REDACTED]");
});
