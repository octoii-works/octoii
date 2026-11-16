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

/** Theme values passed to a document, never credentials or application capabilities. */
export type HtmlTheme = { readonly appearance: "light" | "dark"; readonly variables: Readonly<Record<string, string>> };

/** Read resolved tokens so a saved document follows the device's palette as well as light/dark mode. */
function previewTheme(): HtmlTheme {
	const root = document.documentElement;
	const style = getComputedStyle(root);
	const names = [
		"background",
		"foreground",
		"card",
		"popover",
		"muted-foreground",
		"border",
		"input",
		"primary",
		"primary-foreground",
		"ring",
		"error",
		"error-foreground",
		"warning",
		"warning-foreground",
		"success",
		"success-foreground",
		"info",
		"info-foreground",
		"font-sans",
		"font-mono",
		"text-body",
		"text-body--line-height",
		"text-heading-1",
		"text-heading-2",
		"text-heading-3",
		"text-heading--line-height",
	];
	const variables: Record<string, string> = {};
	for (const name of names) variables[`--${name}`] = style.getPropertyValue(`--${name}`).trim();
	variables["--muted"] = "color-mix(in oklab, var(--foreground) 4%, transparent)";
	// Visual replies use the brand color for accent, rather than the app's neutral hover surface.
	variables["--accent"] = "var(--primary)";
	variables["--radius"] = "0.625rem";
	variables["--chart-1"] = "var(--accent)";
	return { appearance: root.classList.contains("dark") ? "dark" : "light", variables };
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
	theme: () => HtmlTheme,
): (event: { readonly source: unknown; readonly data: unknown }) => void {
	return (event) => {
		if (event.source !== destination || !matches(PreviewMessage, event.data)) return;
		switch (event.data.type) {
			case "octoii.html.ready":
				destination.postMessage({ type: "octoii.html.load", html, theme: theme() }, "*");
				break;
			case "octoii.html.loaded":
				loaded();
				break;
			case "octoii.html.size":
				// A document can scroll within its frame, but cannot grow the conversation without a bound.
				resized(Math.min(20_000, Math.max(80, Math.ceil(event.data.height))));
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
		const sendTheme = (): void => destination.postMessage({ type: "octoii.html.theme", theme: previewTheme() }, "*");
		const observer = new MutationObserver(sendTheme);
		observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class", "style"] });
		const failed = (): void => setFailure("The preview could not load. You can still download the original file.");
		const receive = htmlPreviewListener(
			destination,
			html,
			() => {
				clearTimeout(timeout);
				setLoaded(true);
				sendTheme();
			},
			setHeight,
			previewTheme,
		);
		const timeout = setTimeout(() => setFailure("The preview did not load. You can still download the original file."), 10_000);
		window.addEventListener("message", receive);
		element.addEventListener("error", failed);
		return () => {
			clearTimeout(timeout);
			observer.disconnect();
			window.removeEventListener("message", receive);
			element.removeEventListener("error", failed);
		};
	}, [html, failure]);
	return { frame, height, loaded, failure };
}
