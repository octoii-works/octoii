/** Atomic initial messages use the same authenticated HTTP transport locally and through a tunnel. */
import type { InputContent } from "@octoii-works/octoii-client";
import { Type } from "typebox";
import type { WorktreeSubmission } from "../../core/new-thread.ts";
import { fileAddress } from "../../core/attachments.ts";
import { elementEvidence } from "../../core/element-feedback.ts";
import { matches } from "../../core/schema.ts";
import { jsonOf, succeeded, wordsOf, type SendRequest } from "../../components/state/reach.ts";

type Part = { type: "content"; content: InputContent } | { type: "attachment"; index: number; media_type: string; filename: string };
const Accepted = Type.Object({ thread: Type.String({ minLength: 1 }) });

/** No Git operation runs here. The accepted thread owns the subsequent preparation. */
export async function acceptWorktree(send: SendRequest, submission: WorktreeSubmission): Promise<string> {
	const form = new FormData();
	const parts: Part[] = [];
	const files: Blob[] = [];
	const attach = (bytes: Blob, media_type: string, filename: string): void => {
		parts.push({ type: "attachment", index: files.length, media_type, filename });
		files.push(bytes);
	};
	const { draft, config, key, worktree } = submission;
	if (draft.text.trim() !== "") parts.push({ type: "content", content: { type: "input_text", text: draft.text } });
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
	const { workspace: _workspace, ...unbound } = config;
	form.append("manifest", JSON.stringify({ key, config: unbound, worktree, parts }));
	files.forEach((bytes, index) => form.append(`file-${String(index)}`, bytes));
	// Serialize once so both fetch and the byte-oriented tunnel carry the browser-generated boundary.
	const body = await new Response(form).blob();
	if (body.size > 64 * 1024 * 1024) throw new Error("The task and its attachments exceed the 64 MiB upload limit.");
	const answer = await send({ method: "POST", path: "/git/threads", contentType: body.type, body });
	const accepted = jsonOf(answer);
	if (succeeded(answer) && matches(Accepted, accepted)) return accepted.thread;
	throw new Error(wordsOf(answer) || `The node did not accept the task (${String(answer.status)}).`);
}
