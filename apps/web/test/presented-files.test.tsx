/** File delivery remains explicit in persisted results, and downloads read their original bytes through the node client. */
import { describe, expect, it } from "bun:test";
import { conversationOf, type Entry, type FileRef, type InputContent, type ThreadView } from "@octoii-works/octoii-client";
import { renderToStaticMarkup } from "react-dom/server";
import { htmlPreviewListener } from "../src/components/effects/html-preview.ts";
import { readDownload } from "../src/components/files/file-download.ts";
import { type NodeRequest, type Reach, ReachContext } from "../src/components/state/reach.ts";
import { MessageThreadContext } from "../src/components/thread/message-thread.ts";
import { HtmlPreview } from "../src/components/thread/html-preview.tsx";
import { PresentedFiles } from "../src/components/thread/presented-files.tsx";
import { presentedFilesOf } from "../src/core/presented-files.ts";
import { itemsOf } from "../src/core/timeline.ts";

const report = { ref: "sha256:report", media_type: "application/pdf", filename: "result.pdf" };
const resource: InputContent = { type: "resource_link", file: report, size: 1024 };

/** Results loaded from a completed thread, just as opening it again reads the persisted entries. */
function callsOf(name: string, status: "ok" | "tool-error", output: readonly InputContent[]) {
	const entries: Entry[] = [
		{ id: "01", type: "input", delivery: "d1", actor: "person", content: [{ type: "input_text", text: "Make a report" }] },
		{
			id: "02",
			type: "reply",
			request: "q1",
			response: {
				id: "r1",
				model: "m",
				status: "completed",
				output: [{ type: "function_call", call_id: "c1", name, arguments: '{"path":"reports/result.pdf"}' }],
			},
			calls: [{ call_id: "c1", key: "k1" }],
		},
		{ id: "03", type: "result", call: "k1", status, output: { call_id: "c1", output: [...output] }, metadata: { title: "Verification report" } },
		{ id: "04", type: "turn-end", outcome: "replied" },
	];
	const loaded: Entry[] = JSON.parse(JSON.stringify(entries));
	const view: ThreadView = {
		entries: loaded,
		read: true,
		complete: true,
		status: { turn: "replied", inbox: [], asks: [], compacting: false },
		streaming: new Map(),
		gone: false,
	};
	return itemsOf(conversationOf(view)).flatMap((item) => (item.kind === "turn" ? item.turn.work.flatMap((work) => (work.kind === "call" ? [work] : [])) : []));
}

const reach: Reach = {
	open: () => {
		throw new Error("No socket needed to render files");
	},
	send: async () => ({ status: 200, contentType: "application/octet-stream", bytes: new Uint8Array() }),
	linkExample: "",
	readsLink: () => false,
	pair: async () => undefined,
};

describe("files delivered by tools", () => {
	it("reads explicit resource links from any tool, keeping the display title separate from its filename", () => {
		for (const name of ["present_file", "make_report"]) {
			const [call] = callsOf(name, "ok", [resource]);
			expect(call?.files).toEqual([{ ...resource, title: "Verification report" }]);
			expect(call?.subject).toBe("Verification report");
		}
	});

	it("keeps files delivered before execute failed, while model images and files remain model inputs", () => {
		const [call] = callsOf("execute", "tool-error", [
			resource,
			{ type: "input_text", text: "A later operation failed" },
			{ type: "input_image", file: { ref: "sha256:preview", media_type: "image/png" }, detail: "auto" },
			{ type: "input_file", file: { ref: "sha256:input", media_type: "text/plain" } },
		]);
		expect(call?.state).toBe("failed");
		expect(call?.files).toEqual([{ ...resource, title: "Verification report" }]);
		expect(call?.output).toBe("A later operation failed");
		expect(callsOf("present_file", "tool-error", [{ type: "input_text", text: "Could not save" }])[0]?.files).toEqual([]);
	});

	it("shows a persisted delivery from a failed execute with its display title, original filename and size", () => {
		const calls = callsOf("execute", "tool-error", [resource]);
		const html = renderToStaticMarkup(
			<ReachContext value={reach}>
				<PresentedFiles thread="t1" files={calls.flatMap((call) => call.files)} />
			</ReachContext>,
		);
		expect(html).toContain("Verification report");
		expect(html).toContain("result.pdf");
		expect(html).toContain("1 KB");
		expect(html).toContain('aria-label="More actions for result.pdf"');
		expect(html).not.toContain("/machine");
	});

	it("uses the saved image viewer and reads HTML for a preview without exposing the original's URL", () => {
		const draw = (file: FileRef): string =>
			renderToStaticMarkup(
				<ReachContext value={reach}>
					<MessageThreadContext value={{ thread: "t1", workspace: undefined, pages: [], terminals: [] }}>
						<PresentedFiles thread="t1" files={presentedFilesOf([{ type: "resource_link", file, size: 12 }], undefined)} />
					</MessageThreadContext>
				</ReachContext>,
			);
		const image = draw({ ref: "sha256:image", media_type: "image/png", filename: "original.png" });
		expect(image).toContain('src="/threads/t1/files/image?width_max=768&amp;height_max=256"');
		expect(image).toContain('aria-label="More actions for original.png"');
		const html = draw({ ref: "sha256:html", media_type: "text/html", filename: "page.html" });
		expect(html).toContain('aria-label="More actions for page.html"');
		expect(html).toContain("Loading preview…");
		expect(html).not.toContain("<iframe");
		expect(html).not.toContain("/threads/t1/files/html");
		expect(html).not.toContain("href=");
		expect(html).not.toContain("<img");
	});

	it("shows read HTML in an isolated HTTP frame with an expand action and no document injected into the app", () => {
		const html = renderToStaticMarkup(<HtmlPreview html="<script>parent.secret()</script><p>Saved document</p>" name="page.html" />);
		expect(html).toContain('src="/html-preview.html"');
		expect(html).toContain('sandbox="allow-scripts"');
		expect(html).toContain('referrerPolicy="no-referrer"');
		expect(html).toContain('title="Preview of page.html"');
		expect(html).toContain('aria-label="Expand page.html"');
		expect(html).not.toContain("allow-same-origin");
		expect(html).not.toContain("srcDoc");
		expect(html).not.toContain("blob:");
		expect(html).not.toContain("parent.secret");
		expect(html).not.toContain("Saved document");
		expect(html).not.toContain('aria-label="Download page.html"');
	});
});

describe("isolated HTML preview messages", () => {
	const theme = { appearance: "dark", variables: { "--background": "#181818" } } as const;
	it("sends only the original HTML to its own ready frame and ignores other windows and command messages", () => {
		const sent: { readonly message: unknown; readonly targetOrigin: string | WindowPostMessageOptions | undefined }[] = [];
		const destination: Pick<Window, "postMessage"> = {
			postMessage(message: unknown, targetOrigin?: string | WindowPostMessageOptions): void {
				sent.push({ message, targetOrigin });
			},
		};
		const html = "<!doctype html>\r\n<script>parent.postMessage({type:'command',command:'run'}, '*')</script>";
		let loaded = 0;
		const sizes: number[] = [];
		const receive = htmlPreviewListener(
			destination,
			html,
			() => {
				loaded++;
			},
			(height) => {
				sizes.push(height);
			},
			() => theme,
		);
		for (const source of [null, {}, { postMessage: destination.postMessage }]) {
			receive({ source, data: { type: "octoii.html.ready" } });
			receive({ source, data: { type: "octoii.html.loaded" } });
			receive({ source, data: { type: "octoii.html.size", width: 400, height: 300 } });
		}
		for (const data of [undefined, null, "ready", { type: "command", command: "run" }, { type: "octoii.html.load", html: "replacement" }]) {
			receive({ source: destination, data });
		}
		expect(sent).toEqual([]);
		expect(loaded).toBe(0);
		expect(sizes).toEqual([]);
		receive({ source: destination, data: { type: "octoii.html.ready" } });
		expect(sent).toEqual([{ message: { type: "octoii.html.load", html, theme }, targetOrigin: "*" }]);
		receive({ source: destination, data: { type: "octoii.html.loaded" } });
		expect(loaded).toBe(1);
	});

	it("bounds document height and rejects malformed or nonfinite size messages", () => {
		const destination: Pick<Window, "postMessage"> = { postMessage: () => undefined };
		const sizes: number[] = [];
		const receive = htmlPreviewListener(
			destination,
			"",
			() => undefined,
			(height) => {
				sizes.push(height);
			},
			() => theme,
		);
		for (const height of [0, 240.25, 1400, 300, 1e12]) receive({ source: destination, data: { type: "octoii.html.size", height } });
		expect(sizes).toEqual([80, 241, 1400, 300, 20_000]);
		for (const height of [-1, "300", NaN, Infinity]) receive({ source: destination, data: { type: "octoii.html.size", height } });
		receive({ source: destination, data: { type: "octoii.html.size" } });
		expect(sizes).toEqual([80, 241, 1400, 300, 20_000]);
	});
});

describe("downloading saved originals", () => {
	it("reads untransformed thread bytes with the file's MIME and filename for every downloadable format", async () => {
		const cases = [
			{ name: "报告.pdf", type: "application/pdf", bytes: new TextEncoder().encode("%PDF-1.4\n%%EOF\n") },
			{ name: "archive.zip", type: "application/zip", bytes: new Uint8Array([80, 75, 5, 6, ...new Uint8Array(18)]) },
			{ name: "notes.txt", type: "text/plain", bytes: new TextEncoder().encode("Saved original\n原件\n") },
			{
				name: "image.png",
				type: "image/png",
				bytes: Uint8Array.from(atob("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a2ioAAAAASUVORK5CYII="), (byte) =>
					byte.charCodeAt(0),
				),
			},
			{ name: "page.html", type: "text/html", bytes: new TextEncoder().encode("\uFEFF<!doctype html>\r\n<script>alert('saved')</script>\r\n") },
		];
		for (const { name, type, bytes } of cases) {
			const sent: NodeRequest[] = [];
			const file = await readDownload(
				async (request) => {
					sent.push(request);
					return { status: 200, contentType: "application/octet-stream", bytes };
				},
				"thread with spaces",
				{ ref: "sha256:saved", media_type: type, filename: name },
			);
			expect(sent).toEqual([{ method: "GET", path: "/threads/thread%20with%20spaces/files/saved" }]);
			expect(file.name).toBe(name);
			expect(file.type.split(";")[0]).toBe(type);
			expect(new Uint8Array(await file.arrayBuffer())).toEqual(bytes);
		}
	});

	it("reports denied or missing originals instead of downloading an error response", async () => {
		for (const file of [report, { ref: "sha256:html", media_type: "text/html", filename: "page.html" }]) {
			await expect(
				readDownload(async () => ({ status: 403, contentType: "text/plain", bytes: new TextEncoder().encode("Not authorized") }), "t1", file),
			).rejects.toThrow("Not authorized");
			await expect(readDownload(async () => ({ status: 404, contentType: undefined, bytes: new Uint8Array() }), "t1", file)).rejects.toThrow("(404)");
		}
	});
});
