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
	await writeFile(join(directory, "index.html"), "<html>bundled interface</html>");
	await writeFile(join(directory, "chunk.js"), "export {};");
	await writeFile(join(directory, ".private.html"), "private file");
});
afterAll(async () => {
	await rm(directory, { recursive: true, force: true });
});
const noNetwork = async (): Promise<Response> => {
	throw new Error("Static resources must not reach the node");
};

describe("bundled desktop assets", () => {
	test("serves the interface without a running HTTP server, including pairing routes", async () => {
		const html = process.platform === "darwin" ? '<html data-desktop="mac">bundled interface</html>' : "<html>bundled interface</html>";
		expect(await (await desktopResponse(new Request(origin), directory, origin, noNetwork)).text()).toBe(html);
		expect(await (await desktopResponse(new Request(`${origin}/pair#credential`), directory, origin, noNetwork)).text()).toBe(html);
	});
	test("returns script MIME types and supports HEAD without a body", async () => {
		const script = `${origin}/chunk.js?v=1`;
		expect(await (await desktopResponse(new Request(script), directory, origin, noNetwork)).text()).toBe("export {};");
		const result = await desktopResponse(new Request(script, { method: "HEAD" }), directory, origin, noNetwork);
		expect(result.headers.get("Content-Type")).toBe("text/javascript; charset=utf-8");
		expect(await result.text()).toBe("");
	});
	test("forwards a pairing submission and returns the session cookie", async () => {
		const body = '{"credential":"credential"}';
		const request = new Request(`${origin}/pair`, { method: "POST", headers: { "Content-Type": "application/json" }, body });
		const cookie = "octoii_session_7600=test-session; HttpOnly";
		const response = await desktopResponse(request, directory, origin, async (forwarded) => {
			expect(forwarded).toBe(request);
			expect(await forwarded.text()).toBe(body);
			return new Response(null, { headers: { "Set-Cookie": cookie } });
		});
		expect(response.headers.get("Set-Cookie")).toBe(cookie);
	});
	test("forwards a multipart Git thread submission intact", async () => {
		const form = new FormData();
		form.append("manifest", "{}");
		form.append("file-0", new Blob(["attachment"]), "input.txt");
		const request = new Request(`${origin}/git/threads`, { method: "POST", body: form });
		const response = Response.json({ thread: "accepted" });
		const result = await desktopResponse(request, directory, origin, async (forwarded) => {
			expect(forwarded).toBe(request);
			const received = await forwarded.formData();
			expect(received.get("manifest")).toBe("{}");
			const attachment = received.get("file-0");
			expect(attachment).toBeInstanceOf(Blob);
			if (attachment instanceof Blob) expect(await attachment.text()).toBe("attachment");
			return response;
		});
		expect(result).toBe(response);
	});
	test.each(["/auth/callback?code=code&state=state", "/machine", "/threads/thread/files/hash", "/ws", "/missing.js"])(
		"forwards GET %s to the node",
		async (path) => {
			const request = new Request(`${origin}${path}`);
			const response = new Response("node response");
			const result = await desktopResponse(request, directory, origin, async (forwarded) => {
				expect(forwarded).toBe(request);
				return response;
			});
			expect(result).toBe(response);
		},
	);
	test.each(["/..%2Fsecret.js", "/..%5Csecret.js", "/.private.html"])("does not resolve %s as a bundle asset", async (path) => {
		const absent = new Response(null, { status: 404 });
		expect(await desktopResponse(new Request(`${origin}${path}`), directory, origin, async () => absent)).toBe(absent);
	});
	test("does not replace another origin's page with the desktop interface", async () => {
		const remote = new Request("http://example.com/");
		expect(await (await desktopResponse(remote, directory, origin, async () => new Response("remote"))).text()).toBe("remote");
	});
});

test("startup pages need no scripts and escape failure messages", () => {
	expect(connectingPage()).toContain("Connecting to your node");
	expect(failedPage('<script>alert("x")</script>')).not.toContain("<script>");
	expect(failedPage("offline")).toContain('href="/retry"');
	expect(failedPage("offline")).toContain("default-src 'none'");
});
