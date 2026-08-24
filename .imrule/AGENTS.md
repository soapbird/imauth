# Provider onboarding evidence

Before adding a provider that is absent from `Platform::ALL`, the protobuf
`Platform`, and both SDK platform enums, record the real login surface:

```bash
./scripts/provider-record --url <https-login-url> --domain <provider-domain>
```

- Use the headed, isolated browser by default. Enter usernames, passwords,
  CAPTCHA answers, and 2FA only in the browser; never put them in CLI arguments,
  source, fixtures, or chat.
- During an interactive recording, create checkpoints at meaningful states with
  `checkpoint <name>`, then use `finish`. Use `abort` to preserve an incomplete
  record without claiming it as provider evidence.
- If imauth already owns the relevant browser, attach with
  `--cdp-url <endpoint>` and record the existing login flow instead of opening a
  duplicate session.
- Standard recording is intentionally small: inspect HTML checkpoints,
  cookie/storage state, HAR/network events, console warnings/errors, and the
  first-party JavaScript inventory before deciding login URL, cookie domains,
  session cookie name, and success/failure transitions.
- Use `--deep` only when standard evidence cannot explain the auth behavior. It
  additionally captures raw values, screenshots, JavaScript bodies, and the
  Playwright trace; do not use it as the routine development default.
- Deep raw artifacts under `datasource/records/*/raw/` are local and gitignored.
  Commit only `sanitized/`, `manifest.json`, `report.md`, and
  `redaction-report.json` after `readyForGit` is true and a human review finds no
  credentials, tokens, cookies, or personal data.
- After implementing a new provider or changing an existing provider, repeat a
  real recording and verify that the implemented domains, cookies, and auth
  transitions match the observed session. A static test alone is not sufficient.
