import { createHash, randomUUID } from "node:crypto";
import { createInterface } from "node:readline/promises";
import { stdin, stdout } from "node:process";
import { constants } from "node:fs";
import { chmod, link, lstat, mkdir, open, readFile, readdir, realpath, rm } from "node:fs/promises";
import path from "node:path";
import { chromium } from "playwright";

import {
  artifactDirectoryName,
  createRedactionReport,
  redactHeaders,
  redactHtml,
  redactQueryParameters,
  redactString,
  redactUrl,
  redactValue,
  scanSanitizedText,
} from "./provider-record-redaction.mjs";

const options = parseArguments(process.argv.slice(2));
const targetUrl = new URL(options.url);
const providerDomain = (options.domain ?? targetUrl.hostname).toLowerCase().replace(/^\.+/, "");
const outputRoot = await prepareOutputRoot(path.resolve(options.outputRoot));
const recordRoot = containedPath(outputRoot, artifactDirectoryName(providerDomain));
const rawRoot = path.join(recordRoot, "raw");
const sanitizedRoot = path.join(recordRoot, "sanitized");
const report = createRedactionReport();
const consoleEvents = [];
const networkEntries = [];
const scriptArtifacts = [];
const checkpoints = [];
const pending = new Set();
const requestEntries = new WeakMap();
let scriptSequence = 0;

await createPrivateDirectory(recordRoot);
await ensurePrivateDirectory(sanitizedRoot);
if (options.deep) await ensurePrivateDirectory(rawRoot);

let browser;
let context;
let page;
let ownsBrowser = false;
let tracingStarted = false;
let traceCompleted = false;

try {
  ({ browser, context, page, ownsBrowser } = await openBrowser());
  installCapture(context);
  if (options.deep) {
    try {
      await context.tracing.start({ screenshots: true, snapshots: true, sources: true });
      tracingStarted = true;
    } catch (error) {
      console.warn(`Trace could not start: ${error.message}`);
    }
  }
  await navigateIfNeeded(page);
  await page.waitForLoadState("domcontentloaded", { timeout: 30_000 }).catch(() => {});

  if (options.autoFinish) {
    await page.waitForTimeout(1_000);
    await finish("completed");
  } else {
    await captureCheckpoint("initial");
    console.log("Recorder active. Use the browser normally.");
    console.log("Commands: checkpoint <name>, finish, abort");
    const prompt = createInterface({ input: stdin, output: stdout });
    for (;;) {
      const command = (await prompt.question("imauth-record> ")).trim();
      if (command === "finish") {
        prompt.close();
        await finish("completed");
        break;
      }
      if (command === "abort") {
        prompt.close();
        await finish("aborted");
        break;
      }
      if (command.startsWith("checkpoint ")) {
        await captureCheckpoint(command.slice("checkpoint ".length));
      } else {
        console.log("Expected: checkpoint <name>, finish, or abort");
      }
    }
  }
} finally {
  if (ownsBrowser) await browser?.close().catch(() => {});
}

async function openBrowser() {
  if (options.cdpUrl) {
    const connected = await chromium.connectOverCDP(options.cdpUrl);
    const connectedContext = connected.contexts()[0];
    if (!connectedContext) throw new Error("CDP browser has no accessible context");
    const matchingPage = connectedContext.pages().find((candidate) => {
      try {
        return new URL(candidate.url()).hostname === targetUrl.hostname;
      } catch {
        return false;
      }
    });
    return {
      browser: connected,
      context: connectedContext,
      page: matchingPage ?? await connectedContext.newPage(),
      ownsBrowser: false,
    };
  }
  const launched = await chromium.launch({ headless: options.headless });
  const launchedContext = await launched.newContext();
  return {
    browser: launched,
    context: launchedContext,
    page: await launchedContext.newPage(),
    ownsBrowser: true,
  };
}

async function navigateIfNeeded(candidate) {
  try {
    if (new URL(candidate.url()).hostname === targetUrl.hostname) return;
  } catch {}
  await candidate.goto(options.url, { waitUntil: "domcontentloaded", timeout: 30_000 });
}

function installCapture(browserContext) {
  for (const candidate of browserContext.pages()) installPageCapture(candidate);
  browserContext.on("page", installPageCapture);
  browserContext.on("request", (request) => {
    const entry = {
      startedDateTime: new Date().toISOString(),
      time: 0,
      request: {
        method: request.method(),
        url: request.url(),
        httpVersion: "HTTP/1.1",
        headers: toHarHeaders(request.headers()),
        queryString: [...new URL(request.url()).searchParams].map(([name, value]) => ({ name, value })),
        cookies: [],
        headersSize: -1,
        bodySize: request.postDataBuffer()?.length ?? 0,
        ...(request.postData() ? { postData: { mimeType: request.headers()["content-type"] ?? "", text: request.postData() } } : {}),
      },
      response: null,
      cache: {},
      timings: { send: 0, wait: 0, receive: 0 },
      _startedAt: Date.now(),
      _resourceType: request.resourceType(),
    };
    requestEntries.set(request, entry);
    networkEntries.push(entry);
  });
  browserContext.on("response", (response) => {
    track((async () => {
      const entry = requestEntries.get(response.request());
      if (!entry) return;
      entry.time = Date.now() - entry._startedAt;
      entry.timings.wait = entry.time;
      entry.response = {
        status: response.status(),
        statusText: response.statusText(),
        httpVersion: "HTTP/1.1",
        headers: toHarHeaders(response.headers()),
        cookies: [],
        content: { size: 0, mimeType: response.headers()["content-type"] ?? "" },
        redirectURL: response.headers().location ?? "",
        headersSize: -1,
        bodySize: -1,
      };
      if (entry._resourceType !== "script" || !isFirstParty(response.url())) return;
      const body = await response.body().catch(() => null);
      if (!body) return;
      const name = `script-${String(++scriptSequence).padStart(4, "0")}.js`;
      scriptArtifacts.push({
        name,
        url: response.url(),
        body,
        sha256: createHash("sha256").update(body).digest("hex"),
        bytes: body.length,
        contentType: response.headers()["content-type"] ?? "",
      });
    })());
  });
  browserContext.on("requestfailed", (request) => {
    const entry = requestEntries.get(request);
    if (entry) entry.response = { _error: request.failure()?.errorText ?? "request failed" };
  });
}

function installPageCapture(candidate) {
  candidate.on("console", (message) => {
    if (!["warning", "error", "assert"].includes(message.type())) return;
    consoleEvents.push({
      timestamp: new Date().toISOString(),
      type: message.type(),
      text: message.text(),
      location: message.location(),
      pageUrl: candidate.url(),
    });
  });
  candidate.on("pageerror", (error) => {
    consoleEvents.push({ timestamp: new Date().toISOString(), type: "pageerror", text: error.message, pageUrl: candidate.url() });
  });
}

async function captureCheckpoint(name) {
  const safeName = String(name).toLowerCase().replace(/[^a-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "") || "checkpoint";
  const uniqueName = `${String(checkpoints.length + 1).padStart(2, "0")}-${safeName}`;
  const activePages = context.pages().filter((candidate) => !candidate.isClosed());
  page = activePages.at(-1) ?? page;
  const [html, cookies, storage] = await Promise.all([
    page.content(),
    context.cookies(),
    page.evaluate(() => ({ localStorage: { ...localStorage }, sessionStorage: { ...sessionStorage } })).catch(() => ({ localStorage: {}, sessionStorage: {} })),
  ]);
  if (options.deep) {
    const rawDirectory = path.join(rawRoot, "checkpoints", uniqueName);
    await ensurePrivateDirectory(rawDirectory);
    await Promise.all([
      writePrivateFile(path.join(rawDirectory, "page.html"), html),
      writeJson(path.join(rawDirectory, "cookies.json"), cookies),
      writeJson(path.join(rawDirectory, "storage.json"), storage),
      page.screenshot({ fullPage: true }).then((content) => writePrivateFile(path.join(rawDirectory, "screenshot.png"), content)),
    ]);
  }
  checkpoints.push({ name: uniqueName, url: page.url(), title: await page.title(), html, cookies, storage });
  console.log(`Captured checkpoint: ${uniqueName}`);
}

async function finish(status) {
  if (status === "completed") await captureCheckpoint("final");
  await Promise.allSettled([...pending]);
  if (tracingStarted) {
    await writePrivateTrace(path.join(rawRoot, "trace.zip"));
    traceCompleted = true;
  }

  const har = buildHar();
  if (options.deep) {
    const scriptDirectory = path.join(rawRoot, "javascript");
    await ensurePrivateDirectory(scriptDirectory);
    await Promise.all([
      writeJson(path.join(rawRoot, "network.har"), har),
      writeJson(path.join(rawRoot, "console.json"), consoleEvents),
      ...scriptArtifacts.map(({ name, body }) => writePrivateFile(containedPath(scriptDirectory, name), body)),
    ]);
  }
  await writeSanitizedArtifacts(har, status);
  console.log(`Record written to ${recordRoot}`);
  if (process.exitCode) console.error("Sanitized secret scan failed; do not commit this record.");
}

async function writeSanitizedArtifacts(har, status) {
  for (const checkpoint of checkpoints) {
    const directory = path.join(sanitizedRoot, "checkpoints", checkpoint.name);
    await ensurePrivateDirectory(directory);
    await Promise.all([
      writePrivateFile(path.join(directory, "page.html"), redactHtml(checkpoint.html, report)),
      writeJson(path.join(directory, "state.json"), {
        url: redactUrl(checkpoint.url, report),
        title: redactString(checkpoint.title, report),
        cookies: redactValue(checkpoint.cookies, report),
        storage: redactValue(checkpoint.storage, report),
      }),
    ]);
  }
  await writeJson(path.join(sanitizedRoot, "network.har"), sanitizeHar(har));
  await writeJson(path.join(sanitizedRoot, "console.json"), redactValue(consoleEvents, report));
  await writeJson(
    path.join(sanitizedRoot, "javascript.json"),
    scriptArtifacts.map(({ url, sha256, bytes, contentType }) => ({
      url: redactUrl(url, report),
      sha256,
      bytes,
      contentType,
    })),
  );
  if (options.deep) {
    const scriptDirectory = path.join(sanitizedRoot, "javascript");
    await ensurePrivateDirectory(scriptDirectory);
    for (const artifact of scriptArtifacts) {
      await writePrivateFile(containedPath(scriptDirectory, artifact.name), redactString(artifact.body.toString("utf8"), report));
    }
  }

  const findings = [];
  for (const file of await walkFiles(sanitizedRoot)) {
    const text = await readFile(file, "utf8");
    for (const finding of scanSanitizedText(text)) findings.push({ file: path.relative(recordRoot, file), finding });
  }
  const redactionReport = report.snapshot(findings);
  const files = await checksums(sanitizedRoot);
  const manifest = {
    schemaVersion: 1,
    status,
    createdAt: new Date().toISOString(),
    sourceUrl: redactUrl(options.url, report),
    providerDomain,
    captureMode: options.cdpUrl ? "cdp" : "isolated",
    detailLevel: options.deep ? "deep" : "standard",
    checkpoints: checkpoints.map(({ name, url }) => ({ name, url: redactUrl(url, report) })),
    rawArtifacts: options.deep
      ? ["HTML", "screenshots", "cookies", "storage", "HAR", "console", "first-party JavaScript", ...(traceCompleted ? ["Playwright trace"] : [])]
      : [],
    sanitizedArtifacts: ["HTML checkpoints", "cookie/storage state", "HAR", "console warnings/errors", "JavaScript inventory", ...(options.deep ? ["JavaScript bodies"] : [])],
    sanitizedOmissions: options.deep ? ["screenshots", "Playwright trace"] : ["raw values", "screenshots", "Playwright trace", "JavaScript bodies"],
    sanitizedFiles: files,
    readyForGit: redactionReport.readyForGit,
  };
  await writeJson(path.join(recordRoot, "redaction-report.json"), redactionReport);
  await writeJson(path.join(recordRoot, "manifest.json"), manifest);
  await writePrivateFile(path.join(recordRoot, "report.md"), buildReport(manifest, redactionReport));
  if (!redactionReport.readyForGit) process.exitCode = 2;
}

function sanitizeHar(har) {
  const sanitized = redactValue(har, report);
  for (const entry of sanitized.log.entries) {
    entry.request.url = redactUrl(entry.request.url, report);
    entry.request.queryString = redactQueryParameters(entry.request.queryString, report);
    entry.request.headers = entry.request.headers.map(({ name, value }) => ({ name, value: redactHeaders({ [name]: value }, report)[name] }));
    if (entry.response?.headers) entry.response.headers = entry.response.headers.map(({ name, value }) => ({ name, value: redactHeaders({ [name]: value }, report)[name] }));
  }
  return sanitized;
}

function buildHar() {
  return {
    log: {
      version: "1.2",
      creator: { name: "imauth-provider-recorder", version: "1" },
      pages: [],
      entries: networkEntries.map(({ _startedAt, _resourceType, ...entry }) => ({ ...entry, response: entry.response ?? { _error: "response unavailable" } })),
    },
  };
}

function buildReport(manifest, redactionReport) {
  const deepNote = manifest.detailLevel === "deep" ? "The explicit `--deep` opt-in retains raw artifacts locally under the private `raw/` directory." : "Standard mode retains no raw artifacts; use `--deep` only when the sanitized evidence is insufficient.";
  return `# Provider session record\n\n- Status: ${manifest.status}\n- Source: ${manifest.sourceUrl}\n- Domain: ${manifest.providerDomain}\n- Browser mode: ${manifest.captureMode}\n- Detail: ${manifest.detailLevel}\n- Checkpoints: ${manifest.checkpoints.length}\n- Sanitized ready for Git: ${manifest.readyForGit}\n- Redactions: ${JSON.stringify(redactionReport.redactedCounts)}\n\n${deepNote} Review \`sanitized/\`, \`manifest.json\`, and \`redaction-report.json\` before committing.\n`;
}

function parseArguments(arguments_) {
  const result = { outputRoot: "datasource/records", headless: false, autoFinish: false, deep: false };
  for (let index = 0; index < arguments_.length; index += 1) {
    const argument = arguments_[index];
    if (argument === "--headless") result.headless = true;
    else if (argument === "--auto-finish") result.autoFinish = true;
    else if (argument === "--deep") result.deep = true;
    else if (argument === "--url") result.url = arguments_[++index];
    else if (argument === "--domain") result.domain = arguments_[++index];
    else if (argument === "--cdp-url") result.cdpUrl = arguments_[++index];
    else if (argument === "--output-root") result.outputRoot = arguments_[++index];
    else throw new Error(`Unknown argument: ${argument}`);
  }
  if (!result.url) throw new Error("--url is required");
  const url = new URL(result.url);
  if (!["http:", "https:"].includes(url.protocol)) throw new Error("--url must use http or https");
  return result;
}

function isFirstParty(value) {
  const hostname = new URL(value).hostname;
  return hostname === providerDomain || hostname.endsWith(`.${providerDomain}`);
}

function toHarHeaders(headers) {
  return Object.entries(headers).map(([name, value]) => ({ name, value }));
}

function track(promise) {
  pending.add(promise);
  promise.finally(() => pending.delete(promise));
}

async function writeJson(file, value) {
  await writePrivateFile(file, `${JSON.stringify(value, null, 2)}\n`);
}

async function walkFiles(directory) {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const fullPath = path.join(directory, entry.name);
    if (entry.isSymbolicLink()) throw new Error(`Refusing symbolic link in recorder output: ${fullPath}`);
    if (entry.isDirectory()) files.push(...await walkFiles(fullPath));
    else if (entry.isFile()) files.push(fullPath);
    else throw new Error(`Refusing non-regular recorder output: ${fullPath}`);
  }
  return files.sort();
}

async function prepareOutputRoot(directory) {
  const parsed = path.parse(directory);
  let current = parsed.root;
  for (const part of directory.slice(parsed.root.length).split(path.sep).filter(Boolean)) {
    current = path.join(current, part);
    try {
      const metadata = await lstat(current);
      if (metadata.isSymbolicLink()) {
        throw new Error(`Refusing symbolic link in output root path: ${current}`);
      }
      if (!metadata.isDirectory()) {
        throw new Error(`Recorder output root component is not a directory: ${current}`);
      }
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
      await createPrivateDirectory(current);
    }
  }
  await hardenPrivateDirectory(directory);
  return realpath(directory);
}

function containedPath(root, ...parts) {
  const candidate = path.resolve(root, ...parts);
  const relative = path.relative(root, candidate);
  if (relative.startsWith(`..${path.sep}`) || relative === ".." || path.isAbsolute(relative)) {
    throw new Error(`Recorder output path escapes its root: ${candidate}`);
  }
  return candidate;
}

async function createPrivateDirectory(directory) {
  await mkdir(directory, { mode: 0o700 });
  const metadata = await lstat(directory);
  if (metadata.isSymbolicLink() || !metadata.isDirectory()) {
    throw new Error(`Refusing unsafe recorder directory: ${directory}`);
  }
  if (process.platform !== "win32") await chmod(directory, 0o700);
  await hardenPrivateDirectory(directory);
}

async function ensurePrivateDirectory(directory) {
  const relative = path.relative(recordRoot, containedPath(recordRoot, path.relative(recordRoot, directory)));
  let current = recordRoot;
  for (const part of relative.split(path.sep).filter(Boolean)) {
    current = containedPath(recordRoot, path.relative(recordRoot, current), part);
    try {
      await hardenPrivateDirectory(current);
    } catch (error) {
      if (error?.code !== "ENOENT") throw error;
      await createPrivateDirectory(current);
    }
  }
}

async function hardenPrivateDirectory(directory) {
  if (process.platform === "win32") {
    const metadata = await lstat(directory);
    if (metadata.isSymbolicLink() || !metadata.isDirectory()) {
      throw new Error(`Refusing unsafe recorder directory: ${directory}`);
    }
    return;
  }
  const flags = constants.O_RDONLY | constants.O_DIRECTORY | (constants.O_NOFOLLOW ?? 0);
  const handle = await open(directory, flags);
  try {
    const metadata = await handle.stat();
    if (!metadata.isDirectory()) throw new Error(`Refusing unsafe recorder directory: ${directory}`);
    await handle.chmod(0o700);
  } finally {
    await handle.close();
  }
}

async function writePrivateFile(file, content) {
  containedPath(recordRoot, path.relative(recordRoot, file));
  const parent = path.dirname(file);
  await ensurePrivateDirectory(parent);
  const flags = constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | (constants.O_NOFOLLOW ?? 0);
  const handle = await open(file, flags, 0o600);
  try {
    await handle.writeFile(content);
    await handle.chmod(0o600);
  } finally {
    await handle.close();
  }
}

async function writePrivateTrace(file) {
  containedPath(recordRoot, path.relative(recordRoot, file));
  const temporary = containedPath(rawRoot, `.trace-${randomUUID()}.tmp`);
  try {
    await context.tracing.stop({ path: temporary });
    const metadata = await lstat(temporary);
    if (metadata.isSymbolicLink() || !metadata.isFile()) {
      throw new Error(`Refusing unsafe trace output: ${temporary}`);
    }
    if (process.platform !== "win32") await chmod(temporary, 0o600);
    const handle = await open(temporary, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
    try {
      await handle.chmod(0o600);
    } finally {
      await handle.close();
    }
    await link(temporary, file);
  } finally {
    await rm(temporary, { force: true });
  }
}

async function checksums(directory) {
  const result = [];
  for (const file of await walkFiles(directory)) {
    const content = await readFile(file);
    result.push({ path: path.relative(recordRoot, file), sha256: createHash("sha256").update(content).digest("hex"), bytes: content.length });
  }
  return result;
}
