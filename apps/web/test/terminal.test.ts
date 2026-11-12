/** A terminal as the interface names it on its tab. */
import { describe, expect, it } from "bun:test";
import { endedOf, terminalName } from "../src/core/terminal.ts";

describe("a terminal's name", () => {
	it("is the title its program set, else its number in the thread", () => {
		const size = { rows: 24, cols: 80 };
		expect(terminalName({ id: "t1", number: 2, size, title: "  npm run dev " })).toBe("npm run dev");
		expect(terminalName({ id: "t1", number: 2, size, title: "" })).toBe("Terminal 2");
		expect(terminalName({ id: "t1", number: 3, size, title: "   " })).toBe("Terminal 3");
	});
});

describe("a command line's end, as its thread is told it", () => {
	it("is read from the terminals' note, the last screen after it", () => {
		expect(endedOf("[terminal 2, which ran `npm run dev`, ended: exit 1]\nerror: in use\n[exit 1]")).toEqual({
			number: 2,
			line: "npm run dev",
			how: "exit 1",
			ok: false,
			screen: "error: in use\n[exit 1]",
		});
		expect(endedOf("[terminal 1, which ran `sleep 1`, ended: exit 0]")?.ok).toBe(true);
		expect(endedOf("[terminal 3, which ran `x`, ended: ended by signal 15]")?.how).toBe("signal 15");
		expect(endedOf("Fix it")).toBeUndefined();
	});
});
