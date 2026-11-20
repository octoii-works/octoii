import { expect, it } from "bun:test";
import { createClient, type StopTarget } from "../../src/index.ts";

it("sends the exact stop target and reports a stale target without a second request", async () => {
	const sent: { id: string; method: string; params: unknown }[] = [];
	let receive = (_answer: unknown): void => undefined;
	const client = createClient({
		open: () => ({
			binaryType: "arraybuffer",
			send: (data) => sent.push(JSON.parse(data)),
			close: () => undefined,
			addEventListener: (type, listener) => {
				if (type === "message") receive = (answer) => listener({ data: JSON.stringify(answer) });
			},
		}),
		randomUUID: () => "unused",
		clock: { now: () => 0, setTimer: () => () => undefined },
		report: (error) => {
			throw error;
		},
	});
	try {
		receive({ event: "welcome", session: "session", scopes: ["threads.operate"], machine: { name: "fixture", workspace: "/work" }, extensions: [] });
		const targets: readonly StopTarget[] = [
			{ kind: "turn", id: "turn-A" },
			{ kind: "preparation", token: "attempt-A" },
			{ kind: "queued", deliveries: ["delivery-A", "delivery-B"] },
		];
		for (const target of targets) {
			const stopped = client.stop("thread-A", target);
			const request = sent.at(-1);
			if (request === undefined) throw new Error("Missing stop request");
			expect(request).toEqual({ id: request.id, method: "threads.stop", params: { thread: "thread-A", target } });
			receive({ id: request.id, result: {} });
			await stopped;
		}
		const stopped = client.stop("thread-A", { kind: "turn", id: "older-turn" });
		const request = sent.at(-1);
		if (request === undefined) throw new Error("Missing stop request");
		receive({ id: request.id, error: { code: "stale_stop", message: "This turn already ended." } });
		await expect(stopped).rejects.toMatchObject({ code: "stale_stop" });
		expect(sent).toHaveLength(4);
	} finally {
		client.close();
	}
});
