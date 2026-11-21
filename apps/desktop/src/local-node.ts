/** Reach the node through its CLI, which owns control-socket discovery and service management. */
import { execFile } from "node:child_process";
import { access } from "node:fs/promises";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout } from "node:timers/promises";
import { promisify } from "node:util";

const execute = promisify(execFile);

export type BrowserHost = { readonly origin: string; readonly socket: string; readonly node: string };

/** Only a local node may supply the desktop page or receive a local pairing credential. */
export function localAddress(address: string): URL {
	const url = new URL(address);
	if (url.protocol !== "http:" || !["127.0.0.1", "localhost", "[::1]"].includes(url.hostname) || url.username || url.password) {
		throw new Error("The desktop requires a local Octoii HTTP address.");
	}
	return url;
}

/** CLI calls have a deadline and never include captured stdout or credentials in user-facing errors. */
export class LocalNode {
	readonly program: string;
	readonly home: string;
	readonly #version: string | undefined;

	constructor(program: string, home: string, version?: string) {
		this.program = program;
		this.home = home;
		this.#version = version;
	}

	async command(args: string[]): Promise<string> {
		try {
			const { stdout } = await execute(this.program, ["--home", this.home, ...args], { timeout: 5000, maxBuffer: 1024 * 1024 });
			return stdout;
		} catch (cause) {
			throw new Error(`Octoii ${args[0] ?? "command"} failed. Check the CLI installation and node logs.`, { cause });
		}
	}

	private async describe(): Promise<{ readonly address: URL; readonly browser: BrowserHost | undefined } | undefined> {
		let output: string;
		try {
			output = await this.command(["status", "--json"]);
		} catch (error) {
			// Only the CLI's normal failure can mean an offline node. Missing executables and timeouts are errors.
			const cause = error instanceof Error ? error.cause : undefined;
			if (cause instanceof Error && "code" in cause && cause.code === 1) return undefined;
			throw error;
		}
		const status: unknown = JSON.parse(output);
		if (typeof status !== "object" || status === null || !("loopback" in status) || typeof status.loopback !== "string") {
			throw new Error("Octoii returned an invalid node status.");
		}
		if (this.#version !== undefined && (!("version" in status) || status.version !== this.#version)) {
			throw new Error(
				"The running node version differs from this desktop app. Finish its tasks and update the node before retrying. It has not been restarted.",
			);
		}
		const address = localAddress(status.loopback);
		const browser =
			"control_socket" in status && typeof status.control_socket === "string" && "key" in status && typeof status.key === "string"
				? { origin: address.origin, socket: status.control_socket, node: status.key }
				: undefined;
		return { address, browser };
	}

	async status(): Promise<URL | undefined> {
		return (await this.describe())?.address;
	}

	/** The private socket must belong to the node this window actually reached. */
	async browserHost(address: URL): Promise<BrowserHost | undefined> {
		const status = await this.describe();
		return status?.address.origin === address.origin ? status.browser : undefined;
	}

	async connect(): Promise<URL> {
		const running = await this.status();
		if (running) return running;
		if (process.platform !== "darwin" || resolve(this.home) !== join(homedir(), ".octoii")) {
			throw new Error("Start this node with octoii serve, then retry. Automatic service setup currently supports the default macOS Home.");
		}
		const plist = join(homedir(), "Library/LaunchAgents/com.octoii.serve.plist");
		let action = "start";
		try {
			await access(plist);
		} catch (error) {
			if (!(error instanceof Error) || !("code" in error) || error.code !== "ENOENT") throw error;
			action = "install";
		}
		// launchd may already be starting the service. Its bootstrap can fail while the node is becoming ready.
		let startupError: unknown;
		try {
			await this.command(["service", action]);
		} catch (error) {
			startupError = error;
		}
		const deadline = Date.now() + 15000;
		while (Date.now() < deadline) {
			const ready = await this.status();
			if (ready) return ready;
			await setTimeout(250);
		}
		throw new Error("The local node did not become ready. Check ~/.octoii/var/log/serve.log and the installed service's Home, then retry.", {
			cause: startupError,
		});
	}

	async pairingCredential(origin: string): Promise<string> {
		const output = await this.command(["pair", "--label", "Octoii Desktop"]);
		const url = localAddress(output.trim().split(/\r?\n/)[0] ?? "");
		if (url.origin !== origin || url.pathname !== "/pair" || !url.hash) throw new Error("Octoii returned a pairing link for a different node.");
		return url.hash.slice(1);
	}
}
