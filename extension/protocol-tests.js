// Bundles and runs the wire-protocol tests under node's test runner. They talk
// to the broker, not to VS Code, so they stay out of `vscode-test` (which would
// boot an Electron host just to check byte layouts).
const esbuild = require("esbuild");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const OUT_DIR = path.join(__dirname, "dist", "test");
const SUITES = ["conformance", "broker", "format"];

const bundles = [];
for (const suite of SUITES) {
	const outfile = path.join(OUT_DIR, `${suite}.test.cjs`);
	bundles.push(outfile);
	esbuild.buildSync({
		entryPoints: [path.join(__dirname, "src", "test", "protocol", `${suite}.ts`)],
		outfile,
		bundle: true,
		platform: "node",
		format: "cjs",
		sourcemap: "inline",
		// `node:test` must stay external so the runner sees the same module instance.
		external: ["node:test"],
	});
}

const result = spawnSync(process.execPath, ["--test", ...bundles], { stdio: "inherit" });
process.exit(result.status ?? 1);
