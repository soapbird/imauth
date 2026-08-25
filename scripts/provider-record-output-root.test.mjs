import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
	chmodSync,
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
import { writeFileSync } from "node:fs";
const page = {
  on() {}, isClosed() { return false; }, url() { return "https://example.test/login"; },
  async content() { return "<main>safe</main>"; }, async evaluate() { return { localStorage: {}, sessionStorage: {} }; },
  async title() { return "Example"; }, async screenshot() { return Buffer.from("png"); },
  async waitForLoadState() {}, async waitForTimeout() {}, async goto() {},
};
const context = {
  pages() { return [page]; }, on() {}, async newPage() { return page; }, async cookies() { return []; },
  tracing: {
    async start() { if (process.env.IMAUTH_TEST_TRACE_START_FAILURE) throw new Error("trace unavailable"); },
    async stop({ path }) { writeFileSync(path, "trace"); },
  },
};
export const chromium = { async launch() { return { async newContext() { return context; }, async close() {} }; } };
`;
const recorderWrapper = `
const requested = process.env.IMAUTH_TEST_CHILD_UMASK;
if (requested) {
  process.umask(Number.parseInt(requested, 8));
  process.stderr.write(\`IMAUTH_TEST_CHILD_UMASK=\${process.umask().toString(8).padStart(4, "0")}\\n\`);
}
await import("./provider-record.mjs");
`;

function fixture() {
	const root = realpathSync(
		mkdtempSync(path.join(tmpdir(), "imauth-recorder-root-security-")),
	);
	const runtime = path.join(root, "runtime");
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
	writeFileSync(path.join(runtime, "run-recorder.mjs"), recorderWrapper);
	for (const name of ["provider-record.mjs", "provider-record-redaction.mjs"]) {
		writeFileSync(
			path.join(runtime, name),
			readFileSync(path.join(scriptsDirectory, name)),
		);
	}
	return { root, runtime };
}

function runRecorder(
	runtime,
	outputRoot,
	umask = 0,
	deep = false,
	environment = {},
) {
	const arguments_ = [
		"run-recorder.mjs",
		"--url",
		"https://example.test/login",
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
			...environment,
			IMAUTH_TEST_CHILD_UMASK: umask.toString(8),
		},
	});
}

function assertPrivateTree(current) {
	const metadata = statSync(current);
	assert.equal(
		metadata.mode & 0o777,
		metadata.isDirectory() ? 0o700 : 0o600,
		current,
	);
	if (metadata.isDirectory()) {
		for (const name of readdirSync(current)) {
			assertPrivateTree(path.join(current, name));
		}
	}
}

test("hardens a pre-existing permissive output root", () => {
	const current = fixture();
	try {
		const outputRoot = path.join(current.root, "records");
		mkdirSync(outputRoot);
		chmodSync(outputRoot, 0o777);
		const result = runRecorder(current.runtime, outputRoot);
		assert.equal(result.status, 0, result.stderr);
		assert.equal(statSync(outputRoot).mode & 0o777, 0o700);
	} finally {
		rmSync(current.root, { recursive: true, force: true });
	}
});

test("creates usable private output with a restrictive umask", {
	skip: process.platform === "win32",
}, () => {
	const current = fixture();
	try {
		const outputRoot = path.join(current.root, "records");
		const result = runRecorder(current.runtime, outputRoot, 0o777, true);
		assert.equal(result.status, 0, result.stderr);
		assert.match(result.stderr, /IMAUTH_TEST_CHILD_UMASK=0777/);
		assert.equal(statSync(outputRoot).mode & 0o777, 0o700);
		const [record] = readdirSync(outputRoot);
		const recordRoot = path.join(outputRoot, record);
		assertPrivateTree(recordRoot);
		const trace = path.join(recordRoot, "raw", "trace.zip");
		assert.equal(statSync(trace).mode & 0o777, 0o600);
		const manifest = JSON.parse(
			readFileSync(path.join(recordRoot, "manifest.json")),
		);
		assert.equal(manifest.rawArtifacts.includes("Playwright trace"), true);
	} finally {
		rmSync(current.root, { recursive: true, force: true });
	}
});

test("rejects an existing output path reached through an intermediate symlink", () => {
	const current = fixture();
	try {
		const redirected = path.join(current.root, "redirected");
		const linkedParent = path.join(current.root, "linked-parent");
		mkdirSync(path.join(redirected, "records"), { recursive: true });
		symlinkSync(redirected, linkedParent, "dir");
		const result = runRecorder(
			current.runtime,
			path.join(linkedParent, "records"),
		);
		assert.notEqual(result.status, 0);
		assert.deepEqual(readdirSync(path.join(redirected, "records")), []);
	} finally {
		rmSync(current.root, { recursive: true, force: true });
	}
});

test("does not advertise a trace when tracing never completes", () => {
	const current = fixture();
	try {
		const outputRoot = path.join(current.root, "records");
		const result = runRecorder(current.runtime, outputRoot, 0, true, {
			IMAUTH_TEST_TRACE_START_FAILURE: "1",
		});
		assert.equal(result.status, 0, result.stderr);
		const [record] = readdirSync(outputRoot);
		const recordRoot = path.join(outputRoot, record);
		assert.equal(
			readdirSync(path.join(recordRoot, "raw")).includes("trace.zip"),
			false,
		);
		const manifest = JSON.parse(
			readFileSync(path.join(recordRoot, "manifest.json")),
		);
		assert.equal(manifest.rawArtifacts.includes("Playwright trace"), false);
	} finally {
		rmSync(current.root, { recursive: true, force: true });
	}
});
