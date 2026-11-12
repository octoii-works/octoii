/**
 * A file of the machine's as the files surface shows it: the names on the way to it, from the folder the tree shows,
 * with what the surface puts beside them; its lines, numbered and coloured as its language reads, a page at a time, the
 * next read as the person asks for it, its long lines wrapped where the device says so — or, written in markdown, drawn
 * as the device says, as a reply is; an image; a PDF, opened in the browser's own viewer in a tab of its own; that it is
 * binary; or the node's words where it could not be read.
 */
import type { ReactNode } from "react";
import type { FilePage } from "@octoii-works/octoii-client";
import { File } from "@pierre/diffs/react";
import { machineAddress } from "../../core/attachments.ts";
import { bytesText, crumbsOf, isMarkdown, nameOf } from "../../core/files.ts";
import type { Lines } from "../../core/links.ts";
import { useLineInView } from "../effects/line-in-view.ts";
import { useFileSource } from "../effects/file-source.ts";
import { usePages } from "../effects/pages.ts";
import { CODE_THEMES, useDarkPage } from "../effects/theme.ts";
import { Prose } from "../thread/markdown.tsx";
import { KeptImage } from "../ui/kept-image.tsx";

/**
 * What the view is given: the file at `path`, under the folder `root` the tree shows; whether its long lines wrap; whether
 * one written in markdown is drawn; what stands beside its names.
 */
export type FileViewProps = {
	readonly root: string;
	readonly path: string;
	/** The lines a link named, shown as it opens: marked, and scrolled to. */
	readonly lines?: Lines | undefined;
	readonly wrap: boolean;
	readonly preview: boolean;
	readonly actions: ReactNode;
};

/** The view. */
export function FileView(props: FileViewProps): ReactNode {
	const read = usePages("files.read", { path: props.path, offset: 1 }, (last) =>
		last.type === "text" && last.more ? { path: props.path, offset: last.from + last.lines.length } : undefined,
	);
	return (
		<>
			<div className="flex h-control-lg shrink-0 items-center gap-1 ps-3 pe-2">
				<Crumbs root={props.root} path={props.path} />
				<span className="flex-1" />
				{props.actions}
			</div>
			<div className="min-h-0 flex-1 overflow-auto">
				{read === undefined ? (
					<Note>Reading…</Note>
				) : "pages" in read ? (
					<Contents pages={read.pages} path={props.path} lines={props.lines} wrap={props.wrap} preview={props.preview} readMore={read.readMore} />
				) : null}
				{read?.failure === undefined ? null : <Note>{read.failure}</Note>}
			</div>
		</>
	);
}

/** The names on the way to the file, its own last. */
function Crumbs(props: { readonly root: string; readonly path: string }): ReactNode {
	const crumbs = crumbsOf(props.root, props.path);
	return (
		<div className="flex min-w-0 items-center gap-1 overflow-hidden text-xs text-muted-foreground">
			{crumbs.map((crumb) => (
				<span key={crumb.path} className="flex min-w-0 items-center gap-1 last:shrink-0">
					{crumb.path === props.root ? null : <span className="text-faint-foreground">›</span>}
					<span className={crumb.path === props.path ? "text-foreground" : "truncate"}>{crumb.name}</span>
				</span>
			))}
		</div>
	);
}

/** A line of words where a page shows nothing else. */
function Note(props: { readonly children: ReactNode }): ReactNode {
	return <p className="px-4 py-2 text-xs text-muted-foreground">{props.children}</p>;
}

/** What the file's pages hold: its lines — drawn, where it is written in markdown and `preview` says — an image, a PDF, or that it is binary, as its first page says. */
function Contents(props: {
	readonly pages: readonly [FilePage, ...FilePage[]];
	readonly path: string;
	readonly lines: Lines | undefined;
	readonly wrap: boolean;
	readonly preview: boolean;
	readonly readMore: (() => void) | undefined;
}): ReactNode {
	const [first] = props.pages;
	switch (first.type) {
		case "text": {
			const lines = props.pages.flatMap((page) => (page.type === "text" ? page.lines : []));
			return props.preview && isMarkdown(props.path) ? (
				<DrawnFile lines={lines} readMore={props.readMore} />
			) : (
				<TextFile name={nameOf(props.path)} lines={lines} marked={props.lines} wrap={props.wrap} readMore={props.readMore} />
			);
		}
		case "media":
			return first.media_type === "application/pdf" ? (
				<PdfNote size={first.size} path={first.path} />
			) : (
				<KeptImage className="m-4 max-w-[calc(100%-2rem)]" alt={nameOf(first.path)} address={machineAddress(first.path)} />
			);
		case "binary":
			return <Note>A binary file, {bytesText(first.size)}.</Note>;
	}
}

/**
 * The lines read of a file, numbered and coloured as its name says its language is, those `marked` marked and scrolled
 * to — read on to them where they lie past what is read; and where more follow, what reads them.
 */
function TextFile(props: {
	readonly name: string;
	readonly lines: readonly string[];
	readonly marked: Lines | undefined;
	readonly wrap: boolean;
	readonly readMore: (() => void) | undefined;
}): ReactNode {
	const dark = useDarkPage();
	const view = useLineInView(props.marked?.start, props.lines.length, props.readMore);
	if (props.lines.length === 0) return <Note>An empty file.</Note>;
	return (
		<>
			<div ref={view} className="code-view">
				<File
					file={{ name: props.name, contents: props.lines.join("\n") }}
					selectedLines={props.marked ?? null}
					disableWorkerPool
					options={{ theme: CODE_THEMES, themeType: dark ? "dark" : "light", overflow: props.wrap ? "wrap" : "scroll", disableFileHeader: true }}
				/>
			</div>
			<MoreLines readMore={props.readMore} />
		</>
	);
}

/** The lines read of a file written in markdown, drawn as a reply is; and where more follow, what reads them. */
function DrawnFile(props: { readonly lines: readonly string[]; readonly readMore: (() => void) | undefined }): ReactNode {
	if (props.lines.length === 0) return <Note>An empty file.</Note>;
	return (
		<>
			<Prose text={props.lines.join("\n")} className="max-w-3xl px-6 py-4" />
			<MoreLines readMore={props.readMore} />
		</>
	);
}

/** What reads the lines that follow, where more do. */
function MoreLines(props: { readonly readMore: (() => void) | undefined }): ReactNode {
	return props.readMore === undefined ? null : (
		<button type="button" onClick={props.readMore} className="px-4 py-2 text-xs text-muted-foreground hover:text-foreground">
			Show more lines
		</button>
	);
}

/** A PDF: its length, and what opens it in the browser's own viewer, in a tab of its own. */
function PdfNote(props: { readonly size: number; readonly path: string }): ReactNode {
	const source = useFileSource(machineAddress(props.path));
	const open = (): void => {
		if (source !== undefined) window.open(source, "_blank", "noopener,noreferrer");
	};
	return (
		<p className="flex items-center gap-2 px-4 py-2 text-xs text-muted-foreground">
			A PDF, {bytesText(props.size)}.
			<button type="button" onClick={open} className="text-foreground hover:underline">
				Open in a new tab
			</button>
		</p>
	);
}
