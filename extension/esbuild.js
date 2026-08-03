const esbuild = require("esbuild");
const fs = require("node:fs");
const path = require("node:path");

const production = process.argv.includes('--production');
const watch = process.argv.includes('--watch');

/**
 * Copy the compiled broker binary into bin/<platform>-<arch>/ so it ships inside
 * the VSIX and BrokerManager can spawn it. Resilient: warns (does not fail) if the
 * binary hasn't been built yet.
 * @type {import('esbuild').Plugin}
 */
const copyBrokerBinaryPlugin = {
	name: 'copy-broker-binary',
	setup(build) {
		build.onEnd(() => {
			const exe = process.platform === 'win32' ? 'broker.exe' : 'broker';
			const profile = production ? 'release' : 'debug';
			const src = path.resolve(__dirname, '..', 'broker', 'target', profile, exe);
			const destDir = path.resolve(__dirname, 'bin', `${process.platform}-${process.arch}`);
			const dest = path.join(destDir, exe);
			if (!fs.existsSync(src)) {
				console.warn(`[copy-broker] ${src} not found — run 'cargo build${production ? ' --release' : ''}' in broker/ (skipping)`);
				return;
			}
			fs.mkdirSync(destDir, { recursive: true });
			const staged = `${dest}.staged`;
			try {
				fs.copyFileSync(src, staged);
				if (process.platform !== 'win32') {
					fs.chmodSync(staged, 0o755);
				}
				fs.renameSync(staged, dest);
			} catch (err) {
				fs.rmSync(staged, { force: true });
				console.warn(`[copy-broker] could not stage ${dest}: ${err.message} (skipping)`);
				return;
			}
			console.log(`[copy-broker] ${src} -> ${dest}`);
		});
	},
};

/**
 * @type {import('esbuild').Plugin}
 */
const esbuildProblemMatcherPlugin = {
	name: 'esbuild-problem-matcher',

	setup(build) {
		build.onStart(() => {
			console.log('[watch] build started');
		});
		build.onEnd((result) => {
			result.errors.forEach(({ text, location }) => {
				console.error(`✘ [ERROR] ${text}`);
				console.error(`    ${location.file}:${location.line}:${location.column}:`);
			});
			console.log('[watch] build finished');
		});
	},
};

async function main() {
	const ctx = await esbuild.context({
		entryPoints: [
			'src/extension.ts'
		],
		bundle: true,
		format: 'cjs',
		minify: production,
		sourcemap: !production,
		sourcesContent: false,
		platform: 'node',
		outfile: 'dist/extension.js',
		external: ['vscode'],
		logLevel: 'silent',
		plugins: [
			copyBrokerBinaryPlugin,
			/* add to the end of plugins array */
			esbuildProblemMatcherPlugin,
		],
	});
	if (watch) {
		await ctx.watch();
	} else {
		await ctx.rebuild();
		await ctx.dispose();
	}
}

main().catch(e => {
	console.error(e);
	process.exit(1);
});
