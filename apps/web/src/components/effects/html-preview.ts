/** Reads a saved HTML original through the node, then exchanges document-only messages with its isolated preview. */
import { type RefObject, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { FileRef } from "@octoii-works/octoii-client";
import { Type } from "typebox";
import { matches } from "../../core/schema.ts";
import { readDownload } from "../files/file-download.ts";
import { useReach } from "../state/reach.ts";
import { failureOf, type Outcome } from "./read.ts";

/** A file's saved HTML; none while the authenticated read is pending. */
export function useHtmlFile(thread: string, file: FileRef): Outcome<string> | undefined {
	const { send } = useReach();
	const { ref, filename, media_type } = file;
	const asked = JSON.stringify([thread, ref, filename, media_type]);
	const [read, setRead] = useState<{ readonly asked: string; readonly outcome: Outcome<string> } | undefined>(undefined);
	useEffect(() => {
		let current = true;
		readDownload(send, thread, { ref, media_type, ...(filename === undefined ? {} : { filename }) })
			.then((original) => original.text())
			.then(
				(result) => {
					if (current) setRead({ asked, outcome: { result } });
				},
				(error: unknown) => {
					if (current) setRead({ asked, outcome: { failure: failureOf(error) } });
				},
			);
		return () => {
			current = false;
		};
	}, [send, thread, ref, filename, media_type, asked]);
	return read?.asked === asked ? read.outcome : undefined;
}

const PreviewMessage = Type.Union([
	Type.Object({ type: Type.Literal("octoii.html.ready") }),
	Type.Object({ type: Type.Literal("octoii.html.loaded") }),
	Type.Object({ type: Type.Literal("octoii.html.size"), height: Type.Number({ minimum: 0 }) }),
]);

/**
 * Only the preview window may request its document or report that it loaded and its size. Its opaque origin requires
 * a wildcard target; window identity keeps the original out of unrelated frames, and no message can invoke the app.
 */
export function htmlPreviewListener(
	destination: Pick<Window, "postMessage">,
	html: string,
	loaded: () => void,
	resized: (height: number) => void,
): (event: { readonly source: unknown; readonly data: unknown }) => void {
	return (event) => {
		if (event.source !== destination || !matches(PreviewMessage, event.data)) return;
		switch (event.data.type) {
			case "octoii.html.ready":
				destination.postMessage({ type: "octoii.html.load", html }, "*");
				break;
			case "octoii.html.loaded":
				loaded();
				break;
			case "octoii.html.size":
				// A document can scroll within its frame, but cannot grow the conversation without a bound.
				resized(Math.min(640, Math.max(128, Math.ceil(event.data.height))));
				break;
		}
	};
}

/** Connects a frame before its preview document can announce readiness. */
export function useHtmlFrame(html: string): {
	readonly frame: RefObject<HTMLIFrameElement | null>;
	readonly height: number;
	readonly loaded: boolean;
	readonly failure: string | undefined;
} {
	const frame = useRef<HTMLIFrameElement>(null);
	const [height, setHeight] = useState(240);
	const [loaded, setLoaded] = useState(false);
	const [failure, setFailure] = useState<string | undefined>(undefined);
	useLayoutEffect(() => {
		if (failure !== undefined) return undefined;
		const element = frame.current;
		if (element === null || element.contentWindow === null) return undefined;
		const destination = element.contentWindow;
		const failed = (): void => setFailure("The preview could not load. You can still download the original file.");
		const receive = htmlPreviewListener(
			destination,
			html,
			() => {
				clearTimeout(timeout);
				setLoaded(true);
			},
			setHeight,
		);
		const timeout = setTimeout(() => setFailure("The preview did not load. You can still download the original file."), 10_000);
		window.addEventListener("message", receive);
		element.addEventListener("error", failed);
		return () => {
			clearTimeout(timeout);
			window.removeEventListener("message", receive);
			element.removeEventListener("error", failed);
		};
	}, [html, failure]);
	return { frame, height, loaded, failure };
}
