import { expect, it, mock } from "bun:test";
import type { NodeAnswer, NodeRequest, SendRequest } from "@octoii-works/octoii-client";
import { EMPTY_DRAFT } from "../../src/core/drafts.ts";
import type { ThreadWorkspace } from "../../src/screens/new-thread/request.ts";
import { NewThread, type PendingThread, type PendingThreadStore } from "../../src/screens/new-thread/submission.ts";
import { capturedElement } from "../fixtures.ts";

function store(): PendingThreadStore {
	let saved: PendingThread | undefined;
	let locked: Promise<unknown> = Promise.resolve();
	return {
		read: async () => (saved === undefined ? undefined : structuredClone(saved)),
		exclusive: (action) => {
			const next = locked.then(action);
			locked = next.catch(() => undefined);
			return next;
		},
		write: async (pending) => {
			saved = structuredClone(pending);
		},
		remove: async (key) => {
			if (saved?.key === key) saved = undefined;
		},
	};
}

const accepted: NodeAnswer = { status: 200, contentType: "application/json", bytes: new TextEncoder().encode('{"thread":"accepted"}') };
const config = { workspace: "/project/subfolder" };
const draft = {
	...EMPTY_DRAFT,
	text: "First task",
	attachments: [{ ref: "local", name: "note.txt", media_type: "text/plain", size: 4, bytes: new Blob(["note"]) }],
};

function upload(sent: NodeRequest | undefined): { readonly contentType: string; readonly body: Blob } {
	if (sent?.method !== "POST" || sent.path !== "/threads" || !(sent.body instanceof Blob) || sent.contentType === undefined) {
		throw new Error("Expected a multipart POST /threads");
	}
	return { contentType: sent.contentType, body: sent.body };
}

async function formOf(request: ReturnType<typeof upload>): Promise<FormData> {
	return new Response(request.body, { headers: { "content-type": request.contentType } }).formData();
}

const current = { kind: "current" } as const;
const submissions: readonly ThreadWorkspace[] = [current, { kind: "isolated", path: "/project" }, { kind: "isolated", path: "/project", base: "a".repeat(40) }];

it.each(submissions)("sends the original workspace intent directly and replays its bytes after a lost response", async (workspace) => {
	const kept = store();
	const send = mock<SendRequest>(async () => accepted);
	send.mockRejectedValueOnce(new Error("lost answer"));
	await expect(new NewThread(send, kept).send(config, workspace, draft)).rejects.toThrow("lost answer");
	expect(send).toHaveBeenCalledTimes(1);
	const saved = await kept.read();
	if (saved === undefined) throw new Error("The first request was never saved");
	expect(saved.workspace).toEqual(workspace);
	expect(saved.project).toBe(workspace.kind === "isolated" ? workspace.path : config.workspace);
	expect(saved.draft.text).toBe(draft.text);
	expect(saved.thread).toBeUndefined();
	const original = upload(send.mock.calls[0]?.[0]);
	const form = await formOf(original);
	const manifest = JSON.parse(String(form.get("manifest")));
	expect(manifest.key).toBe(saved.key);
	expect(manifest.workspace).toEqual(workspace);
	const file = form.get("file-0");
	if (!(file instanceof Blob)) throw new Error("Missing attachment");
	expect(await file.text()).toBe("note");
	const restored = new NewThread(send, kept);
	const receipt = { key: saved.key, thread: "accepted" };
	expect(await restored.resume(saved.key)).toEqual(receipt);
	const replay = upload(send.mock.calls[1]?.[0]);
	expect(replay.contentType).toBe(original.contentType);
	expect(await replay.body.arrayBuffer()).toEqual(await original.body.arrayBuffer());
	expect(send).toHaveBeenCalledTimes(2);
	expect((await kept.read())?.thread).toBe("accepted");
	expect(await new NewThread(send, kept).resume(saved.key)).toEqual(receipt);
	expect(send).toHaveBeenCalledTimes(2);
	await restored.clear(saved.key);
	expect(await restored.read()).toBeUndefined();
});

it("freezes evidence before the first POST and replays it after the source is gone", async () => {
	const feedback = capturedElement();
	const kept = store();
	let first = true;
	const send = mock<SendRequest>(async (request) => {
		if (request.method === "GET") {
			if (!first) throw new Error("Source thread was removed");
			expect(request.path).toBe(`/threads/${feedback.thread}/files/${feedback.screenshot.ref.replace(/^sha256:/, "")}`);
			return { status: 200, contentType: "image/png", bytes: new TextEncoder().encode("saved screenshot bytes") };
		}
		if (first) {
			first = false;
			expect(await kept.read()).toBeDefined();
			throw new Error("lost answer");
		}
		return accepted;
	});
	await expect(new NewThread(send, kept).send(config, current, { ...draft, feedback: [feedback] })).rejects.toThrow("lost answer");
	const saved = await kept.read();
	if (saved === undefined) throw new Error("Missing evidence snapshot");
	expect((await new NewThread(send, kept).resume(saved.key)).thread).toBe("accepted");
	expect(send.mock.calls.map(([request]) => request.method)).toEqual(["GET", "POST", "POST"]);
	const replay = upload(send.mock.calls[2]?.[0]);
	const original = upload(send.mock.calls[1]?.[0]);
	expect(replay.contentType).toBe(original.contentType);
	expect(await replay.body.arrayBuffer()).toEqual(await original.body.arrayBuffer());
	const form = await formOf(replay);
	const manifest = String(form.get("manifest"));
	expect(manifest.indexOf("note.txt")).toBeLessThan(manifest.indexOf("Historical browser context"));
	expect(manifest.indexOf("Historical browser context")).toBeLessThan(manifest.indexOf("selection.png"));
	const screenshot = form.get("file-1");
	if (!(screenshot instanceof Blob)) throw new Error("Missing screenshot");
	expect(await screenshot.text()).toBe("saved screenshot bytes");
});

it("waits for the frozen draft to be saved before submitting it", async () => {
	const kept = { ...store(), write: async () => Promise.reject(new Error("storage full")) };
	const send = mock<SendRequest>(async () => accepted);
	await expect(new NewThread(send, kept).send({}, current, draft)).rejects.toThrow("storage full");
	expect(send).not.toHaveBeenCalled();
});

it("replays the same request if saving a received acknowledgement failed", async () => {
	const kept = store();
	const write = mock(kept.write);
	write.mockImplementationOnce(kept.write).mockRejectedValueOnce(new Error("disk full"));
	const send = mock<SendRequest>(async () => accepted);
	const starting = new NewThread(send, { ...kept, write });
	await expect(starting.send({}, current, draft)).rejects.toThrow("disk full");
	const saved = await kept.read();
	if (saved === undefined) throw new Error("The request should still be saved");
	expect(saved.thread).toBeUndefined();
	expect(await starting.resume(saved.key)).toEqual({ key: saved.key, thread: "accepted" });
	const original = upload(send.mock.calls[0]?.[0]);
	const replay = upload(send.mock.calls[1]?.[0]);
	expect(replay.contentType).toBe(original.contentType);
	expect(await replay.body.arrayBuffer()).toEqual(await original.body.arrayBuffer());
});

it("coalesces local sends and preserves a saved submission across concurrent attempts", async () => {
	const kept = store();
	const send = mock<SendRequest>(async () => accepted);
	const starting = new NewThread(send, kept);
	const first = starting.send({}, current, draft);
	expect(starting.send({}, current, draft)).toBe(first);
	const parallel = new NewThread(send, kept).send({}, current, { ...draft, text: "different" });
	await expect(parallel).rejects.toThrow("already saved");
	await first;
	expect((await kept.read())?.draft.text).toBe(draft.text);
	expect(send).toHaveBeenCalledTimes(1);
});

it("uses the displayed key when resuming or removing a saved submission", async () => {
	const kept = store();
	const send = mock<SendRequest>(async () => accepted);
	const starting = new NewThread(send, kept);
	const first = await starting.send({}, current, draft);
	await starting.clear(first.key);
	const next = await starting.send({}, current, { ...draft, text: "new task" });
	await expect(starting.resume(first.key)).rejects.toThrow("has changed");
	await starting.clear(first.key);
	expect((await starting.read())?.key).toBe(next.key);
	expect(send).toHaveBeenCalledTimes(2);
});
