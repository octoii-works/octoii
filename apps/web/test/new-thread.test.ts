import { describe, expect, it, mock } from "bun:test";
import { NewThread } from "../src/core/new-thread.ts";
import { EMPTY_DRAFT } from "../src/core/drafts.ts";

function client() {
	return {
		create: mock(async () => "thread-1"),
		keepFile: mock(async () => ({ ref: "sha256:kept", media_type: "text/plain" })),
		deliver: mock(async () => "delivery-1"),
	};
}

const draft = {
	...EMPTY_DRAFT,
	text: "First task",
	attachments: [{ ref: "local", name: "note.txt", media_type: "text/plain", size: 4, bytes: new Blob(["note"]) }],
};

describe("starting a thread", () => {
	it("keeps the thread and draft after an upload failure, delivering only the successful retry", async () => {
		const calls = client();
		calls.keepFile.mockRejectedValueOnce(new Error("upload failed"));
		const starting = new NewThread(calls);
		await expect(starting.send({ workspace: "/project" }, draft)).rejects.toThrow("upload failed");
		expect(calls.deliver).not.toHaveBeenCalled();
		expect(draft.attachments[0]?.bytes).toBeInstanceOf(Blob);
		expect(await starting.send({ workspace: "/project" }, { ...draft, text: "Updated task" })).toBe("thread-1");
		expect(calls.create).toHaveBeenCalledTimes(1);
		expect(calls.deliver).toHaveBeenCalledWith("thread-1", [
			{ type: "input_text", text: "Updated task" },
			{ type: "input_file", file: { ref: "sha256:kept", media_type: "text/plain", filename: "note.txt" } },
		]);
	});

	it("reuses the created thread when delivery is rejected", async () => {
		const calls = client();
		calls.deliver.mockRejectedValueOnce(new Error("rejected"));
		const starting = new NewThread(calls);
		await expect(starting.send({}, draft)).rejects.toThrow("rejected");
		await starting.send({}, draft);
		expect(calls.create).toHaveBeenCalledTimes(1);
	});

	it("coalesces concurrent submissions and allows a distinct task after success", async () => {
		const calls = client();
		const starting = new NewThread(calls);
		const first = starting.send({}, draft);
		expect(starting.send({}, draft)).toBe(first);
		await first;
		expect(calls.deliver).toHaveBeenCalledTimes(1);
		await starting.send({}, draft);
		expect(calls.create).toHaveBeenCalledTimes(2);
	});

	it("does not reuse a thread for a different workspace, and retains the original for retry", async () => {
		const calls = client();
		calls.create.mockResolvedValueOnce("original").mockResolvedValueOnce("other");
		calls.keepFile.mockRejectedValueOnce(new Error("upload failed"));
		const starting = new NewThread(calls);
		await expect(starting.send({ workspace: "/original" }, draft)).rejects.toThrow("upload failed");
		expect(await starting.send({ workspace: "/other" }, draft)).toBe("other");
		expect(await starting.send({ workspace: "/original" }, draft)).toBe("original");
		expect(calls.create).toHaveBeenCalledTimes(2);
	});

	it("retries creation after a refusal without uploading or delivering first", async () => {
		const calls = client();
		calls.create.mockRejectedValueOnce(new Error("forbidden"));
		const starting = new NewThread(calls);
		await expect(starting.send({}, draft)).rejects.toThrow("forbidden");
		expect(calls.keepFile).not.toHaveBeenCalled();
		expect(calls.deliver).not.toHaveBeenCalled();
		await starting.send({}, draft);
		expect(calls.create).toHaveBeenCalledTimes(2);
	});
});
