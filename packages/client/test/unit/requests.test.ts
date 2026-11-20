import { expect, test } from "bun:test";
import { CALL_PATH, createClient, type Command } from "../../src/index.ts";

test("a keyed delivery retries the same command after a lost HTTP reply", async () => {
	const sent: Command[] = [];
	const client = createClient({
		open: () => ({ binaryType: "arraybuffer", send: () => undefined, close: () => undefined, addEventListener: () => undefined }),
		send: async (request) => {
			expect(request.path).toBe(CALL_PATH);
			sent.push(JSON.parse(String(request.body)));
			if (sent.length === 1) throw new Error("HTTP response lost");
			return { status: 200, contentType: "application/json", bytes: new TextEncoder().encode('{"result":{"delivery":"committed"}}') };
		},
		randomUUID: () => "same-key",
		clock: {
			now: () => 0,
			setTimer: (callback) => {
				queueMicrotask(callback);
				return () => undefined;
			},
		},
		report: (error) => {
			throw error;
		},
	});
	try {
		expect(client.status.value.status).toBe("connecting");
		expect(await client.deliver("thread", [{ type: "input_text", text: "hello" }])).toBe("committed");
		expect(sent).toHaveLength(2);
		expect(sent[0]).toEqual(sent[1]);
		expect(sent[1]?.params).toMatchObject({ key: "same-key" });
	} finally {
		client.close();
	}
});
