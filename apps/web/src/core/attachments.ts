/**
 * The files a message gives its model, as the person attaches them: each kept by the node by its content, named by
 * its reference; an image, a PDF, or text — what a model reads — its type told with it, as its bytes alone may not tell
 * it. A message is sent as parts, its words first, then each file; read back, a message's files are those parts that
 * name a kept file.
 */
import type { InputContent } from "@octoii-works/octoii-client";

/**
 * A file given with a message: its reference, what it is, its name and its size in bytes — and, where its thread is not
 * made yet to keep it, its bytes, sent once it is.
 */
export type Attachment = { readonly ref: string; readonly media_type: string; readonly name: string; readonly size: number; readonly bytes?: Blob };

/** The images a model looks at. */
const IMAGE_TYPES: readonly string[] = ["image/png", "image/jpeg", "image/gif", "image/webp"];

/** The other files a model reads: a PDF, or text. */
const FILE_TYPES: readonly string[] = ["application/pdf", "text/plain", "text/markdown", "text/csv", "text/html", "application/json", "application/xml"];

/** The names of text files a browser types as nothing, or as something else, by their extension: given as plain text. */
const TEXT_EXTENSIONS: readonly string[] = [
	"txt",
	"log",
	"md",
	"zig",
	"ts",
	"tsx",
	"js",
	"jsx",
	"py",
	"rs",
	"go",
	"c",
	"h",
	"cpp",
	"java",
	"kt",
	"swift",
	"sh",
	"toml",
	"yaml",
	"yml",
	"ini",
	"sql",
	"css",
];

/** Whether a file is an image, which the timeline shows as it is. */
export function isImage(mediaType: string): boolean {
	return IMAGE_TYPES.includes(mediaType);
}

/** What a file is given to a model as: its own type where a model takes it, Markdown or plain text by its name; none where a model reads no such file. */
export function givenTypeOf(file: { readonly type: string; readonly name: string }): string | undefined {
	if (IMAGE_TYPES.includes(file.type) || FILE_TYPES.includes(file.type)) return file.type;
	const extension = file.name.includes(".") ? (file.name.split(".").at(-1)?.toLowerCase() ?? "") : "";
	if (extension === "md") return "text/markdown";
	if (file.type.startsWith("text/") || TEXT_EXTENSIONS.includes(extension)) return "text/plain";
	return undefined;
}

/** A message's content: its words first, where it has any, then each file, as the file the thread keeps. */
export function contentOf(text: string, attachments: readonly Attachment[]): readonly InputContent[] {
	const files = attachments.map((attachment): InputContent => {
		const file = { ref: attachment.ref, media_type: attachment.media_type, filename: attachment.name };
		return isImage(attachment.media_type) ? { type: "input_image", file, detail: "auto" } : { type: "input_file", file };
	});
	return text.trim() === "" ? files : [{ type: "input_text", text }, ...files];
}

/** The files a message gives: each part of it that names a kept file. */
export function attachmentsOf(content: readonly InputContent[]): readonly Attachment[] {
	return content.flatMap((part) => {
		if (part.type === "input_text") return [];
		const { ref, media_type: mediaType, filename } = part.file;
		return [{ ref, media_type: mediaType, name: filename ?? "file", size: 0 }];
	});
}

/** Where the file the thread `thread` keeps by `ref` is read back from. */
export function fileAddress(thread: string, ref: string): string {
	return `/threads/${encodeURIComponent(thread)}/files/${ref.replace(/^sha256:/, "")}`;
}

/** Where the machine's file at `path` is read from — fitted to `box`, where it is an image and one is given. */
export function machineAddress(path: string, box?: { readonly width_max: number; readonly height_max: number }): string {
	const fitted = box === undefined ? "" : `&width_max=${String(box.width_max)}&height_max=${String(box.height_max)}`;
	return `/machine?path=${encodeURIComponent(path)}${fitted}`;
}

/** A file's size as the person reads it. */
export function sizeLabel(bytes: number): string {
	if (bytes < 1024) return `${String(bytes)} B`;
	if (bytes < 1024 * 1024) return `${String(Math.round(bytes / 1024))} KB`;
	return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
