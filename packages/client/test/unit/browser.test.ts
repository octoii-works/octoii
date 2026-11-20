/** The capture wrapper sends one explicit RPC and leaves thread delivery to the user. */
import { describe, expect, it } from "bun:test";
import { CALL_PATH, createClient, type Command, type ElementFeedback, type PickElement } from "../../src/index.ts";

describe("picking a browser element", () => {
	it("keeps capture separate from delivery and returns the persisted feedback unchanged", async () => {
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
		const sent: Command[] = [];
		const client = createClient({
			open: () => ({ binaryType: "arraybuffer", send: () => undefined, close: () => undefined, addEventListener: () => undefined }),
			send: async (request) => {
				expect(request.path).toBe(CALL_PATH);
				sent.push(JSON.parse(String(request.body)));
				const first = sent.length === 1;
				const reply = first ? { result: capture } : { error: { code: "page_changed", message: "The page changed; select again." } };
				return { status: first ? 200 : 422, contentType: "application/json", bytes: new TextEncoder().encode(JSON.stringify(reply)) };
			},
			randomUUID: () => "unused",
			clock: { now: () => 0, setTimer: () => () => undefined },
			report: (error) => {
				throw error;
			},
		});
		try {
			expect(await client.pickElement(pick)).toEqual(capture);
			expect(sent).toEqual([{ method: "browser.pick", params: pick }]);
			await expect(client.pickElement(pick)).rejects.toMatchObject({ code: "page_changed" });
			expect(sent).toHaveLength(2);
		} finally {
			client.close();
		}
	});
});
