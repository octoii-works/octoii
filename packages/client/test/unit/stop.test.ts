import { expect, it } from "bun:test";
import { CALL_PATH, createClient, type Command, type StopTarget } from "../../src/index.ts";

it("sends the exact stop target and reports a stale target without a second request", async () => {
	const sent: Command[] = [];
	const client = createClient({
		open: () => ({
			binaryType: "arraybuffer",
			send: () => {
				throw new Error("Stops use HTTP");
			},
			close: () => undefined,
			addEventListener: () => undefined,
		}),
		send: async (request) => {
			expect(request.path).toBe(CALL_PATH);
			expect(request.method).toBe("POST");
			const command: Command = JSON.parse(String(request.body));
			sent.push(command);
			const stale = command.method === "threads.stop" && command.params.target.kind === "turn" && command.params.target.id === "older-turn";
			const reply = stale ? { error: { code: "stale_stop", message: "This turn already ended." } } : { result: {} };
			return { status: stale ? 422 : 200, contentType: "application/json", bytes: new TextEncoder().encode(JSON.stringify(reply)) };
		},
		randomUUID: () => "unused",
		clock: { now: () => 0, setTimer: () => () => undefined },
		report: (error) => {
			throw error;
		},
	});
	try {
		const targets: readonly StopTarget[] = [
			{ kind: "turn", id: "turn-A" },
			{ kind: "preparation", token: "attempt-A" },
			{ kind: "queued", deliveries: ["delivery-A", "delivery-B"] },
		];
		for (const target of targets) {
			await client.stop("thread-A", target);
			expect(sent.at(-1)).toEqual({ method: "threads.stop", params: { thread: "thread-A", target } });
		}
		await expect(client.stop("thread-A", { kind: "turn", id: "older-turn" })).rejects.toMatchObject({ code: "stale_stop" });
		expect(sent).toHaveLength(4);
	} finally {
		client.close();
	}
});
