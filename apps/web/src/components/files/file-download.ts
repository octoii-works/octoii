/** Original thread files read through the page's authenticated node client, and saved with their own filenames. */
import type { FileRef } from "@octoii-works/octoii-client";
import { fileAddress } from "../../core/attachments.ts";
import type { SendRequest } from "@octoii-works/octoii-client";
import { succeeded, wordsOf } from "../state/reach.ts";

/** Reads the saved original, never the current machine path or an image fitted for display. */
export async function readDownload(send: SendRequest, thread: string, file: FileRef): Promise<File> {
	const answer = await send({ method: "GET", path: fileAddress(thread, file.ref) });
	if (!succeeded(answer)) throw new Error(wordsOf(answer) || `The node did not give the file (${String(answer.status)}).`);
	return new File([new Uint8Array(answer.bytes)], file.filename ?? "file", { type: file.media_type });
}

/** Starts a download. Its address is never opened as a document, including when the original is HTML. */
export function saveDownload(file: File): void {
	const address = URL.createObjectURL(new Blob([file], { type: "application/octet-stream" }));
	const link = document.createElement("a");
	link.href = address;
	link.download = file.name;
	document.body.append(link);
	link.click();
	link.remove();
	// Browsers may begin consuming the address after click returns.
	setTimeout(() => URL.revokeObjectURL(address), 60_000);
}
