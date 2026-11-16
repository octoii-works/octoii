import { describe, expect, test } from "bun:test";
import { LocalNode, localAddress } from "../src/local-node.ts";
import { pairLocalNode } from "../src/pairing.ts";
import type { Cookie } from "electron";
import { homedir } from "node:os";
import { join } from "node:path";

class TestNode extends LocalNode {
	readonly calls: string[][] = [];
	constructor(readonlyOutput: string, version?: string) {
		super("unused", "/isolated-test-home", version);
		this.output = readonlyOutput;
	}
	output: string;
	override async command(args: string[]): Promise<string> {
		this.calls.push(args);
		return this.output;
	}
}

const address = new URL("http://127.0.0.1:7600");
const cookie: Cookie = {
	name: "octoii_session_7600",
	value: "test-session",
	domain: "127.0.0.1",
	path: "/",
	secure: false,
	httpOnly: true,
	session: false,
	sameSite: "strict",
};

describe("local node discovery", () => {
	test("uses the node's discovered port without starting a second service", async () => {
		const node = new TestNode('{"loopback":"http://127.0.0.1:8700"}');
		expect((await node.connect()).origin).toBe("http://127.0.0.1:8700");
		expect(node.calls).toEqual([["status", "--json"]]);
	});
	test.skipIf(process.platform !== "darwin")("waits for readiness even when launchd already has the service loaded", async () => {
		class StartingNode extends LocalNode {
			readonly calls: string[][] = [];
			override async command(args: string[]): Promise<string> {
				this.calls.push(args);
				if (this.calls.length === 1) throw new Error("offline", { cause: Object.assign(new Error("offline"), { code: 1 }) });
				if (args[0] === "service") throw new Error("already loaded");
				return '{"loopback":"http://127.0.0.1:8700"}';
			}
		}
		const node = new StartingNode("unused", join(homedir(), ".octoii"));
		expect((await node.connect()).port).toBe("8700");
		expect(node.calls.map((args) => args[0])).toEqual(["status", "service", "status"]);
	});
	test("version mismatches do not restart a running node", async () => {
		const node = new TestNode('{"loopback":"http://127.0.0.1:7600","version":"0.0.1"}', "0.0.2");
		await expect(node.connect()).rejects.toThrow("version differs");
		expect(node.calls).toEqual([["status", "--json"]]);
	});
	test("rejects malformed status without starting a service", async () => {
		const node = new TestNode('{"loopback":7}');
		await expect(node.connect()).rejects.toThrow("invalid node status");
		expect(node.calls).toHaveLength(1);
	});
	test("missing executable does not trigger service installation", async () => {
		await expect(new LocalNode("/nonexistent/octoii-test", "/isolated-test-home").connect()).rejects.toThrow("CLI installation");
	});
	test("never accepts network or credential-bearing origins", () => {
		expect(() => localAddress("https://example.com")).toThrow();
		expect(() => localAddress("http://127.0.0.1.example.com")).toThrow();
		expect(() => localAddress("http://user:password@127.0.0.1")).toThrow();
	});
	test("reads the CLI's local link while ignoring a following remote link", async () => {
		const node = new TestNode("http://127.0.0.1:7600/pair#credential\nhttps://app.octoii.com/pair#remote\n");
		expect(await node.pairingCredential(address.origin)).toBe("credential");
	});
	test("does not redeem a pairing link for a different node", async () => {
		const node = new TestNode("http://127.0.0.1:7601/pair#credential");
		await expect(node.pairingCredential(address.origin)).rejects.toThrow("different node");
	});
});

describe("desktop pairing", () => {
	test("preserves an existing cookie instead of silently granting new access", async () => {
		const node = new TestNode("");
		await pairLocalNode(node, address, {
			cookies: { get: async () => [cookie], flushStore: async () => {} },
			fetch: () => {
				throw new Error("Must not pair again");
			},
		});
		expect(node.calls).toHaveLength(0);
	});
	test("redeems once in the window session and flushes the HttpOnly cookie", async () => {
		const node = new TestNode("http://127.0.0.1:7600/pair#credential");
		let paired = false;
		let flushed = false;
		await pairLocalNode(node, address, {
			cookies: {
				get: async () => (paired ? [cookie] : []),
				flushStore: async () => {
					flushed = true;
				},
			},
			fetch: async (url, options) => {
				expect(url).toBe("http://127.0.0.1:7600/pair");
				expect(options?.redirect).toBe("error");
				expect(options?.body).toBe('{"credential":"credential"}');
				paired = true;
				return new Response("{}", { status: 200 });
			},
		});
		expect(flushed).toBe(true);
	});
	test("fails if a successful response did not establish a cookie", async () => {
		const node = new TestNode("http://127.0.0.1:7600/pair#credential");
		await expect(
			pairLocalNode(node, address, {
				cookies: { get: async () => [], flushStore: async () => {} },
				fetch: async () => new Response("{}", { status: 200 }),
			}),
		).rejects.toThrow("did not establish");
	});
});
