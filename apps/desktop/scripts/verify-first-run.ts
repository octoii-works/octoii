/** Opt-in macOS smoke check: an isolated launchd job, fresh node Home and bundled runtime; no user service changes. */
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { cp, mkdir, mkdtemp, rename, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { setTimeout } from "node:timers/promises";
import { LocalNode } from "../src/local-node.ts";
import { installBundledRuntime } from "../src/runtime.ts";

if (process.platform !== "darwin" || !process.getuid) throw new Error("Run this check in a logged-in macOS account.");
const execute = promisify(execFile);
const temporary = await mkdtemp("/private/tmp/octoii-first-run-");
const label = `com.octoii.test.${temporary.split("-").at(-1)}`;
const domain = `gui/${process.getuid()}`;
const target = `${domain}/${label}`;
const plist = join(temporary, "test.plist");
const source = join(temporary, "app-runtime");
const home = join(temporary, "node");
const launchctl = async (...args: string[]): Promise<void> => {
	await execute("/bin/launchctl", args, { timeout: 10000 });
};
const xml = (value: string): string => value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
let loaded = false;
try {
	await cp(fileURLToPath(new URL("../dist/runtime", import.meta.url)), source, { recursive: true });
	const runtime = await installBundledRuntime(source, join(temporary, "installed"));
	const node = new LocalNode(runtime.program, home, runtime.version);
	assert.equal(await node.status(), undefined);
	await assert.rejects(node.connect(), /Start this node/);
	console.log("PASS: fresh Home is offline and reports an actionable setup error");
	// The production installer prepares logs before bootstrap. This isolated job uses a distinct label and port.
	await mkdir(join(home, "var/log"), { recursive: true });
	const log = join(home, "var/log/serve.log");
	await writeFile(
		plist,
		`<?xml version="1.0"?><plist version="1.0"><dict>
<key>Label</key><string>${label}</string>
<key>ProgramArguments</key><array>${[runtime.program, "--home", home, "serve", "--port", "0"].map((arg) => `<string>${xml(arg)}</string>`).join("")}</array>
<key>EnvironmentVariables</key><dict><key>PATH</key><string>/usr/bin:/bin:/usr/sbin:/sbin</string></dict>
<key>RunAtLoad</key><true/><key>KeepAlive</key><true/>
<key>StandardOutPath</key><string>${xml(log)}</string><key>StandardErrorPath</key><string>${xml(log)}</string>
</dict></plist>`,
	);
	async function ready(): Promise<URL> {
		const deadline = Date.now() + 15000;
		while (Date.now() < deadline) {
			const address = await node.status();
			if (address) return address;
			await setTimeout(100);
		}
		throw new Error("Isolated node did not become ready within 15 seconds.");
	}
	await launchctl("bootstrap", domain, plist);
	loaded = true;
	const address = await ready();
	assert.equal((await node.connect()).origin, address.origin);
	const page = await fetch(address, { signal: AbortSignal.timeout(5000) });
	assert.equal(page.status, 200);
	assert.match(await page.text(), /<html/);
	console.log("PASS: launchd starts the bundled node with a minimal PATH and serves bundled Web assets");
	const credential = await node.pairingCredential(address.origin);
	const redeem = (): Promise<Response> =>
		fetch(new URL("/pair", address), {
			method: "POST",
			headers: { "Content-Type": "application/json", Origin: address.origin },
			body: JSON.stringify({ credential }),
			signal: AbortSignal.timeout(5000),
		});
	const paired = await redeem();
	assert.equal(paired.status, 200);
	assert.ok(paired.headers.get("set-cookie")?.includes("HttpOnly"), "pairing establishes an HttpOnly cookie");
	assert.equal((await redeem()).status, 403);
	console.log("PASS: fresh pairing establishes an HttpOnly session and rejects credential reuse");
	const accessBefore: unknown = JSON.parse(await node.command(["access", "--json"]));
	await rename(source, join(temporary, "moved-app-runtime"));
	await launchctl("bootout", target);
	loaded = false;
	await launchctl("bootstrap", domain, plist);
	loaded = true;
	await ready();
	const accessAfter: unknown = JSON.parse(await node.command(["access", "--json"]));
	assert.deepEqual(accessAfter, accessBefore);
	console.log("PASS: the installed runtime restarts after moving its source payload and retains authorization");
} catch (error) {
	// Never print pairing credentials or control responses. The error alone identifies the failed assertion.
	console.error(`First-run smoke check failed: ${error instanceof Error ? error.message : "unknown error"}`);
	process.exitCode = 1;
} finally {
	if (loaded) await launchctl("bootout", target);
	await rm(temporary, { recursive: true, force: true });
}
console.log("Cleaned up the isolated launchd job and all temporary runtime/data files.");
