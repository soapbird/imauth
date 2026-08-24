import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
	mkdirSync,
	mkdtempSync,
	readdirSync,
	readFileSync,
	realpathSync,
	rmSync,
	statSync,
	symlinkSync,
	writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const scriptsDirectory = path.dirname(fileURLToPath(import.meta.url));
const fakePlaywright = `
import { mkdirSync, readdirSync, symlinkSync, writeFileSync } from "node:fs";
import path from "node:path";
function maybePlantSymlink() {
  const target = process.env.IMAUTH_TEST_SYMLINK_TARGET;
  const outputRoot = process.env.IMAUTH_TEST_OUTPUT_ROOT;
  if (!target || !outputRoot) return;
  const record = readdirSync(outputRoot)[0];
  const rawRoot = path.join(outputRoot, record, "raw");
  mkdirSync(rawRoot, { recursive: true });
  symlinkSync(target, path.join(rawRoot, "checkpoints"), "dir");
}
const page = {
  on() {},
  isClosed() { return false; },
  url() { return "https://example.test/login"; },
  async content() { maybePlantSymlink(); return '<input name="password" value="super-secret">'; },
  async evaluate() { return { localStorage: { token: "super-secret" }, sessionStorage: {} }; },
  async title() { return "Example"; },
  async screenshot() {
    const target = process.env.IMAUTH_TEST_FILE_SYMLINK_TARGET;
    const outputRoot = process.env.IMAUTH_TEST_OUTPUT_ROOT;
    if (target && outputRoot) {
      const record = readdirSync(outputRoot)[0];
      symlinkSync(target, path.join(outputRoot, record, "raw", "checkpoints", "01-final", "screenshot.png"));
    }
    return Buffer.from("fake-png");
  },
  async waitForLoadState() {},
  async waitForTimeout() {},
  async goto() {},
};

const context = {
  pages() { return [page]; },
  on() {},
  async newPage() { return page; },
  async cookies() { return [{ name: "session", value: "super-secret", domain: ".example.test", path: "/" }]; },
  tracing: {
    async start() {},
    async stop({ path: tracePath }) { writeFileSync(tracePath, "fake-trace"); },
  },
};
export const chromium = {
  async launch() {
    return {
      contexts() { return [context]; },
      async newContext() { return context; },
      async close() {},
    };
  },
};
`;
function fixture() {
	const root = realpathSync(
		mkdtempSync(path.join(tmpdir(), "imauth-recorder-security-")),
	);
	const runtime = path.join(root, "runtime");
	const outputRoot = path.join(root, "records");
	mkdirSync(path.join(runtime, "node_modules", "playwright"), {
		recursive: true,
	});
	writeFileSync(path.join(runtime, "package.json"), '{"type":"module"}\n');
	writeFileSync(
		path.join(runtime, "node_modules", "playwright", "package.json"),
		'{"type":"module","exports":"./index.mjs"}\n',
	);
	writeFileSync(
		path.join(runtime, "node_modules", "playwright", "index.mjs"),
		fakePlaywright,
	);
	for (const name of ["provider-record.mjs", "provider-record-redaction.mjs"]) {
		writeFileSync(
			path.join(runtime, name),
			readFileSync(path.join(scriptsDirectory, name)),
		);
	}
	return { root, runtime, outputRoot };
}

function runRecorder({
	deep = false,
	domain = "example.test",
	outputRoot,
	runtime,
	symlinkTarget,
	fileSymlinkTarget,
}) {
	const arguments_ = [
		"provider-record.mjs",
		"--url",
		"https://example.test/login",
		"--domain",
		domain,
		"--output-root",
		outputRoot,
		"--headless",
		"--auto-finish",
	];
	if (deep) arguments_.push("--deep");
	return spawnSync(process.execPath, arguments_, {
		cwd: runtime,
		encoding: "utf8",
		env: {
			...process.env,
			IMAUTH_TEST_OUTPUT_ROOT: outputRoot,
			...(symlinkTarget ? { IMAUTH_TEST_SYMLINK_TARGET: symlinkTarget } : {}),
			...(fileSymlinkTarget
				? { IMAUTH_TEST_FILE_SYMLINK_TARGET: fileSymlinkTarget }
				: {}),
		},
		umask: 0,
	});
}

function onlyRecord(outputRoot) {
	const entries = readdirSync(outputRoot);
	assert.equal(entries.length, 1);
	return path.join(outputRoot, entries[0]);
}

function assertModes(root) {
	const visit = (current) => {
		const stat = statSync(current);
		assert.equal(
			stat.mode & 0o777,
			stat.isDirectory() ? 0o700 : 0o600,
			current,
		);
		if (stat.isDirectory())
			for (const name of readdirSync(current)) visit(path.join(current, name));
	};
	visit(root);
}

test("uses restrictive modes and keeps raw data only after explicit deep opt-in", () => {
	const standard = fixture();
	const deep = fixture();
	try {
		const standardRun = runRecorder({
			domain: "../../../../escape",
			outputRoot: standard.outputRoot,
			runtime: standard.runtime,
		});
		assert.equal(standardRun.status, 0, standardRun.stderr);
		assert.equal(statSync(standard.outputRoot).mode & 0o777, 0o700);
		const standardRecord = onlyRecord(standard.outputRoot);
		assert.match(path.basename(standardRecord), /^escape-\d{8}_\d{6}$/);
		assert.equal(readdirSync(standardRecord).includes("raw"), false);
		assertModes(standardRecord);

		const deepRun = runRecorder({
			deep: true,
			outputRoot: deep.outputRoot,
			runtime: deep.runtime,
		});
		assert.equal(deepRun.status, 0, deepRun.stderr);
		const deepRecord = onlyRecord(deep.outputRoot);
		const readRecord = (...parts) =>
			readFileSync(path.join(deepRecord, ...parts), "utf8");
		assert.equal(
			readRecord("raw", "checkpoints", "01-final", "cookies.json").includes(
				"super-secret",
			),
			true,
		);
		assert.equal(
			readRecord("sanitized", "checkpoints", "01-final", "state.json").includes(
				"super-secret",
			),
			false,
		);
		assertModes(deepRecord);
	} finally {
		rmSync(standard.root, { recursive: true, force: true });
		rmSync(deep.root, { recursive: true, force: true });
	}
});

test("rejects a symlink output root", () => {
	const current = fixture();
	try {
		const redirected = path.join(current.root, "redirected");
		mkdirSync(redirected);
		symlinkSync(redirected, current.outputRoot, "dir");
		const result = runRecorder({
			outputRoot: current.outputRoot,
			runtime: current.runtime,
		});
		assert.notEqual(result.status, 0);
		assert.deepEqual(readdirSync(redirected), []);
	} finally {
		rmSync(current.root, { recursive: true, force: true });
	}
});

test("rejects a symlink planted inside the raw output tree", () => {
	const current = fixture();
	try {
		const redirected = path.join(current.root, "redirected");
		mkdirSync(redirected);
		const result = runRecorder({
			deep: true,
			outputRoot: current.outputRoot,
			runtime: current.runtime,
			symlinkTarget: redirected,
		});
		assert.notEqual(result.status, 0);
		assert.deepEqual(readdirSync(redirected), []);
	} finally {
		rmSync(current.root, { recursive: true, force: true });
	}
});

test("does not follow or replace a symlink planted at a sensitive file boundary", () => {
	const current = fixture();
	try {
		const redirected = path.join(current.root, "redirected-secret");
		writeFileSync(redirected, "unchanged");
		const result = runRecorder({
			deep: true,
			outputRoot: current.outputRoot,
			runtime: current.runtime,
			fileSymlinkTarget: redirected,
		});
		assert.notEqual(result.status, 0);
		assert.equal(readFileSync(redirected, "utf8"), "unchanged");
	} finally {
		rmSync(current.root, { recursive: true, force: true });
	}
});
