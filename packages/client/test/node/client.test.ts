/**
 * The client against the node itself, on a Home of the test's own: welcomed as the session its token holds; the list of
 * threads as it stands, then each change; a thread followed from its latest entries, on from where its reader stood
 * after the node started again, its older entries read back as asked; an input delivered once and its turn put
 * together; the settings as they change; a terminal's screen and what it writes; a file kept for a thread.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { conversationOf, type Followable } from "../../src/index.ts";
import { Node, until } from "./node.ts";

let node: Node;
const reported: unknown[] = [];
const report = (error: unknown): void => void reported.push(error);

beforeAll(async () => {
	node = await Node.start();
});

afterAll(async () => {
	await node.remove();
	expect(reported).toEqual([]);
});

/** Watches `followable` while `body` runs, its view read through `view`, then lets it go. */
async function watching<View>(followable: Followable<View>, body: (view: () => View) => Promise<void>): Promise<void> {
	const stop = followable.subscribe(() => undefined);
	try {
		await body(() => followable.view());
	} finally {
		stop();
		followable.release();
	}
}

describe("a client of the node's", () => {
	test("is welcomed as the session its token holds, and its calls are answered by their ids", async () => {
		const client = node.client({ report });
		await until(() => client.status.value.status === "open", "the welcome");
		const open = client.status.value;
		expect(open.status === "open" && open.scopes.includes("threads.operate") && open.machine.name.length > 0 && open.machine.workspace.length > 0).toBe(true);
		const thread = await client.create({ title: "first" });
		expect((await client.threads()).map((row) => row.id)).toContain(thread);
		client.close();
	});

	test("watches the threads as they stand, then each as it changes or goes", async () => {
		const client = node.client({ report });
		await watching(client.watchThreads(), async (list) => {
			await until(() => list() !== undefined, "the list");
			const thread = await client.create({ title: "watched" });
			await until(() => list()?.some((row) => row.id === thread) === true, "the thread made");
			await client.configure(thread, { title: "renamed" });
			await until(() => list()?.find((row) => row.id === thread)?.config.title === "renamed", "the title changed");
			await client.mark(thread, { pinned: true });
			await until(() => list()?.find((row) => row.id === thread)?.marks.pinned === true, "the pin");
			await client.delete(thread);
			await until(() => list()?.every((row) => row.id !== thread) === true, "the thread gone");
		});
		client.close();
	});

	test("follows a thread from its latest entries, and on from where its reader stood after the node starts again", async () => {
		const client = node.client({ report });
		await until(() => client.status.value.status === "open", "the welcome");
		const thread = await client.create({ title: "followed" });
		await watching(client.follow(thread), async (view) => {
			await until(() => view().read, "its entries read");
			const first = view().entries.length;
			await client.configure(thread, { title: "one" });
			await until(() => view().entries.length === first + 1, "the change");
			await node.stop();
			await until(() => client.status.value.status !== "open", "the drop");
			await node.serve();
			await until(() => client.status.value.status === "open", "the welcome again");
			await client.configure(thread, { title: "two" });
			await until(() => view().entries.length === first + 2, "the change after");
			await Bun.sleep(100);
			expect(view().entries.length).toBe(first + 2);
			expect(view().status?.turn).toBe("created");
		});
		client.close();
	});

	test("keeps a thread's latest entries alone, as many as it keeps, and reads older ones back as asked", async () => {
		const client = node.client({ report, entriesKept: 2 });
		await until(() => client.status.value.status === "open", "the welcome");
		const thread = await client.create({ title: "long" });
		for (const title of ["a", "b", "c", "d"]) await client.configure(thread, { title });
		await watching(client.follow(thread), async (view) => {
			await until(() => view().read, "its entries read");
			expect(view().entries.length).toBe(2);
			expect(view().complete).toBe(false);
			await client.readOlder(thread);
			expect(view().entries.length).toBe(5);
			expect(view().complete).toBe(true);
		});
		client.close();
	});

	test("delivers an input once, and the turn that takes it is put together as it ends", async () => {
		const client = node.client({ report });
		await until(() => client.status.value.status === "open", "the welcome");
		const thread = await client.create({ title: "asked" });
		await watching(client.follow(thread), async (view) => {
			const delivery = await client.deliver(thread, [{ type: "input_text", text: "hello" }]);
			await until(() => conversationOf(view()).turns.at(-1)?.end !== undefined, "the turn's end");
			const turn = conversationOf(view()).turns.at(-1);
			expect(turn?.inputs.map((each) => each.delivery)).toEqual([delivery]);
			expect(turn?.end?.outcome).toBe("failed");
		});
		client.close();
	});

	test("watches the settings as they change: the environment's, and this device's", async () => {
		const client = node.client({ report });
		await watching(client.watchSettings(), async (settings) => {
			await until(() => settings() !== undefined, "the settings");
			await client.setSettings({ environment: { thread_mode: "auto" }, device: { theme: "dark" } });
			await until(() => settings()?.environment.thread_mode === "auto", "the mode");
			expect(settings()?.device).toEqual({ theme: "dark" });
		});
		client.close();
	});

	test("follows a terminal: its screen drawn first, then what it writes", async () => {
		const client = node.client({ report });
		await until(() => client.status.value.status === "open", "the welcome");
		const thread = await client.create({ title: "terminal" });
		const terminal = await client.openTerminal(thread, { rows: 24, cols: 80 });
		let screens = 0;
		let written = "";
		const view = client.terminal(
			terminal,
			{ rows: 24, cols: 80 },
			{
				screen: () => void (screens += 1),
				output: (bytes) => void (written += new TextDecoder().decode(bytes)),
			},
		);
		await until(() => screens > 0, "the screen");
		view.write("echo hi-$((40+2))\n");
		await until(() => written.includes("hi-42"), "what it wrote");
		view.stop();
		await client.closeTerminal(terminal);
		client.close();
	});

	test("has a thread keep a file, named by its content", async () => {
		const client = node.client({ report });
		await until(() => client.status.value.status === "open", "the welcome");
		const thread = await client.create({ title: "files" });
		const kept = await client.keepFile(thread, new Blob(["some words"], { type: "text/plain" }), "words.txt");
		expect(kept.ref.startsWith("sha256:")).toBe(true);
		expect(kept.filename).toBe("words.txt");
		client.close();
	});
});
