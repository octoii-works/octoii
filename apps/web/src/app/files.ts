/**
 * Files a thread keeps for its messages: a file's bytes posted to the node's `/threads/{thread}/files`, which keeps
 * them by their content and answers with the file as the thread keeps it, which a message names, as JSON carries bytes
 * badly.
 */
import type { FileRef } from "@octoii-works/octoii-client";
import { matches } from "../core/schema.ts";
import { Type } from "typebox";
import { jsonOf, type SendRequest, succeeded, wordsOf } from "../components/state/reach.ts";

const KeptSchema = Type.Object({ ref: Type.String(), media_type: Type.String(), filename: Type.Optional(Type.Union([Type.String(), Type.Null()])) });

/**
 * Sends `bytes`, named `name`, to the thread `thread` to keep: the file as it keeps it, an image fitted to what models
 * read; rejects with the node's words where it refuses them.
 */
export async function keepFile(send: SendRequest, thread: string, bytes: Blob, name: string): Promise<FileRef> {
	const path = `/threads/${encodeURIComponent(thread)}/files?name=${encodeURIComponent(name)}`;
	const answer = await send({ method: "POST", path, contentType: bytes.type === "" ? "application/octet-stream" : bytes.type, body: bytes });
	const told = jsonOf(answer);
	if (succeeded(answer) && matches(KeptSchema, told)) return told;
	throw new Error(wordsOf(answer) || `The node did not keep the file (${String(answer.status)}).`);
}
