import { expect, it, mock, spyOn } from "bun:test";
import type { Config } from "@octoii-works/octoii-client";
import type { SendRequest } from "@octoii-works/octoii-client";
import { acceptThread, threadRequest, type ThreadWorkspace } from "../../src/screens/new-thread/request.ts";

const unreachable: SendRequest = async () => {
	throw new Error("Unexpected request");
};
const current: ThreadWorkspace = { kind: "current" };
const isolated: ThreadWorkspace = {
	kind: "isolated",
	path: "/project",
};
const attachment = { ref: "local", name: "notes.txt", media_type: "text/plain", size: 4, bytes: new Blob(["note"]) };
const draft = { text: "Task", feedback: [], attachments: [attachment] };
const submission = { key: "fixed", config: { workspace: "/project", model: "model" }, workspace: current, draft };

it.each([
	{ workspace: current, folder: "/project", config: { model: "model", workspace: "/project" } },
	{ workspace: current, folder: undefined, config: { model: "model" } },
	{ workspace: isolated, folder: "/project", config: { model: "model" } },
])("encodes the ordered message and source directory intent for each workspace", async ({ workspace, folder, config }) => {
	const input: Config = folder === undefined ? { model: "model" } : { model: "model", workspace: folder };
	const request = await threadRequest(unreachable, { ...submission, config: input, workspace });
	const form = await new Response(request.body, { headers: { "content-type": request.contentType } }).formData();
	expect([...form.keys()]).toEqual(["manifest", "file-0"]);
	expect(JSON.parse(String(form.get("manifest")))).toEqual({
		key: "fixed",
		config,
		workspace,
		parts: [
			{ type: "content", content: { type: "input_text", text: "Task" } },
			{ type: "attachment", index: 0, media_type: "text/plain", filename: "notes.txt" },
		],
	});
	const file = form.get("file-0");
	if (!(file instanceof Blob)) throw new Error("Missing attachment");
	expect(await file.text()).toBe("note");
});

it("rejects empty messages, out-of-limit parts, and manifests over the UTF-8 size limit", async () => {
	await expect(threadRequest(unreachable, { ...submission, draft: { ...draft, text: " ", attachments: [] } })).rejects.toThrow("between 1 and 128 parts");
	await expect(threadRequest(unreachable, { ...submission, draft: { ...draft, attachments: Array.from({ length: 128 }, () => attachment) } })).rejects.toThrow(
		"between 1 and 128 parts",
	);
	await expect(threadRequest(unreachable, { ...submission, draft: { ...draft, text: "界".repeat(350_000) } })).rejects.toThrow("1 MiB upload limit");
});

it("rejects an attachment without original bytes", async () => {
	const { bytes: _bytes, ...reference } = attachment;
	await expect(threadRequest(unreachable, { ...submission, draft: { ...draft, attachments: [reference] } })).rejects.toThrow("original bytes");
});

it("rejects oversized files and multipart bodies", async () => {
	const limit = 32 * 1024 * 1024;
	const bytes = new Blob([new Uint8Array(limit + 1)]);
	await expect(threadRequest(unreachable, { ...submission, draft: { ...draft, attachments: [{ ...attachment, bytes }] } })).rejects.toThrow("32 MiB");
	const full = { ...attachment, bytes: bytes.slice(0, limit) };
	await expect(threadRequest(unreachable, { ...submission, draft: { ...draft, attachments: [full, full] } })).rejects.toThrow("64 MiB");
});

it("rejects HTTP errors and malformed receipts", async () => {
	const request = await threadRequest(unreachable, submission);
	for (const status of [403, 422, 200]) {
		const send: SendRequest = async () => ({ status, contentType: "text/plain", bytes: new TextEncoder().encode("rejected") });
		await expect(acceptThread(send, request)).rejects.toThrow("rejected");
	}
});

it("preserves the original multipart header when the Blob MIME type changes", async () => {
	const original = Response.prototype.blob;
	const blob = spyOn(Response.prototype, "blob").mockImplementation(async function (this: Response) {
		return new Blob([await original.call(this)], { type: "application/octet-stream" });
	});
	try {
		const request = await threadRequest(unreachable, submission);
		const send = mock<SendRequest>(async ({ method, path, contentType, body }) => {
			expect(method).toBe("POST");
			expect(path).toBe("/threads");
			expect(contentType).toStartWith("multipart/form-data; boundary=");
			const form = await new Response(body, { headers: { "content-type": contentType ?? "" } }).formData();
			expect(form.get("manifest")).toContain('"key":"fixed"');
			return { status: 200, contentType: "application/json", bytes: new TextEncoder().encode('{"thread":"accepted"}') };
		});
		expect(await acceptThread(send, request)).toBe("accepted");
		expect(send).toHaveBeenCalledTimes(1);
	} finally {
		blob.mockRestore();
	}
});
