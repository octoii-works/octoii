import { afterEach, beforeEach, expect, test } from "bun:test";
import { chmod, mkdir, mkdtemp, readFile, readdir, rename, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { installBundledRuntime } from "../src/runtime.ts";

let folder: string;
let source: string;
let installed: string;
const id = "a".repeat(64);
beforeEach(async () => {
	folder = await mkdtemp(join(tmpdir(), "octoii-runtime-"));
	source = join(folder, "app/runtime");
	installed = join(folder, "installed");
	await mkdir(join(source, "bin"), { recursive: true });
	await mkdir(join(source, "share/octoii/app"), { recursive: true });
	await writeFile(join(source, "bin/octoii"), "node");
	await chmod(join(source, "bin/octoii"), 0o755);
	await writeFile(join(source, "share/octoii/app/index.html"), "web");
	await writeFile(join(source, "manifest.json"), JSON.stringify({ id, version: "0.0.1" }));
});
afterEach(async () => {
	await rm(folder, { recursive: true, force: true });
});

test("installed runtime remains usable after the app is moved", async () => {
	const runtime = await installBundledRuntime(source, installed);
	await rename(join(folder, "app"), join(folder, "moved-app"));
	expect(await readFile(runtime.program, "utf8")).toBe("node");
	expect(await readFile(join(installed, id, "share/octoii/app/index.html"), "utf8")).toBe("web");
	expect(runtime.version).toBe("0.0.1");
	expect(await readdir(installed)).toEqual([id]);
});

test("reopening never overwrites an existing runtime", async () => {
	const first = await installBundledRuntime(source, installed);
	await writeFile(join(source, "bin/octoii"), "changed");
	expect(await installBundledRuntime(source, installed)).toEqual(first);
	expect(await readFile(first.program, "utf8")).toBe("node");
});

test("a new runtime leaves the previous service executable in place", async () => {
	const previous = await installBundledRuntime(source, installed);
	await writeFile(join(source, "manifest.json"), JSON.stringify({ id: "b".repeat(64), version: "0.0.2" }));
	const next = await installBundledRuntime(source, installed);
	expect(next.program).not.toBe(previous.program);
	expect(await readFile(previous.program, "utf8")).toBe("node");
	expect(await readdir(installed)).toHaveLength(2);
});

test("incomplete payloads leave no installed or partial runtime", async () => {
	await rm(join(source, "share/octoii/app/index.html"));
	await expect(installBundledRuntime(source, installed)).rejects.toThrow();
	expect(await readdir(installed)).toEqual([]);
});

test("manifest identifiers cannot escape the runtime directory", async () => {
	await writeFile(join(source, "manifest.json"), JSON.stringify({ id: "../elsewhere", version: "0.0.1" }));
	await expect(installBundledRuntime(source, installed)).rejects.toThrow("manifest is invalid");
});
