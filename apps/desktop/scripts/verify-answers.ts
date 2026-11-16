/** A scripted local model drives real node requests; two independent clients answer them. No external model calls. */
import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createClient, type Ask, type Client } from "@octoii-works/octoii-client";

const program = fileURLToPath(new URL("../dist/runtime/bin/octoii", import.meta.url));
const temporary = await mkdtemp("/private/tmp/octoii-answers-");
const home = join(temporary, "node");
const workspace = join(temporary, "workspace");
const target = join(temporary, "outside.txt");
const clients: Client[] = [];
let node: ReturnType<typeof Bun.spawn> | undefined;
let model: ReturnType<typeof Bun.serve> | undefined;
let origin = "";

async function command(...args: string[]): Promise<string> {
	const process = Bun.spawn([program, "--home", home, ...args], { stdout: "pipe", stderr: "ignore" });
	const output = await new Response(process.stdout).text();
	if (await process.exited) throw new Error(`Test node command failed: ${args[0]}`);
	return output;
}

async function until(ready: () => boolean | Promise<boolean>, description: string): Promise<void> {
	const deadline = Date.now() + 15000;
	while (Date.now() < deadline) {
		if (await ready()) return;
		await Bun.sleep(25);
	}
	throw new Error(`Timed out: ${description}`);
}

async function start(): Promise<void> {
	node = Bun.spawn([program, "--home", home, "serve", "--port", origin ? new URL(origin).port : "0"], { stdout: "ignore", stderr: "ignore" });
	await until(async () => {
		try {
			const status: { loopback: string } = JSON.parse(await command("status", "--json"));
			origin = status.loopback;
			return true;
		} catch {
			return false;
		}
	}, "node ready");
}

async function stop(): Promise<void> {
	node?.kill("SIGTERM");
	await node?.exited;
	node = undefined;
}

async function connect(): Promise<Client> {
	const token = (await command("token", "--label", "answer acceptance")).trim();
	const client = createClient({
		open: (protocol) => new WebSocket(`${origin.replace("http:", "ws:")}/ws`, { protocols: [protocol], headers: { authorization: `Bearer ${token}` } }),
		randomUUID: () => crypto.randomUUID(),
		clock: {
			now: () => Date.now(),
			setTimer: (callback, ms) => {
				const timer = setTimeout(callback, ms);
				return () => clearTimeout(timer);
			},
		},
		report: (error) => console.error(error instanceof Error ? error.message : String(error)),
	});
	clients.push(client);
	await until(() => client.status.value.status === "open", "client connected");
	return client;
}

async function ask(client: Client, thread: string): Promise<Ask> {
	let request: Ask | undefined;
	await until(async () => {
		request = (await client.threads()).find((row) => row.id === thread)?.status.asks[0];
		return request !== undefined;
	}, "request waiting for a person");
	assert.ok(request);
	return request;
}

async function scenario(client: Client, kind: "question" | "approval"): Promise<string> {
	const thread = await client.create({ title: `Acceptance ${kind}`, workspace, model: "ollama/octoii-acceptance", mode: "default" });
	await client.deliver(thread, [{ type: "input_text", text: `[${kind}] Run the isolated acceptance scenario.` }]);
	return thread;
}

/** Only the controlled model protocol fields used by this fixture. */
type ModelRequest = { model: string; messages: { role: string; content: unknown }[]; tools?: unknown[] };

try {
	await mkdir(workspace);
	// Ollama's existing adapter discovers this test model. An occupied port fails; no existing server is stopped.
	model = Bun.serve({
		hostname: "127.0.0.1",
		port: 11434,
		async fetch(request) {
			if (new URL(request.url).pathname === "/v1/models") return Response.json({ data: [{ id: "octoii-acceptance" }] });
			// SAFETY: the acceptance scenario supplies the Chat Completions adapter's known request shape.
			const body = (await request.json()) as ModelRequest;
			if (body.model !== "octoii-acceptance") return new Response("Test model only", { status: 404 });
			const question = body.messages.some((message) => message.role === "user" && JSON.stringify(message.content).includes("[question]"));
			const approval = body.messages.some((message) => message.role === "user" && JSON.stringify(message.content).includes("[approval]"));
			const callsTool = !body.messages.some((message) => message.role === "tool") && !!body.tools?.length && (question || approval);
			const args = question
				? { questions: [{ question: "Which database?", options: [{ label: "SQLite" }, { label: "Postgres" }] }] }
				: { path: target, content: "approved once" };
			const delta = callsTool
				? {
						role: "assistant",
						tool_calls: [
							{ index: 0, id: "call_acceptance", type: "function", function: { name: question ? "question" : "write", arguments: JSON.stringify(args) } },
						],
					}
				: { role: "assistant", content: "Acceptance scenario completed." };
			const chunk = (delta: unknown, finish: string | null): string =>
				`data: ${JSON.stringify({ id: "chatcmpl-test", object: "chat.completion.chunk", created: 1, model: body.model, choices: [{ index: 0, delta, finish_reason: finish }] })}\n\n`;
			return new Response(`${chunk(delta, null)}${chunk({}, callsTool ? "tool_calls" : "stop")}data: [DONE]\n\n`, {
				headers: { "Content-Type": "text/event-stream" },
			});
		},
	});
	await start();
	const first = await connect();
	const second = await connect();
	await first.refreshModels();
	for (let round = 0; round < 5; round++) {
		const thread = await scenario(first, "question");
		const request = await ask(first, thread);
		const results = await Promise.allSettled([
			first.answer(thread, request.key, { answers: [{ chosen: ["SQLite"] }] }),
			second.answer(thread, request.key, { answers: [{ chosen: ["Postgres"] }] }),
		]);
		assert.equal(results.filter((result) => result.status === "fulfilled").length, 1);
		const rejected = results.find((result) => result.status === "rejected");
		assert.ok(rejected?.status === "rejected" && rejected.reason.code === "not_given");
		await until(async () => (await first.threads()).find((row) => row.id === thread)?.status.turn === "replied", "question completed");
		assert.equal((await second.threads()).find((row) => row.id === thread)?.status.asks.length, 0);
	}
	console.log("PASS: five concurrent answers accept exactly one decision and both clients see the request gone");
	const denied = await scenario(first, "approval");
	const waiting = await ask(first, denied);
	await stop();
	await until(() => first.status.value.status !== "open", "disconnect observed");
	await assert.rejects(first.answer(denied, waiting.key, { decision: "deny", reason: "keep the file" }));
	await start();
	await until(() => first.status.value.status === "open" && second.status.value.status === "open", "clients reconnected");
	assert.equal((await ask(second, denied)).key, waiting.key);
	await first.answer(denied, waiting.key, { decision: "deny", reason: "keep the file" });
	await until(async () => (await first.threads()).find((row) => row.id === denied)?.status.turn === "replied", "denial completed");
	await assert.rejects(readFile(target), { code: "ENOENT" });
	console.log("PASS: waiting approval survives node restart, offline answering fails, and denial performs no write");
	const allowed = await scenario(first, "approval");
	await first.answer(allowed, (await ask(first, allowed)).key, { decision: "allow" });
	await until(async () => (await first.threads()).find((row) => row.id === allowed)?.status.turn === "replied", "allowed write completed");
	assert.equal(await readFile(target, "utf8"), "approved once");
	console.log("PASS: allowing the real tool writes only the intended temporary file");
} finally {
	for (const client of clients) client.close();
	await stop();
	await model?.stop(true);
	await rm(temporary, { recursive: true, force: true });
}
