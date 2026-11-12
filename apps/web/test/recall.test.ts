/** What the person sent before, called back into the composer. */
import { describe, expect, it } from "bun:test";
import { earlier, later } from "../src/core/recall.ts";

describe("what was sent, called back", () => {
	const sent = ["first", "second"];

	it("goes back one message at a time from the latest, and forward to what was being written", () => {
		const back = earlier(sent, undefined, "half");
		expect(back).toEqual({ text: "second", recalled: { at: 1, draft: "half" } });
		const further = earlier(sent, back?.recalled, "second");
		expect(further).toEqual({ text: "first", recalled: { at: 0, draft: "half" } });
		expect(earlier(sent, further?.recalled, "first")).toBeUndefined();
		expect(later(sent, further?.recalled)).toEqual({ text: "second", recalled: { at: 1, draft: "half" } });
		expect(later(sent, back?.recalled)).toEqual({ text: "half", recalled: undefined });
		expect(later(sent, undefined)).toBeUndefined();
		expect(earlier([], undefined, "")).toBeUndefined();
	});
});
