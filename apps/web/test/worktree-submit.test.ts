import { capturedElement } from "./fixtures.ts";
import { expect, it, mock, spyOn } from "bun:test";
import { acceptWorktree, type WorktreeSubmission } from "../src/extensions/git/submit.ts";
import type { SendRequest } from "../src/components/state/reach.ts";

const submission: WorktreeSubmission = {
	key: "fixed",
	config: { workspace: "/project", model: "model" },
	worktree: { path: "/project", destination: "/task", branch: "task", base: "a".repeat(40) },
	draft: { text: "Task", feedback: [], attachments: [{ ref: "local", name: "notes.txt", media_type: "text/plain", size: 4, bytes: new Blob(["note"]) }] },
};

it("uploads an ordered manifest and original bytes over the common node transport", async () => {
	const send = mock<SendRequest>(async () => ({
		status: 200,
		contentType: "application/json",
		bytes: new TextEncoder().encode(JSON.stringify({ thread: "accepted", start: {} })),
	}));
	expect(await acceptWorktree(send, submission)).toBe("accepted");
	const request = send.mock.calls[0]?.[0];
	expect(request?.path).toBe("/git/threads");
	expect(request?.method).toBe("POST");
	if (!(request?.body instanceof Blob)) throw new Error("Expected serialized multipart");
	const form = await new Response(request.body, { headers: { "content-type": request.contentType ?? "" } }).formData();
	expect(JSON.parse(String(form.get("manifest")))).toEqual({
		key: "fixed",
		config: { model: "model" },
		worktree: submission.worktree,
		parts: [
			{ type: "content", content: { type: "input_text", text: "Task" } },
			{ type: "attachment", index: 0, media_type: "text/plain", filename: "notes.txt" },
		],
	});
	const file = form.get("file-0");
	if (!(file instanceof Blob)) throw new Error("Missing attachment");
	expect(await file.text()).toBe("note");
});

it("keeps rejected and malformed acknowledgements recoverable", async () => {
	for (const status of [403, 422, 200]) {
		const send = mock<SendRequest>(async () => ({ status, contentType: "text/plain", bytes: new TextEncoder().encode("rejected") }));
		await expect(acceptWorktree(send, submission)).rejects.toThrow("rejected");
	}
});

it("does not submit attachment references without their original bytes", async () => {
	const send = mock<SendRequest>(async () => {
		throw new Error("Unexpected request");
	});
	await expect(
		acceptWorktree(send, {
			...submission,
			draft: { ...submission.draft, attachments: [{ ref: "sha256:old", name: "old.txt", media_type: "text/plain", size: 4 }] },
		}),
	).rejects.toThrow("original bytes");
	expect(send).not.toHaveBeenCalled();
});

it("copies browser evidence from its source thread in message order", async () => {
	const feedback = capturedElement();
	const send = mock<SendRequest>(async (request) => ({
		status: 200,
		contentType: undefined,
		bytes: new TextEncoder().encode(request.method === "GET" ? "screenshot" : JSON.stringify({ thread: "accepted" })),
	}));
	await acceptWorktree(send, { ...submission, draft: { ...submission.draft, feedback: [feedback] } });
	expect(send.mock.calls[0]?.[0].path).toContain(`/threads/${feedback.thread}/files/`);
	const body = send.mock.calls[1]?.[0].body;
	if (!(body instanceof Blob)) throw new Error("Missing multipart");
	const form = await new Response(body, { headers: { "content-type": send.mock.calls[1]?.[0].contentType ?? "" } }).formData();
	const manifest = String(form.get("manifest"));
	expect(manifest.indexOf("notes.txt")).toBeLessThan(manifest.indexOf("Historical browser context"));
	expect(manifest.indexOf("Historical browser context")).toBeLessThan(manifest.indexOf("selection.png"));
	const screenshot = form.get("file-1");
	if (!(screenshot instanceof Blob)) throw new Error("Missing screenshot");
	expect(await screenshot.text()).toBe("screenshot");
});

it("preserves the response content type when the Blob MIME type changes", async () => {
	const original = Response.prototype.blob;
	const blob = spyOn(Response.prototype, "blob").mockImplementation(async function (this: Response) {
		return new Blob([await original.call(this)], { type: "application/octet-stream" });
	});
	try {
		const send = mock<SendRequest>(async (request) => {
			expect(request.contentType).toStartWith("multipart/form-data; boundary=");
			const form = await new Response(request.body, { headers: { "content-type": request.contentType ?? "" } }).formData();
			expect(form.get("manifest")).toContain('"key":"fixed"');
			return { status: 200, contentType: "application/json", bytes: new TextEncoder().encode('{"thread":"accepted"}') };
		});
		expect(await acceptWorktree(send, submission)).toBe("accepted");
	} finally {
		blob.mockRestore();
	}
});
