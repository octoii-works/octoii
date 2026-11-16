import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { desktopResponse } from "../src/assets.ts";
import { connectingPage, failedPage } from "../src/startup.ts";

let directory: string;
const origin = "http://127.0.0.1:7600";
beforeAll(async () => {
	directory = await mkdtemp(join(tmpdir(), "octoii-desktop-assets-"));
	await writeFile(join(directory, "index.html"), "bundled interface");
	await writeFile(join(directory, "chunk.js"), "export {};");
});
afterAll(async () => {
	await rm(directory, { recursive: true, force: true });
});
const noNetwork = async (): Promise<Response> => {
	throw new Error("Static resources must not reach the node");
};

describe("bundled desktop assets", () => {
	test("serves the interface without a running HTTP server, including pairing routes", async () => {
		expect(await (await desktopResponse(new Request(origin), directory, origin, noNetwork)).text()).toBe("bundled interface");
		expect(await (await desktopResponse(new Request(`${origin}/pair`), directory, origin, noNetwork)).text()).toBe("bundled interface");
	});
	test("returns script MIME types and supports HEAD without a body", async () => {
		const result = await desktopResponse(new Request(`${origin}/chunk.js`, { method: "HEAD" }), directory, origin, noNetwork);
		expect(result.headers.get("Content-Type")).toBe("text/javascript; charset=utf-8");
		expect(await result.text()).toBe("");
	});
	test("forwards pairing bodies and file requests unchanged", async () => {
		const request = new Request(`${origin}/pair`, { method: "POST", body: "credential" });
		await desktopResponse(request, directory, origin, async (forwarded) => {
			expect(forwarded).toBe(request);
			expect(await forwarded.text()).toBe("credential");
			return new Response(null, { status: 200 });
		});
		const file = new Request(`${origin}/threads/thread/files/hash`);
		await desktopResponse(file, directory, origin, async (forwarded) => {
			expect(forwarded).toBe(file);
			return new Response("attachment");
		});
	});
	test("never resolves encoded separators or hidden files outside the bundle", async () => {
		expect((await desktopResponse(new Request(`${origin}/..%2Fsecret.js`), directory, origin, noNetwork)).status).toBe(404);
		expect((await desktopResponse(new Request(`${origin}/..%5Csecret.js`), directory, origin, noNetwork)).status).toBe(404);
		expect((await desktopResponse(new Request(`${origin}/.private.html`), directory, origin, noNetwork)).status).toBe(404);
	});
	test("does not replace another origin's page with the desktop interface", async () => {
		const remote = new Request("http://example.com/");
		expect(await (await desktopResponse(remote, directory, origin, async () => new Response("remote"))).text()).toBe("remote");
	});
	test("missing assets return 404 instead of falling back to backend scripts", async () => {
		expect((await desktopResponse(new Request(`${origin}/missing.js`), directory, origin, noNetwork)).status).toBe(404);
	});
});

test("startup pages need no scripts and escape failure messages", () => {
	expect(connectingPage()).toContain("Connecting to your node");
	expect(failedPage('<script>alert("x")</script>')).not.toContain("<script>");
	expect(failedPage("offline")).toContain('href="/retry"');
	expect(failedPage("offline")).toContain("default-src 'none'");
});
