/** Local macOS package: use the active Cargo cache and the installed Electron runtime; never publish. */
import { cp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import type { Configuration } from "electron-builder";

if (process.platform !== "darwin") throw new Error("This packaging command targets the current macOS host.");
if (process.env["CARGO_BUILD_TARGET"]) throw new Error("Package on the target Mac without CARGO_BUILD_TARGET; cross-compilation is not configured.");
const desktop = fileURLToPath(new URL("../", import.meta.url));
const repository = fileURLToPath(new URL("../../../", import.meta.url));
const web = Bun.spawn(["bun", "scripts/build.ts"], { cwd: desktop, stdout: "inherit", stderr: "inherit" });
if ((await web.exited) !== 0) throw new Error("Desktop build failed");
const cargo = Bun.spawn(["cargo", "build", "--locked", "-p", "octoii-node", "--bin", "octoii", "--message-format=json-render-diagnostics"], {
	cwd: repository,
	stdout: "pipe",
	stderr: "inherit",
});
const output = await new Response(cargo.stdout).text();
if ((await cargo.exited) !== 0) throw new Error("Node build failed");
let executable: string | undefined;
for (const line of output.trim().split("\n")) {
	const artifact: unknown = JSON.parse(line);
	if (
		typeof artifact === "object" &&
		artifact !== null &&
		"reason" in artifact &&
		artifact.reason === "compiler-artifact" &&
		"executable" in artifact &&
		typeof artifact.executable === "string"
	)
		executable = artifact.executable;
}
if (!executable) throw new Error("Cargo did not report the node executable");
const runtime = join(desktop, "dist/runtime");
await rm(runtime, { recursive: true, force: true });
await mkdir(join(runtime, "bin"), { recursive: true });
await cp(executable, join(runtime, "bin/octoii"));
await cp(join(desktop, "dist/web"), join(runtime, "share/octoii/app"), { recursive: true });
const digest = createHash("sha256");
for (const entry of (await readdir(runtime, { recursive: true, withFileTypes: true })).sort((a, b) =>
	join(a.parentPath, a.name).localeCompare(join(b.parentPath, b.name)),
)) {
	if (entry.isFile()) {
		const path = join(entry.parentPath, entry.name).slice(runtime.length + 1);
		digest
			.update(path)
			.update("\0")
			.update(await readFile(join(runtime, path)));
	}
}
const versionProcess = Bun.spawn([executable, "--version"], { stdout: "pipe", stderr: "inherit" });
const version = (await new Response(versionProcess.stdout).text()).trim().replace(/^octoii /, "");
if ((await versionProcess.exited) !== 0 || !/^\d+\.\d+\.\d+(?:-[a-zA-Z0-9.-]+)?$/.test(version)) throw new Error("Could not read the built node version");
await writeFile(join(runtime, "manifest.json"), JSON.stringify({ id: digest.digest("hex"), version, profile: "dev" }));
const electron = Bun.spawn(["node", "-e", "require('electron')"], { cwd: desktop, stdout: "inherit", stderr: "inherit" });
if ((await electron.exited) !== 0) throw new Error("Electron runtime installation failed");
const configuration: Configuration = {
	appId: "com.octoii.desktop",
	productName: "Octoii",
	directories: { output: "out" },
	electronDist: join(desktop, "node_modules/electron/dist"),
	electronVersion: JSON.parse(await readFile(join(desktop, "node_modules/electron/package.json"), "utf8")).version,
	files: ["dist/main.js", "dist/web/**/*", "package.json"],
	extraResources: [{ from: "dist/runtime", to: "runtime" }],
	npmRebuild: false,
	mac: {
		identity: "-",
		notarize: false,
		hardenedRuntime: false,
		category: "public.app-category.developer-tools",
		icon: join(repository, "brand/icon-512.png"),
	},
};
const configurationPath = join(desktop, "dist/builder.json");
await writeFile(configurationPath, JSON.stringify(configuration));
const packaging = Bun.spawn(
	[
		"node",
		join(desktop, "node_modules/electron-builder/cli.js"),
		"--projectDir",
		desktop,
		"--config",
		configurationPath,
		"--mac",
		"--dir",
		`--${process.arch}`,
		"--publish",
		"never",
	],
	{ stdout: "inherit", stderr: "inherit" },
);
if ((await packaging.exited) !== 0) throw new Error("macOS packaging failed");
console.log("Built local Octoii.app (ad-hoc signed, not notarized; node uses the dev profile)");
