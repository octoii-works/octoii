/** The ordered initial message, encoded once for the node's local and tunnel HTTP transports. */
import type { Config } from "@octoii-works/octoii-client";
import { Type } from "typebox";
import { fileAddress } from "../../core/attachments.ts";
import type { Draft } from "../../core/drafts.ts";
import { elementEvidence } from "../../core/element-feedback.ts";
import { matches } from "../../core/schema.ts";
import { jsonOf, succeeded, wordsOf, type SendRequest } from "../../components/state/reach.ts";

export type ThreadWorkspace = { readonly kind: "current" } | { readonly kind: "isolated"; readonly path: string; readonly base?: string };
export type ThreadRequest = { readonly contentType: string; readonly body: Blob };

type Submission = { readonly key: string; readonly config: Config; readonly workspace: ThreadWorkspace; readonly draft: Draft };
type Part = { type: "content"; content: { type: "input_text"; text: string } } | { type: "attachment"; index: number; media_type: string; filename: string };
const Accepted = Type.Object({ thread: Type.String({ minLength: 1 }) });
const PARTS_MAX = 128;
const MANIFEST_MAX = 1024 * 1024;
const FILE_MAX = 32 * 1024 * 1024;
const BODY_MAX = 64 * 1024 * 1024;

/** Copies historical screenshots before saving the request so it can be replayed without reading their source again. */
export async function threadRequest(send: SendRequest, { draft, config, key, workspace }: Submission): Promise<ThreadRequest> {
	const hasText = draft.text.trim() !== "";
	const count = Number(hasText) + draft.attachments.length + draft.feedback.length * 2;
	if (count === 0 || count > PARTS_MAX) throw new Error("The message requires between 1 and 128 parts.");
	const parts: Part[] = [];
	const files: Blob[] = [];
	let given = 0;
	const attach = (bytes: Blob, media_type: string, filename: string): void => {
		if (bytes.size > FILE_MAX) throw new Error(`${filename} exceeds the 32 MiB attachment limit.`);
		given += bytes.size;
		checkBodySize(given);
		parts.push({ type: "attachment", index: files.length, media_type, filename });
		files.push(bytes);
	};
	if (hasText) parts.push({ type: "content", content: { type: "input_text", text: draft.text } });
	for (const file of draft.attachments) {
		if (file.bytes === undefined) throw new Error(`Attach ${file.name} again: its original bytes are unavailable.`);
		attach(file.bytes, file.media_type, file.name);
	}
	for (const feedback of draft.feedback) {
		const answer = await send({ method: "GET", path: fileAddress(feedback.thread, feedback.screenshot.ref) });
		if (!succeeded(answer)) throw new Error(wordsOf(answer) || "The saved browser screenshot could not be read.");
		parts.push({ type: "content", content: { type: "input_text", text: elementEvidence(feedback) } });
		attach(new Blob([new Uint8Array(answer.bytes)]), feedback.screenshot.media_type, feedback.screenshot.filename ?? "screenshot.png");
	}
	const { workspace: _project, ...unbound } = config;
	const manifest = JSON.stringify({ key, config: workspace.kind === "isolated" ? unbound : config, workspace, parts });
	if (new TextEncoder().encode(manifest).byteLength > MANIFEST_MAX) throw new Error("The message metadata exceeds the 1 MiB upload limit.");
	const form = new FormData();
	form.append("manifest", manifest);
	files.forEach((bytes, index) => form.append(`file-${String(index)}`, bytes));
	// Serialize once so both fetch and the byte-oriented tunnel carry the browser-generated boundary.
	const response = new Response(form);
	const contentType = response.headers.get("content-type");
	if (contentType === null) throw new Error("The browser did not encode the attachment boundary.");
	// Blob.type lowercases MIME parameters, but multipart boundaries are case-sensitive.
	const body = await response.blob();
	checkBodySize(body.size);
	return { contentType, body };
}

function checkBodySize(size: number): void {
	if (size > BODY_MAX) throw new Error("The task and its attachments exceed the 64 MiB upload limit.");
}

/** Replays the frozen bytes and their original case-sensitive multipart boundary. */
export async function acceptThread(send: SendRequest, request: ThreadRequest): Promise<string> {
	const answer = await send({ method: "POST", path: "/threads", ...request });
	const accepted = jsonOf(answer);
	if (succeeded(answer) && matches(Accepted, accepted)) return accepted.thread;
	throw new Error(wordsOf(answer) || `The node did not accept the task (${String(answer.status)}).`);
}
