/** The last desktop route, keyed by the node's Home or explicitly selected origin, never by a pairing URL. */
import { readFileSync, renameSync, writeFileSync } from "node:fs";
import { routeAddress, routeOf } from "../../web/src/core/route.ts";

export class LastLocation {
	readonly #file: string;
	constructor(file: string) {
		this.#file = file;
	}

	restore(address: URL, node: string): URL {
		const restored = new URL(address);
		// An explicit deep link or pairing link always wins over remembered navigation.
		if (address.pathname !== "/" || address.hash || address.search) return restored;
		try {
			const kept: unknown = JSON.parse(readFileSync(this.#file, "utf8"));
			if (typeof kept === "object" && kept !== null && "node" in kept && kept.node === node && "hash" in kept && typeof kept.hash === "string") {
				restored.hash = routeAddress(routeOf(kept.hash));
			}
		} catch (error) {
			if (!(error instanceof SyntaxError) && !(error instanceof URIError) && !(error instanceof Error && "code" in error && error.code === "ENOENT")) {
				console.warn("Could not restore desktop navigation.");
			}
		}
		return restored;
	}

	remember(address: URL, origin: string, node: string): void {
		if (address.origin !== origin || address.pathname !== "/" || address.search) return;
		try {
			const hash = routeAddress(routeOf(address.hash));
			// A tiny atomic write on navigation survives both window closure and application exit without a flush race.
			writeFileSync(`${this.#file}.tmp`, JSON.stringify({ node, hash }), { mode: 0o600 });
			renameSync(`${this.#file}.tmp`, this.#file);
		} catch {
			console.warn("Could not save desktop navigation.");
		}
	}
}
