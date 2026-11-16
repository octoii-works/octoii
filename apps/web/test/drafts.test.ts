import { describe, expect, it } from "bun:test";
import { afterSubmission, type Draft, draftKey, EMPTY_DRAFT } from "../src/core/drafts.ts";
import { capturedElement } from "./fixtures.ts";

describe("a draft's place", () => {
	it("is its thread, or a new thread of its project or of none", () => {
		expect(draftKey({ thread: "t1" })).toBe("t1");
		expect(draftKey({ project: "/work/octoii" })).toBe("new:/work/octoii");
		expect(draftKey({ project: undefined })).toBe("new:");
	});
});

describe("a submitted draft snapshot", () => {
	const file = { ref: "sha256:file", media_type: "text/plain", name: "notes.txt", size: 10 };
	const feedback = capturedElement();
	const sent: Draft = { text: "Fix this element", attachments: [file], feedback: [feedback] };

	it("clears acknowledged text, files and captures together", () => {
		expect(afterSubmission(sent, sent)).toEqual(EMPTY_DRAFT);
	});

	it("keeps newer text and files or captures added while sending", () => {
		const nextFile = { ...file, ref: "sha256:another" };
		const nextCapture = capturedElement({ captured_at: feedback.captured_at + 1 });
		const current = { text: "Fix this element and its label", attachments: [file, nextFile], feedback: [feedback, nextCapture] };
		expect(afterSubmission(current, sent)).toEqual({ text: current.text, attachments: [nextFile], feedback: [nextCapture] });
		expect(sent.feedback).toEqual([feedback]);
	});

	it("preserves a file or capture removed and added again after the snapshot, even with the same saved reference", () => {
		const reattached = { ...file };
		const selectedAgain = { ...feedback };
		expect(afterSubmission({ text: sent.text, attachments: [reattached], feedback: [selectedAgain] }, sent)).toEqual({
			text: "",
			attachments: [reattached],
			feedback: [selectedAgain],
		});
	});
});
