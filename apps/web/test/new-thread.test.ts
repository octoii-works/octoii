import type { Client, MethodName, ParamsOf, ResultOf } from "@octoii-works/octoii-client";
import { describe, expect, it, mock, spyOn } from "bun:test";
import { NewThread, type PendingThread, type PendingThreadStore } from "../src/core/new-thread.ts";
import { EMPTY_DRAFT } from "../src/core/drafts.ts";

function client() {
	const create = mock(async (_params: ParamsOf<"threads.create">) => ({ thread: "thread-1" }));
	const deliver = mock(async (_params: ParamsOf<"threads.deliver">) => ({ delivery: "delivery-1" }));
	return {
		create,
		deliver,
		keepFile: mock<Client["keepFile"]>(async () => ({ ref: "sha256:kept", media_type: "text/plain" })),
		call: async <Name extends MethodName>(method: Name, params: ParamsOf<Name>): Promise<ResultOf<Name>> => {
			// SAFETY: the test transport dispatches only the two methods used by NewThread.
			if (method === "threads.create") return await create(params as ParamsOf<"threads.create">);
			if (method === "threads.deliver") return await deliver(params as ParamsOf<"threads.deliver">);
			throw new Error(`Unexpected method: ${method}`);
		},
	};
}

function store(): PendingThreadStore {
	let saved: PendingThread | undefined;
	return {
		read: async () => (saved === undefined ? undefined : structuredClone(saved)),
		exclusive: (action) => action(),
		begin: async (pending) => {
			if (saved !== undefined) throw new Error("A task is already saved");
			saved = structuredClone(pending);
			return pending;
		},
		write: async (pending) => {
			saved = structuredClone(pending);
		},
		remove: async (key) => {
			if (saved?.key === key) saved = undefined;
		},
	};
}

const draft = {
	...EMPTY_DRAFT,
	text: "First task",
	attachments: [{ ref: "local", name: "note.txt", media_type: "text/plain", size: 4, bytes: new Blob(["note"]) }],
};

describe("recovering a submitted task", () => {
	it("keeps the original draft and attachment bytes through upload failure and client recreation", async () => {
		const calls = client();
		const kept = store();
		calls.keepFile.mockRejectedValueOnce(new Error("upload failed"));
		await expect(new NewThread(calls, kept).send({ workspace: "/project" }, draft)).rejects.toThrow("upload failed");
		expect(calls.deliver).not.toHaveBeenCalled();
		const restored = new NewThread(calls, kept);
		expect(await (await restored.read())?.draft.attachments[0]?.bytes?.text()).toBe("note");
		expect(await restored.resume()).toBe("thread-1");
		expect(calls.create).toHaveBeenCalledTimes(1);
		expect(calls.deliver.mock.calls[0]?.[0].content).toEqual([
			{ type: "input_text", text: "First task" },
			{ type: "input_file", file: { ref: "sha256:kept", media_type: "text/plain", filename: "note.txt" } },
		]);
	});

	it("reuses the creation key if the acknowledgement was lost", async () => {
		const calls = client();
		const kept = store();
		calls.create.mockRejectedValueOnce(new Error("connection closed"));
		await expect(new NewThread(calls, kept).send({}, draft)).rejects.toThrow();
		await new NewThread(calls, kept).resume();
		expect(calls.create.mock.calls[1]).toEqual(calls.create.mock.calls[0]);
	});

	it("replays the exact content and delivery key after a lost answer without uploading again", async () => {
		const calls = client();
		const kept = store();
		calls.deliver.mockRejectedValueOnce(new Error("answer lost"));
		await expect(new NewThread(calls, kept).send({}, draft)).rejects.toThrow();
		const restored = new NewThread(calls, kept);
		await restored.resume();
		expect(calls.deliver.mock.calls[1]).toEqual(calls.deliver.mock.calls[0]);
		expect(calls.keepFile).toHaveBeenCalledTimes(1);
		await restored.resume();
		expect(calls.deliver).toHaveBeenCalledTimes(2);
		await restored.acknowledge();
		expect(await restored.read()).toBeUndefined();
	});

	it("does not perform remote work if saving the initial snapshot fails", async () => {
		const calls = client();
		const kept = {
			...store(),
			begin: async (): Promise<PendingThread> => {
				throw new Error("storage full");
			},
		};
		await expect(new NewThread(calls, kept).send({}, draft)).rejects.toThrow("storage full");
		expect(calls.create).not.toHaveBeenCalled();
		expect(calls.deliver).not.toHaveBeenCalled();
	});

	it("never delivers before the exact content has been durably saved", async () => {
		const calls = client();
		const kept = store();
		const write = kept.write;
		kept.write = async (pending) => {
			if (pending.content !== undefined) throw new Error("storage full");
			await write(pending);
		};
		await expect(new NewThread(calls, kept).send({}, draft)).rejects.toThrow("storage full");
		expect(calls.deliver).not.toHaveBeenCalled();
	});

	it("coalesces concurrent sends and never replaces a pending message with new input", async () => {
		const calls = client();
		const kept = store();
		const starting = new NewThread(calls, kept);
		const first = starting.send({}, draft);
		expect(starting.send({}, draft)).toBe(first);
		await first;
		await expect(starting.send({}, { ...draft, text: "different" })).rejects.toThrow("already saved");
		expect(calls.deliver).toHaveBeenCalledTimes(1);
	});
});

it("refuses old or future-dated recovery without remote writes", async () => {
	const now = 2_000_000_000_000;
	const clock = spyOn(Date, "now").mockReturnValue(now);
	try {
		for (const savedAt of [now - 24 * 60 * 60 * 1000, now + 60_000]) {
			const kept = store();
			await kept.begin({ key: "old", savedAt, config: {}, draft });
			const calls = client();
			await expect(new NewThread(calls, kept).resume()).rejects.toThrow("too old");
			expect(calls.create).not.toHaveBeenCalled();
			expect(calls.deliver).not.toHaveBeenCalled();
		}
	} finally {
		clock.mockRestore();
	}
});

it("an old acknowledgement cannot remove a newer submission", async () => {
	const kept = store();
	const first = new NewThread(client(), kept);
	await first.send({}, draft);
	await first.acknowledge();
	const next = new NewThread(client(), kept);
	await next.send({}, draft);
	const saved = await next.read();
	await first.acknowledge();
	expect((await next.read())?.key).toBe(saved?.key);
});
