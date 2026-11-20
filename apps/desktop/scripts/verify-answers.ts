/** A scripted local model drives real node requests; two independent clients answer them. No external model calls. */
import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import {
	CALL_PATH,
	createClient,
	WATCH_PATH,
	type Ask,
	type Client,
	type Command,
	type NodeAnswer,
	type NodeRequest,
	type SendRequest,
} from "@octoii-works/octoii-client";

const program = process.argv[2] ?? fileURLToPath(new URL("../dist/runtime/bin/octoii", import.meta.url));
const temporary = await mkdtemp(join(tmpdir(), "octoii-answers-"));
const home = join(temporary, "node");
const nodeTemporary = join(temporary, "node-tmp");
const workspace = join(temporary, "workspace");
const target = join(temporary, "outside.txt");
const environment = { ...process.env, TMPDIR: nodeTemporary, TMP: nodeTemporary, TEMP: nodeTemporary };
const completion = "Acceptance scenario completed.";
const clients: Client[] = [];
let node: ReturnType<typeof Bun.spawn> | undefined;
let model: ReturnType<typeof Bun.serve> | undefined;
let finishStream: (() => void) | undefined;
let origin = "";

async function command(...args: string[]): Promise<string> {
	const process = Bun.spawn([program, "--home", home, ...args], { env: environment, stdout: "pipe", stderr: "ignore" });
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
	node = Bun.spawn([program, "--home", home, "serve", "--port", origin ? new URL(origin).port : "0"], {
		env: environment,
		stdout: "ignore",
		stderr: "ignore",
	});
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

type TransportHooks = {
	readonly watch?: (socket: WebSocket) => void;
	readonly send?: (request: NodeRequest, forward: SendRequest) => Promise<NodeAnswer>;
};

async function connect(hooks: TransportHooks = {}): Promise<Client> {
	const token = (await command("token", "--label", "answer acceptance")).trim();
	const send: SendRequest = async ({ method, path, contentType, body }) => {
		const answer = await fetch(`${origin}${path}`, {
			method,
			headers: { authorization: `Bearer ${token}`, ...(contentType === undefined ? {} : { "content-type": contentType }) },
			...(body === undefined ? {} : { body }),
		});
		return { status: answer.status, contentType: answer.headers.get("content-type") ?? undefined, bytes: new Uint8Array(await answer.arrayBuffer()) };
	};
	const client = createClient({
		open: (protocol) => {
			const socket = new WebSocket(`${origin.replace("http:", "ws:")}${WATCH_PATH}`, { protocols: [protocol], headers: { authorization: `Bearer ${token}` } });
			hooks.watch?.(socket);
			return socket;
		},
		send: (request) => (hooks.send === undefined ? send(request) : hooks.send(request, send)),
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

async function scenario(client: Client, kind: "question" | "approval" | "stream"): Promise<string> {
	const thread = await client.create({ title: `Acceptance ${kind}`, workspace, model: "ollama/octoii-acceptance", mode: "default" });
	await client.deliver(thread, [{ type: "input_text", text: `[${kind}] Run the isolated acceptance scenario.` }]);
	return thread;
}

/** Only the controlled model protocol fields used by this fixture. */
type ModelRequest = { model: string; messages: { role: string; content: unknown }[]; tools?: unknown[] };

try {
	await Promise.all([mkdir(workspace), mkdir(nodeTemporary)]);
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
				: { role: "assistant", content: completion };
			const chunk = (delta: unknown, finish: string | null): string =>
				`data: ${JSON.stringify({ id: "chatcmpl-test", object: "chat.completion.chunk", created: 1, model: body.model, choices: [{ index: 0, delta, finish_reason: finish }] })}\n\n`;
			const end = `${chunk({}, callsTool ? "tool_calls" : "stop")}data: [DONE]\n\n`;
			const headers = { "Content-Type": "text/event-stream" };
			if (body.messages.some((message) => message.role === "user" && JSON.stringify(message.content).includes("[stream]"))) {
				const encoder = new TextEncoder();
				return new Response(
					new ReadableStream({
						start(controller) {
							controller.enqueue(encoder.encode(chunk(delta, null)));
							finishStream = () => {
								controller.enqueue(encoder.encode(end));
								controller.close();
							};
						},
					}),
					{ headers },
				);
			}
			return new Response(`${chunk(delta, null)}${end}`, { headers });
		},
	});
	await start();
	const first = await connect();
	const second = await connect();
	await first.refreshModels();
	const otherThread = await first.create({ title: "Acceptance other thread", workspace, model: "ollama/octoii-acceptance", mode: "default" });
	for (let round = 0; round < 5; round++) {
		const thread = await scenario(first, "question");
		const request = await ask(first, thread);
		await assert.rejects(second.answer(otherThread, request.id, { answers: [{ chosen: ["SQLite"] }] }), { code: "not_given" });
		const results = await Promise.allSettled([
			first.answer(thread, request.id, { answers: [{ chosen: ["SQLite"] }] }),
			second.answer(thread, request.id, { answers: [{ chosen: ["Postgres"] }] }),
		]);
		assert.equal(results.filter((result) => result.status === "fulfilled").length, 1);
		const rejected = results.find((result) => result.status === "rejected");
		assert.ok(rejected?.status === "rejected" && rejected.reason.code === "not_given");
		await until(async () => (await first.threads()).find((row) => row.id === thread)?.status.turn === "replied", "question completed");
		assert.equal((await second.threads()).find((row) => row.id === thread)?.status.asks.length, 0);
	}
	console.log("PASS: answers are scoped to their thread, and five reply races each accept one decision");
	const denied = await scenario(first, "approval");
	const waiting = await ask(first, denied);
	await stop();
	await until(() => first.status.value.status !== "open", "disconnect observed");
	await assert.rejects(first.answer(denied, waiting.id, { decision: "deny", reason: "keep the file" }));
	await start();
	await until(() => first.status.value.status === "open" && second.status.value.status === "open", "clients reconnected");
	assert.equal((await ask(second, denied)).id, waiting.id);
	await first.answer(denied, waiting.id, { decision: "deny", reason: "keep the file" });
	await until(async () => (await first.threads()).find((row) => row.id === denied)?.status.turn === "replied", "denial completed");
	await assert.rejects(readFile(target), { code: "ENOENT" });
	console.log("PASS: waiting approval survives node restart, offline answering fails, and denial performs no write");
	const allowed = await scenario(first, "approval");
	await first.answer(allowed, (await ask(first, allowed)).id, { decision: "allow" });
	await until(async () => (await first.threads()).find((row) => row.id === allowed)?.status.turn === "replied", "allowed write completed");
	assert.equal(await readFile(target, "utf8"), "approved once");
	console.log("PASS: allowing the real tool writes only the intended temporary file");
	let answerCalls = 0;
	let dropped = false;
	let watch: WebSocket | undefined;
	const uncertain = await connect({
		watch: (socket) => {
			watch = socket;
		},
		send: async (request, forward) => {
			if (request.path !== CALL_PATH || typeof request.body !== "string") return forward(request);
			// SAFETY: CALL_PATH receives the client's typed JSON commands.
			const call = JSON.parse(request.body) as Command;
			if (call.method !== "threads.answer") return forward(request);
			answerCalls++;
			watch?.close();
			const answer = await forward(request);
			assert.equal(answer.status, 200, "the exact answer was committed while the watch was down");
			dropped = true;
			throw new Error("simulated lost HTTP response after commit");
		},
	});
	const committed = await scenario(uncertain, "question");
	const question = await ask(uncertain, committed);
	const followed = uncertain.follow(committed);
	const watched = uncertain.watchThreads();
	await until(
		() => followed.view().status?.asks[0]?.id === question.id && watched.view()?.find((row) => row.id === committed)?.status.asks[0]?.id === question.id,
		"subscriptions see the waiting question",
	);
	await assert.rejects(uncertain.answer(committed, question.id, { answers: [{ chosen: ["SQLite"] }] }), { code: "connection-lost" });
	assert.equal(dropped, true);
	await until(() => uncertain.status.value.status === "open", "reconnected after lost acknowledgement");
	await until(async () => (await uncertain.threads()).find((row) => row.id === committed)?.status.turn === "replied", "committed answer completed");
	assert.equal((await uncertain.threads()).find((row) => row.id === committed)?.status.asks.length, 0);
	assert.equal((await second.threads()).find((row) => row.id === committed)?.status.asks.length, 0);
	await until(
		() => followed.view().status?.turn === "replied" && watched.view()?.find((row) => row.id === committed)?.status.turn === "replied",
		"subscriptions recover automatically",
	);
	assert.equal(followed.view().status?.asks.length, 0);
	assert.equal(watched.view()?.find((row) => row.id === committed)?.status.asks.length, 0);
	followed.release();
	watched.release();
	assert.equal(answerCalls, 1);
	console.log("PASS: a lost acknowledgement reports uncertainty, reconnect recovers the committed answer, and no answer is resent");

	let activeSocket: WebSocket | undefined;
	const reader = await connect({
		watch: (socket) => {
			activeSocket = socket;
		},
	});
	const streamedThread = await scenario(reader, "stream");
	const reading = reader.follow(streamedThread);
	const completePart = () => [...reading.view().streaming].find(([, stream]) => stream.parts.some((part) => part.type === "delta" && part.text === completion));
	await until(() => completePart() !== undefined, "first live output");
	const initial = completePart();
	assert.ok(initial && activeSocket);
	let reset = false;
	const observeReset = reading.subscribe(() => {
		if (reading.view().streaming.size === 0) reset = true;
	});
	activeSocket.close();
	assert.ok((await reader.threads()).some((row) => row.id === streamedThread));
	await until(() => reset && completePart()?.[0] === initial[0], "the same request replayed after reset");
	observeReset();
	assert.ok(finishStream);
	finishStream();
	await until(
		() => reading.view().status?.turn === "replied" && reading.view().streaming.size === 0 && reading.view().entries.some((entry) => entry.type === "reply"),
		"the request closed and the durable reply arrived",
	);
	reading.release();
	console.log("PASS: reconnect resets and replays the same live request; close leaves its durable reply");
} finally {
	for (const client of clients) client.close();
	await stop();
	await model?.stop(true);
	await rm(temporary, { recursive: true, force: true });
}
