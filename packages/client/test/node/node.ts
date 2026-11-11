/**
 * The node itself, as a test runs it: `octoii serve` on a Home of the test's own, on the loopback, the client given a
 * session's bearer token its commands made — stopped and started again on the same port where a test drops the
 * connection, its Home kept.
 */
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { type Client, type ClientOptions, createClient, type FileRef } from "../../src/index.ts";

/** The program the workspace builds. */
const PROGRAM = resolve(import.meta.dir, "../../../../target/debug/octoii");

/** A node a test runs, on its Home and port. */
export class Node {
	readonly home: string;
	port = 0;
	#serving: ReturnType<typeof Bun.spawn> | undefined;

	private constructor(home: string) {
		this.home = home;
	}

	/** A node on a Home of its own, served till it is stopped. */
	static async start(): Promise<Node> {
		const node = new Node(mkdtempSync(join(tmpdir(), "octoii-client-")));
		await node.serve();
		return node;
	}

	/** Serves the node, on its port where it had one, else on one the system gives. */
	async serve(): Promise<void> {
		this.#serving = Bun.spawn([PROGRAM, "--home", this.home, "serve", "--port", String(this.port)], { stdout: "ignore", stderr: "ignore" });
		for (let attempt = 0; attempt < 200; attempt++) {
			const status = Bun.spawnSync([PROGRAM, "--home", this.home, "status", "--json"]);
			if (status.exitCode === 0) {
				this.port = Number(new URL(JSON.parse(status.stdout.toString()).loopback).port);
				return;
			}
			await Bun.sleep(25);
		}
		throw new Error("the node did not come up");
	}

	/** Stops the node: its sockets close with it. */
	async stop(): Promise<void> {
		this.#serving?.kill("SIGTERM");
		await this.#serving?.exited;
		this.#serving = undefined;
	}

	/** A bearer token for a session of every scope's. */
	token(): string {
		const made = Bun.spawnSync([PROGRAM, "--home", this.home, "token", "--label", "test"]);
		if (made.exitCode !== 0) throw new Error(made.stderr.toString());
		return made.stdout.toString().trim();
	}

	/** A client of the node's, holding a session of its own, as `options` says the rest. */
	client(options: Partial<ClientOptions> = {}): Client {
		const token = this.token();
		return createClient({
			open: (protocol) => new WebSocket(`ws://127.0.0.1:${this.port}/ws`, { protocols: [protocol], headers: { authorization: `Bearer ${token}` } }),
			randomUUID: () => crypto.randomUUID(),
			clock: { now: () => Date.now(), setTimer: (callback, ms) => clearTimeout.bind(undefined, setTimeout(callback, ms)) },
			report: (error) => {
				throw error;
			},
			keepFile: async (thread, bytes, name) => {
				const response = await fetch(`http://127.0.0.1:${this.port}/threads/${thread}/files?name=${encodeURIComponent(name)}`, {
					method: "POST",
					headers: { authorization: `Bearer ${token}`, "content-type": bytes.type || "application/octet-stream" },
					body: bytes,
				});
				if (!response.ok) throw new Error(await response.text());
				return (await response.json()) as FileRef;
			},
			...options,
		});
	}

	/** Stops the node and removes its Home. */
	async remove(): Promise<void> {
		await this.stop();
		rmSync(this.home, { recursive: true, force: true });
	}
}

/** Waits till `holds` is true, at most five seconds. */
export async function until(holds: () => boolean, what: string): Promise<void> {
	for (let waited = 0; waited < 5_000; waited += 10) {
		if (holds()) return;
		await Bun.sleep(10);
	}
	throw new Error(`waited in vain for ${what}`);
}
