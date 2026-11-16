/** The capture wrapper sends one explicit RPC and leaves thread delivery to the user. */
import { describe, expect, it } from "bun:test";
import { createClient, type ElementFeedback, type PickElement } from "../../src/index.ts";

describe("picking a browser element", () => {
	it("keeps capture separate from delivery and returns the persisted feedback unchanged", async () => {
		const sent: { id: string; method: string; params: unknown }[] = [];
		let receive = (_data: unknown): void => undefined;
		const client = createClient({
			open: () => ({
				binaryType: "arraybuffer",
				send: (data) => sent.push(JSON.parse(data)),
				close: () => undefined,
				addEventListener: (type, listener) => {
					if (type === "message") receive = (data) => listener({ data: JSON.stringify(data) });
				},
			}),
			randomUUID: () => "unused",
			clock: { now: () => 0, setTimer: () => () => undefined },
			report: (error) => {
				throw error;
			},
		});
		receive({
			event: "welcome",
			session: "session",
			scopes: ["browser.operate", "threads.operate"],
			machine: { name: "fixture", workspace: "/work" },
			extensions: [],
		});
		const pick: PickElement = {
			thread: "thread-A",
			page: "page-A",
			x: 20,
			y: 30,
			expected_url: "https://example.com",
			view: { width: 800, height: 600, scale: 2 },
		};
		const capture: ElementFeedback = {
			thread: pick.thread,
			page: pick.page,
			url: pick.expected_url,
			document: "doc",
			captured_at: 123,
			viewport: { width: 800, height: 600, scale: 2, scroll_x: 0, scroll_y: 0 },
			point: { x: 20, y: 30 },
			element: { tag: "button", label: "Submit", rect: { x: 10, y: 20, width: 40, height: 20 } },
			screenshot: { ref: "sha256:captured", media_type: "image/png" },
		};
		const result = client.pickElement(pick);
		const request = sent[0];
		if (request === undefined) throw new Error("The capture request");
		expect(sent).toEqual([{ id: request.id, method: "browser.pick", params: pick }]);
		receive({ id: request.id, result: capture });
		expect(await result).toEqual(capture);
		expect(sent).toHaveLength(1);

		const failure = client.pickElement(pick);
		const failed = sent[1];
		if (failed === undefined) throw new Error("The second capture request");
		receive({ id: failed.id, error: { code: "page_changed", message: "The page changed; select again." } });
		await expect(failure).rejects.toMatchObject({ code: "page_changed" });
		expect(sent).toHaveLength(2);
		client.close();
	});
});
